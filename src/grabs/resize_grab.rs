use std::cell::RefCell;

use smithay::{
    desktop::Window,
    input::{
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
            GrabStartData, MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
        },
        SeatHandler,
    },
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Point, Size},
    wayland::compositor::with_states,
};

use crate::state::DriftWm;

/// Tracks the resize lifecycle for a window. Stored in the surface data map
/// (wrapped in `RefCell`) so that `compositor::commit()` can reposition
/// top/left-edge resizes.
#[derive(Default, Clone, Copy)]
pub enum ResizeState {
    #[default]
    Idle,
    Resizing {
        edges: xdg_toplevel::ResizeEdge,
        initial_window_location: Point<i32, Logical>,
        initial_window_size: Size<i32, Logical>,
    },
    WaitingForLastCommit {
        edges: xdg_toplevel::ResizeEdge,
        initial_window_location: Point<i32, Logical>,
        initial_window_size: Size<i32, Logical>,
    },
}

pub struct ResizeSurfaceGrab {
    pub start_data: GrabStartData<DriftWm>,
    pub window: Window,
    pub edges: xdg_toplevel::ResizeEdge,
    pub initial_window_location: Point<i32, Logical>,
    pub initial_window_size: Size<i32, Logical>,
    pub last_window_size: Size<i32, Logical>,
}

/// Check if `edges` includes a horizontal/vertical component via raw bit values.
/// ResizeEdge values: Top=1, Bottom=2, Left=4, Right=8, combinations are ORed.
pub fn has_top(edges: xdg_toplevel::ResizeEdge) -> bool {
    edges as u32 & 1 != 0
}
pub fn has_bottom(edges: xdg_toplevel::ResizeEdge) -> bool {
    edges as u32 & 2 != 0
}
pub fn has_left(edges: xdg_toplevel::ResizeEdge) -> bool {
    edges as u32 & 4 != 0
}
pub fn has_right(edges: xdg_toplevel::ResizeEdge) -> bool {
    edges as u32 & 8 != 0
}

impl PointerGrab<DriftWm> for ResizeSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        _focus: Option<(<DriftWm as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        let delta = event.location - self.start_data.location;

        let mut new_w = self.initial_window_size.w;
        let mut new_h = self.initial_window_size.h;

        if has_left(self.edges) {
            new_w -= delta.x as i32;
        } else if has_right(self.edges) {
            new_w += delta.x as i32;
        }
        if has_top(self.edges) {
            new_h -= delta.y as i32;
        } else if has_bottom(self.edges) {
            new_h += delta.y as i32;
        }

        // Clamp to minimum 1×1
        new_w = new_w.max(1);
        new_h = new_h.max(1);

        let new_size = Size::from((new_w, new_h));

        // Only send configure when size actually changed
        if new_size != self.last_window_size {
            self.last_window_size = new_size;

            let toplevel = self.window.toplevel().unwrap();
            toplevel.with_pending_state(|state| {
                state.size = Some(new_size);
                state.states.set(xdg_toplevel::State::Resizing);
            });
            toplevel.send_pending_configure();
        }

        handle.motion(data, None, event);
    }

    fn relative_motion(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        focus: Option<(<DriftWm as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            // Grab released — transition to WaitingForLastCommit
            let toplevel = self.window.toplevel().unwrap();
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
            });
            toplevel.send_pending_configure();

            // Store WaitingForLastCommit so commit() can do final repositioning
            let surface = toplevel.wl_surface().clone();
            let edges = self.edges;
            let initial_window_location = self.initial_window_location;
            let initial_window_size = self.initial_window_size;
            with_states(&surface, |states| {
                states
                    .data_map
                    .get_or_insert(|| RefCell::new(ResizeState::Idle))
                    .replace(ResizeState::WaitingForLastCommit {
                        edges,
                        initial_window_location,
                        initial_window_size,
                    });
            });

            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut DriftWm, handle: &mut PointerInnerHandle<'_, DriftWm>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<DriftWm> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut DriftWm) {}
}
