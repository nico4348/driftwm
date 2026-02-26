use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState, keyboard::XkbConfig, pointer::CursorImageStatus},
    reexports::{
        calloop::{LoopHandle, LoopSignal},
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
        },
    },
    utils::{Logical, Point},
    wayland::output::OutputManagerState,
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        cursor_shape::CursorShapeManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
    },
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::idle_inhibit::IdleInhibitManagerState;
use smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::gles::{GlesPixelProgram, GlesRenderer, element::PixelShaderElement};
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::utils::{Size, Transform};

use driftwm::canvas::{self, CanvasPos, MomentumState};
use driftwm::config::Config;
use smithay::wayland::shell::wlr_layer::Layer as WlrLayer;

pub use crate::focus::FocusTarget;

/// Log an error result with context, discarding the Ok value.
#[inline]
pub fn log_err(context: &str, result: Result<impl Sized, impl std::fmt::Display>) {
    if let Err(e) = result {
        tracing::error!("{context}: {e}");
    }
}

/// Wrapper held by the calloop event loop — gives callbacks access
/// to both compositor state and the Wayland display.
pub struct CalloopData {
    pub state: DriftWm,
    pub display: Display<DriftWm>,
}

/// Saved state for a fullscreen window — restored on exit.
pub struct FullscreenState {
    pub window: Window,
    pub saved_location: Point<i32, Logical>,
    pub saved_camera: Point<f64, Logical>,
    pub saved_zoom: f64,
}

/// Central compositor state.
pub struct DriftWm {
    pub start_time: Instant,
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, CalloopData>,
    pub loop_signal: LoopSignal,

    // Desktop
    pub space: Space<Window>,
    pub popups: PopupManager,

    // Protocol state
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<DriftWm>,
    pub data_device_state: DataDeviceState,

    // Input
    pub seat: Seat<DriftWm>,

    // Viewport / camera / zoom
    pub camera: Point<f64, Logical>,
    pub zoom: f64,
    /// Zoom animation target. When Some, zoom lerps toward this value each frame.
    pub zoom_target: Option<f64>,
    /// Last rendered zoom — for shader/damage change detection.
    pub last_rendered_zoom: f64,
    /// Saved (camera, zoom) for ZoomToFit toggle-back.
    pub overview_return: Option<(Point<f64, Logical>, f64)>,
    /// Timestamp of the last scroll-pan event. Used to keep panning sticky
    /// within a scroll gesture (150ms window) even if a window slides under.
    pub last_scroll_pan: Option<Instant>,
    /// Scroll momentum: velocity, friction, frame-based skip.
    pub momentum: MomentumState,
    /// Monotonic frame counter, incremented each render tick.
    pub frame_counter: u64,

    /// Auto-pan velocity when dragging a window to viewport edge.
    /// Set by MoveSurfaceGrab, cleared when grab ends or cursor leaves edge zone.
    pub edge_pan_velocity: Option<Point<f64, Logical>>,

    // Cursor
    pub cursor_status: CursorImageStatus,
    /// True while a compositor grab (pan/resize) owns the cursor icon.
    /// Blocks client cursor updates in `cursor_image()`.
    pub grab_cursor: bool,
    pub cursor_buffers: HashMap<String, (MemoryRenderBuffer, Point<i32, Logical>)>,

    // Backend (moved here so protocol handlers can access the renderer)
    pub backend: Option<WinitGraphicsBackend<GlesRenderer>>,
    /// Compiled background shader program (compiled once at startup).
    pub background_shader: Option<GlesPixelProgram>,
    /// Cached shader background element (stable Id for damage tracking).
    pub cached_bg_element: Option<PixelShaderElement>,
    /// Camera position at last render — used to detect movement and update uniforms.
    pub last_rendered_camera: Point<f64, Logical>,
    /// Pre-loaded tile image for tiled background (loaded once at startup).
    /// Buffer is (w+1)×(h+1) with the last col/row duplicated for 1px overlap.
    /// Stores (buffer, original_width, original_height).
    pub background_tile: Option<(MemoryRenderBuffer, i32, i32)>,

