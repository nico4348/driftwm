use crate::state::{CalloopData, DriftWm, FocusTarget};
use driftwm::window_ext::WindowExt;
use smithay::{
    delegate_xwayland_shell,
    desktop::Window,
    input::pointer::{CursorImageStatus, Focus, GrabStartData},
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{Resource, protocol::wl_surface::WlSurface},
    },
    utils::{Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        compositor::with_states,
        selection::SelectionTarget,
        seat::WaylandFocus,
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        xwm::{Reorder, ResizeEdge, X11Wm, XwmHandler, XwmId},
        X11Surface,
    },
};

use super::xdg_shell::resize_cursor;

fn x11_edge_to_xdg(edge: ResizeEdge) -> xdg_toplevel::ResizeEdge {
    match edge {
        ResizeEdge::Top => xdg_toplevel::ResizeEdge::Top,
        ResizeEdge::Bottom => xdg_toplevel::ResizeEdge::Bottom,
        ResizeEdge::Left => xdg_toplevel::ResizeEdge::Left,
        ResizeEdge::Right => xdg_toplevel::ResizeEdge::Right,
        ResizeEdge::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
        ResizeEdge::TopRight => xdg_toplevel::ResizeEdge::TopRight,
        ResizeEdge::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
    }
}

// ---- Calloop wrappers (X11Wm::start_wm requires D = CalloopData) ----

impl XwmHandler for CalloopData {
    fn xwm_state(&mut self, xwm: XwmId) -> &mut X11Wm {
        XwmHandler::xwm_state(&mut self.state, xwm)
    }
    fn new_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::new_window(&mut self.state, xwm, window);
    }
    fn new_override_redirect_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::new_override_redirect_window(&mut self.state, xwm, window);
    }
    fn map_window_request(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::map_window_request(&mut self.state, xwm, window);
    }
    fn mapped_override_redirect_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::mapped_override_redirect_window(&mut self.state, xwm, window);
    }
    fn unmapped_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::unmapped_window(&mut self.state, xwm, window);
    }
    fn destroyed_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::destroyed_window(&mut self.state, xwm, window);
    }
    fn configure_request(
        &mut self, xwm: XwmId, window: X11Surface,
        x: Option<i32>, y: Option<i32>, w: Option<u32>, h: Option<u32>,
        reorder: Option<Reorder>,
    ) {
        XwmHandler::configure_request(&mut self.state, xwm, window, x, y, w, h, reorder);
    }
    fn configure_notify(
        &mut self, xwm: XwmId, window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        above: Option<smithay::xwayland::xwm::X11Window>,
    ) {
        XwmHandler::configure_notify(&mut self.state, xwm, window, geometry, above);
    }
    fn resize_request(&mut self, xwm: XwmId, window: X11Surface, button: u32, edge: ResizeEdge) {
        XwmHandler::resize_request(&mut self.state, xwm, window, button, edge);
    }
    fn move_request(&mut self, xwm: XwmId, window: X11Surface, button: u32) {
        XwmHandler::move_request(&mut self.state, xwm, window, button);
    }
    fn allow_selection_access(&mut self, xwm: XwmId, sel: SelectionTarget) -> bool {
        XwmHandler::allow_selection_access(&mut self.state, xwm, sel)
    }
    fn send_selection(&mut self, xwm: XwmId, sel: SelectionTarget, mime: String, fd: std::os::fd::OwnedFd) {
        XwmHandler::send_selection(&mut self.state, xwm, sel, mime, fd);
    }
    fn new_selection(&mut self, xwm: XwmId, sel: SelectionTarget, mimes: Vec<String>) {
        XwmHandler::new_selection(&mut self.state, xwm, sel, mimes);
    }
    fn cleared_selection(&mut self, xwm: XwmId, sel: SelectionTarget) {
        XwmHandler::cleared_selection(&mut self.state, xwm, sel);
    }
}

impl XWaylandShellHandler for CalloopData {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        XWaylandShellHandler::xwayland_shell_state(&mut self.state)
    }
    fn surface_associated(&mut self, xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        XWaylandShellHandler::surface_associated(&mut self.state, xwm, wl_surface, surface);
    }
}

// ---- Primary impls on DriftWm (Wayland dispatch uses DriftWm as state type) ----

impl XwmHandler for DriftWm {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.x11_wm.as_mut().expect("X11Wm not started")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        tracing::info!("X11 map request: {:?}", window.class());
        if let Err(err) = window.set_mapped(true) {
            tracing::warn!("Failed to set X11 window mapped: {err}");
            return;
        }

        let smithay_window = Window::new_x11_window(window.clone());

        // X11 size is known upfront — center accounting for window size.
        // Check window rules for explicit positioning.
        let geo = window.geometry();
        let class = window.class();
        let rule = self.config.match_window_rule(&class).cloned();

