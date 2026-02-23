use std::collections::HashMap;

use smithay::input::keyboard::{ModifiersState, Keysym, keysyms};

#[derive(Clone, Debug)]
pub enum Action {
    SpawnCommand(String),
    CloseWindow,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl Modifiers {
    pub fn alt() -> Self {
        Self { alt: true, ..Default::default() }
    }

    #[allow(dead_code)]
    pub fn logo() -> Self {
        Self { logo: true, ..Default::default() }
    }

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

pub struct Config {
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
        let terminal = detect_terminal();
        tracing::info!("Terminal command: {terminal}");

        let bindings = HashMap::from([
            (
                KeyCombo { modifiers: Modifiers::alt(), sym: Keysym::from(keysyms::KEY_Return) },
                Action::SpawnCommand(terminal),
            ),
            (
                KeyCombo { modifiers: Modifiers::alt(), sym: Keysym::from(keysyms::KEY_q) },
                Action::CloseWindow,
            ),
        ]);

        Self { bindings }
    }
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