    // Protocols
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub cursor_shape_state: CursorShapeManagerState,
    pub viewporter_state: ViewporterState,
    pub fractional_scale_state: FractionalScaleManagerState,
    pub xdg_activation_state: XdgActivationState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,
    pub pointer_constraints_state: PointerConstraintsState,
    pub relative_pointer_state: RelativePointerManagerState,
    pub keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState,
    pub idle_inhibit_state: IdleInhibitManagerState,
    pub presentation_state: PresentationState,
    pub layer_shell_state: WlrLayerShellState,
    pub foreign_toplevel_state: driftwm::protocols::foreign_toplevel::ForeignToplevelManagerState,

    /// True when pointer focus is a layer surface (screen-fixed, not canvas-relative).
    /// Guards synthetic pointer adjustments in camera/zoom animations.
    pub pointer_over_layer: bool,

    // Keybindings and settings
    pub config: Config,

    /// Surfaces awaiting their first buffer commit, to be centered once size is known.
    pub pending_center: HashSet<WlSurface>,

    // Window navigation
    /// Camera animation target. When Some, camera lerps toward this point each frame.
    pub camera_target: Option<Point<f64, Logical>>,
    /// Timestamp of the last rendered frame, for delta-time computation.
    pub last_frame_instant: Instant,
    /// MRU focus history: index 0 = most recently focused.
    pub focus_history: Vec<Window>,
    /// Active Alt-Tab cycling index into focus_history. None when not cycling.
    pub cycle_state: Option<usize>,
    /// Saved (camera, zoom) to return to when toggling home a second time.
    pub home_return: Option<(Point<f64, Logical>, f64)>,

    // Key repeat for compositor bindings (smithay's repeat only applies to
    // client-forwarded keys, not intercepted compositor actions).
    /// Currently held repeatable action: (keycode, action, next_fire_time).
    pub held_action: Option<(u32, driftwm::config::Action, Instant)>,

    /// Active fullscreen window state. When Some, viewport is locked.
    pub fullscreen: Option<FullscreenState>,
}