        let pos = if let Some(ref rule) = rule
            && let Some((x, y)) = rule.position
        {
            // Rule coords: window-center, Y-up. Convert to canvas: top-left, Y-down.
            (x - geo.size.w / 2, -y - geo.size.h / 2)
        } else {
            self.active_output()
                .and_then(|o| self.space.output_geometry(&o))
                .map(|viewport| {
                    let cam = self.camera();
                    let z = self.zoom();
                    (
                        (cam.x + viewport.size.w as f64 / (2.0 * z)) as i32 - geo.size.w / 2,
                        (cam.y + viewport.size.h as f64 / (2.0 * z)) as i32 - geo.size.h / 2,
                    )
                })
                .unwrap_or((0, 0))
        };

        window
            .configure(Rectangle::from_size(geo.size))
            .ok();

        let activate = rule.as_ref().is_none_or(|r| !r.widget);
        self.space.map_element(smithay_window.clone(), pos, activate);
        self.space.raise_element(&smithay_window, true);
        self.enforce_below_windows();
        // Focus, decorations, and applied_rule storage are deferred to
        // surface_associated(), which fires once the wl_surface is paired.
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        tracing::debug!("X11 override-redirect mapped: {:?}", window.class());
        self.x11_override_redirect.push(window);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        tracing::info!("X11 unmapped: {:?}", window.class());
        self.x11_override_redirect.retain(|w| w != &window);

