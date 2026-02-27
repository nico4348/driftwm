use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            ImportDma,
            damage::OutputDamageTracker,
            element::{
                Kind,
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                render_elements,
                utils::RescaleRenderElement,
            },
            gles::{GlesRenderer, Uniform, UniformName, UniformType, element::PixelShaderElement},
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::{
        EventLoop,
        timer::{TimeoutAction, Timer},
    },
    utils::{Physical, Point, Rectangle, Transform},
};
use std::time::Duration;

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::desktop::layer_map_for_output;
use smithay::wayland::shell::wlr_layer::Layer as WlrLayer;

use crate::render::{build_cursor_elements, build_layer_elements, build_tile_background_elements};
use crate::state::{CalloopData, log_err};
use driftwm::canvas;

render_elements! {
    pub OutputRenderElements<=GlesRenderer>;
    Background=RescaleRenderElement<PixelShaderElement>,
    Tile=RescaleRenderElement<MemoryRenderBufferRenderElement<GlesRenderer>>,
    Window=RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Layer=WaylandSurfaceRenderElement<GlesRenderer>,
    Cursor=MemoryRenderBufferRenderElement<GlesRenderer>,
}

/// Uniform declarations for background shaders.
/// Shaders receive only u_camera — zoom is handled externally via RescaleRenderElement.
const BG_UNIFORMS: &[UniformName<'static>] = &[UniformName {
    name: std::borrow::Cow::Borrowed("u_camera"),
    type_: UniformType::_2f,
}];

