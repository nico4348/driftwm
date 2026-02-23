use std::cell::RefCell;

use crate::grabs::{ResizeState, has_left, has_top};
use crate::state::{ClientState, DriftWm};
use smithay::{
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{protocol::wl_buffer::WlBuffer, Client},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, with_states, CompositorClientState,
            CompositorHandler, CompositorState,
        },
        shell::xdg::XdgToplevelSurfaceData,
        shm::{ShmHandler, ShmState},
    },
};

impl CompositorHandler for DriftWm {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) {
        // Update renderer surface state (buffer dimensions, surface_view, textures).
        // Without this, bbox_from_surface_tree() can't see any surfaces and returns 0x0.
        smithay::backend::renderer::utils::on_commit_buffer_handler::<DriftWm>(surface);

        // For subsurfaces, walk up to root and notify the window
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            let window = self
                .space
                .elements()
                .find(|w| w.toplevel().unwrap().wl_surface() == &root)
                .cloned();
            if let Some(window) = window {
                window.on_commit();

                // During resize, adjust window position for top/left edge drags
                self.handle_resize_commit(&window, &root);
            }
        }

        // Handle popup commits
        self.popups.commit(surface);

        // Send initial configure for unmapped xdg toplevels
        ensure_initial_configure(surface, self);
    }
}

/// If a surface belongs to an xdg toplevel that hasn't been configured yet,
/// send the initial configure event so the client can start rendering.
fn ensure_initial_configure(
    surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    state: &DriftWm,
) {
    if let Some(window) = state
        .space
        .elements()
        .find(|w| w.toplevel().unwrap().wl_surface() == surface)
    {
        let toplevel = window.toplevel().unwrap();
        let initial_configure_sent = smithay::wayland::compositor::with_states(
            toplevel.wl_surface(),
            |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            },
        );
        if !initial_configure_sent {
            toplevel.send_configure();
        }
    }
}

impl DriftWm {
    /// When resizing from top or left edges, the window position must shift
    /// to compensate for the size change — otherwise the opposite edge moves.
    fn handle_resize_commit(
        &mut self,
        window: &smithay::desktop::Window,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let resize_state = with_states(surface, |states| {
            *states
                .data_map
                .get_or_insert(|| RefCell::new(ResizeState::Idle))
                .borrow()
        });

        let (edges, initial_window_location, initial_window_size) = match resize_state {
            ResizeState::Resizing { edges, initial_window_location, initial_window_size }
            | ResizeState::WaitingForLastCommit { edges, initial_window_location, initial_window_size } => {
                (edges, initial_window_location, initial_window_size)
            }
            ResizeState::Idle => return,
        };

        let current_geo = window.geometry();
        let mut new_loc = initial_window_location;

        // Compute position absolutely from initial location to avoid cumulative drift
        if has_top(edges) {
            new_loc.y = initial_window_location.y + (initial_window_size.h - current_geo.size.h);
        }
        if has_left(edges) {
            new_loc.x = initial_window_location.x + (initial_window_size.w - current_geo.size.w);
        }

        self.space.map_element(window.clone(), new_loc, false);

        // If we're waiting for the final commit, go idle
        if matches!(resize_state, ResizeState::WaitingForLastCommit { .. }) {
            with_states(surface, |states| {
                states
                    .data_map
                    .get_or_insert(|| RefCell::new(ResizeState::Idle))
                    .replace(ResizeState::Idle);
            });
        }
    }
}

impl BufferHandler for DriftWm {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for DriftWm {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(DriftWm);
delegate_shm!(DriftWm);
