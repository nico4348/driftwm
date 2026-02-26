use std::cell::RefCell;

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, ButtonState, Event, InputBackend, InputEvent, KeyState,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, Focus, GrabStartData, MotionEvent},
    },
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Point, SERIAL_COUNTER},
    wayland::compositor::with_states,
};

use driftwm::canvas::{self, CanvasPos, ScreenPos, canvas_to_screen, screen_to_canvas};
use driftwm::config::Action;
use crate::grabs::{MoveSurfaceGrab, PanGrab, ResizeState, ResizeSurfaceGrab};
use crate::state::{DriftWm, FocusTarget, log_err};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

impl DriftWm {
    /// Process a single input event from any backend (winit, libinput, etc).
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard::<I>(event),
            InputEvent::PointerMotionAbsolute { event } => {
                self.on_pointer_motion_absolute::<I>(event)
            }
            InputEvent::PointerButton { event } => self.on_pointer_button::<I>(event),
            InputEvent::PointerAxis { event } => self.on_pointer_axis::<I>(event),
            _ => {}
        }
    }

    fn on_keyboard<I: InputBackend>(&mut self, event: I::KeyboardKeyEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let time = Event::time_msec(&event);
        let key_state = event.state();
        let keycode = event.key_code();
        let keycode_u32: u32 = keycode.into();

        // Clear key repeat on release of the held key
        if key_state == KeyState::Released
            && let Some((held_keycode, _, _)) = &self.held_action
            && *held_keycode == keycode_u32
        {
            self.held_action = None;
        }

        let keyboard = self.seat.get_keyboard().unwrap();

        let action = keyboard.input(
            self,
            keycode,
            key_state,
            serial,
            time,
            |state, modifiers, handle| {
                // 1. If cycling is active and the cycle modifier was released, end cycle
                if state.cycle_state.is_some()
                    && !state.config.cycle_modifier.is_pressed(modifiers)
                {
                    state.end_cycle();
                    return FilterResult::Forward;
                }

                if key_state == KeyState::Pressed {
                    let sym = handle.modified_sym();

                    // 2. Normal binding lookup (includes cycle bindings)
                    if let Some(action) = state.config.lookup(modifiers, sym) {
                        return FilterResult::Intercept(action.clone());
                    }
                }
                FilterResult::Forward
            },
        );

        if let Some(ref action) = action {
            // Set up key repeat for repeatable actions
            if action.is_repeatable() {
                let delay = std::time::Duration::from_millis(self.config.repeat_delay as u64);
                self.held_action = Some((keycode_u32, action.clone(), std::time::Instant::now() + delay));
            } else {
                // Non-repeatable action pressed — cancel any active repeat
                self.held_action = None;
            }
            self.execute_action(action);
        }
    }

    pub fn execute_action(&mut self, action: &Action) {
        self.momentum.stop();
        match action {
            Action::SpawnCommand(cmd) => {
                tracing::info!("Spawning: {cmd}");
                let mut parts = cmd.split_whitespace();
                if let Some(program) = parts.next() {
                    log_err(
                        "spawn command",
                        std::process::Command::new(program).args(parts).spawn(),
                    );
                }
            }
            Action::CloseWindow => {
                let keyboard = self.seat.get_keyboard().unwrap();
                if let Some(focus) = keyboard.current_focus() {
                    let window = self
                        .space
                        .elements()
                        .find(|w| w.toplevel().unwrap().wl_surface() == &focus.0)
                        .cloned();
                    if let Some(window) = window {
                        window.toplevel().unwrap().send_close();
                    }
                }
            }
            Action::NudgeWindow(dir) => {
                let keyboard = self.seat.get_keyboard().unwrap();
                if let Some(focus) = keyboard.current_focus() {
                    let window = self
                        .space
                        .elements()
                        .find(|w| w.toplevel().unwrap().wl_surface() == &focus.0)
                        .cloned();
                    if let Some(window) = window
                        && let Some(loc) = self.space.element_location(&window)
                    {
                        let step = self.config.nudge_step;
                        let (ux, uy) = dir.to_unit_vec();
                        let offset = (
                            (ux * step as f64).round() as i32,
                            (uy * step as f64).round() as i32,
                        );
                        let new_loc = loc + Point::from(offset);
                        self.space.map_element(window, new_loc, false);
                    }
                }
            }
            Action::PanViewport(dir) => {
                self.camera_target = None;
                self.zoom_target = None;
                self.overview_return = None;
                let step = self.config.pan_step / self.zoom;
                let (ux, uy) = dir.to_unit_vec();
                let delta: Point<f64, smithay::utils::Logical> =
                    Point::from((ux * step, uy * step));
                self.camera += delta;
                self.update_output_from_camera();

                // Shift pointer so cursor stays at the same screen position
                let pointer = self.seat.get_pointer().unwrap();
                let pos = pointer.current_location();
                let new_pos = pos + delta;
                let under = self.surface_under(new_pos);
                let serial = SERIAL_COUNTER.next_serial();
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: new_pos,
                        serial,
                        time: self.start_time.elapsed().as_millis() as u32,
                    },
                );
                pointer.frame(self);
            }
            Action::CenterWindow => {
                let keyboard = self.seat.get_keyboard().unwrap();
                if let Some(focus) = keyboard.current_focus() {
                    let window = self
                        .space
                        .elements()
                        .find(|w| w.toplevel().unwrap().wl_surface() == &focus.0)
                        .cloned();
                    if let Some(window) = window {
                        self.navigate_to_window(&window);
                    }
                }
            }
            Action::CenterNearest(dir) => {
                // Origin: focused window center, or viewport center if none
                let keyboard = self.seat.get_keyboard().unwrap();
                let focused = keyboard.current_focus().and_then(|focus| {
                    self.space
                        .elements()
                        .find(|w| w.toplevel().unwrap().wl_surface() == &focus.0)
                        .cloned()
                });

                let origin = if let Some(ref w) = focused {
                    let loc = self.space.element_location(w).unwrap_or_default();
                    let size = w.geometry().size;
                    Point::from((
                        loc.x as f64 + size.w as f64 / 2.0,
                        loc.y as f64 + size.h as f64 / 2.0,
                    ))
                } else {
                    let viewport_size = self.get_viewport_size();
                    Point::from((
                        self.camera.x + viewport_size.w as f64 / (2.0 * self.zoom),
                        self.camera.y + viewport_size.h as f64 / (2.0 * self.zoom),
                    ))
                };

                let windows = self.space.elements().map(|w| {
                    let loc = self.space.element_location(w).unwrap_or_default();
                    let size = w.geometry().size;
                    let center = Point::from((
                        loc.x as f64 + size.w as f64 / 2.0,
                        loc.y as f64 + size.h as f64 / 2.0,
                    ));
                    (w.clone(), center)
                }).collect::<Vec<_>>();

                let nearest = canvas::find_nearest(
                    origin,
                    dir,
                    windows.into_iter(),
                    focused.as_ref(),
                );
                if let Some(window) = nearest {
                    self.navigate_to_window(&window);
                }
            }
            Action::CycleWindows { backward } => {
                if self.focus_history.is_empty() {
                    return;
                }

                let len = self.focus_history.len();
                if let Some(ref mut idx) = self.cycle_state {
                    if *backward {
                        *idx = (*idx + len - 1) % len;
                    } else {
                        *idx = (*idx + 1) % len;
                    }
                } else {
                    // First Tab press: jump to previous window (index 1)
                    self.cycle_state = Some(1 % len);
                }

                let idx = self.cycle_state.unwrap();
                if let Some(window) = self.focus_history.get(idx).cloned() {
                    self.navigate_to_window(&window);
                }
            }
            Action::HomeToggle => {
                let viewport_size = self.get_viewport_size();

                // "At home" means zoom ≈ 1.0 AND origin visible. At lower zoom
                // the origin is visible from afar, but you're not really home.
                let at_home = (self.zoom - 1.0).abs() < 0.01
                    && canvas::is_origin_visible(self.camera, viewport_size, self.zoom);

                if at_home {
                    // We're at home — return to saved position if we have one
                    if let Some((target_camera, target_zoom)) = self.home_return.take() {
                        self.camera_target = Some(target_camera);
                        self.zoom_target = Some(target_zoom);
                    }
                } else {
                    // Not at home — save current position+zoom and go home at zoom=1.0
                    self.home_return = Some((self.camera, self.zoom));
                    self.overview_return = None;
                    let home = Point::from((
                        -(viewport_size.w as f64) / 2.0,
                        -(viewport_size.h as f64) / 2.0,
                    ));
                    self.camera_target = Some(home);
                    self.zoom_target = Some(1.0);
                }
            }
            Action::ZoomIn => {
                let new_zoom = (self.zoom * self.config.zoom_step).min(canvas::MAX_ZOOM);
                let new_zoom = canvas::snap_zoom(new_zoom);
                self.zoom_to_anchored(new_zoom);
            }
            Action::ZoomOut => {
                let new_zoom = (self.zoom / self.config.zoom_step).max(self.min_zoom());
                let new_zoom = canvas::snap_zoom(new_zoom);
                self.zoom_to_anchored(new_zoom);
            }
            Action::ZoomReset => {
                self.zoom_to_anchored(1.0);
            }
            Action::ZoomToFit => {
                if let Some((saved_camera, saved_zoom)) = self.overview_return.take() {
                    // Toggle back from overview
                    self.camera_target = Some(saved_camera);
                    self.zoom_target = Some(saved_zoom);
                } else {
                    // Compute bounding box of all windows
                    let viewport = self.get_viewport_size();
                    let bbox = canvas::all_windows_bbox(
                        self.space.elements().map(|w| {
                            let loc = self.space.element_location(w).unwrap_or_default();
                            let size = w.geometry().size;
                            (loc, size)
                        }),
                    );
                    if let Some(bbox) = bbox {
                        let fit_zoom = canvas::zoom_to_fit(
                            bbox, viewport, self.config.zoom_fit_padding,
                        );
                        // Center camera on bbox center
                        let bbox_cx = bbox.loc.x as f64 + bbox.size.w as f64 / 2.0;
                        let bbox_cy = bbox.loc.y as f64 + bbox.size.h as f64 / 2.0;
                        let new_camera: Point<f64, smithay::utils::Logical> = Point::from((
                            bbox_cx - viewport.w as f64 / (2.0 * fit_zoom),
                            bbox_cy - viewport.h as f64 / (2.0 * fit_zoom),
                        ));
                        self.overview_return = Some((self.camera, self.zoom));
                        self.camera_target = Some(new_camera);
                        self.zoom_target = Some(fit_zoom);
                    }
                }
            }
        }
    }

    /// Animate zoom to `target_zoom`, anchored on viewport center (for keyboard actions).
    fn zoom_to_anchored(&mut self, target_zoom: f64) {
        self.overview_return = None;
        let viewport = self.get_viewport_size();
        let vp_center_canvas = Point::from((
            self.camera.x + viewport.w as f64 / (2.0 * self.zoom),
            self.camera.y + viewport.h as f64 / (2.0 * self.zoom),
        ));
        let vp_center_screen = Point::from((
            viewport.w as f64 / 2.0,
            viewport.h as f64 / 2.0,
        ));
        let new_camera = canvas::zoom_anchor_camera(
            vp_center_canvas, vp_center_screen, target_zoom,
        );
        self.zoom_target = Some(target_zoom);
        self.camera_target = Some(new_camera);
    }

    fn on_pointer_motion_absolute<I: InputBackend>(
        &mut self,
        event: I::PointerMotionAbsoluteEvent,
    ) {
        let output = match self.space.outputs().next() {
            Some(o) => o.clone(),
            None => return,
        };
        let output_geo = self.space.output_geometry(&output).unwrap();

        // position_transformed gives screen-local coords (0..width, 0..height)
        let screen_pos = event.position_transformed(output_geo.size);
        // Convert to canvas coords: canvas = screen / zoom + camera
        let canvas_pos = screen_to_canvas(ScreenPos(screen_pos), self.camera, self.zoom).0;

        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        let under = self.surface_under(canvas_pos);

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: canvas_pos,
                serial,
                time: Event::time_msec(&event),
            },
        );
        pointer.frame(self);
    }

    /// Priority order when button pressed:
    /// 1. Mod+Shift + button on window → move (left) or resize (right)
    /// 2. Mod + left-drag → pan canvas (regardless of what's under cursor)
    /// 3. Left-click on empty canvas → pan canvas
    /// 4. Normal click → click-to-focus + forward to client
    fn on_pointer_button<I: InputBackend>(&mut self, event: I::PointerButtonEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let button = event.button_code();
        let button_state = event.state();
        let pointer = self.seat.get_pointer().unwrap();

        if button_state == ButtonState::Pressed {
            self.last_scroll_pan = None;
            self.momentum.stop();
            let pos = pointer.current_location();
            let keyboard = self.seat.get_keyboard().unwrap();
            let mods = keyboard.modifier_state();
            let wm_mod = self.config.mod_key.is_pressed(&mods);

            // 1. Mod+Shift + button on window → move (left) or resize (right)
            if wm_mod && mods.shift {
                let element_under = self
                    .space
                    .element_under(pos)
                    .map(|(w, _)| w.clone());

                if let Some(window) = element_under {
                    if button == BTN_LEFT {
                        let initial_window_location =
                            self.space.element_location(&window).unwrap();
                        let start_data = GrabStartData {
                            focus: None,
                            button,
                            location: pos,
                        };
                        let grab = MoveSurfaceGrab {
                            start_data,
                            window,
                            initial_window_location,
                        };
                        pointer.set_grab(self, grab, serial, Focus::Clear);
                        return;
                    }

                    if button == BTN_RIGHT {
                        self.start_compositor_resize(&pointer, &window, pos, button, serial);
                        return;
                    }
                }
            }

            // 2. Mod + left-drag → pan canvas (anywhere)
            if wm_mod && button == BTN_LEFT {
                let grab = self.make_pan_grab(pos, button, false);
                pointer.set_grab(self, grab, serial, Focus::Clear);
                return;
            }

            // 3 & 4. Check what's under the pointer
            let element_under = self
                .space
                .element_under(pos)
                .map(|(w, _)| w.clone());

            if let Some(window) = element_under {
                // 4. Normal click on window: focus + raise + forward
                self.space.raise_element(&window, true);
                keyboard.set_focus(
                    self,
                    Some(FocusTarget(window.toplevel().unwrap().wl_surface().clone())),
                    serial,
                );
            } else {
                // 3. Left-click on empty canvas → pan (or click-to-unfocus)
                if button == BTN_LEFT {
                    let grab = self.make_pan_grab(pos, button, true);
                    pointer.set_grab(self, grab, serial, Focus::Clear);
                    return;
                }
            }
        }

        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time: Event::time_msec(&event),
            },
        );
        pointer.frame(self);
    }

    /// Start a compositor-side resize grab. Edges are inferred from which
    /// quadrant of the window the pointer is in.
    fn start_compositor_resize(
        &mut self,
        pointer: &smithay::input::pointer::PointerHandle<DriftWm>,
        window: &smithay::desktop::Window,
        pos: Point<f64, smithay::utils::Logical>,
        button: u32,
        serial: smithay::utils::Serial,
    ) {
        let initial_window_location = self.space.element_location(window).unwrap();
        let initial_window_size = window.geometry().size;

        // Determine edges from pointer position within a 3×3 grid on the window.
        // Corners → diagonal resize, edge strips → cardinal resize.
        let rel_x = pos.x - initial_window_location.x as f64;
        let rel_y = pos.y - initial_window_location.y as f64;
        let w = initial_window_size.w as f64;
        let h = initial_window_size.h as f64;
        let in_left = rel_x < w / 3.0;
        let in_right = rel_x > w * 2.0 / 3.0;
        let in_top = rel_y < h / 3.0;
        let in_bottom = rel_y > h * 2.0 / 3.0;
        let edges = match (in_left, in_right, in_top, in_bottom) {
            (true, _, true, _) => xdg_toplevel::ResizeEdge::TopLeft,
            (_, true, true, _) => xdg_toplevel::ResizeEdge::TopRight,
            (true, _, _, true) => xdg_toplevel::ResizeEdge::BottomLeft,
            (_, true, _, true) => xdg_toplevel::ResizeEdge::BottomRight,
            (true, _, _, _) => xdg_toplevel::ResizeEdge::Left,
            (_, true, _, _) => xdg_toplevel::ResizeEdge::Right,
            (_, _, true, _) => xdg_toplevel::ResizeEdge::Top,
            (_, _, _, true) => xdg_toplevel::ResizeEdge::Bottom,
            _ => xdg_toplevel::ResizeEdge::BottomRight, // center fallback
        };

        // Store resize state for commit() repositioning
        let wl_surface = window.toplevel().unwrap().wl_surface().clone();
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

        window.toplevel().unwrap().with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
        });

        self.grab_cursor = true;
        self.cursor_status = CursorImageStatus::Named(resize_cursor(edges));

        let start_data = GrabStartData {
            focus: None,
            button,
            location: pos,
        };
        let grab = ResizeSurfaceGrab {
            start_data,
            window: window.clone(),
            edges,
            initial_window_location,
            initial_window_size,
            last_window_size: initial_window_size,
        };
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn on_pointer_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let keyboard = self.seat.get_keyboard().unwrap();
        let mods = keyboard.modifier_state();
        let wm_mod = self.config.mod_key.is_pressed(&mods);
        let pointer = self.seat.get_pointer().unwrap();
        let pos = pointer.current_location();

        // Mod+scroll → zoom (vertical axis), cursor-anchored, immediate (no animation)
        if wm_mod {
            // Smooth scroll (trackpad) provides amount(); discrete scroll (mouse wheel)
            // provides amount_v120() where 120 = one notch. Fall back between them.
            let v = event.amount(Axis::Vertical)
                .or_else(|| event.amount_v120(Axis::Vertical).map(|v| v * 15.0 / 120.0))
                .unwrap_or(0.0);
            if v != 0.0 {
                let steps = -v * self.config.scroll_speed / 30.0;
                let factor = self.config.zoom_step.powf(steps);
                // No snap_zoom here — continuous scroll needs fine control.
                // snap_zoom's ±0.05 dead zone blocks small trackpad deltas.
                let new_zoom = (self.zoom * factor).clamp(self.min_zoom(), canvas::MAX_ZOOM);

                if new_zoom != self.zoom {
                    self.overview_return = None;
                    let screen_pos = canvas_to_screen(
                        CanvasPos(pos), self.camera, self.zoom,
                    ).0;
                    self.camera = canvas::zoom_anchor_camera(pos, screen_pos, new_zoom);
                    self.zoom = new_zoom;
                    self.zoom_target = None;
                    self.camera_target = None;
                    self.momentum.stop();
                    self.update_output_from_camera();

                    // Re-evaluate focus at the (unchanged) canvas position
                    let under = self.surface_under(pos);
                    let serial = SERIAL_COUNTER.next_serial();
                    pointer.motion(
                        self,
                        under,
                        &MotionEvent {
                            location: pos,
                            serial,
                            time: Event::time_msec(&event),
                        },
                    );
                }
            }
            let frame = AxisFrame::new(Event::time_msec(&event));
            pointer.axis(self, frame);
            pointer.frame(self);
            return;
        }

        // Pan viewport when: scroll on empty canvas, or
        // continuing a recent scroll-pan (within 150ms, so a window
        // sliding under mid-gesture doesn't steal the scroll).
        let over_window = self.space.element_under(pos).is_some();
        let recent_pan = self
            .last_scroll_pan
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(150));
        if !over_window || recent_pan {
            self.last_scroll_pan = Some(std::time::Instant::now());
            let h = event.amount(Axis::Horizontal).unwrap_or(0.0);
            let v = event.amount(Axis::Vertical).unwrap_or(0.0);
            if h != 0.0 || v != 0.0 {
                let s = self.config.scroll_speed;
                // Convert screen delta to canvas delta
                let canvas_delta: Point<f64, smithay::utils::Logical> = Point::from((
                    h * s / self.zoom,
                    v * s / self.zoom,
                ));
                self.drift_pan(canvas_delta);

                // Move pointer by canvas delta so cursor stays at the same screen position
                let new_pos = pos + canvas_delta;
                let serial = SERIAL_COUNTER.next_serial();
                let under = self.surface_under(new_pos);
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: new_pos,
                        serial,
                        time: Event::time_msec(&event),
                    },
                );
            }
            let frame = AxisFrame::new(Event::time_msec(&event));
            pointer.axis(self, frame);
            pointer.frame(self);
            return;
        }

        // Over a window without Mod: forward scroll to the client
        let mut frame = AxisFrame::new(Event::time_msec(&event))
            .source(event.source());

        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                frame = frame
                    .value(axis, amount)
                    .relative_direction(axis, event.relative_direction(axis));
            }
            if let Some(v120) = event.amount_v120(axis) {
                frame = frame.v120(axis, v120 as i32);
            }
        }

        pointer.axis(self, frame);
        pointer.frame(self);
    }

    /// Build a PanGrab for click-drag viewport panning.
    fn make_pan_grab(
        &self,
        canvas_pos: Point<f64, smithay::utils::Logical>,
        button: u32,
        from_empty_canvas: bool,
    ) -> PanGrab {
        let screen_pos = canvas_to_screen(CanvasPos(canvas_pos), self.camera, self.zoom).0;
        PanGrab {
            start_data: GrabStartData {
                focus: None,
                button,
                location: canvas_pos,
            },
            last_screen_pos: screen_pos,
            start_screen_pos: screen_pos,
            from_empty_canvas,
            dragged: false,
        }
    }

    /// Find the Wayland surface and local coordinates under the given canvas position.
    /// This is the foundation for all hit-testing — focus, gestures, resize grabs.
    pub fn surface_under(
        &self,
        pos: Point<f64, smithay::utils::Logical>,
    ) -> Option<(FocusTarget, Point<f64, smithay::utils::Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, window_loc)| {
                window
                    .surface_under(
                        pos - window_loc.to_f64(),
                        smithay::desktop::WindowSurfaceType::ALL,
                    )
                    .map(|(surface, surface_loc)| {
                        (FocusTarget(surface), (surface_loc + window_loc).to_f64())
                    })
            })
    }
}

/// Map resize edge to the appropriate directional cursor icon.
fn resize_cursor(edges: xdg_toplevel::ResizeEdge) -> CursorIcon {
    match edges {
        xdg_toplevel::ResizeEdge::Top => CursorIcon::NResize,
        xdg_toplevel::ResizeEdge::Bottom => CursorIcon::SResize,
        xdg_toplevel::ResizeEdge::Left => CursorIcon::WResize,
        xdg_toplevel::ResizeEdge::Right => CursorIcon::EResize,
        xdg_toplevel::ResizeEdge::TopLeft => CursorIcon::NwResize,
        xdg_toplevel::ResizeEdge::TopRight => CursorIcon::NeResize,
        xdg_toplevel::ResizeEdge::BottomLeft => CursorIcon::SwResize,
        xdg_toplevel::ResizeEdge::BottomRight => CursorIcon::SeResize,
        _ => CursorIcon::Default,
    }
}
