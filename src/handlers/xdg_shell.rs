use std::cell::RefCell;

use crate::grabs::{MoveSurfaceGrab, ResizeState, ResizeSurfaceGrab};
use crate::state::DriftWm;
use smithay::{
    delegate_xdg_shell,
    desktop::Window,
    input::pointer::{Focus, GrabStartData},
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{Resource, protocol::wl_seat},
    },
    utils::Serial,
    wayland::{
        compositor::with_states,
        seat::WaylandFocus,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
    },
};

impl XdgShellHandler for DriftWm {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        tracing::info!("New toplevel surface");
        let window = Window::new_wayland_window(surface);

        // Place window at center of first output
        let pos = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
            .map(|geo| (geo.size.w / 2 - 300, geo.size.h / 2 - 200))
            .unwrap_or((0, 0));

        // Send initial configure — the client won't render until it gets this
        window.toplevel().unwrap().send_configure();

        self.space.map_element(window, pos, true);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let _ = self.popups.track_popup(smithay::desktop::PopupKind::Xdg(surface));
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
        surface.send_configure().ok();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        // Collect first to avoid holding an immutable borrow on space
        let window = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &wl_surface)
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }
    }

    fn move_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
    ) {
        let wl_surface = surface.wl_surface().clone();
        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &wl_surface)
            .cloned()
        else {
            return;
        };

        let pointer = self.seat.get_pointer().unwrap();
        let Some(start_data) = check_grab(&pointer, &wl_surface) else {
            return;
        };

        let initial_window_location = self.space.element_location(&window).unwrap();
        let grab = MoveSurfaceGrab {
            start_data,
            window,
            initial_window_location,
        };
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let wl_surface = surface.wl_surface().clone();
        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &wl_surface)
            .cloned()
        else {
            return;
        };

        let pointer = self.seat.get_pointer().unwrap();
        let Some(start_data) = check_grab(&pointer, &wl_surface) else {
            return;
        };

        let initial_window_location = self.space.element_location(&window).unwrap();
        let initial_window_size = window.geometry().size;

        // Store resize state in the surface data map for commit() repositioning
        with_states(&wl_surface, |states| {
            states
                .data_map
                .get_or_insert(|| RefCell::new(ResizeState::Idle))
                .replace(ResizeState::Resizing {
                    edges,
                    initial_window_location,
                    initial_window_size,
                });
        });

        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
        });

        let grab = ResizeSurfaceGrab {
            start_data,
            window,
            edges,
            initial_window_location,
            initial_window_size,
            last_window_size: initial_window_size,
        };
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }
}

delegate_xdg_shell!(DriftWm);

/// Validate that the pointer has an active grab starting on the given surface.
/// Returns the `GrabStartData` if the button click that started the grab
/// originated on this surface (preventing a client from stealing another's grab).
fn check_grab(
    pointer: &smithay::input::pointer::PointerHandle<DriftWm>,
    surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
) -> Option<GrabStartData<DriftWm>> {
    let start_data = pointer.grab_start_data()?;
    let (focus, _) = start_data.focus.as_ref()?;

    // The button press must have been on this surface (or a child of it)
    if !focus.same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}
