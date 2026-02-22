# driftwm

A trackpad-first infinite canvas Wayland compositor.

Windows float on an unbounded 2D plane. You pan, zoom, and navigate with
trackpad gestures. No workspaces, no tiling — just drift.

Built on [smithay](https://github.com/Smithay/smithay).

## How it works

The screen is a viewport onto an infinite 2D plane. Each window has absolute
coordinates on this plane. You move around with trackpad gestures:

- **2-finger pan** on empty desktop — scroll the canvas
- **2-finger pinch** — zoom in/out (bird's eye view)
- **3-finger pan** on a window — move that window
- **4-finger pan** — jump to the nearest window in that direction
- **4/5-finger pinch** — toggle home position

Mouse equivalents use `Super` + click/drag for everything.

The background is part of the canvas — it scrolls and scales with the viewport,
giving you spatial grounding as you navigate. Default is a GLSL dot-grid shader;
you can swap in any custom shader (noise, gradients, animated patterns) or a
seamless tiled image.

See [docs/DESIGN.md](docs/DESIGN.md) for the full specification.

## Status

Early development. Current milestone: **1 — Window appears** (complete).

The compositor opens a window via the winit backend, renders a dark background,
accepts xdg-shell clients, and handles keyboard/pointer input. You can run
terminals and GUI apps inside it.

## Build & run

Requires Rust (edition 2024).

```bash
cargo build
cargo run                         # run nested in existing Wayland session
RUST_LOG=debug cargo run          # with debug logging
```

In another terminal, launch apps inside driftwm:

```bash
WAYLAND_DISPLAY=wayland-1 foot       # or alacritty, ptyxis, etc.
```

The socket name is printed at startup — use that if it differs from `wayland-1`.

## Keybinds

| Shortcut       | Action         |
|----------------|----------------|
| `Super+Return` | Open terminal  |
| `Super+Q`      | Close window   |

More keybinds planned — see [docs/DESIGN.md](docs/DESIGN.md#keyboard-shortcuts).

## Configuration

Config file: `~/.config/driftwm/config.toml` (respects `XDG_CONFIG_HOME`).
Not yet implemented — coming in a later milestone.

## Architecture

```
src/
├── main.rs          # entry point, event loop, wayland socket
├── state.rs         # DriftWm struct, protocol state
├── winit.rs         # winit backend + render loop
├── input.rs         # keyboard/pointer handling
└── handlers/
    ├── mod.rs       # seat, data device, output delegates
    ├── compositor.rs # compositor + SHM handlers
    └── xdg_shell.rs  # xdg-shell (window management)
```

## Milestones

1. **Window appears** — winit backend, xdg-shell, terminal renders *(done)*
2. Move and resize — drag/resize windows, stacking
3. Infinite canvas — viewport panning
4. Canvas background — shader dot-grid
5. Trackpad gestures — libinput gesture events
6. Zoom — GPU-scaled rendering, pinch to zoom
7. Decorations — SSD fallback, resize grabs
8. Default widgets — eww preset
9. Multi-monitor — multiple viewports
10. Layer shell — waybar, fuzzel, notifications
11. XWayland — X11 app support
12. Polish — animations, shadows, config file

## License

MIT
