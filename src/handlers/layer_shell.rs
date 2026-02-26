use smithay::{
    delegate_layer_shell,
    desktop::{self, PopupKind, layer_map_for_output},
    reexports::wayland_server::{Resource, protocol::wl_output::WlOutput},
    utils::SERIAL_COUNTER,
    wayland::shell::{
        wlr_layer::{
            Layer, LayerSurface, WlrLayerShellHandler, WlrLayerShellState,
        },
        xdg::PopupSurface,
    },
};

use crate::state::{DriftWm, FocusTarget};

impl WlrLayerShellHandler for DriftWm {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        tracing::info!("New layer surface: {namespace}");

        // Resolve output: use requested output or fall back to the first available
        let resolved_output = output
            .as_ref()
            .and_then(|wl_out| {
                let client = wl_out.client()?;
                self.space.outputs().find(|o| {
                    o.client_outputs(&client)
                        .any(|co| co == *wl_out)
                })
            })
            .cloned()
            .or_else(|| self.space.outputs().next().cloned());

        let Some(resolved_output) = resolved_output else {
            tracing::warn!("No output available for layer surface");
            return;
        };

        // Wrap protocol-level LayerSurface in the desktop-level type for layer map
        let desktop_surface = desktop::LayerSurface::new(surface, namespace);

        let mut map = layer_map_for_output(&resolved_output);
        if let Err(e) = map.map_layer(&desktop_surface) {
            tracing::warn!("Failed to map layer surface: {e}");
        }
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        tracing::info!("Layer surface destroyed");

        // Reset pointer_over_layer — the surface may have been under the pointer.
        // Next motion event will re-evaluate, but this prevents stale state in between.
        self.pointer_over_layer = false;

        // Find which output this surface was on and unmap it
        let wl_surface = surface.wl_surface().clone();
        for output in self.space.outputs().cloned().collect::<Vec<_>>() {
            let mut map = layer_map_for_output(&output);
            let found = map
                .layers()
                .find(|l| l.wl_surface() == &wl_surface)
                .cloned();
            if let Some(layer) = found {
                map.unmap_layer(&layer);
                break;
            }
        }

        // If this surface had exclusive keyboard focus, return focus to the top window
        let keyboard = self.seat.get_keyboard().unwrap();
        let current_focus = keyboard.current_focus();
        if current_focus.as_ref().is_some_and(|f| f.0 == wl_surface) {
            let serial = SERIAL_COUNTER.next_serial();
            let new_focus = self
                .focus_history
                .first()
                .map(|w| FocusTarget(w.toplevel().unwrap().wl_surface().clone()));
            keyboard.set_focus(self, new_focus, serial);
        }
    }

    fn new_popup(&mut self, _parent: LayerSurface, popup: PopupSurface) {
        let popup = PopupKind::Xdg(popup);
        self.unconstrain_popup(&popup);

        if let Err(err) = self.popups.track_popup(popup) {
            tracing::warn!("error tracking layer popup: {err}");
        }
    }
}

delegate_layer_shell!(DriftWm);
