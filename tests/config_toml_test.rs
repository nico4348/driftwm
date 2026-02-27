use driftwm::config::{Action, Config, MouseAction, BTN_RIGHT};
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

fn alt() -> ModifiersState {
    mods(true, false, false, false)
}

fn ctrl() -> ModifiersState {
    mods(false, true, false, false)
}

// ── TOML round-trip integration tests ─────────────────────────────────────

#[test]
fn empty_toml_produces_defaults() {
    let config = Config::from_toml("").unwrap();
    // mod_key defaults to Super
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_q));
    assert!(
        matches!(result, Some(Action::CloseWindow)),
        "empty config should use Super as mod_key"
    );
}

#[test]
fn toml_mod_key_alt_switches_all_bindings() {
    let config = Config::from_toml("mod_key = \"alt\"").unwrap();
    // Alt+q should now work (not Super+q)
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_q));
    assert!(
        matches!(result, Some(Action::CloseWindow)),
        "mod_key=alt should bind Alt+q to CloseWindow"
    );
    // Super+q should NOT be bound
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_q));
    assert!(result.is_none(), "Super+q should not be bound when mod_key=alt");
}

#[test]
fn toml_keybinding_override() {
    let toml = r#"
        [keybindings]
        "Mod+x" = "exec alacritty"
    "#;
    let config = Config::from_toml(toml).unwrap();
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_x));
    assert!(
        matches!(result, Some(Action::Exec(s)) if s == "alacritty"),
        "user binding Mod+x should resolve to exec alacritty"
    );
    // Default bindings should still be present
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_q));
    assert!(
        matches!(result, Some(Action::CloseWindow)),
        "default Mod+q should still work after adding Mod+x"
    );
}

#[test]
fn toml_keybinding_unbind_with_none() {
    let toml = r#"
        [keybindings]
        "Mod+q" = "none"
    "#;
    let config = Config::from_toml(toml).unwrap();
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_q));
    assert!(result.is_none(), "Mod+q should be unbound after setting to none");
    // Other bindings should still work
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_c));
    assert!(
        matches!(result, Some(Action::CenterWindow)),
        "Mod+c should still work after unbinding Mod+q"
    );
}

#[test]
fn toml_mouse_binding_override() {
    let toml = r#"
        [mouse]
        "Mod+Right" = "pan-viewport"
    "#;
    let config = Config::from_toml(toml).unwrap();
    let result = config.mouse_button_lookup(&logo(), BTN_RIGHT);
    assert!(
        matches!(result, Some(MouseAction::PanViewport)),
        "Mod+Right should resolve to PanViewport"
    );
}

#[test]
fn toml_mouse_binding_unbind_with_none() {
    let toml = r#"
        [mouse]
        "Mod+Scroll" = "none"
    "#;
    let config = Config::from_toml(toml).unwrap();
    let result = config.mouse_scroll_lookup(&logo());
    assert!(result.is_none(), "Mod+Scroll should be unbound after setting to none");
}

#[test]
fn toml_scalar_overrides() {
    let toml = r#"
        [input.scroll]
        speed = 2.5
        friction = 0.92

        [navigation]
        nudge_step = 50

        [zoom]
        step = 1.2
    "#;
    let config = Config::from_toml(toml).unwrap();
    assert!((config.scroll_speed - 2.5).abs() < f64::EPSILON);
    assert!((config.friction - 0.92).abs() < f64::EPSILON);
    assert_eq!(config.nudge_step, 50);
    assert!((config.zoom_step - 1.2).abs() < f64::EPSILON);
}

#[test]
fn toml_invalid_keybinding_is_skipped() {
    let toml = r#"
        [keybindings]
        "Mod+nonexistent_key_xyz" = "close-window"
        "Mod+c" = "center-window"
    "#;
    let config = Config::from_toml(toml).unwrap();
    // Valid binding should still work
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_c));
    assert!(matches!(result, Some(Action::CenterWindow)));
}

#[test]
fn toml_invalid_action_is_skipped() {
    let toml = r#"
        [keybindings]
        "Mod+x" = "not-a-real-action"
        "Mod+c" = "center-window"
    "#;
    let config = Config::from_toml(toml).unwrap();
    // The invalid action binding should be skipped
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_x));
    assert!(result.is_none());
    // Valid binding should still work
    let result = config.lookup(&logo(), Keysym::from(keysyms::KEY_c));
    assert!(matches!(result, Some(Action::CenterWindow)));
}

#[test]
fn toml_deny_unknown_fields() {
    let toml = "typo_field = \"oops\"";
    let result = Config::from_toml(toml);
    assert!(result.is_err(), "unknown top-level field should be rejected");
}

#[test]
fn toml_cycle_modifier_ctrl() {
    let config = Config::from_toml("cycle_modifier = \"ctrl\"").unwrap();
    // Cycle bindings should now use Ctrl
    let result = config.lookup(&ctrl(), Keysym::from(keysyms::KEY_Tab));
    assert!(
        matches!(result, Some(Action::CycleWindows { backward: false })),
        "cycle_modifier=ctrl should bind Ctrl+Tab"
    );
    // Alt+Tab should no longer be bound for cycling
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_Tab));
    assert!(result.is_none(), "Alt+Tab should not be bound when cycle_modifier=ctrl");
}

#[test]
fn toml_background_tilde_expansion() {
    let toml = r#"
        [background]
        shader_path = "~/shaders/bg.frag"
    "#;
    let config = Config::from_toml(toml).unwrap();
    if let Some(ref path) = config.background.shader_path {
        assert!(!path.starts_with("~"), "tilde should be expanded");
    }
}