/// Initialize the winit backend: create a window, set up the output, and
/// start the render loop timer.
pub fn init_winit(
    event_loop: &mut EventLoop<'static, CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let (backend, mut winit_evt) = winit::init::<GlesRenderer>()?;

    // Store backend on state so protocol handlers can access the renderer
    data.state.backend = Some(backend);

    // Create an Output representing the winit window (a virtual monitor)
    let size = data.state.backend.as_ref().unwrap().window_size();
    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(), // unknown physical size
            subpixel: Subpixel::Unknown,
            make: "driftwm".to_string(),
            model: "winit".to_string(),
        },
    );
    let mode = Mode {
        size,
        refresh: 60_000, // 60 Hz in mHz
    };
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, None);
    output.set_preferred(mode);

    // Advertise the output as a wl_output global so clients can see it
    output.create_global::<crate::state::DriftWm>(&data.display.handle());

    // Create DMA-BUF global — advertise GPU buffer formats to clients
    let formats = data
        .state
        .backend
        .as_mut()
        .unwrap()
        .renderer()
        .dmabuf_formats();
    let dmabuf_global = data
        .state
        .dmabuf_state
        .create_global::<crate::state::DriftWm>(&data.display.handle(), formats);
    data.state.dmabuf_global = Some(dmabuf_global);

    // Compile background shader: explicit path > built-in dot grid default
    let shader_source = if let Some(path) = data.state.config.background.shader_path.as_deref() {
        std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read shader {path}: {e}"))
    } else if data.state.config.background.tile_path.is_none() {
        driftwm::config::DEFAULT_SHADER.to_string()
    } else {
        String::new()
    };
    if !shader_source.is_empty() {
        let shader = data
            .state
            .backend
            .as_mut()
            .unwrap()
            .renderer()
            .compile_custom_pixel_shader(&shader_source, BG_UNIFORMS)
            .expect("Failed to compile background shader");
        data.state.background_shader = Some(shader.clone());

        // Create the cached element once — its stable Id lets the damage tracker
        // recognise it across frames and skip re-rendering when nothing changed.
        // Area is in canvas space; zoom scaling is applied externally.
        let area = Rectangle::from_size(size.to_logical(1));
        data.state.cached_bg_element = Some(PixelShaderElement::new(
            shader,
            area,
            Some(vec![area]),
            1.0,
            vec![Uniform::new("u_camera", (0.0f32, 0.0f32))],
            Kind::Unspecified,
        ));
    }

    // Load tile image if tile_path is set (and no shader — shader takes priority)
    if data.state.background_shader.is_none()
        && let Some(path) = data.state.config.background.tile_path.as_deref()
    {
        let img = image::open(path)
            .unwrap_or_else(|e| panic!("Failed to load tile image {path}: {e}"))
            .into_rgba8();
        let (w, h) = img.dimensions();
        let raw = img.into_raw();

        // Build (w+2)×(h+2) buffer: duplicate last 2 cols/rows so adjacent
        // tiles overlap by 2 opaque pixels, covering sub-pixel rounding gaps.
        let pad = 2usize;
        let ew = w as usize + pad;
        let eh = h as usize + pad;
        let mut expanded = vec![0u8; ew * eh * 4];
        for y in 0..h as usize {
            let src_row = y * w as usize * 4;
            let dst_row = y * ew * 4;
            expanded[dst_row..dst_row + w as usize * 4]
                .copy_from_slice(&raw[src_row..src_row + w as usize * 4]);
            // Duplicate last pixel into the extra columns
            let last_px = &raw[src_row + (w as usize - 1) * 4..src_row + w as usize * 4];
            for p in 0..pad {
                let dst = dst_row + (w as usize + p) * 4;
                expanded[dst..dst + 4].copy_from_slice(last_px);
            }
        }
        // Duplicate last row into the extra rows
        let last_row: Vec<u8> = expanded[(h as usize - 1) * ew * 4..h as usize * ew * 4].to_vec();
        for p in 0..pad {
            let dst = (h as usize + p) * ew * 4;
            expanded[dst..dst + ew * 4].copy_from_slice(&last_row);
        }

        let buffer = MemoryRenderBuffer::from_slice(
            &expanded,
            Fourcc::Abgr8888,
            (ew as i32, eh as i32),
            1,
            Transform::Normal,
            None,
        );
        data.state.background_tile = Some((buffer, w as i32, h as i32));
    }

    // Centre the viewport so canvas origin (0, 0) is in the middle of the screen
    let logical_size = size.to_logical(1);
    data.state.camera = Point::from((
        -(logical_size.w as f64) / 2.0,
        -(logical_size.h as f64) / 2.0,
    ));

    // Map the output into the space at the initial camera position
    data.state
        .space
        .map_output(&output, data.state.camera.to_i32_round());

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    // Render loop: fires immediately, then re-arms at ~60fps
    let timer = Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, data| {
            // --- Advance frame counter ---
            data.state.frame_counter = data.state.frame_counter.wrapping_add(1);

            // --- Dispatch winit events ---
            let mut stop = false;
            winit_evt.dispatch_new_events(|event| match event {
                WinitEvent::Resized { size, scale_factor } => {
                    let new_mode = Mode {
                        size,
                        refresh: 60_000,
                    };
                    output.change_current_state(
                        Some(new_mode),
                        None,
                        Some(smithay::output::Scale::Fractional(scale_factor)),
                        None,
                    );
                }
                WinitEvent::Input(event) => {
                    data.state.process_input_event(event);
                }
                WinitEvent::CloseRequested => {
                    stop = true;
                }
                _ => {}
            });

            if stop {
                data.state.loop_signal.stop();
                return TimeoutAction::Drop;
            }

            // --- Dispatch Wayland client messages before rendering ---
            log_err(
                "dispatch_clients",
                data.display.dispatch_clients(&mut data.state),
            );
            log_err("flush_clients", data.display.flush_clients());

            // --- Delta time ---
            let now = std::time::Instant::now();
            let dt = now - data.state.last_frame_instant;
            data.state.last_frame_instant = now;

            // --- Key repeat for compositor bindings ---
            data.state.apply_key_repeat();

            // --- Scroll momentum ---
            data.state.apply_scroll_momentum();

            // --- Edge auto-pan (window drag near viewport edges) ---
            data.state.apply_edge_pan();

            // --- Camera animation (window navigation) ---
            data.state.apply_camera_animation(dt);

            // --- Zoom animation ---
            data.state.apply_zoom_animation(dt);

            // --- Update cached background element (before taking backend) ---
            // Shader renders at canvas scale; zoom is applied externally via RescaleRenderElement.
            let camera_moved = data.state.camera != data.state.last_rendered_camera;
            let zoom_changed = data.state.zoom != data.state.last_rendered_zoom;
            if let Some(ref mut elem) = data.state.cached_bg_element {
                let scale = output.current_scale().integer_scale();
                let output_size = output
                    .current_mode()
                    .map(|m| m.size.to_logical(scale))
                    .unwrap_or((1, 1).into());
                // Canvas-space visible area: viewport / zoom
                let canvas_w = (output_size.w as f64 / data.state.zoom).ceil() as i32;
                let canvas_h = (output_size.h as f64 / data.state.zoom).ceil() as i32;
                let canvas_area = Rectangle::from_size((canvas_w, canvas_h).into());
                // resize() no-ops when area is unchanged (internal guard)
                elem.resize(canvas_area, Some(vec![canvas_area]));
                // update_uniforms() always bumps commit_counter — only call on change
                if camera_moved || zoom_changed {
                    elem.update_uniforms(vec![Uniform::new(
                        "u_camera",
                        (data.state.camera.x as f32, data.state.camera.y as f32),
                    )]);
                }
            }

            // --- Take backend to split borrow from state ---
            let mut backend = data.state.backend.take().unwrap();

            // --- Build cursor element ---
            let cursor_elements = build_cursor_elements(&mut data.state, backend.renderer());

            // --- Render ---
            let mut age = backend.buffer_age().unwrap_or(0);
            // Force full repaint when tiles move — all tile elements share the
            // same buffer Id, so the damage tracker can't track them individually.
            if data.state.background_tile.is_some() && (camera_moved || zoom_changed) {
                age = 0;
            }
            let render_ok = match backend.bind() {
                Ok((renderer, mut framebuffer)) => {
                    // Compute visible canvas rect and collect window elements
                    let viewport_size = data.state.get_viewport_size();
                    let visible_rect = canvas::visible_canvas_rect(
                        data.state.camera.to_i32_round(),
                        viewport_size,
                        data.state.zoom,
                    );
                    let output_scale = output.current_scale().fractional_scale();
                    let window_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = data
                        .state
                        .space
                        .render_elements_for_region(renderer, &visible_rect, output_scale, 1.0);

                    // Wrap each window element in RescaleRenderElement for zoom
                    let zoomed_windows: Vec<OutputRenderElements> = window_elements
                        .into_iter()
                        .map(|elem| {
                            OutputRenderElements::Window(RescaleRenderElement::from_element(
                                elem,
                                Point::<i32, Physical>::from((0, 0)),
                                data.state.zoom,
                            ))
                        })
                        .collect();

                    // Background: shader or tiled image
                    let bg_elements: Vec<OutputRenderElements> =
                        if let Some(ref elem) = data.state.cached_bg_element {
                            vec![OutputRenderElements::Background(
                                RescaleRenderElement::from_element(
                                    elem.clone(),
                                    Point::<i32, Physical>::from((0, 0)),
                                    data.state.zoom,
                                ),
                            )]
                        } else if data.state.background_tile.is_some() {
                            build_tile_background_elements(&data.state, renderer, &output)
                        } else {
                            vec![]
                        };

                    // Build layer surface elements (screen-fixed, NOT zoomed)
                    let is_fullscreen = data.state.fullscreen.is_some();
                    let overlay_elements = build_layer_elements(&output, renderer, WlrLayer::Overlay);
                    let top_elements = if !is_fullscreen {
                        build_layer_elements(&output, renderer, WlrLayer::Top)
                    } else {
                        vec![]
                    };
                    let bottom_elements = if !is_fullscreen {
                        build_layer_elements(&output, renderer, WlrLayer::Bottom)
                    } else {
                        vec![]
                    };
                    let background_layer_elements =
                        build_layer_elements(&output, renderer, WlrLayer::Background);

                    // Compose all elements (first = topmost):
                    // cursor > overlay > top > zoomed_windows > bottom > bg_shader/tiles > background_layers
                    let clear_color = [0.0f32, 0.0, 0.0, 1.0];
                    let mut all_elements: Vec<OutputRenderElements> = Vec::with_capacity(
                        cursor_elements.len()
                            + overlay_elements.len()
                            + top_elements.len()
                            + zoomed_windows.len()
                            + bottom_elements.len()
                            + bg_elements.len()
                            + background_layer_elements.len(),
                    );
                    all_elements.extend(
                        cursor_elements
                            .into_iter()
                            .map(OutputRenderElements::Cursor),
                    );
                    all_elements.extend(overlay_elements);
                    all_elements.extend(top_elements);
                    all_elements.extend(zoomed_windows);
                    all_elements.extend(bottom_elements);
                    all_elements.extend(bg_elements);
                    all_elements.extend(background_layer_elements);

                    let result = damage_tracker.render_output(
                        renderer,
                        &mut framebuffer,
                        age,
                        &all_elements,
                        clear_color,
                    );
                    if let Err(err) = result {
                        tracing::warn!("Render error: {err:?}");
                    }
                    true
                }
                Err(err) => {
                    tracing::warn!("Backend bind error: {err:?}");
                    false
                }
            };
            if render_ok && let Err(err) = backend.submit(None) {
                tracing::warn!("Submit error: {err:?}");
            }

            // --- Record camera+zoom for next-frame change detection ---
            data.state.last_rendered_camera = data.state.camera;
            data.state.last_rendered_zoom = data.state.zoom;

            // --- Put backend back ---
            data.state.backend = Some(backend);

            // --- Foreign toplevel refresh ---
            {
                let keyboard = data.state.seat.get_keyboard().unwrap();
                let focused = keyboard.current_focus().map(|f| f.0);
                driftwm::protocols::foreign_toplevel::refresh::<crate::state::DriftWm>(
                    &mut data.state.foreign_toplevel_state,
                    &data.state.space,
                    focused.as_ref(),
                    &output,
                );
            }

            // --- Post-render: send frame callbacks to clients ---
            let time = data.state.start_time.elapsed();
            for window in data.state.space.elements() {
                window.send_frame(&output, time, Some(Duration::ZERO), |_, _| {
                    Some(output.clone())
                });
            }

            // Layer surface frame callbacks
            {
                let layer_map = layer_map_for_output(&output);
                for layer_surface in layer_map.layers() {
                    layer_surface.send_frame(
                        &output,
                        time,
                        Some(Duration::ZERO),
                        |_, _| Some(output.clone()),
                    );
                }
            }

            // --- Cleanup ---
            data.state.space.refresh();
            data.state.popups.cleanup();
            layer_map_for_output(&output).cleanup();
            log_err("flush_clients", data.display.flush_clients());

            TimeoutAction::ToDuration(Duration::from_millis(16))
        })?;

    Ok(())
}
