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
    utils::{Logical, Point},
};

use crate::state::DriftWm;

pub struct MoveSurfaceGrab {
    pub start_data: GrabStartData<DriftWm>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}

impl PointerGrab<DriftWm> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut DriftWm,
        handle: &mut PointerInnerHandle<'_, DriftWm>,
        _focus: Option<(<DriftWm as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // Move window by the delta from grab start
        let delta = event.location - self.start_data.location;
        let new_loc = self.initial_window_location + Point::from((delta.x as i32, delta.y as i32));
        data.space.map_element(self.window.clone(), new_loc, false);

        // Forward motion with no focus — cursor stays free, window moves under it
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
