# Protocol Roadmap

Current: 28 protocols implemented. Below are missing protocols to add before launch,
prioritized by user-facing impact.

## Launch Targets

### single-pixel-buffer (wp-single-pixel-buffer-v1)
Lets clients create 1×1 solid-color buffers without SHM allocation. GTK4 uses this heavily
for backgrounds and separators. Trivial to add (~5 lines, no handler trait).

### xdg-dialog (xdg-dialog-v1)
Marks toplevel windows as modal dialogs. File pickers and confirmation dialogs should stay
on top of their parent and not get lost on the canvas.

### xdg-foreign (xdg-foreign-v2)
Lets apps export/import surface handles across process boundaries. xdg-desktop-portal needs
this for screen sharing dialogs (the portal process references the requesting app's window).

### idle-notify (ext-idle-notify-v1)
Notifies clients when the user goes idle. Required by swayidle for auto-lock and screen
dimming. (Note: idle-*inhibit* is already implemented — that prevents idle, this detects it.)

### content-type (wp-content-type-v1)
Surface content type hints (none/photo/video/game). Enables adaptive sync and
compositor-side optimizations for games and video playback.

### xwayland-keyboard-grab
Lets Xwayland grab compositor keybindings. Improves keyboard handling for X11 games
and apps that need raw key access.

## Post-Launch

### text-input + input-method + virtual-keyboard
Full IME stack for CJK input and on-screen keyboards. Add when needed.

### alpha-modifier (wp-alpha-modifier-v1)
Per-surface opacity control.

### tablet-manager (zwp_tablet_v2)
Stylus and drawing tablet input. Add when needed.

### security-context (wp-security-context-v1)
Identifies sandboxed clients (Flatpak, Snap). Allows per-sandbox permission policies.

### drm-syncobj
Explicit GPU synchronization for Vulkan compositing. Matters for multi-GPU setups.

### fifo + commit-timing
Advanced frame scheduling.

### Other
xdg-toplevel-icon, drm-lease, ext-data-control, ext-foreign-toplevel-list,
kde-decoration, xdg-system-bell, xdg-toplevel-tag — niche or already covered.
