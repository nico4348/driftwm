use driftwm::config::{Action, Config, Direction};
use smithay::input::keyboard::{Keysym, ModifiersState, keysyms};

/// Build a ModifiersState with only the specified flags set.
fn mods(alt: bool, ctrl: bool, shift: bool, logo: bool) -> ModifiersState {
    ModifiersState {
        alt,
        ctrl,
        shift,
        logo,
        ..ModifiersState::default()
    }
}

fn alt() -> ModifiersState {
    mods(true, false, false, false)
}

fn alt_shift() -> ModifiersState {
    mods(true, false, true, false)
}

fn alt_ctrl() -> ModifiersState {
    mods(true, true, false, false)
}

fn no_mods() -> ModifiersState {
    ModifiersState::default()
}

// --- Alt+Return → SpawnCommand ---

#[test]
fn alt_return_resolves_to_spawn_command() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_Return));
    assert!(result.is_some(), "Alt+Return should be bound");
    assert!(
        matches!(result.unwrap(), Action::SpawnCommand(_)),
        "Alt+Return should resolve to SpawnCommand"
    );
}

// --- Alt+q → CloseWindow ---

#[test]
fn alt_q_resolves_to_close_window() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_q));
    assert!(result.is_some(), "Alt+q should be bound");
    assert!(
        matches!(result.unwrap(), Action::CloseWindow),
        "Alt+q should resolve to CloseWindow"
    );
}

// --- Alt+Shift+Arrow → NudgeWindow ---

#[test]
fn alt_shift_up_resolves_to_nudge_window_up() {
    let config = Config::default();
    let result = config.lookup(&alt_shift(), Keysym::from(keysyms::KEY_Up));
    assert!(result.is_some(), "Alt+Shift+Up should be bound");
    assert!(
        matches!(result.unwrap(), Action::NudgeWindow(Direction::Up)),
        "Alt+Shift+Up should resolve to NudgeWindow(Up)"
    );
}

#[test]
fn alt_shift_down_resolves_to_nudge_window_down() {
    let config = Config::default();
    let result = config.lookup(&alt_shift(), Keysym::from(keysyms::KEY_Down));
    assert!(
        matches!(result.unwrap(), Action::NudgeWindow(Direction::Down)),
        "Alt+Shift+Down should resolve to NudgeWindow(Down)"
    );
}

#[test]
fn alt_shift_left_resolves_to_nudge_window_left() {
    let config = Config::default();
    let result = config.lookup(&alt_shift(), Keysym::from(keysyms::KEY_Left));
    assert!(
        matches!(result.unwrap(), Action::NudgeWindow(Direction::Left)),
        "Alt+Shift+Left should resolve to NudgeWindow(Left)"
    );
}

#[test]
fn alt_shift_right_resolves_to_nudge_window_right() {
    let config = Config::default();
    let result = config.lookup(&alt_shift(), Keysym::from(keysyms::KEY_Right));
    assert!(
        matches!(result.unwrap(), Action::NudgeWindow(Direction::Right)),
        "Alt+Shift+Right should resolve to NudgeWindow(Right)"
    );
}

// --- Alt+Ctrl+Arrow → PanViewport ---

#[test]
fn alt_ctrl_left_resolves_to_pan_viewport_left() {
    let config = Config::default();
    let result = config.lookup(&alt_ctrl(), Keysym::from(keysyms::KEY_Left));
    assert!(result.is_some(), "Alt+Ctrl+Left should be bound");
    assert!(
        matches!(result.unwrap(), Action::PanViewport(Direction::Left)),
        "Alt+Ctrl+Left should resolve to PanViewport(Left)"
    );
}

#[test]
fn alt_ctrl_right_resolves_to_pan_viewport_right() {
    let config = Config::default();
    let result = config.lookup(&alt_ctrl(), Keysym::from(keysyms::KEY_Right));
    assert!(
        matches!(result.unwrap(), Action::PanViewport(Direction::Right)),
        "Alt+Ctrl+Right should resolve to PanViewport(Right)"
    );
}

#[test]
fn alt_ctrl_up_resolves_to_pan_viewport_up() {
    let config = Config::default();
    let result = config.lookup(&alt_ctrl(), Keysym::from(keysyms::KEY_Up));
    assert!(
        matches!(result.unwrap(), Action::PanViewport(Direction::Up)),
        "Alt+Ctrl+Up should resolve to PanViewport(Up)"
    );
}

#[test]
fn alt_ctrl_down_resolves_to_pan_viewport_down() {
    let config = Config::default();
    let result = config.lookup(&alt_ctrl(), Keysym::from(keysyms::KEY_Down));
    assert!(
        matches!(result.unwrap(), Action::PanViewport(Direction::Down)),
        "Alt+Ctrl+Down should resolve to PanViewport(Down)"
    );
}

// --- Unbound / wrong modifier → None ---

