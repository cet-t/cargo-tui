use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use std::path::PathBuf;

// ── KeyBind ───────────────────────────────────────────────────

/// A single key binding: a key code plus optional modifiers.
#[derive(Clone)]
pub struct KeyBind {
    pub code:      KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    pub fn char(c: char) -> Self {
        Self { code: KeyCode::Char(c), modifiers: KeyModifiers::NONE }
    }

    pub fn code(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::NONE }
    }

    /// Returns true when `key` matches this binding.
    /// For Char keys, shift is already encoded in the character itself,
    /// so only ctrl/alt modifiers are checked.
    pub fn matches(&self, key: &KeyEvent) -> bool {
        if key.code != self.code { return false; }
        if matches!(self.code, KeyCode::Char(_)) {
            let mask = KeyModifiers::CONTROL | KeyModifiers::ALT;
            (key.modifiers & mask) == (self.modifiers & mask)
        } else {
            key.modifiers == self.modifiers
        }
    }

    /// Parse a human-readable key string such as `"q"`, `"K"`, `"ctrl+c"`, `"enter"`.
    fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').collect();
        let key_str = parts.last().ok_or_else(|| "empty key string".to_string())?;
        let mut modifiers = KeyModifiers::NONE;
        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl"  => modifiers |= KeyModifiers::CONTROL,
                "alt"   => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                other   => return Err(format!("unknown modifier: {other}")),
            }
        }
        let code = if key_str.chars().count() == 1 {
            // Single character — preserve original case.
            KeyCode::Char(key_str.chars().next().unwrap())
        } else {
            match key_str.to_lowercase().as_str() {
                "enter"     => KeyCode::Enter,
                "esc"       => KeyCode::Esc,
                "backspace" => KeyCode::Backspace,
                "delete"    => KeyCode::Delete,
                "up"        => KeyCode::Up,
                "down"      => KeyCode::Down,
                "left"      => KeyCode::Left,
                "right"     => KeyCode::Right,
                "tab"       => KeyCode::Tab,
                "space"     => KeyCode::Char(' '),
                "f1"        => KeyCode::F(1),
                "f2"        => KeyCode::F(2),
                "f3"        => KeyCode::F(3),
                "f4"        => KeyCode::F(4),
                "f5"        => KeyCode::F(5),
                "f6"        => KeyCode::F(6),
                "f7"        => KeyCode::F(7),
                "f8"        => KeyCode::F(8),
                "f9"        => KeyCode::F(9),
                "f10"       => KeyCode::F(10),
                "f11"       => KeyCode::F(11),
                "f12"       => KeyCode::F(12),
                other       => return Err(format!("unknown key: {other}")),
            }
        };
        Ok(Self { code, modifiers })
    }
}

impl Default for KeyBind {
    fn default() -> Self { Self::char('\0') }
}

impl<'de> Deserialize<'de> for KeyBind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        KeyBind::parse(&s).map_err(serde::de::Error::custom)
    }
}

// ── KeyConfig ─────────────────────────────────────────────────

/// All configurable key bindings.
/// Arrow keys (Up/Down/Left/Right) always work in addition to the configured keys.
#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct KeyConfig {
    /// Quit the application
    pub quit:       KeyBind,
    /// Switch to Build/Run tab
    pub tab_1:      KeyBind,
    /// Switch to Package tab
    pub tab_2:      KeyBind,
    /// Switch to Test tab
    pub tab_3:      KeyBind,
    /// Cycle to the next tab
    pub tab_next:   KeyBind,
    /// Cycle to the previous tab
    pub tab_prev:   KeyBind,
    /// Move selection down
    pub down:       KeyBind,
    /// Move selection up
    pub up:         KeyBind,
    /// Run the selected command / add the selected crate
    pub run:        KeyBind,
    /// Re-run the last command
    pub rerun:      KeyBind,
    /// Kill the running process
    pub kill:       KeyBind,
    /// Open inline search (Package tab)
    pub pkg_search: KeyBind,
    /// Remove the selected installed crate (Package tab)
    pub pkg_remove: KeyBind,
    /// Toggle between Installed / Search sections (Package tab)
    pub pkg_toggle: KeyBind,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            quit:       KeyBind::char('q'),
            tab_1:      KeyBind::char('1'),
            tab_2:      KeyBind::char('2'),
            tab_3:      KeyBind::char('3'),
            tab_next:   KeyBind::char(']'),
            tab_prev:   KeyBind::char('['),
            down:       KeyBind::char('j'),
            up:         KeyBind::char('k'),
            run:        KeyBind::code(KeyCode::Enter),
            rerun:      KeyBind::char('r'),
            kill:       KeyBind::char('K'),
            pkg_search: KeyBind::char('s'),
            pkg_remove: KeyBind::char('d'),
            pkg_toggle: KeyBind::code(KeyCode::Tab),
        }
    }
}

// ── Config ────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub keys: KeyConfig,
}

/// Load config from `path` if given, otherwise from the platform default location.
/// Falls back to `Config::default()` if the file is missing or invalid.
pub fn load(path: Option<&std::path::Path>) -> Config {
    let resolved = path
        .map(|p| p.to_path_buf())
        .or_else(default_config_path);

    resolved
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn default_config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
        Some(base.join("cargo-tui").join("config.toml"))
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        Some(home.join(".config").join("cargo-tui").join("config.toml"))
    }
}
