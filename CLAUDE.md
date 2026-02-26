# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

driftwm — a trackpad-first infinite canvas Wayland compositor written in Rust. Windows float on an unbounded 2D plane navigated via trackpad gestures (pan, zoom, pinch). No workspaces, no tiling. Built on [smithay](https://github.com/Smithay/smithay).

The project is in early development (milestone 7 complete). See `docs/DESIGN.md` for the full specification and `docs/CAVEATS.md` for architectural pitfalls.

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

- `state.rs` — DriftWm struct, CalloopData, FullscreenState, ClientState, camera/zoom animation, focus history, key repeat
- `config.rs` — keybindings, actions (SpawnCommand, CloseWindow, NudgeWindow, PanViewport, CenterWindow, CenterNearest, CycleWindows, HomeToggle, Zoom*, ToggleFullscreen), config
- `canvas.rs` — coordinate transforms (ScreenPos/CanvasPos), camera math, cone search, zoom helpers (zoom_to_fit, zoom_anchor_camera, snap_zoom, dynamic_min_zoom)
- `focus.rs` — FocusTarget(WlSurface) newtype with KeyboardTarget/PointerTarget/TouchTarget impls
- `winit.rs` — winit backend init + render loop (~60fps timer), RescaleRenderElement zoom pipeline, cursor/shader/tile rendering
- `input.rs` — keyboard/pointer handling, camera-offset pointer coords, scroll panning with momentum, zoom input, fullscreen guards, surface_under() hit-testing
- `grabs/` — `move_grab.rs` (MoveSurfaceGrab), `resize_grab.rs` (ResizeSurfaceGrab, ResizeState), `pan_grab.rs` (PanGrab for viewport panning)
- `handlers/` — `compositor.rs` (commit, resize repositioning, dmabuf, layer commit), `layer_shell.rs` (wlr-layer-shell handler), `xdg_shell.rs` (CSD move/resize, window centering, fullscreen, popup grabs), `mod.rs` (seat, data device, output, cursor_shape, foreign toplevel, 20 protocol delegates)
- `protocols/` — `foreign_toplevel.rs` (zwlr-foreign-toplevel-management-v1, adapted from niri)

Planned additions (from DESIGN.md):

- `input/` — gesture state machine, mouse fallbacks (currently flat `input.rs`)
- `window/` — decorations, z-order/stacking
- `output.rs` — multi-monitor / viewport management

## Key Design Decisions

- **CSD-first**: compositor advertises only `close` and `fullscreen` capabilities (no maximize/minimize). SSD fallback for XWayland/Qt apps that need it.
- **Gesture-driven**: 2-finger pan/pinch for viewport, 3-finger for window manipulation, 4-finger for navigation. Mouse equivalents use Super+click modifiers.
- **Canvas background**: scrolls with viewport (not fixed to screen). Default is a GLSL dot-grid shader; static shaders are cached and only re-render on viewport changes.
- **Widgets**: eww windows as regular `xdg-toplevel` surfaces placed near `(0, 0)`, matched by window rules (`app_id = "eww-*"`).
- **External tools**: launcher, lock screen, screenshots are external programs (bemenu-run, swaylock, grim) — not built into the compositor.

## Reference Codebases

- **[niri](https://github.com/niri-wm/niri)** — a scrollable tiling Wayland compositor also built on smithay. When stuck or unsure how to implement a smithay feature (layer shell, xwayland, udev backend, etc.), explore niri's codebase for a working reference. Local clone at `/tmp/niri` (if missing: `git clone --depth 1 https://github.com/niri-wm/niri.git /tmp/niri`).

## Smithay API Reference

When you discover smithay API signatures by reading source in `~/.cargo/registry/src/`, document them in `docs/smithay-api.md` so you don't need to re-read the source next time. Include trait signatures, key type definitions, and how pieces fit together.

## Rust Edition

Uses Rust edition **2024** — be aware of edition-specific language features and defaults.