#[test]
fn unbound_key_returns_none() {
    let config = Config::default();
    let result = config.lookup(&no_mods(), Keysym::from(keysyms::KEY_a));
    assert!(
        result.is_none(),
        "bare 'a' with no modifiers should not be bound"
    );
}

#[test]
fn ctrl_return_returns_none_when_only_alt_return_is_bound() {
    let config = Config::default();
    let ctrl_only = mods(false, true, false, false);
    let result = config.lookup(&ctrl_only, Keysym::from(keysyms::KEY_Return));
    assert!(
        result.is_none(),
        "Ctrl+Return should not be bound (only Alt+Return is)"
    );
}

#[test]
fn bare_return_returns_none() {
    let config = Config::default();
    let result = config.lookup(&no_mods(), Keysym::from(keysyms::KEY_Return));
    assert!(
        result.is_none(),
        "Return without modifiers should not be bound"
    );
}

#[test]
fn alt_shift_return_returns_none() {
    let config = Config::default();
    // Alt+Shift+Return is not explicitly bound — only Alt+Return and Alt+Shift+Arrows are
    let result = config.lookup(&alt_shift(), Keysym::from(keysyms::KEY_Return));
    assert!(result.is_none(), "Alt+Shift+Return should not be bound");
}

// --- Alt+a → HomeToggle ---

#[test]
fn alt_a_resolves_to_home_toggle() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_a));
    assert!(result.is_some(), "Alt+a should be bound");
    assert!(
        matches!(result.unwrap(), Action::HomeToggle),
        "Alt+a should resolve to HomeToggle"
    );
}

// --- Alt+c → CenterWindow ---

#[test]
fn alt_c_resolves_to_center_window() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_c));
    assert!(result.is_some(), "Alt+c should be bound");
    assert!(
        matches!(result.unwrap(), Action::CenterWindow),
        "Alt+c should resolve to CenterWindow"
    );
}

#[test]
fn alt_c_does_not_conflict_with_existing_bindings() {
    let config = Config::default();
    // Alt+c must be CenterWindow, not CloseWindow or anything else
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_c));
    assert!(
        !matches!(result, Some(Action::CloseWindow)),
        "Alt+c must not resolve to CloseWindow (that is Alt+q)"
    );
}

// --- Alt+Arrow → CenterNearest ---

#[test]
fn alt_up_resolves_to_center_nearest_up() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_Up));
    assert!(result.is_some(), "Alt+Up should be bound");
    assert!(
        matches!(result.unwrap(), Action::CenterNearest(Direction::Up)),
        "Alt+Up should resolve to CenterNearest(Up)"
    );
}

#[test]
fn alt_down_resolves_to_center_nearest_down() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_Down));
    assert!(result.is_some(), "Alt+Down should be bound");
    assert!(
        matches!(result.unwrap(), Action::CenterNearest(Direction::Down)),
        "Alt+Down should resolve to CenterNearest(Down)"
    );
}

#[test]
fn alt_left_resolves_to_center_nearest_left() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_Left));
    assert!(result.is_some(), "Alt+Left should be bound");
    assert!(
        matches!(result.unwrap(), Action::CenterNearest(Direction::Left)),
        "Alt+Left should resolve to CenterNearest(Left)"
    );
}

#[test]
fn alt_right_resolves_to_center_nearest_right() {
    let config = Config::default();
    let result = config.lookup(&alt(), Keysym::from(keysyms::KEY_Right));
    assert!(result.is_some(), "Alt+Right should be bound");
    assert!(
        matches!(result.unwrap(), Action::CenterNearest(Direction::Right)),
        "Alt+Right should resolve to CenterNearest(Right)"
    );
}

// --- Ctrl+Tab → CycleWindows ---

fn ctrl() -> ModifiersState {
    mods(false, true, false, false)
}

fn ctrl_shift() -> ModifiersState {
    mods(false, true, true, false)
}

#[test]
fn ctrl_tab_resolves_to_cycle_windows_forward() {
    let config = Config::default();
    let result = config.lookup(&ctrl(), Keysym::from(keysyms::KEY_Tab));
    assert!(result.is_some(), "Ctrl+Tab should be bound");
    assert!(
        matches!(result.unwrap(), Action::CycleWindows { backward: false }),
        "Ctrl+Tab should resolve to CycleWindows {{ backward: false }}"
    );
}

#[test]
fn ctrl_shift_tab_resolves_to_cycle_windows_backward() {
    let config = Config::default();
    let result = config.lookup(&ctrl_shift(), Keysym::from(keysyms::KEY_ISO_Left_Tab));
    assert!(result.is_some(), "Ctrl+Shift+Tab should be bound");
    assert!(
        matches!(result.unwrap(), Action::CycleWindows { backward: true }),
        "Ctrl+Shift+Tab should resolve to CycleWindows {{ backward: true }}"
    );
}
