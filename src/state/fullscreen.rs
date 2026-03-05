use smithay::{
    desktop::Window,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Point},
};

use super::{DriftWm, FocusTarget, FullscreenState};

impl DriftWm {
    /// Enter fullscreen for the given window: lock viewport, expand window to fill screen.
    pub fn enter_fullscreen(&mut self, window: &Window) {
        let output = self.active_output().unwrap();

        // If already fullscreen on this output, exit first
        if self.fullscreen.contains_key(&output) {
            self.exit_fullscreen();
        }

        let viewport_size = self.get_viewport_size();
        let saved_location = self.space.element_location(window).unwrap_or_default();

        self.fullscreen.insert(output, FullscreenState {
            window: window.clone(),
            saved_location,
            saved_camera: self.camera(),
            saved_zoom: self.zoom(),
        });

        // Tell the client to go fullscreen at output size
        window.toplevel().unwrap().with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Fullscreen);
            state.size = Some(viewport_size);
        });
        window.toplevel().unwrap().send_configure();

        // Lock viewport: stop all animations and momentum
        self.with_output_state(|os| {
            os.zoom = 1.0;
            os.zoom_target = None;
            os.zoom_animation_center = None;
            os.camera_target = None;
            os.momentum.stop();
            os.overview_return = None;
        });
        // Top/Bottom layers are hidden during fullscreen — reset stale pointer state
        self.pointer_over_layer = false;

        // Snap camera to integer for pixel-perfect alignment
        let camera_i32 = self.camera().to_i32_round();
        self.set_camera(Point::from((camera_i32.x as f64, camera_i32.y as f64)));

        // Place window at viewport origin and raise
        self.space.map_element(window.clone(), camera_i32, true);
        self.space.raise_element(window, true);
        self.enforce_below_windows();
        self.update_output_from_camera();

        // Ensure keyboard focus is on the fullscreen window
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let surface = window.toplevel().unwrap().wl_surface().clone();
        keyboard.set_focus(self, Some(FocusTarget(surface)), serial);
    }

    /// Exit fullscreen on the active output: restore window position, camera, and zoom.
    pub fn exit_fullscreen(&mut self) {
        let Some(output) = self.active_output() else { return };
        self.exit_fullscreen_on(&output);
    }

    /// Exit fullscreen on a specific output.
    pub fn exit_fullscreen_on(&mut self, output: &smithay::output::Output) {
        let Some(fs) = self.fullscreen.remove(output) else {
            return;
        };

        // Tell client to leave fullscreen
        fs.window.toplevel().unwrap().with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
            state.size = None;
        });
        fs.window.toplevel().unwrap().send_configure();

        // Restore window position, camera, zoom on the specific output
        self.space.map_element(fs.window, fs.saved_location, false);
        {
            let mut os = super::output_state(output);
            os.camera = fs.saved_camera;
            os.zoom = fs.saved_zoom;
        }
        self.update_output_from_camera();
    }

    /// Find which output holds a fullscreen window by its surface.
    pub fn find_fullscreen_output_for_surface(
        &self,
        wl_surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> Option<smithay::output::Output> {
        self.fullscreen.iter()
            .find(|(_, fs)| fs.window.toplevel().unwrap().wl_surface() == wl_surface)
            .map(|(o, _)| o.clone())
    }

    /// Exit fullscreen and remap the pointer to maintain its screen position
    /// under the restored camera/zoom. Returns the new canvas position.
    pub fn exit_fullscreen_remap_pointer(
        &mut self,
        canvas_pos: Point<f64, Logical>,
    ) -> Point<f64, Logical> {
        let old_camera = self.camera();
        let old_zoom = self.zoom();
        self.exit_fullscreen();
        let screen: Point<f64, Logical> = Point::from((
            (canvas_pos.x - old_camera.x) * old_zoom,
            (canvas_pos.y - old_camera.y) * old_zoom,
        ));
        let cur_zoom = self.zoom();
        let cur_camera = self.camera();
        let new_pos = Point::from((
            screen.x / cur_zoom + cur_camera.x,
            screen.y / cur_zoom + cur_camera.y,
        ));
        self.warp_pointer(new_pos);
        new_pos
    }
}
