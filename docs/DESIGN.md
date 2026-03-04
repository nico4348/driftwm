# driftwm

A trackpad-first infinite canvas Wayland compositor.

Windows float on an unbounded 2D plane. You pan, zoom, and navigate with
trackpad gestures. No workspaces, no tiling — just drift.

## Tech stack

- **Language**: Rust
- **Compositor library**: [smithay](https://github.com/Smithay/smithay) — handles Wayland protocol, EGL/Vulkan rendering, input via libinput
- **Rendering**: smithay's built-in OpenGL or Vulkan backend
- **Input**: libinput (via smithay) — provides trackpad gesture events (swipe, pinch, hold)
- **Event loop**: [calloop](https://github.com/Smithay/calloop) — smithay's event loop. All async sources (libinput, wayland clients, timers for animations/edge-pan) are wired through it
- **Protocols**:

  Implemented:
  - `wl_compositor` — surface management
  - `wl_shm` — CPU shared-memory buffers
  - `xdg-shell` — core window management (toplevel, popup, popup grabs)
  - `wl_seat` — keyboard, pointer input
  - `wl_data_device` — clipboard / drag-and-drop (cross-app)
  - `wl_output` + `xdg-output` — monitor info
  - `wp_cursor_shape` — client cursor shape negotiation
  - `wp_linux_dmabuf` v3 — GPU buffer sharing (GTK4, Qt6, browsers)
  - `wp_viewporter` — surface cropping/scaling
  - `wp_fractional_scale` — HiDPI fractional scaling
  - `xdg-activation` — cross-app focus requests
  - `wp_primary_selection` — middle-click paste
  - `wlr-data-control` — wl-copy/wl-paste clipboard access
  - `wp_pointer_constraints` — pointer lock/confine
  - `wp_relative_pointer` — relative motion events
  - `keyboard-shortcuts-inhibit` — let apps grab shortcuts
  - `idle-inhibit` — prevent screen dimming
  - `wp_presentation_time` — frame timing feedback
  - `wlr-screencopy` — screenshot/screencast support (grim, OBS)
  - `xdg-decoration` — negotiate SSD vs CSD (CSD-first strategy)
  - `ext-session-lock` — screen locking (swaylock)
  - `wlr-layer-shell` — status bars, launchers, overlays (waybar, fuzzel)
  - `zwlr-foreign-toplevel-management` — taskbar window switching

  Not yet implemented:
  - `ext-image-capture-source` + `ext-image-copy-capture` — newer screenshot/screencast capture (replaces wlr-screencopy, used by xdg-desktop-portal-wlr for OBS/Firefox screen share)
  - XWayland — run X11 apps (milestone 13)

## Core concept: infinite canvas

The screen is a viewport onto an infinite 2D plane. Each window has absolute
`(x, y)` coordinates on this plane. The viewport has a camera position `(cx, cy)`
and a zoom level `z` (default 1.0).

A window at canvas coords `(wx, wy)` is rendered on screen at:

```
screen_x = (wx - cx) * z
screen_y = (wy - cy) * z
screen_w = w * z
screen_h = h * z
```

### Zoom behavior

- **Maximum zoom**: `1.0` — windows are never rendered larger than native resolution
- **Minimum zoom**: dynamic — computed so all windows fit within the viewport (zoom-to-fit)
- **Snap-to-1.0**: when pinch-zooming near 1.0, snap to exactly 1.0 (dead zone ±0.05).
  Avoids the "99% zoom" state
- **Zoom anchor**: always cursor position — the canvas point under the cursor stays
  fixed during both zoom in and zoom out (same as Google Maps / Figma)
- **Cursor size**: fixed — does not scale with zoom level

## Multi-monitor

Multiple monitors = multiple viewports on the same canvas. Each monitor has its
own `(cx, cy)` and `z`. They can look at different parts of the canvas or
overlap. Panning on one monitor moves only that monitor's viewport.

```
Monitor 0: viewport at (0, 0)       Monitor 1: viewport at (3000, 500)
┌──────────────┐                    ┌──────────────┐
│  [terminal]  │                    │   [browser]  │
│        [vim] │                    │              │
└──────────────┘                    └──────────────┘
         ← same infinite canvas →
```

## Input

All input methods — trackpad, mouse, keyboard — feed into the same actions.
Panning is the most frequent action on an infinite canvas, so there are many
ways to do it. All pan methods feed into the momentum system — a quick flick
carries the viewport smoothly until friction stops it.

### Trackpad gestures

Requires libinput (udev backend). All gesture bindings are configurable via
`[gestures.on-window]`, `[gestures.on-canvas]`, and `[gestures.anywhere]` in
config. Context resolution: specific context checked first, then anywhere as
fallback. Unbound gestures are forwarded to the focused app.

Once a gesture starts, the target is **locked for the gesture's duration** (even
if the surface under the cursor changes mid-gesture).

Default bindings:

| Gesture                      | Context   | Action                             |
| ---------------------------- | --------- | ---------------------------------- |
| 2-finger pinch               | on-canvas | Zoom in/out                        |
| 2-finger pinch               | on-window | Forward to app (unbound)           |
| 3-finger swipe               | anywhere  | Pan viewport (continuous)          |
| 3-finger doubletap-swipe     | on-window | Move window                        |
| Alt+3-finger swipe           | on-window | Resize window                      |
| 3-finger pinch               | anywhere  | Zoom in/out (continuous)           |
| Mod+3-finger swipe           | anywhere  | Center nearest window (threshold)  |
| Mod+3-finger pinch-in        | anywhere  | Zoom-to-fit                        |
| Mod+3-finger pinch-out       | anywhere  | Home toggle                        |
| Mod+3-finger hold            | anywhere  | Center focused window              |
| 4-finger swipe               | anywhere  | Center nearest window (threshold)  |
| 4-finger pinch-in            | anywhere  | Zoom-to-fit                        |
| 4-finger pinch-out           | anywhere  | Home toggle                        |
| 4-finger hold                | anywhere  | Center focused window              |

Gesture triggers are either **continuous** (per-frame dx/dy or scale updates) or
**threshold** (accumulate input, fire once). For swipe, the action determines
which: `pan-viewport` is continuous, `center-nearest` is threshold. For pinch,
the trigger determines which: `pinch` is continuous, `pinch-in`/`pinch-out` are
threshold. Per-direction swipe overrides (`swipe-up`, `swipe-down`, etc.) are
also available for mapping individual directions to discrete actions.

**3-finger doubletap-swipe**: Tap with three fingers on a window (libinput
generates BTN_MIDDLE via tap-to-click), then immediately start a 3-finger
swipe. The compositor buffers the middle click for 300ms — if a 3-finger swipe
follows, the click is suppressed and the swipe enters move-window mode. If no
swipe follows, the click is flushed to the app as a normal middle-click (paste).

**Alt+3-finger resize**: Edges inferred from pointer position in the window
(same quadrant logic as mouse). Uses Alt instead of Mod to avoid conflict with
Mod+3-finger navigation gestures.

**Mod+3-finger alternatives**: All 4-finger gestures (navigate, overview, home,
center) are also available as Mod+3-finger for smaller trackpads where 4-finger
gestures are awkward.

**Threshold swipe (center-nearest)**: Accumulates swipe delta until a 16px
threshold, detects one of 8 directions (4 cardinal + 4 diagonal using 45°
sectors), then fires the action once.

**Threshold pinch**: Pinch-in fires when scale < 0.8, pinch-out when
scale > 1.2.

**Hold**: Place fingers on the trackpad and lift without swiping or pinching.
Action fires on release.

### Mouse equivalents

Mouse bindings are context-aware via `[mouse.on-window]`, `[mouse.on-canvas]`,
and `[mouse.anywhere]`. Default bindings:

| Action           | Trigger                            | Context   |
| ---------------- | ---------------------------------- | --------- |
| Pan viewport     | Left-click drag                    | on-canvas |
| Pan viewport     | `Mod` + left-drag                  | anywhere  |
| Zoom             | Mouse wheel                        | on-canvas |
| Zoom             | `Mod` + mouse wheel                | anywhere  |
| Pan viewport     | Trackpad scroll                    | on-canvas |
| Pan viewport     | `Mod` + trackpad scroll            | anywhere  |
| Move window      | `Alt` + left-drag                  | on-window |
| Resize window    | `Alt` + right-drag                 | on-window |
| Center nearest   | `Mod+Ctrl` + left-drag (natural)   | anywhere  |
| Toggle fullscreen| `Alt` + middle-click               | on-window |

**Trackpad vs mouse wheel**: both produce axis events but serve different
purposes. Separate triggers (`trackpad-scroll` and `wheel-scroll`) allow
per-device bindings — by default trackpad scroll pans the viewport while mouse
wheel zooms on canvas.

### Edge auto-pan

When dragging a window to the viewport edge, the viewport auto-pans in that
direction. Speed is depth-proportional — deeper into the zone means faster
panning (quadratic ramp, like a joystick). All 8 directions (corners =
diagonal blend). Stops when cursor leaves the zone or the drag ends.

### Window snapping

When dragging a window near another window's edge, the dragged window snaps to
align edges magnetically. Configurable via `[snap]` in the config file (enable/
disable, threshold distance).

## Keyboard shortcuts

Minimal set. Defaults below, all configurable via `[keybinds]` table (maps key combo → built-in action or `exec` command). Implementation: data-driven binding lookup from day one, initially populated from defaults, later merged with user config.

### Window management

| Shortcut            | Action                                 |
| ------------------- | -------------------------------------- |
| `Alt-Tab`           | Cycle windows forward (raise+center)   |
| `Alt-Shift-Tab`     | Cycle windows backward                 |
| `Super+Q`           | Close focused window                   |
| `Super+C`           | Center focused window + reset zoom     |
| `Super+F`           | Toggle fullscreen                      |
| `Super+Shift+Arrow` | Nudge focused window 20px in direction |

### Navigation

| Shortcut      | Action                             |
| ------------- | ---------------------------------- |
| `Super+Arrow` | Center nearest window in direction |
| `Super+A`     | Toggle home (0, 0) ↔ previous pos  |
| `Super+W`     | Zoom-to-fit — show all windows     |
| `Super+1-4`   | Go to canvas corner (↙ ↖ ↗ ↘)     |

### Viewport

| Shortcut           | Action               |
| ------------------ | -------------------- |
| `Super+Ctrl+Arrow` | Pan viewport by step |
| `Super+Plus`       | Zoom in              |
| `Super+Minus`      | Zoom out             |
| `Super+0`          | Reset zoom to 1.0    |

### Launchers

| Shortcut       | Action                     |
| -------------- | -------------------------- |
| `Super+Return` | Open terminal              |
| `Super+D`      | Open launcher (bemenu-run) |
| `Super+Space`  | Switch keyboard layout     |

### Media / hardware keys

| Shortcut                | Action            |
| ----------------------- | ----------------- |
| `XF86AudioRaiseVolume`  | Volume up         |
| `XF86AudioLowerVolume`  | Volume down       |
| `XF86AudioMute`         | Toggle mute       |
| `XF86MonBrightnessUp`   | Brightness up     |
| `XF86MonBrightnessDown` | Brightness down   |
| `Print`                 | Screenshot (grim) |

### Session

| Shortcut             | Action                 |
| -------------------- | ---------------------- |
| `Super+L`            | Lock screen (swaylock) |
| `Super+Ctrl+Shift+Q` | Exit compositor        |

## Window decorations

**Strategy**: CSD-first via `xdg-decoration` protocol. Compositor advertises
only `close` and `fullscreen` capabilities via `xdg-toplevel` — no maximize,
no minimize. GTK/Qt apps hide those buttons automatically.

- **CSD apps** (GTK4, GTK3, most GNOME apps): draw their own title bar with
  close button only. Compositor does nothing.
- **Borderless windows**: window rules can set `decoration = "none"` — client
  removes its CSD via `xdg-decoration`, compositor draws nothing. Used for
  widgets and special windows.
- **SSD fallback** (XWayland apps, some Qt apps that render with zero
  decorations): compositor draws a minimal title bar + close button.
  - 25px title bar with rounded top corners (radius 8)
  - Thin × close button, right-aligned with 8px padding
  - Gaussian drop shadow (radius 14, GLSL shader)
  - Invisible resize borders (8px) around SSD windows for edge/corner resize
- **Interaction**: click title bar to drag, click × to close, drag borders to
  resize, hover × changes cursor to pointer.
- **Window rules**: `decoration` field controls mode — `"client"` (default,
  CSD), `"server"` (force SSD), `"none"` (borderless).
- **Configuration**: only `bg_color` and `fg_color` are configurable in
  `[decorations]` — everything else (dimensions, corner radius, shadow) is
  hardcoded.
- **Snapping**: window snapping accounts for SSD title bar boundaries.

## Focus model

**Click-to-focus.** Clicking or gesture-interacting with a window focuses and
raises it. This avoids accidental focus changes when panning over windows.

- Click on window → focus + raise
- 3-finger drag on window → focus + raise (at gesture start)
- 4-finger pan jump → focus + raise target window
- During a gesture, keyboard input goes to the focused window (the one being
  dragged, or the previously focused window if gesturing on desktop)

## Window placement

New windows open at the **center of the current viewport** — wherever the user
is looking. Placing at `(0, 0)` would be wrong since the user could be far away
on the canvas.

## Stacking / overlap

Windows can overlap. Click or gesture-interact with a window to raise it.
No minimize, no maximize (fullscreen replaces maximize). Hidden windows aren't
hidden — they're just somewhere else on the canvas. Pan to find them.

## Widgets

**Interim**: waybar via layer-shell for status bar, swayosd for volume/brightness
OSD. Works today.

**Planned**: `driftwm-shell` — a separate project providing a home screen and
system shell using [Fabric](https://github.com/Fabric-Development/fabric)
(Python GTK4 widget framework). Widgets are layer-shell surfaces placed at the
home position near `(0, 0)`. Use home gesture to peek at them, repeat to go back.

### driftwm-shell roadmap

1. **Basic home screen** — clock, date, coords/zoom, Fabric scaffold, GTK theming
2. **Quick settings** — volume, brightness, wifi, bluetooth, keyboard layout
3. **System tray** — StatusNotifierItem protocol support, tray icon display
4. **Logout menu** — shutdown, reboot, logout, suspend, lock
5. **Notifications** — freedesktop daemon, popup toasts, dismiss/actions
6. **OSD, media controls, calendar, lock screen** (with shader support)

### Customization surface

- **GTK theme + custom CSS** — colors, fonts, look and feel
- **Widget toggles** — show/hide individual widgets
- **Widget position/size** — where on the home screen each widget sits
- **Per-widget settings** — clock format, what quick settings to show, etc.

## Canvas background

The background is part of the canvas — it scrolls with the viewport, not stuck
to the screen. This provides spatial awareness when panning and makes the canvas
feel like a real surface.

### Background modes

1. **Shader** (default): GLSL fragment shader. Compositor passes
   `(cx, cy, z, time, resolution)` as uniforms. Ships with a built-in dot grid
   shader as default. Users can swap to any custom shader — noise, gradients,
   animated patterns, etc.
2. **Tiled image**: user provides a seamless (loopable) texture. Repeats
   infinitely across the canvas. Scales with zoom.

Both modes are infinite by nature.

Static shaders (no `time` dependency) are cached and only re-rendered when the
viewport changes (pan/zoom). Zero idle GPU cost. Animated shaders force a
redraw every frame — opt-in via `animate = true`.

Config example:

```toml
[background]
mode = "shader"                                # or "tile"
shader_path = "~/.config/driftwm/bg.frag"      # omit for built-in dot grid
bg_color = "#1e1e2e"                           # passed as uniform
animate = false                                # true: redraw every frame (for time-based shaders)

# mode = "tile"
# tile_path = "~/.config/driftwm/tile.png"
```

## Configuration

Config file: `~/.config/driftwm/config.toml` (respects `XDG_CONFIG_HOME`).

### Trackpad / libinput

The compositor owns the input devices on real hardware, so basic libinput
settings are exposed in config:

```toml
[input.trackpad]
tap_to_click = true        # default: true
tap_and_drag = true        # double-tap-hold = drag. default: true
natural_scroll = true      # default: true
accel_speed = 0.0          # pointer acceleration (-1.0 to 1.0). default: 0.0
```

Trackpad gestures and mouse bindings are fully configurable via context-aware
sections (`on-window`, `on-canvas`, `anywhere`). See `config.example.toml` for
the full default binding set and trigger/action vocabulary.

### Keyboard

```toml
[input.keyboard]
repeat_rate = 25       # keys per second. default: 25
repeat_delay = 300     # ms before repeat starts. default: 300
```

### Scroll / viewport panning

```toml
[input.scroll]
canvas_speed = 1.5     # multiplier for viewport pan deltas. default: 1.5
friction = 0.96        # momentum decay per frame (0.90 = snappy, 0.98 = floaty). default: 0.96
```

Only affects viewport panning. Scroll events forwarded to windows use raw deltas
(no multiplier, no momentum).

### Cursor

```toml
[cursor]
theme = "Adwaita"      # default: "default"
size = 24              # default: 24
```

## Launcher

Not built into the compositor. `Super+D` runs whatever command is configured
(default: `fuzzel`). Users can swap to wofi, tofi, bemenu-run, etc.

```toml
[keybindings]
"mod+d" = "exec fuzzel"
```

## Ecosystem tools

All external — compositor delegates to standard Wayland tools.

| Tool           | Purpose                              |
| -------------- | ------------------------------------ |
| `waybar`       | Status bar (coords/zoom, clock, kbd) |
| `swaync`       | Quick settings + notifications       |
| `swayosd`      | Volume/brightness OSD                |
| `fuzzel`       | App launcher                         |
| `crystal-dock` | Dock / taskbar                       |
| `swaylock`     | Lock screen (`ext-session-lock`)     |

Waybar modules: canvas x,y,z from driftwm, clock/date, keyboard layout,
swaync integration, logout menu.

## Theming / integration

The compositor inherits the desktop theme automatically:

- **GTK theme**: apps read from `gsettings` / dconf (persists from GNOME config)
- **Icons**: same, via `gsettings`
- **Cursor**: set via `[cursor]` config (compositor also exports `XCURSOR_THEME`/`XCURSOR_SIZE` to child processes)
- **Fonts**: system fontconfig, no compositor involvement

## Dev workflow

### Nested Wayland (primary method)

Wayland compositors can run inside an existing Wayland session as a window.
smithay provides two backends:

- **winit backend**: runs compositor as a regular window on your current desktop.
  Perfect for development. No VM needed.
- **udev/libinput backend**: takes over real hardware (DRM/KMS). For production.

Development loop:

```bash
# From your GNOME Wayland session:
cargo run                        # opens driftwm as a window on your desktop
# Inside that window, apps think they're on a real compositor
WAYLAND_DISPLAY=wayland-1 foot   # open a terminal inside driftwm
```

### Limitations of nested mode

- Trackpad gestures may be intercepted by the parent compositor (GNOME) before
  reaching your nested instance. Test gesture code on real hardware or in a VM.
- Multi-monitor can't be tested nested — need real hardware or VM with virtual
  displays.

### When you need real hardware testing

```bash
# Switch to a TTY (Ctrl+Alt+F3), log in, run:
cargo run -- --backend udev
# This takes over the GPU directly. Ctrl+Alt+F2 to get back to GNOME.
```

### Logging

Use `RUST_LOG=debug cargo run` for smithay/libinput event traces. Essential for
debugging gesture recognition and input handling.

## Architecture sketch

```
src/
├── main.rs
├── lib.rs
├── canvas.rs
├── focus.rs
├── decorations.rs
├── render.rs
├── backend/
│   ├── mod.rs
│   ├── winit.rs
│   └── udev.rs
├── state/
│   ├── mod.rs
│   ├── animation.rs
│   ├── navigation.rs
│   └── fullscreen.rs
├── config/
│   ├── mod.rs
│   ├── types.rs
│   ├── parse.rs
│   ├── defaults.rs
│   └── toml.rs
├── input/
│   ├── mod.rs
│   ├── actions.rs
│   ├── pointer.rs
│   └── gestures.rs
├── grabs/
│   ├── mod.rs
│   ├── move_grab.rs
│   ├── resize_grab.rs
│   ├── pan_grab.rs
│   └── navigate_grab.rs
├── handlers/
│   ├── mod.rs
│   ├── compositor.rs
│   ├── xdg_shell.rs
│   └── layer_shell.rs
└── protocols/
    ├── mod.rs
    ├── foreign_toplevel.rs
    └── screencopy.rs
```

## Milestones

Ordered to maximize what can be developed in winit (nested) mode before
requiring real hardware (udev/TTY). Milestones 1–8 work entirely in winit.

1. **Window appears** _(done)_
2. **Move and resize** _(done)_
3. **Infinite canvas** _(done)_
4. **Canvas background** _(done)_
5. **Window navigation** _(done)_
6. **Zoom** _(done)_
7. **Layer shell** _(done)_
8. **Config file** _(done)_
9. **udev backend** _(done)_
10. **Trackpad gestures** _(done)_
11. **Window rules** — app_id matching, widget mode, state file, xdg-decoration _(done)_
12. **Decorations** — SSD fallback, title bar, shadows, resize grab zones _(done)_
13. XWayland — X11 app support
14. Screenshot/screencast — wlr-screencopy, screen capture
15. Multi-monitor — multiple viewports on same canvas

Separate project: **driftwm-shell** — GTK4 home screen + system widgets
via Fabric (see Widgets section).
