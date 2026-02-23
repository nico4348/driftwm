# Caveats

Things to keep in mind as the codebase grows.

## Never block the event loop

calloop is single-threaded. A 50ms DNS lookup, a slow file read, a stuck subprocess — anything that blocks the main thread freezes the entire compositor. All I/O must be async or offloaded.

## Client misbehavior must not crash the compositor

Clients can disconnect at any time, send malformed requests, or go unresponsive. Every piece of client-derived data should be validated. Prefer `if let` over `unwrap()` for anything from a client.

## Double-buffered state

Client state changes (attach buffer, set damage, set title) are not visible until `wl_surface.commit()`. Never read uncommitted state — it may be half-updated.

## Frame callbacks are mandatory

After rendering, call `window.send_frame()` for each visible window. This tells clients "your frame was displayed, you can draw the next one." Without it, clients either stop rendering or waste CPU drawing frames that never display.

## Input device ownership is exclusive

On real hardware (udev backend), the compositor owns all input devices via libinput. No other process can read them. In nested mode (winit), the parent compositor owns input and you only see translated events — no raw gestures.

## Serials must be monotonically increasing

`SERIAL_COUNTER.next_serial()` generates unique serials for input events. Reusing or going backwards breaks client-side validation. Always generate a fresh serial per event.

## What to unit test

Smithay glue code (handlers, delegates) is not worth testing — it's framework boilerplate. Write tests for **your** logic:

- **Canvas/viewport math** (milestone 3): coordinate transforms, screen↔canvas conversion, viewport clipping. Pure functions, very testable.
- **Gesture state machine** (milestone 5): feed event sequences, assert state transitions and emitted commands.
- **Keybinding lookup** (when data-driven): binding table resolution, modifier matching, conflict detection.
- **Config parsing** (milestone 12): TOML deserialization, defaults, validation.

Manual testing is fine for everything else until you have a headless backend for integration tests.
