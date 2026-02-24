# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

driftwm — a trackpad-first infinite canvas Wayland compositor written in Rust. Windows float on an unbounded 2D plane navigated via trackpad gestures (pan, zoom, pinch). No workspaces, no tiling. Built on [smithay](https://github.com/Smithay/smithay).

The project is in early development (milestone 4 complete). See `docs/DESIGN.md` for the full specification and `docs/CAVEATS.md` for architectural pitfalls.

## Conventions

- Documentation files (except README.md) live in `docs/`.
- Config path: `~/.config/driftwm/config.toml` (respects `XDG_CONFIG_HOME`).

## Build & Run

```bash
cargo build              # build
cargo run                # run nested in existing Wayland session (winit backend)
cargo run -- --backend udev   # run on real hardware (from TTY)
cargo test               # run tests
cargo test test_name     # run a single test
cargo clippy             # lint
```

Use `RUST_LOG=debug cargo run` for smithay/libinput event traces.

## Architecture

The compositor uses a **camera/viewport** model: the screen is a viewport onto an infinite 2D plane. Each window has absolute `(x, y)` canvas coordinates. The viewport has a camera `(cx, cy)` and zoom `z`. Screen position = `(wx - cx) * z`. Multiple monitors = multiple independent viewports on the same canvas.

Current source layout:

- `state.rs` — DriftWm struct, CalloopData, ClientState
- `config.rs` — keybindings, actions (SpawnCommand, CloseWindow, NudgeWindow, PanViewport), config
- `winit.rs` — winit backend init + render loop (~60fps timer), cursor element rendering
- `input.rs` — keyboard/pointer handling, camera-offset pointer coords, scroll panning with momentum, surface_under() hit-testing
- `grabs/` — `move_grab.rs` (MoveSurfaceGrab), `resize_grab.rs` (ResizeSurfaceGrab, ResizeState), `pan_grab.rs` (PanGrab for viewport panning)
- `handlers/` — `compositor.rs` (commit, resize repositioning), `xdg_shell.rs` (CSD move/resize, window centering), `mod.rs` (seat, data device, output, cursor_shape delegates)

Planned additions (from DESIGN.md):

- `canvas.rs` — viewport math, coordinate transforms, zoom
- `input/` — gesture state machine, mouse fallbacks (currently flat `input.rs`)
- `window/` — decorations, z-order/stacking
- `shell/` — layer shell, xwayland
- `output.rs` — multi-monitor / viewport management
- `render.rs` — frame rendering, damage tracking, zoom scaling

## Key Design Decisions

- **CSD-first**: compositor advertises only `close` and `fullscreen` capabilities (no maximize/minimize). SSD fallback for XWayland/Qt apps that need it.
- **Gesture-driven**: 2-finger pan/pinch for viewport, 3-finger for window manipulation, 4-finger for navigation. Mouse equivalents use Super+click modifiers.
- **Canvas background**: scrolls with viewport (not fixed to screen). Default is a GLSL dot-grid shader; static shaders are cached and only re-render on viewport changes.
- **Widgets**: eww windows as regular `xdg-toplevel` surfaces placed near `(0, 0)`, matched by window rules (`app_id = "eww-*"`).
- **External tools**: launcher, lock screen, screenshots are external programs (bemenu-run, swaylock, grim) — not built into the compositor.

## Smithay API Reference

When you discover smithay API signatures by reading source in `~/.cargo/registry/src/`, document them in `docs/smithay-api.md` so you don't need to re-read the source next time. Include trait signatures, key type definitions, and how pieces fit together.

## Rust Edition

Uses Rust edition **2024** — be aware of edition-specific language features and defaults.
