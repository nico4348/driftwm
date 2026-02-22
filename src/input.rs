use smithay::{
    backend::input::{
        AbsolutePositionEvent, ButtonState, Event, InputBackend, InputEvent, KeyState,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::{keysyms, FilterResult, Keysym},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Point, SERIAL_COUNTER},
};

use crate::state::DriftWm;

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

        let keyboard = self.seat.get_keyboard().unwrap();

        let action = keyboard.input(
            self,
            keycode,
            key_state,
            serial,
            time,
            |_state, modifiers, handle| {
                let sym = handle.modified_sym();

                if key_state == KeyState::Pressed && modifiers.logo {
                    if sym == Keysym::from(keysyms::KEY_Return) {
                        return FilterResult::Intercept(KeyAction::SpawnTerminal);
                    }
                    if sym == Keysym::from(keysyms::KEY_q) {
                        return FilterResult::Intercept(KeyAction::CloseWindow);
                    }
                }

                FilterResult::Forward
            },
        );

        // Handle intercepted compositor shortcuts
        if let Some(action) = action {
            match action {
                KeyAction::SpawnTerminal => {
                    tracing::info!("Spawning terminal: {}", self.terminal_cmd);
                    if let Err(e) = std::process::Command::new(&self.terminal_cmd).spawn() {
                        tracing::error!("Failed to spawn terminal: {e}");
                    }
                }
                KeyAction::CloseWindow => {
                    // Close the focused window (topmost in stacking order)
                    let keyboard = self.seat.get_keyboard().unwrap();
                    if let Some(focus) = keyboard.current_focus() {
                        let window = self
                            .space
                            .elements()
                            .find(|w| w.toplevel().unwrap().wl_surface() == &focus)
                            .cloned();
                        if let Some(window) = window {
                            window.toplevel().unwrap().send_close();
                        }
                    }
                }
            }
        }
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
        let pos = event.position_transformed(output_geo.size);

        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        let under = self.surface_under(pos);

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial,
                time: Event::time_msec(&event),
            },
        );
        pointer.frame(self);
    }

    fn on_pointer_button<I: InputBackend>(&mut self, event: I::PointerButtonEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let button = event.button_code();
        let button_state = event.state();
        let pointer = self.seat.get_pointer().unwrap();

        // Click-to-focus + raise
        if button_state == ButtonState::Pressed {
            let pos = pointer.current_location();
            if let Some((window, _)) = self
                .space
                .element_under(pos)
                .map(|(w, p)| (w.clone(), p))
            {
                self.space.raise_element(&window, true);
                let keyboard = self.seat.get_keyboard().unwrap();
                keyboard.set_focus(
                    self,
                    Some(window.toplevel().unwrap().wl_surface().clone()),
                    serial,
                );
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

    fn on_pointer_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let pointer = self.seat.get_pointer().unwrap();

        let mut frame = AxisFrame::new(Event::time_msec(&event));

        let horizontal_amount = event.amount(smithay::backend::input::Axis::Horizontal);
        let vertical_amount = event.amount(smithay::backend::input::Axis::Vertical);

        if let Some(h) = horizontal_amount {
            frame = frame.value(smithay::backend::input::Axis::Horizontal, h);
        }
        if let Some(v) = vertical_amount {
            frame = frame.value(smithay::backend::input::Axis::Vertical, v);
        }

        let horizontal_discrete = event.amount_v120(smithay::backend::input::Axis::Horizontal);
        let vertical_discrete = event.amount_v120(smithay::backend::input::Axis::Vertical);

        if let Some(h) = horizontal_discrete {
            frame = frame.v120(smithay::backend::input::Axis::Horizontal, h as i32);
        }
        if let Some(v) = vertical_discrete {
            frame = frame.v120(smithay::backend::input::Axis::Vertical, v as i32);
        }

        pointer.axis(self, frame);
        pointer.frame(self);
    }

    /// Find the Wayland surface and local coordinates under the given canvas position.
    /// This is the foundation for all hit-testing — focus, gestures, resize grabs.
    pub fn surface_under(
        &self,
        pos: Point<f64, smithay::utils::Logical>,
    ) -> Option<(WlSurface, Point<f64, smithay::utils::Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, window_loc)| {
                window
                    .surface_under(
                        pos - window_loc.to_f64(),
                        smithay::desktop::WindowSurfaceType::ALL,
                    )
                    .map(|(surface, surface_loc)| {
                        (surface, (surface_loc + window_loc).to_f64())
                    })
            })
    }
}

/// Actions that keyboard shortcuts can trigger.
enum KeyAction {
    SpawnTerminal,
    CloseWindow,
}
