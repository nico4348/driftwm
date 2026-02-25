use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState, keyboard::XkbConfig, pointer::CursorImageStatus},
    reexports::{
        calloop::{LoopHandle, LoopSignal},
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
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::gles::{GlesPixelProgram, GlesRenderer, element::PixelShaderElement};
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::utils::Transform;

use driftwm::canvas::MomentumState;
use driftwm::config::Config;

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

    // Viewport / camera
    pub camera: Point<f64, Logical>,
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
    /// Stores (buffer, width, height) since MemoryRenderBuffer doesn't expose size.
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
    /// Saved camera position to return to when toggling home a second time.
    pub home_return: Option<Point<f64, Logical>>,
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
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
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
            config,
            pending_center: HashSet::new(),
            camera_target: None,
            last_frame_instant: Instant::now(),
            focus_history: Vec::new(),
            cycle_state: None,
            home_return: None,
        }
    }

    /// Apply scroll momentum each frame. Skips frames where a scroll event
    /// already moved the camera (via frame counter). Otherwise decays velocity.
    pub fn apply_scroll_momentum(&mut self) {
        let Some(delta) = self.momentum.tick(self.frame_counter) else {
            return;
        };

        self.camera += delta;
        self.update_output_from_camera();

        // Move pointer so cursor stays at the same screen position
        let pointer = self.seat.get_pointer().unwrap();
        let pos = pointer.current_location();
        let new_pos = pos + delta;
        let under = self.surface_under(new_pos);
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
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

    /// Apply edge auto-pan each frame during a window drag near viewport edges.
    /// Synthetic pointer motion keeps cursor at the same screen position and
    /// lets the active MoveSurfaceGrab reposition the window automatically.
    pub fn apply_edge_pan(&mut self) {
        let Some(velocity) = self.edge_pan_velocity else { return; };
        self.camera += velocity;
        self.update_output_from_camera();

        // Shift pointer canvas position so screen position stays fixed
        let pointer = self.seat.get_pointer().unwrap();
        let pos = pointer.current_location();
        let new_pos = pos + velocity;
        let under = self.surface_under(new_pos);
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
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

    /// Apply a viewport pan delta with momentum accumulation.
    /// Call this from any input path that should drift (scroll, click-drag, future gestures).
    pub fn drift_pan(&mut self, delta: Point<f64, Logical>) {
        self.camera_target = None; // Cancel animation on manual input
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
        let pointer = self.seat.get_pointer().unwrap();
        let pos = pointer.current_location();
        let new_pos = pos + delta;
        let under = self.surface_under(new_pos);
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
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

    /// Navigate the viewport to center on a window: raise, focus, animate camera.
    pub fn navigate_to_window(&mut self, window: &Window) {
        self.space.raise_element(window, true);
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let surface = window.toplevel().unwrap().wl_surface().clone();
        keyboard.set_focus(self, Some(FocusTarget(surface)), serial);

        // Compute target camera to center this window
        let window_loc = self.space.element_location(window).unwrap_or_default();
        let window_size = window.geometry().size;
        let viewport_size = self
            .space
            .outputs()
            .next()
            .and_then(|o| o.current_mode())
            .map(|m| m.size.to_logical(1))
            .unwrap_or((1, 1).into());
        let target = driftwm::canvas::camera_to_center_window(window_loc, window_size, viewport_size);

        self.momentum.stop();
        self.camera_target = Some(target);
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
}
