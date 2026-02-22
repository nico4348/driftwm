use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker,
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    desktop::space::render_output,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::{
        timer::{TimeoutAction, Timer},
        EventLoop,
    },
    utils::Transform,
};
use std::time::Duration;

use crate::state::CalloopData;

/// Initialize the winit backend: create a window, set up the output, and
/// start the render loop timer.
pub fn init_winit(
    event_loop: &mut EventLoop<'static, CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, mut winit_evt) = winit::init::<GlesRenderer>()?;

    // Create an Output representing the winit window (a virtual monitor)
    let size = backend.window_size();
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

    // Map the output into the space at (0, 0)
    data.state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    // Render loop: fires immediately, then re-arms at ~60fps
    let timer = Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, data| {
            // --- Dispatch winit events ---
            // CloseRequested already calls loop_signal.stop(), so we don't
            // need to inspect PumpStatus separately.
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
            data.display.dispatch_clients(&mut data.state).unwrap();
            data.display.flush_clients().unwrap();

            // --- Render ---
            // buffer_age() before bind() to avoid borrow conflicts.
            // First frame returns None → 0 (full redraw), which is correct.
            let age = backend.buffer_age().unwrap_or(0);
            {
                let (renderer, mut framebuffer) = backend.bind().unwrap();

                let result = render_output(
                    &output,
                    renderer,
                    &mut framebuffer,
                    1.0, // alpha
                    age,
                    [&data.state.space],
                    &[] as &[smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement<GlesRenderer>],
                    &mut damage_tracker,
                    [0.1, 0.1, 0.1, 1.0], // dark grey background
                );

                if let Err(err) = result {
                    tracing::warn!("Render error: {err:?}");
                }
                // renderer + framebuffer drop here, releasing the mutable borrow
            }
            backend.submit(None).unwrap();

            // --- Post-render: send frame callbacks to clients ---
            let time = data.state.start_time.elapsed();
            for window in data.state.space.elements() {
                window.send_frame(
                    &output,
                    time,
                    Some(Duration::ZERO),
                    |_, _| Some(output.clone()),
                );
            }

            // --- Cleanup ---
            data.state.space.refresh();
            data.state.popups.cleanup();
            data.display.flush_clients().unwrap();

            TimeoutAction::ToDuration(Duration::from_millis(16))
        })?;

    Ok(())
}
