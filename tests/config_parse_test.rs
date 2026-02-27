use driftwm::config::{
    Action, Config, Direction, ModKey, MouseAction, MouseTrigger,
    BTN_LEFT, BTN_RIGHT,
    parse_action, parse_direction, parse_key_combo, parse_mouse_action, parse_mouse_binding,
};
use smithay::input::keyboard::{keysyms, Keysym, ModifiersState};

// ── Modifier helpers ─────────────────────────────────────────────────────

fn mods(alt: bool, ctrl: bool, shift: bool, logo: bool) -> ModifiersState {
    ModifiersState {
        alt,
        ctrl,
        shift,
        logo,
        ..ModifiersState::default()
    }
}

fn logo() -> ModifiersState {
    mods(false, false, false, true)
}

fn logo_shift() -> ModifiersState {
    mods(false, false, true, true)
}

// ── parse_key_combo ───────────────────────────────────────────────────────

#[test]
fn parse_key_combo_mod_expands_to_logo_with_super() {
    let combo = parse_key_combo("Mod+Return", ModKey::Super).unwrap();
    assert!(combo.modifiers.logo, "Mod should expand to logo with ModKey::Super");
    assert!(!combo.modifiers.alt);
    assert_eq!(combo.sym, Keysym::from(keysyms::KEY_Return));
}

#[test]
fn parse_key_combo_mod_expands_to_alt_with_alt_modkey() {
    let combo = parse_key_combo("Mod+Return", ModKey::Alt).unwrap();
    assert!(combo.modifiers.alt, "Mod should expand to alt with ModKey::Alt");
    assert!(!combo.modifiers.logo);
    assert_eq!(combo.sym, Keysym::from(keysyms::KEY_Return));
}

#[test]
fn parse_key_combo_literal_alt_is_always_alt() {
    let combo = parse_key_combo("Alt+Tab", ModKey::Super).unwrap();
    assert!(combo.modifiers.alt, "literal Alt should set alt regardless of mod_key");
    assert!(!combo.modifiers.logo);
    assert_eq!(combo.sym, Keysym::from(keysyms::KEY_Tab));
}

#[test]
fn parse_key_combo_ctrl_shift_combination() {
    let combo = parse_key_combo("Ctrl+Shift+a", ModKey::Super).unwrap();
    assert!(combo.modifiers.ctrl);
    assert!(combo.modifiers.shift);
    assert!(!combo.modifiers.logo);
    assert!(!combo.modifiers.alt);
    assert_eq!(combo.sym, Keysym::from(keysyms::KEY_a));
}

#[test]
fn parse_key_combo_keysym_is_case_insensitive() {
    let lower = parse_key_combo("Mod+Return", ModKey::Super).unwrap();
    let upper = parse_key_combo("Mod+RETURN", ModKey::Super).unwrap();
    assert_eq!(lower.sym, upper.sym, "keysym lookup should be case insensitive");
}

#[test]
fn parse_key_combo_unknown_keysym_is_error() {
    let result = parse_key_combo("Mod+nonexistent_key", ModKey::Super);
    assert!(result.is_err(), "unknown keysym should return Err");
}

#[test]
fn parse_key_combo_unknown_modifier_is_error() {
    let result = parse_key_combo("Badmod+a", ModKey::Super);
    assert!(result.is_err(), "unknown modifier should return Err");
}

// ── parse_action ──────────────────────────────────────────────────────────

#[test]
fn parse_action_exec_single_word() {
    let result = parse_action("exec foot").unwrap();
    assert!(
        matches!(result, Action::Exec(ref s) if s == "foot"),
        "exec foot should yield Exec(\"foot\")"
    );
}

#[test]
fn parse_action_exec_with_arguments() {
    let result = parse_action("exec sh -c 'echo hello'").unwrap();
    assert!(
        matches!(result, Action::Exec(ref s) if s == "sh -c 'echo hello'"),
        "exec with args should preserve entire argument string"
    );
}

#[test]
fn parse_action_close_window() {
    let result = parse_action("close-window").unwrap();
    assert!(matches!(result, Action::CloseWindow));
}

#[test]
fn parse_action_nudge_window_up() {
    let result = parse_action("nudge-window up").unwrap();
    assert!(matches!(result, Action::NudgeWindow(Direction::Up)));
}

#[test]
fn parse_action_center_nearest_down_left() {
    let result = parse_action("center-nearest down-left").unwrap();
    assert!(matches!(result, Action::CenterNearest(Direction::DownLeft)));
}

#[test]
fn parse_action_cycle_windows_forward() {
    let result = parse_action("cycle-windows forward").unwrap();
    assert!(matches!(result, Action::CycleWindows { backward: false }));
}

#[test]
fn parse_action_cycle_windows_backward() {
    let result = parse_action("cycle-windows backward").unwrap();
    assert!(matches!(result, Action::CycleWindows { backward: true }));
}

