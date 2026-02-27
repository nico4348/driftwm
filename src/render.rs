use smithay::{
    backend::renderer::{
        element::{
            Kind,
            memory::{MemoryRenderBufferRenderElement},
            utils::RescaleRenderElement,
        },
        gles::GlesRenderer,
    },
    input::pointer::CursorImageStatus,
    output::Output,
    utils::{Physical, Point},
};

use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::desktop::layer_map_for_output;
use smithay::wayland::shell::wlr_layer::Layer as WlrLayer;

use driftwm::canvas::{CanvasPos, canvas_to_screen};

use crate::winit::OutputRenderElements;

/// Build tiled background elements for the current frame.
///
/// Each tile is a (w+2)×(h+2) buffer with the last col/row duplicated,
/// stepped at the original (w, h) interval. The 1px overlap covers any
/// sub-pixel rounding gaps from RescaleRenderElement at fractional zoom.
pub fn build_tile_background_elements(
    state: &crate::state::DriftWm,
    renderer: &mut GlesRenderer,
    output: &Output,
) -> Vec<OutputRenderElements> {
    let scale = output.current_scale().integer_scale();
    let output_size = output
        .current_mode()
        .map(|m| m.size.to_logical(scale))
        .unwrap_or((1, 1).into());

    let Some((tile_buf, tw, th)) = &state.background_tile else {
        return vec![];
    };
    let tw = *tw;
    let th = *th;
    if tw <= 0 || th <= 0 {
        return vec![];
    }

    let cam_x = state.camera.x;
    let cam_y = state.camera.y;
    let zoom = state.zoom;

    // Visible canvas area: viewport / zoom
    let visible_w = output_size.w as f64 / zoom;
    let visible_h = output_size.h as f64 / zoom;

    // First visible tile: snap camera to tile grid
    let start_x = (cam_x / tw as f64).floor() as i64 * tw as i64;
    let start_y = (cam_y / th as f64).floor() as i64 * th as i64;
    let end_x = (cam_x + visible_w).ceil() as i64;
    let end_y = (cam_y + visible_h).ceil() as i64;

    let mut elements = Vec::new();
    let mut ty = start_y;
    while ty < end_y {
        let mut tx = start_x;
        while tx < end_x {
            let canvas_rel_x = tx as f64 - cam_x;
            let canvas_rel_y = ty as f64 - cam_y;
            let pos: Point<f64, Physical> = (canvas_rel_x, canvas_rel_y).into();

            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                pos,
                tile_buf,
                None,
                None,
                None,
                Kind::Unspecified,
            ) {
                elements.push(OutputRenderElements::Tile(
                    RescaleRenderElement::from_element(
                        elem,
                        Point::<i32, Physical>::from((0, 0)),
                        zoom,
                    ),
                ));
            }
            tx += tw as i64;
        }
        ty += th as i64;
    }
    elements
}

/// Build render elements for all layer surfaces on the given layer.
/// Layer surfaces are screen-fixed (not zoomed), so they use raw WaylandSurfaceRenderElement.
pub fn build_layer_elements(
    output: &Output,
    renderer: &mut GlesRenderer,
    layer: WlrLayer,
) -> Vec<OutputRenderElements> {
    let map = layer_map_for_output(output);
    let output_scale = output.current_scale().fractional_scale();
    let mut elements = Vec::new();

    for surface in map.layers_on(layer).rev() {
        let geo = map.layer_geometry(surface).unwrap_or_default();
        let loc = geo.loc.to_physical_precise_round(output_scale);
        elements.extend(
            surface
                .render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    loc,
                    smithay::utils::Scale::from(output_scale),
                    1.0,
                )
                .into_iter()
                .map(OutputRenderElements::Layer),
        );
    }

    elements
}

/// Resolve which xcursor name to load for the current cursor status.
pub fn cursor_icon_name(status: &CursorImageStatus) -> Option<&'static str> {
    match status {
        CursorImageStatus::Hidden => None,
        CursorImageStatus::Named(icon) => Some(icon.name()),
        // Client-provided surface cursor — fall back to default for now
        CursorImageStatus::Surface(_) => Some("default"),
    }
}

/// Build the cursor render element(s) for the current frame.
pub fn build_cursor_elements(
    state: &mut crate::state::DriftWm,
    renderer: &mut GlesRenderer,
) -> Vec<MemoryRenderBufferRenderElement<GlesRenderer>> {
    let pointer = state.seat.get_pointer().unwrap();
    let canvas_pos = pointer.current_location();
    // Custom elements are in screen-local physical coords
    let screen_pos = canvas_to_screen(CanvasPos(canvas_pos), state.camera, state.zoom).0;
    let physical_pos: Point<f64, Physical> = (screen_pos.x, screen_pos.y).into();

    // Extract cursor name before borrowing state mutably for load_xcursor
    let Some(name) = cursor_icon_name(&state.cursor_status) else {
        return vec![];
    };

    // Try loading by CSS name, fall back to "default"
    let loaded = state.load_xcursor(name).is_some();
    if !loaded && state.load_xcursor("default").is_none() {
        return vec![];
    }
    let key = if loaded { name } else { "default" };
    let (buffer, hotspot) = state.cursor_buffers.get(key).unwrap();
    let hotspot = *hotspot;

    let pos = physical_pos - Point::from((hotspot.x as f64, hotspot.y as f64));
    match MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos,
        buffer,
        None,
        None,
        None,
        Kind::Cursor,
    ) {
        Ok(elem) => vec![elem],
        Err(_) => vec![],
    }
}