/// Per-client state stored by wayland-server for each connected client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl DriftWm {
    pub fn new(
        dh: DisplayHandle,
        loop_handle: LoopHandle<'static, CalloopData>,
        loop_signal: LoopSignal,
    ) -> Self {
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new_with_capabilities::<Self>(
            &dh,
            [xdg_toplevel::WmCapabilities::Fullscreen],
        );
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let cursor_shape_state = CursorShapeManagerState::new::<Self>(&dh);
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(&dh);
        let xdg_activation_state = XdgActivationState::new::<Self>(&dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        let data_control_state =
            DataControlState::new::<Self, _>(&dh, Some(&primary_selection_state), |_| true);
        let pointer_constraints_state = PointerConstraintsState::new::<Self>(&dh);
        let relative_pointer_state = RelativePointerManagerState::new::<Self>(&dh);
        let keyboard_shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<Self>(&dh);
        let idle_inhibit_state = IdleInhibitManagerState::new::<Self>(&dh);
        let presentation_state = PresentationState::new::<Self>(&dh, 1); // CLOCK_MONOTONIC
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let foreign_toplevel_state =
            driftwm::protocols::foreign_toplevel::ForeignToplevelManagerState::new::<Self, _>(&dh, |_| true);

        let config = Config::default();

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat-0");
        seat.add_keyboard(XkbConfig::default(), config.repeat_delay, config.repeat_rate)
            .expect("Failed to add keyboard");
        seat.add_pointer();
        Self {
            start_time: Instant::now(),
            display_handle: dh,
            loop_handle,
            loop_signal,
            space: Space::default(),
            popups: PopupManager::default(),
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
            camera: Point::from((0.0, 0.0)),
            zoom: 1.0,
            zoom_target: None,
            last_rendered_zoom: f64::NAN,
            overview_return: None,
            last_scroll_pan: None,
            momentum: MomentumState::new(config.friction),
            frame_counter: 0,
            edge_pan_velocity: None,
            cursor_status: CursorImageStatus::default_named(),
            grab_cursor: false,
            cursor_buffers: HashMap::new(),
            backend: None,
            background_shader: None,
            cached_bg_element: None,
            last_rendered_camera: Point::from((f64::NAN, f64::NAN)),
            background_tile: None,
            dmabuf_state: DmabufState::new(),
            dmabuf_global: None,
            cursor_shape_state,
            viewporter_state,
            fractional_scale_state,
            xdg_activation_state,
            primary_selection_state,
            data_control_state,
            pointer_constraints_state,
            relative_pointer_state,
            keyboard_shortcuts_inhibit_state,
            idle_inhibit_state,
            presentation_state,
            layer_shell_state,
            foreign_toplevel_state,
            pointer_over_layer: false,
            config,
            pending_center: HashSet::new(),
            camera_target: None,
            last_frame_instant: Instant::now(),
            focus_history: Vec::new(),
            cycle_state: None,
            home_return: None,
            held_action: None,
            fullscreen: None,
        }
    }

    /// Fire held compositor action if repeat delay/rate has elapsed.
    pub fn apply_key_repeat(&mut self) {
        let Some((_, ref action, next_fire)) = self.held_action else {
            return;
        };
        let now = Instant::now();
        if now < next_fire {
            return;
        }
        let action = action.clone();
        let rate_interval = Duration::from_millis(1000 / self.config.repeat_rate.max(1) as u64);
        self.held_action.as_mut().unwrap().2 = now + rate_interval;
        self.execute_action(&action);
    }

    /// Compute focus target at the given canvas position, respecting whether
    /// the pointer is currently over a layer surface or a canvas window.
    fn focus_under(
        &self,
        canvas_pos: Point<f64, Logical>,
    ) -> Option<(FocusTarget, Point<f64, Logical>)> {
        if self.pointer_over_layer {
            let screen_pos =
                canvas::canvas_to_screen(CanvasPos(canvas_pos), self.camera, self.zoom).0;
            self.layer_surface_under(
                screen_pos,
                canvas_pos,
                &[WlrLayer::Overlay, WlrLayer::Top, WlrLayer::Bottom, WlrLayer::Background],
            )
        } else {
            self.surface_under(canvas_pos)
        }
    }

    /// Send a synthetic pointer motion to keep the cursor at the same screen
    /// position after a camera or zoom change.
    fn warp_pointer(&mut self, new_pos: Point<f64, Logical>) {
        let under = self.focus_under(new_pos);
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        pointer.motion(
            self,
            under,
            &smithay::input::pointer::MotionEvent {
                location: new_pos,
                serial,
                time: self.start_time.elapsed().as_millis() as u32,
            },
        );
        pointer.frame(self);
    }

    /// Apply scroll momentum each frame. Skips frames where a scroll event
    /// already moved the camera (via frame counter). Otherwise decays velocity.
    pub fn apply_scroll_momentum(&mut self) {
        let Some(delta) = self.momentum.tick(self.frame_counter) else {
            return;
        };

        self.camera += delta;
        self.update_output_from_camera();

        // Shift pointer canvas position so screen position stays fixed
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.warp_pointer(pos + delta);
    }

    /// Apply edge auto-pan each frame during a window drag near viewport edges.
    /// Synthetic pointer motion keeps cursor at the same screen position and
    /// lets the active MoveSurfaceGrab reposition the window automatically.
    pub fn apply_edge_pan(&mut self) {
        let Some(velocity) = self.edge_pan_velocity else { return; };
        // velocity is screen-space speed; convert to canvas delta
        let canvas_delta = Point::from((velocity.x / self.zoom, velocity.y / self.zoom));
        self.camera += canvas_delta;
        self.update_output_from_camera();

        // Shift pointer canvas position so screen position stays fixed
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.warp_pointer(pos + canvas_delta);
    }

    /// Apply a viewport pan delta with momentum accumulation.
    /// Call this from any input path that should drift (scroll, click-drag, future gestures).
    pub fn drift_pan(&mut self, delta: Point<f64, Logical>) {
        self.camera_target = None; // Cancel animation on manual input
        self.zoom_target = None;
        self.overview_return = None;
        self.momentum.accumulate(delta, self.frame_counter);
        self.camera += delta;
        self.update_output_from_camera();
    }

    /// Sync each output's position to the current camera, so render_output
    /// automatically applies the canvas→screen transform.
    pub fn update_output_from_camera(&mut self) {
        let camera_i32 = self.camera.to_i32_round();
        for output in self.space.outputs().cloned().collect::<Vec<_>>() {
            self.space.map_output(&output, camera_i32);
        }
    }

    /// Advance the camera animation toward `camera_target` using frame-rate independent lerp.
    /// Shifts the pointer by the camera delta so the cursor stays at the same screen position.
    pub fn apply_camera_animation(&mut self, dt: Duration) {
        let Some(target) = self.camera_target else {
            return;
        };

        let old_camera = self.camera;

        let base = self.config.animation_speed;
        let reference_dt = 1.0 / 60.0;
        let dt_secs = dt.as_secs_f64();
        let factor = 1.0 - (1.0 - base).powf(dt_secs / reference_dt);

        let dx = target.x - self.camera.x;
        let dy = target.y - self.camera.y;

        // Snap when sub-pixel close
        if dx * dx + dy * dy < 0.25 {
            self.camera = target;
            self.camera_target = None;
        } else {
            self.camera = Point::from((
                self.camera.x + dx * factor,
                self.camera.y + dy * factor,
            ));
        }

        self.update_output_from_camera();

        // Shift pointer so cursor stays at the same screen position
        let delta = self.camera - old_camera;
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.warp_pointer(pos + delta);
    }

    /// Navigate the viewport to center on a window: raise, focus, animate camera.
    /// If returning from overview (ZoomToFit), also restores the saved zoom level.
    pub fn navigate_to_window(&mut self, window: &Window) {
        self.space.raise_element(window, true);
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let surface = window.toplevel().unwrap().wl_surface().clone();
        keyboard.set_focus(self, Some(FocusTarget(surface)), serial);

        // If in overview, restore saved zoom; otherwise keep current zoom
        let target_zoom = if let Some((_, saved_zoom)) = self.overview_return.take() {
            saved_zoom
        } else {
            self.zoom
        };

        let window_loc = self.space.element_location(window).unwrap_or_default();
        let window_size = window.geometry().size;
        let viewport_size = self.get_viewport_size();
        let target = driftwm::canvas::camera_to_center_window(
            window_loc, window_size, viewport_size, target_zoom,
        );

        self.momentum.stop();
        self.camera_target = Some(target);
        self.zoom_target = Some(target_zoom);
    }

    /// Dynamic minimum zoom based on the current window layout.
    /// Allows zooming out far enough to see all windows.
    pub fn min_zoom(&self) -> f64 {
        let viewport = self.get_viewport_size();
        driftwm::canvas::dynamic_min_zoom(
            self.space.elements().map(|w| {
                let loc = self.space.element_location(w).unwrap_or_default();
                let size = w.geometry().size;
                (loc, size)
            }),
            viewport,
            self.config.zoom_fit_padding,
        )
    }

    /// Logical viewport size from the first output.
    pub fn get_viewport_size(&self) -> Size<i32, Logical> {
        self.space
            .outputs()
            .next()
            .and_then(|o| o.current_mode())
            .map(|m| m.size.to_logical(1))
            .unwrap_or((1, 1).into())
    }

    /// Advance zoom animation toward `zoom_target` using frame-rate independent lerp.
    /// Adjusts pointer canvas position so the cursor stays at the same screen position.
    pub fn apply_zoom_animation(&mut self, dt: Duration) {
        let Some(target) = self.zoom_target else {
            return;
        };

        let old_zoom = self.zoom;

        let base = self.config.animation_speed;
        let reference_dt = 1.0 / 60.0;
        let dt_secs = dt.as_secs_f64();
        let factor = 1.0 - (1.0 - base).powf(dt_secs / reference_dt);

        let dz = target - self.zoom;
        if dz.abs() < 0.001 {
            self.zoom = target;
            self.zoom_target = None;
        } else {
            self.zoom += dz * factor;
        }

        // Adjust pointer so cursor stays at the same screen position.
        // screen = (canvas - camera) * zoom  ⟹  new_canvas = screen / new_zoom + camera
        if self.zoom != old_zoom {
            let pos = self.seat.get_pointer().unwrap().current_location();
            let screen_x = (pos.x - self.camera.x) * old_zoom;
            let screen_y = (pos.y - self.camera.y) * old_zoom;
            let new_pos = Point::from((
                screen_x / self.zoom + self.camera.x,
                screen_y / self.zoom + self.camera.y,
            ));
            self.warp_pointer(new_pos);
        }
    }

    /// Update focus history with the given surface (push to front / move to front).
    /// Should NOT be called during Alt-Tab cycling (history is frozen).
    pub fn update_focus_history(&mut self, surface: &WlSurface) {
        let window = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == surface)
            .cloned();
        if let Some(window) = window {
            self.focus_history.retain(|w| w != &window);
            self.focus_history.insert(0, window);
        }
    }

    /// End Alt-Tab cycling: commit the selected window to focus history.
    pub fn end_cycle(&mut self) {
        let idx = self.cycle_state.take();
        if let Some(idx) = idx
            && let Some(window) = self.focus_history.get(idx).cloned()
        {
            self.focus_history.retain(|w| w != &window);
            self.focus_history.insert(0, window);
        }
    }

    /// Load an xcursor image by name and cache the resulting MemoryRenderBuffer.
    /// Returns a reference to the cached (buffer, hotspot) pair.
    pub fn load_xcursor(
        &mut self,
        name: &str,
    ) -> Option<&(MemoryRenderBuffer, Point<i32, Logical>)> {
        if !self.cursor_buffers.contains_key(name) {
            let theme_name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
            let theme = xcursor::CursorTheme::load(&theme_name);
            let path = theme.load_icon(name)?;
            let data = std::fs::read(path).ok()?;
            let images = xcursor::parser::parse_xcursor(&data)?;

            // Pick the image closest to 24px (standard cursor size)
            let target_size = std::env::var("XCURSOR_SIZE")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(24);
            let image = images
                .iter()
                .min_by_key(|img| (img.size as i32 - target_size as i32).unsigned_abs())?;

            let buffer = MemoryRenderBuffer::from_slice(
                &image.pixels_rgba,
                Fourcc::Abgr8888,
                (image.width as i32, image.height as i32),
                1,
                Transform::Normal,
                None,
            );
            let hotspot = Point::from((image.xhot as i32, image.yhot as i32));
            self.cursor_buffers
                .insert(name.to_string(), (buffer, hotspot));
        }
        self.cursor_buffers.get(name)
    }

    /// Enter fullscreen for the given window: lock viewport, expand window to fill screen.
    pub fn enter_fullscreen(&mut self, window: &Window) {
        // If already fullscreen (same or different window), exit first
        if self.fullscreen.is_some() {
            self.exit_fullscreen();
        }

        let viewport_size = self.get_viewport_size();
        let saved_location = self.space.element_location(window).unwrap_or_default();

        self.fullscreen = Some(FullscreenState {
            window: window.clone(),
            saved_location,
            saved_camera: self.camera,
            saved_zoom: self.zoom,
        });

        // Tell the client to go fullscreen at output size
        window.toplevel().unwrap().with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Fullscreen);
            state.size = Some(viewport_size);
        });
        window.toplevel().unwrap().send_configure();

        // Lock viewport: stop all animations and momentum
        self.zoom = 1.0;
        self.zoom_target = None;
        self.camera_target = None;
        self.momentum.stop();
        self.overview_return = None;
        self.home_return = None;
        // Top/Bottom layers are hidden during fullscreen — reset stale pointer state
        self.pointer_over_layer = false;

        // Snap camera to integer for pixel-perfect alignment
        let camera_i32 = self.camera.to_i32_round();
        self.camera = Point::from((camera_i32.x as f64, camera_i32.y as f64));

        // Place window at viewport origin and raise
        self.space.map_element(window.clone(), camera_i32, true);
        self.space.raise_element(window, true);
        self.update_output_from_camera();

        // Ensure keyboard focus is on the fullscreen window
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let surface = window.toplevel().unwrap().wl_surface().clone();
        keyboard.set_focus(self, Some(FocusTarget(surface)), serial);
    }

    /// Exit fullscreen: restore window position, camera, and zoom.
    pub fn exit_fullscreen(&mut self) {
        let Some(fs) = self.fullscreen.take() else {
            return;
        };

        // Tell client to leave fullscreen
        fs.window.toplevel().unwrap().with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
            state.size = None;
        });
        fs.window.toplevel().unwrap().send_configure();

        // Restore window position, camera, zoom
        self.space.map_element(fs.window, fs.saved_location, false);
        self.camera = fs.saved_camera;
        self.zoom = fs.saved_zoom;
        self.update_output_from_camera();
    }
}
