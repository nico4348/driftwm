# driftwm

A trackpad-first infinite canvas Wayland compositor.

Windows float on an unbounded 2D plane. You pan, zoom, and navigate with
trackpad gestures. No workspaces, no tiling — just drift.

Inspired by [vxwm](https://codeberg.org/wh1tepearl/vxwm). Built on [smithay](https://github.com/Smithay/smithay).

## How it works

The screen is a viewport onto an infinite 2D plane. Each window has absolute
coordinates on this plane. You move around with trackpad gestures:

- **2-finger scroll** on empty desktop — pan the canvas
- **3-finger scroll** anywhere — pan the canvas (ignores windows)
- **3-finger hold+drag** on a window — move that window
- **2-finger pinch** — zoom in/out (bird's eye view)
- **4-finger scroll** — jump to the nearest window in that direction
- **4/5-finger pinch** — toggle home position

Mouse equivalents use `Super` + click/drag for everything.

The background is part of the canvas — it scrolls and scales with the viewport,
giving you spatial grounding as you navigate. Default is a GLSL dot-grid shader;
you can swap in any custom shader (noise, gradients, animated patterns) or a
seamless tiled image.

See [docs/DESIGN.md](docs/DESIGN.md) for the full specification.

## Status

Early development. Current milestone: **5 — Window navigation**.

The compositor runs nested via winit backend, renders xdg-shell clients on an
infinite 2D canvas with viewport panning (scroll, click-drag, keyboard), scroll
momentum with friction decay, GLSL shader backgrounds, tiled image backgrounds,
edge auto-pan during window drag, and compositor-rendered xcursor. 18 Wayland
protocols implemented including DMA-BUF, popups, and cross-app clipboard.

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

| Shortcut               | Action                          |
|------------------------|---------------------------------|
| `Alt+Return`           | Open terminal                   |
| `Alt+Q`                | Close window                    |
| `Alt+Shift+Left-click` | Drag to move window             |
| `Alt+Shift+Right-click`| Drag to resize window           |
| `Alt+Left-click`       | Drag to pan canvas              |
| `Alt+Shift+Arrow`      | Nudge focused window by 20px    |
| `Alt+Ctrl+Arrow`       | Pan viewport by step            |
| Scroll on empty canvas | Pan viewport (with momentum)    |
| Click on empty canvas  | Drag to pan viewport            |

CSD-initiated move/resize (title bar drag, border drag) also works.

More keybinds planned — see [docs/DESIGN.md](docs/DESIGN.md#keyboard-shortcuts).

## Configuration

Config file: `~/.config/driftwm/config.toml` (respects `XDG_CONFIG_HOME`).
Not yet implemented — coming in a later milestone.

## Architecture

```
src/
├── main.rs          # entry point, event loop, wayland socket
├── state.rs         # DriftWm struct, protocol state
├── config.rs        # keybindings, actions, config
├── winit.rs         # winit backend + render loop
├── input.rs         # keyboard/pointer handling, Alt+click move/resize
├── grabs/
│   ├── mod.rs       # grab module re-exports
│   ├── move_grab.rs # interactive window move (PointerGrab)
│   ├── resize_grab.rs # interactive window resize (PointerGrab)
│   └── pan_grab.rs  # viewport pan via click-drag (PointerGrab)
└── handlers/
    ├── mod.rs       # seat, data device, output delegates
    ├── compositor.rs # compositor + SHM handlers, resize commit logic
    └── xdg_shell.rs  # xdg-shell (window management, CSD move/resize)
```

## Milestones

1. **Window appears** — winit backend, xdg-shell, terminal renders *(done)*
2. **Move and resize** — drag/resize windows, CSD support *(done)*
3. **Infinite canvas** — viewport panning, scroll momentum, xcursor rendering *(done)*
4. **Canvas background** — GLSL shaders, tiled images, edge auto-pan *(done)*
5. Window navigation — Super+C center, Super+Arrow jump, Alt-Tab cycle
6. Zoom — GPU-scaled rendering, pinch to zoom
7. Decorations — SSD fallback, resize grab zones
8. Layer shell — waybar, fuzzel, notifications
9. Config file — TOML parsing, user keybindings
10. udev backend — DRM/KMS, libinput, session management
11. Trackpad gestures — 3-finger pan/hold-to-move, gesture state machine
12. Multi-monitor — multiple viewports on same canvas
13. XWayland — X11 app support
14. Widgets + polish — eww preset, animations, shadows, damage optimization

## License

GPL-3.0-or-later