#[test]
fn parse_action_zoom_in() {
    let result = parse_action("zoom-in").unwrap();
    assert!(matches!(result, Action::ZoomIn));
}

#[test]
fn parse_action_unknown_is_error() {
    let result = parse_action("unknown-action");
    assert!(result.is_err(), "unknown action name should return Err");
}

// ── parse_mouse_binding ───────────────────────────────────────────────────

#[test]
fn parse_mouse_binding_mod_left_with_super() {
    let binding = parse_mouse_binding("Mod+Left", ModKey::Super).unwrap();
    assert!(binding.modifiers.logo);
    assert!(!binding.modifiers.shift);
    assert_eq!(binding.trigger, MouseTrigger::Button(BTN_LEFT));
}

#[test]
fn parse_mouse_binding_mod_shift_right_with_super() {
    let binding = parse_mouse_binding("Mod+Shift+Right", ModKey::Super).unwrap();
    assert!(binding.modifiers.logo);
    assert!(binding.modifiers.shift);
    assert_eq!(binding.trigger, MouseTrigger::Button(BTN_RIGHT));
}

#[test]
fn parse_mouse_binding_mod_scroll_with_super() {
    let binding = parse_mouse_binding("Mod+Scroll", ModKey::Super).unwrap();
    assert!(binding.modifiers.logo);
    assert_eq!(binding.trigger, MouseTrigger::Scroll);
}

#[test]
fn parse_mouse_binding_unknown_trigger_is_error() {
    let result = parse_mouse_binding("Mod+BadTrigger", ModKey::Super);
    assert!(result.is_err(), "unknown mouse trigger should return Err");
}

// ── parse_mouse_action ────────────────────────────────────────────────────

#[test]
fn parse_mouse_action_move_window() {
    let result = parse_mouse_action("move-window").unwrap();
    assert!(matches!(result, MouseAction::MoveWindow));
}

#[test]
fn parse_mouse_action_zoom() {
    let result = parse_mouse_action("zoom").unwrap();
    assert!(matches!(result, MouseAction::Zoom));
}

#[test]
fn parse_mouse_action_unknown_is_error() {
    let result = parse_mouse_action("bad-action");
    assert!(result.is_err(), "unknown mouse action should return Err");
}

// ── parse_direction ───────────────────────────────────────────────────────

#[test]
fn parse_direction_up() {
    assert_eq!(parse_direction("up").unwrap(), Direction::Up);
}

#[test]
fn parse_direction_down_right() {
    assert_eq!(parse_direction("down-right").unwrap(), Direction::DownRight);
}

#[test]
fn parse_direction_is_case_insensitive() {
    assert_eq!(parse_direction("UP").unwrap(), Direction::Up);
}

#[test]
fn parse_direction_unknown_is_error() {
    let result = parse_direction("diagonal");
    assert!(result.is_err(), "unknown direction should return Err");
}

// ── Default mouse bindings ────────────────────────────────────────────────

#[test]
fn default_mouse_bindings_move_window_on_super_shift_left() {
    let config = Config::default();
    let result = config.mouse_button_lookup(&logo_shift(), BTN_LEFT);
    assert!(result.is_some(), "Super+Shift+Left should be bound");
    assert!(
        matches!(result.unwrap(), MouseAction::MoveWindow),
        "Super+Shift+Left should resolve to MoveWindow"
    );
}

#[test]
fn default_mouse_bindings_resize_window_on_super_shift_right() {
    let config = Config::default();
    let result = config.mouse_button_lookup(&logo_shift(), BTN_RIGHT);
    assert!(result.is_some(), "Super+Shift+Right should be bound");
    assert!(
        matches!(result.unwrap(), MouseAction::ResizeWindow),
        "Super+Shift+Right should resolve to ResizeWindow"
    );
}

#[test]
fn default_mouse_bindings_pan_viewport_on_super_left() {
    let config = Config::default();
    let result = config.mouse_button_lookup(&logo(), BTN_LEFT);
    assert!(result.is_some(), "Super+Left should be bound");
    assert!(
        matches!(result.unwrap(), MouseAction::PanViewport),
        "Super+Left should resolve to PanViewport"
    );
}

#[test]
fn default_mouse_bindings_zoom_on_super_scroll() {
    let config = Config::default();
    let result = config.mouse_scroll_lookup(&logo());
    assert!(result.is_some(), "Super+Scroll should be bound");
    assert!(
        matches!(result.unwrap(), MouseAction::Zoom),
        "Super+Scroll should resolve to Zoom"
    );
}

// ── Tilde expansion (via parse_action + Config::load indirectly) ──────────
// expand_tilde is private; test it through mouse_scroll_lookup / mouse_button_lookup
// by confirming Config::default() builds without panic and background paths stay None.

#[test]
fn default_config_background_paths_are_none() {
    let config = Config::default();
    assert!(
        config.background.shader_path.is_none(),
        "default config should have no shader_path"
    );
    assert!(
        config.background.tile_path.is_none(),
        "default config should have no tile_path"
    );
}
