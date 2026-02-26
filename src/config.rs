use std::collections::HashMap;
use std::f64::consts::FRAC_1_SQRT_2;

use smithay::input::keyboard::{Keysym, ModifiersState, keysyms};

#[derive(Clone, Debug, PartialEq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl Direction {
    /// Normalized direction vector for this direction.
    pub fn to_unit_vec(&self) -> (f64, f64) {
        match self {
            Direction::Up => (0.0, -1.0),
            Direction::Down => (0.0, 1.0),
            Direction::Left => (-1.0, 0.0),
            Direction::Right => (1.0, 0.0),
            Direction::UpLeft => (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
            Direction::UpRight => (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
            Direction::DownLeft => (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Direction::DownRight => (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    SpawnCommand(String),
    CloseWindow,
    NudgeWindow(Direction),
    PanViewport(Direction),
    CenterWindow,
    CenterNearest(Direction),
    CycleWindows { backward: bool },
    HomeToggle,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomToFit,
    ToggleFullscreen,
}

impl Action {
    /// Actions that should auto-repeat when their key is held.
    pub fn is_repeatable(&self) -> bool {
        matches!(
            self,
            Action::ZoomIn
                | Action::ZoomOut
                | Action::NudgeWindow(_)
                | Action::PanViewport(_)
                | Action::CycleWindows { .. }
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

/// Which physical key acts as the window-manager modifier.
/// Alt for dev (nested winit); Super for production (standalone on TTY).
#[derive(Clone, Copy, Debug)]
pub enum ModKey {
    Alt,
    Super,
}

impl ModKey {
    /// Base modifier pattern with only the WM mod key set.
    fn base(self) -> Modifiers {
        match self {
            ModKey::Alt => Modifiers {
                alt: true,
                ..Modifiers::EMPTY
            },
            ModKey::Super => Modifiers {
                logo: true,
                ..Modifiers::EMPTY
            },
        }
    }

    /// Check if this mod key is pressed in the given modifier state.
    pub fn is_pressed(self, state: &ModifiersState) -> bool {
        match self {
            ModKey::Alt => state.alt,
            ModKey::Super => state.logo,
        }
    }
}

/// Which modifier must be held during window cycling (Alt-Tab style).
/// Ctrl for dev (GNOME intercepts Alt-Tab); Alt for production.
#[derive(Clone, Copy, Debug)]
pub enum CycleModifier {
    Alt,
    Ctrl,
}

impl CycleModifier {
    pub fn is_pressed(self, state: &ModifiersState) -> bool {
        match self {
            CycleModifier::Alt => state.alt,
            CycleModifier::Ctrl => state.ctrl,
        }
    }

    fn base(self) -> Modifiers {
        match self {
            CycleModifier::Alt => Modifiers {
                alt: true,
                ..Modifiers::EMPTY
            },
            CycleModifier::Ctrl => Modifiers {
                ctrl: true,
                ..Modifiers::EMPTY
            },
        }
    }
}

impl Modifiers {
    const EMPTY: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        logo: false,
    };

    fn from_state(state: &ModifiersState) -> Self {
        Self {
            ctrl: state.ctrl,
            alt: state.alt,
            shift: state.shift,
            logo: state.logo,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub modifiers: Modifiers,
    pub sym: Keysym,
}

/// Built-in dot grid shader — used when no shader_path or tile_path is configured.
pub const DEFAULT_SHADER: &str = include_str!("../assets/shaders/pink_cloud.glsl");

#[derive(Clone, Debug, Default)]
pub struct BackgroundConfig {
    /// Path to a GLSL fragment shader. If set, shader is compiled and rendered fullscreen.
    pub shader_path: Option<String>,
    /// Path to a tile image (PNG/JPG). If set, image is tiled across the canvas.
    pub tile_path: Option<String>,
}

pub struct Config {
    pub mod_key: ModKey,
    /// Multiplier for scroll deltas. Higher = faster initial scroll. 1.0 = raw trackpad.
    pub scroll_speed: f64,
    /// Scroll momentum decay factor per frame. 0.92 = snappy, 0.96 = floaty.
    pub friction: f64,
    /// Pixels per keyboard nudge (Mod+Shift+Arrow).
    pub nudge_step: i32,
    /// Pixels per keyboard pan (Mod+Ctrl+Arrow).
    pub pan_step: f64,
    /// Keyboard repeat delay (ms) and rate (keys/sec).
    pub repeat_delay: i32,
    pub repeat_rate: i32,
    /// Edge auto-pan: activation zone width in pixels from viewport edge.
    pub edge_zone: f64,
    /// Edge auto-pan: speed range (px/frame). Quadratic ramp from min to max.
    pub edge_pan_min: f64,
    pub edge_pan_max: f64,
    /// Base lerp factor for camera animation (frame-rate independent). 0.15 = smooth.
    pub animation_speed: f64,
    /// Modifier held during window cycling. Release commits selection.
    pub cycle_modifier: CycleModifier,
    /// Zoom step multiplier per keypress. 1.1 = 10% per press.
    pub zoom_step: f64,
    /// Padding (canvas pixels) around the bounding box for ZoomToFit.
    pub zoom_fit_padding: f64,
    pub background: BackgroundConfig,
    bindings: HashMap<KeyCombo, Action>,
}

impl Config {
    pub fn lookup(&self, modifiers: &ModifiersState, sym: Keysym) -> Option<&Action> {
        let combo = KeyCombo {
            modifiers: Modifiers::from_state(modifiers),
            sym,
        };
        self.bindings.get(&combo)
    }
}

impl Default for Config {
    fn default() -> Self {
        let mod_key = ModKey::Alt;
        let cycle_modifier = CycleModifier::Ctrl;
        let terminal = detect_terminal();
        let launcher = detect_launcher();
        tracing::info!("Terminal command: {terminal}");
        tracing::info!("Launcher command: {launcher}");

        let m = mod_key.base();
        let m2 = m.clone();
        let m_shift = Modifiers {
            shift: true,
            ..m.clone()
        };
        let m_ctrl = Modifiers {
            ctrl: true,
            ..m.clone()
        };
        let cyc = cycle_modifier.base();
        let cyc_shift = Modifiers {
            shift: true,
            ..cyc.clone()
        };

        let bindings = HashMap::from([
            (
                KeyCombo {
                    modifiers: m.clone(),
                    sym: Keysym::from(keysyms::KEY_Return),
                },
                Action::SpawnCommand(terminal),
            ),
            (
                KeyCombo {
                    modifiers: m.clone(),
                    sym: Keysym::from(keysyms::KEY_d),
                },
                Action::SpawnCommand(launcher),
            ),
            (
                KeyCombo {
                    modifiers: m,
                    sym: Keysym::from(keysyms::KEY_q),
                },
                Action::CloseWindow,
            ),
            // Window nudge: Mod+Shift+Arrow
            (
                KeyCombo {
                    modifiers: m_shift.clone(),
                    sym: Keysym::from(keysyms::KEY_Up),
                },
                Action::NudgeWindow(Direction::Up),
            ),
            (
                KeyCombo {
                    modifiers: m_shift.clone(),
                    sym: Keysym::from(keysyms::KEY_Down),
                },
                Action::NudgeWindow(Direction::Down),
            ),
            (
                KeyCombo {
                    modifiers: m_shift.clone(),
                    sym: Keysym::from(keysyms::KEY_Left),
                },
                Action::NudgeWindow(Direction::Left),
            ),
            (
                KeyCombo {
                    modifiers: m_shift,
                    sym: Keysym::from(keysyms::KEY_Right),
                },
                Action::NudgeWindow(Direction::Right),
            ),
            // Viewport panning: Mod+Ctrl+Arrow
            (
                KeyCombo {
                    modifiers: m_ctrl.clone(),
                    sym: Keysym::from(keysyms::KEY_Up),
                },
                Action::PanViewport(Direction::Up),
            ),
            (
                KeyCombo {
                    modifiers: m_ctrl.clone(),
                    sym: Keysym::from(keysyms::KEY_Down),
                },
                Action::PanViewport(Direction::Down),
            ),
            (
                KeyCombo {
                    modifiers: m_ctrl.clone(),
                    sym: Keysym::from(keysyms::KEY_Left),
                },
                Action::PanViewport(Direction::Left),
            ),
            (
                KeyCombo {
                    modifiers: m_ctrl,
                    sym: Keysym::from(keysyms::KEY_Right),
                },
                Action::PanViewport(Direction::Right),
            ),
            // Home toggle: Mod+a
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_a),
                },
                Action::HomeToggle,
            ),
            // Center focused window: Mod+c
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_c),
                },
                Action::CenterWindow,
            ),
            // Navigate to nearest window: Mod+Arrow
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_Up),
                },
                Action::CenterNearest(Direction::Up),
            ),
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_Down),
                },
                Action::CenterNearest(Direction::Down),
            ),
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_Left),
                },
                Action::CenterNearest(Direction::Left),
            ),
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_Right),
                },
                Action::CenterNearest(Direction::Right),
            ),
            // Window cycling: CycleMod+Tab / CycleMod+Shift+Tab
            (
                KeyCombo {
                    modifiers: cyc,
                    sym: Keysym::from(keysyms::KEY_Tab),
                },
                Action::CycleWindows { backward: false },
            ),
            (
                KeyCombo {
                    modifiers: cyc_shift,
                    sym: Keysym::from(keysyms::KEY_ISO_Left_Tab),
                },
                Action::CycleWindows { backward: true },
            ),
            // Zoom controls
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_equal),
                },
                Action::ZoomIn,
            ),
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_minus),
                },
                Action::ZoomOut,
            ),
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_0),
                },
                Action::ZoomReset,
            ),
            (
                KeyCombo {
                    modifiers: m2.clone(),
                    sym: Keysym::from(keysyms::KEY_w),
                },
                Action::ZoomToFit,
            ),
            // Fullscreen: Mod+f
            (
                KeyCombo {
                    modifiers: m2,
                    sym: Keysym::from(keysyms::KEY_f),
                },
                Action::ToggleFullscreen,
            ),
        ]);

        Self {
            mod_key,
            scroll_speed: 1.5,
            friction: 0.96,
            nudge_step: 20,
            pan_step: 100.0,
            repeat_delay: 200,
            repeat_rate: 25,
            edge_zone: 100.0,
            edge_pan_min: 4.0,
            edge_pan_max: 30.0,
            animation_speed: 0.3,
            cycle_modifier,
            zoom_step: 1.1,
            zoom_fit_padding: 100.0,
            background: BackgroundConfig::default(),
            bindings,
        }
    }
}

fn detect_launcher() -> String {
    if let Ok(launcher) = std::env::var("LAUNCHER")
        && !launcher.is_empty()
    {
        return launcher;
    }
    for cmd in ["fuzzel", "wofi", "bemenu-run", "tofi"] {
        if std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            return cmd.to_string();
        }
    }
    "fuzzel".to_string()
}

fn detect_terminal() -> String {
    if let Ok(term) = std::env::var("TERMINAL")
        && !term.is_empty()
    {
        return term;
    }
    for cmd in ["foot", "alacritty", "ptyxis", "kitty", "wezterm"] {
        if std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            return cmd.to_string();
        }
    }
    "foot".to_string()
}
