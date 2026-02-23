# Smithay 0.7.0 API Reference

Quick reference for key smithay APIs used in driftwm. See the source at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-0.7.0/`.

## PointerGrab System

### `PointerGrab<D>` trait
Source: `src/input/pointer/grab.rs`

13-method trait for intercepting pointer events during a grab:
```rust
trait PointerGrab<D: SeatHandler>: Send + Downcast {
    fn motion(&mut self, data: &mut D, handle: &mut PointerInnerHandle<'_, D>,
              focus: Option<(PointerFocus, Point<f64, Logical>)>, event: &MotionEvent);
    fn relative_motion(&mut self, data: &mut D, handle: &mut PointerInnerHandle<'_, D>,
                       focus: Option<(PointerFocus, Point<f64, Logical>)>, event: &RelativeMotionEvent);
    fn button(&mut self, data: &mut D, handle: &mut PointerInnerHandle<'_, D>, event: &ButtonEvent);
    fn axis(&mut self, data: &mut D, handle: &mut PointerInnerHandle<'_, D>, details: AxisFrame);
    fn frame(&mut self, data: &mut D, handle: &mut PointerInnerHandle<'_, D>);
    fn gesture_swipe_begin/update/end(...);  // 3 methods
    fn gesture_pinch_begin/update/end(...);  // 3 methods
    fn gesture_hold_begin/end(...);          // 2 methods
    fn start_data(&self) -> &GrabStartData<D>;
    fn unset(&mut self, data: &mut D);
}
```

### `GrabStartData<D>`
```rust
pub struct GrabStartData<D: SeatHandler> {
    pub focus: Option<(<D as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
    pub button: u32,
    pub location: Point<f64, Logical>,
}
```

### `PointerHandle` (external API)
```rust
impl PointerHandle<D> {
    fn set_grab(&self, data: &mut D, grab: G, serial: Serial, focus: Focus);
    fn unset_grab(&self, data: &mut D, serial: Serial, time: u32);
    fn button(&self, data: &mut D, event: &ButtonEvent);
    // button() updates pressed_buttons BEFORE calling grab.button()
    fn grab_start_data(&self) -> Option<GrabStartData<D>>;
    fn current_location(&self) -> Point<f64, Logical>;
}
```

### `PointerInnerHandle` (inside grab methods)
```rust
impl PointerInnerHandle<'_, D> {
    fn motion(&mut self, data: &mut D, focus: Option<(Focus, Point)>, event: &MotionEvent);
    fn button(&mut self, data: &mut D, event: &ButtonEvent);
    fn axis(&mut self, data: &mut D, details: AxisFrame);
    fn frame(&mut self, data: &mut D);
    fn unset_grab(&mut self, handler: &mut dyn PointerGrab<D>, data: &mut D,
                  serial: Serial, time: u32, restore_focus: bool);
    fn current_pressed(&self) -> &[u32];
    fn current_focus(&self) -> Option<(PointerFocus, Point<f64, Logical>)>;
    fn current_location(&self) -> Point<f64, Logical>;
    // + gesture forwarding methods
}
```

### `Focus` enum
```rust
pub enum Focus { Keep, Clear }
```

## Key Patterns

### DataMap (surface user data)
Source: `src/utils/user_data.rs`

```rust
// get_or_insert returns &T (immutable!) — use RefCell for mutation
states.data_map.get_or_insert(|| RefCell::new(MyState::default())).borrow()     // read
states.data_map.get_or_insert(|| RefCell::new(MyState::default())).replace(val) // write
```

### xdg_toplevel::ResizeEdge
Plain enum (NOT bitflags). Values: None=0, Top=1, Bottom=2, Left=4, Right=8,
TopLeft=5, TopRight=9, BottomLeft=6, BottomRight=10.
Use `(edge as u32) & bit` for component checks.

### ToplevelSurface resize protocol
```rust
toplevel.with_pending_state(|state| {
    state.size = Some(new_size);
    state.states.set(xdg_toplevel::State::Resizing);
});
toplevel.send_pending_configure();
```

### Keyboard modifier state
```rust
let modifiers = self.seat.get_keyboard().unwrap().modifier_state();
if modifiers.alt { ... }
```