        if let Some(smithay_window) = self.find_x11_window(&window) {
            if let Some(wl_surface) = smithay_window.wl_surface() {
                let keyboard = self.seat.get_keyboard().unwrap();
                if keyboard.current_focus().is_some_and(|f| f.0 == *wl_surface) {
                    keyboard.set_focus(
                        self,
                        None::<FocusTarget>,
                        SERIAL_COUNTER.next_serial(),
                    );
                }
                self.decorations.remove(&wl_surface.id());
                self.pending_ssd.remove(&wl_surface.id());
                self.pending_center.remove(&*wl_surface);
            }

            let fs_output = self
                .fullscreen
                .iter()
                .find(|(_, fs)| fs.window == smithay_window)
                .map(|(o, _)| o.clone());
            if let Some(output) = fs_output {
                let fs = self.fullscreen.remove(&output).unwrap();
                crate::state::output_state(&output).camera = fs.saved_camera;
                crate::state::output_state(&output).zoom = fs.saved_zoom;
                self.update_output_from_camera();
            }

            self.focus_history.retain(|w| w != &smithay_window);
            self.space.unmap_elem(&smithay_window);
        }
    }

    fn destroyed_window(&mut self, xwm: XwmId, window: X11Surface) {
        self.unmapped_window(xwm, window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let old_geo = window.geometry();
        let mut new_geo = old_geo;
        if let Some(w) = w {
            new_geo.size.w = w as i32;
        }
        if let Some(h) = h {
            new_geo.size.h = h as i32;
        }
        // Honor position + size from the request so X11 CSD resize works
        // from all edges. The X11 loc is internal to XWayland; we apply the
        // delta to the compositor Space position.
        if let Some(x) = x {
            new_geo.loc.x = x;
        }
        if let Some(y) = y {
            new_geo.loc.y = y;
        }
        window.configure(new_geo).ok();

        // Apply X11 position delta to Space element location so left/top
        // edge CSD resizes visually move the correct edge.
        let dx = new_geo.loc.x - old_geo.loc.x;
        let dy = new_geo.loc.y - old_geo.loc.y;
        if (dx != 0 || dy != 0)
            && let Some(smithay_window) = self.find_x11_window(&window)
            && let Some(loc) = self.space.element_location(&smithay_window)
        {
            let new_loc = loc + smithay::utils::Point::from((dx, dy));
            self.space.map_element(smithay_window, new_loc, false);
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<smithay::xwayland::xwm::X11Window>,
    ) {
    }

    fn resize_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32, edge: ResizeEdge) {
        let Some(smithay_window) = self.find_x11_window(&window) else { return };
        let Some(wl_surface) = smithay_window.wl_surface().map(|s| s.into_owned()) else { return };

        let pointer = self.seat.get_pointer().unwrap();
        let start_data = GrabStartData {
            focus: Some((FocusTarget(wl_surface.clone()), pointer.current_location())),
            button: 0x110, // BTN_LEFT
            location: pointer.current_location(),
        };

        let xdg_edge = x11_edge_to_xdg(edge);
        let initial_window_location = self.space.element_location(&smithay_window).unwrap();
        let initial_window_size = smithay_window.geometry().size;

        // Store resize state for commit() repositioning
        with_states(&wl_surface, |states| {
            states
                .data_map
                .get_or_insert(|| std::cell::RefCell::new(crate::grabs::ResizeState::Idle))
                .replace(crate::grabs::ResizeState::Resizing {
                    edges: xdg_edge,
                    initial_window_location,
                    initial_window_size,
                });
        });

        self.grab_cursor = true;
        self.cursor_status = CursorImageStatus::Named(resize_cursor(xdg_edge));

        let output = self.active_output().unwrap();
        let serial = SERIAL_COUNTER.next_serial();
        let grab = crate::grabs::ResizeSurfaceGrab {
            start_data,
            window: smithay_window,
            edges: xdg_edge,
            initial_window_location,
            initial_window_size,
            last_window_size: initial_window_size,
            output,
            last_clamped_location: pointer.current_location(),
            last_x11_configure: None,
        };
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        let Some(smithay_window) = self.find_x11_window(&window) else { return };
        let Some(wl_surface) = smithay_window.wl_surface().map(|s| s.into_owned()) else { return };

        if driftwm::config::applied_rule(&wl_surface).is_some_and(|r| r.widget) {
            return;
        }

        let pointer = self.seat.get_pointer().unwrap();
        let start_data = GrabStartData {
            focus: Some((FocusTarget(wl_surface), pointer.current_location())),
            button: 0x110, // BTN_LEFT
            location: pointer.current_location(),
        };

        let initial_window_location = self.space.element_location(&smithay_window).unwrap();
        let grab = crate::grabs::MoveSurfaceGrab::new(
            start_data,
            smithay_window,
            initial_window_location,
            self.active_output().unwrap(),
        );
        let serial = SERIAL_COUNTER.next_serial();
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn allow_selection_access(&mut self, _xwm: XwmId, _sel: SelectionTarget) -> bool {
        true
    }

    fn send_selection(&mut self, _xwm: XwmId, sel: SelectionTarget, mime: String, fd: std::os::fd::OwnedFd) {
        if let Some(wm) = self.x11_wm.as_mut() {
            wm.send_selection(sel, mime, fd, self.loop_handle.clone()).ok();
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, sel: SelectionTarget, mimes: Vec<String>) {
        if let Some(wm) = self.x11_wm.as_mut() {
            wm.new_selection(sel, Some(mimes)).ok();
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, sel: SelectionTarget) {
        if let Some(wm) = self.x11_wm.as_mut() {
            wm.new_selection(sel, None).ok();
        }
    }
}

impl XWaylandShellHandler for DriftWm {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(&mut self, _xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        tracing::debug!("X11 surface associated: {:?}", surface.class());

        let Some(smithay_window) = self.find_x11_window(&surface) else {
            return;
        };

        // Apply window rules — store in wl_surface data_map (now available)
        let class = surface.class();
        let rule = self.config.match_window_rule(&class).cloned();
        if let Some(ref rule) = rule {
            let applied = driftwm::config::AppliedWindowRule {
                widget: rule.widget,
                no_focus: rule.no_focus,
                decoration: rule.decoration.clone(),
            };
            with_states(&wl_surface, |states| {
                states.data_map.insert_if_missing_threadsafe(|| {
                    std::sync::Mutex::new(applied.clone())
                });
                *states.data_map.get::<std::sync::Mutex<driftwm::config::AppliedWindowRule>>()
                    .unwrap().lock().unwrap() = applied;
            });
        }

        // SSD decorations: check MOTIF hints + window rule overrides
        let wants_ssd = smithay_window.wants_ssd();
        let rule_forces_ssd = rule.as_ref()
            .is_some_and(|r| r.decoration == driftwm::config::DecorationMode::Server);
        let rule_forces_none = rule.as_ref()
            .is_some_and(|r| r.decoration == driftwm::config::DecorationMode::None);

        if (wants_ssd || rule_forces_ssd) && !rule_forces_none {
            let geo = smithay_window.geometry();
            if geo.size.w > 0 && !self.decorations.contains_key(&wl_surface.id()) {
                let deco = crate::decorations::WindowDecoration::new(
                    geo.size.w, true, &self.config.decorations,
                );
                self.decorations.insert(wl_surface.id(), deco);
                self.pending_ssd.insert(wl_surface.id());
            }
        }

        // Focus — skip for widgets and no_focus windows
        let should_focus = rule.as_ref().is_none_or(|r| !r.widget && !r.no_focus);
        if should_focus {
            let serial = SERIAL_COUNTER.next_serial();
            let keyboard = self.seat.get_keyboard().unwrap();
            keyboard.set_focus(self, Some(FocusTarget(wl_surface)), serial);
        } else {
            // Widget/no_focus: refocus previous window if this stole focus
            self.focus_history.retain(|w| w != &smithay_window);
            if let Some(prev) = self.focus_history.first().cloned() {
                let serial = SERIAL_COUNTER.next_serial();
                let keyboard = self.seat.get_keyboard().unwrap();
                let focus = prev.wl_surface().map(|s| FocusTarget(s.into_owned()));
                keyboard.set_focus(self, focus, serial);
            }
        }
    }
}

delegate_xwayland_shell!(DriftWm);
