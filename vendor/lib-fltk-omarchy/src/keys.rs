//! Human-readable shortcuts: "Ctrl+S", "Ctrl+Shift+P", "Alt+Enter", "F5", "Escape".
//!
//! **Portable shortcuts**: write `Mod` (also `Primary`, `CmdOrCtrl`) for
//! the platform's primary modifier: Cmd on macOS, Ctrl elsewhere.
//! `"Mod+,"` is Preferences everywhere (`Ctrl+,` on Linux and Windows,
//! `Cmd+,` on macOS). `Ctrl` always means the Control key. [`label`] shows
//! a spec the way the platform writes it ("Ctrl+," / "⌘,").

use crate::platform::{Os, OS};
use fltk::{app, enums::{Key, Shortcut}};

/// The platform's primary modifier: Cmd (`Shortcut::Meta`) on macOS, Ctrl
/// elsewhere. Copy, select all, toggle-in-selection and app shortcuts use it.
pub const PRIMARY: Shortcut = if matches!(OS, Os::MacOs) { Shortcut::Meta } else { Shortcut::Ctrl };

/// True when the primary modifier is held in the current event.
pub fn primary_held() -> bool {
    app::event_state().contains(PRIMARY)
}

/// True when `state` includes the primary modifier.
pub fn is_primary(state: Shortcut) -> bool {
    state.contains(PRIMARY)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    pub mods: Shortcut,
    pub key: Key,
}

pub fn parse(spec: &str) -> Result<Keys, String> {
    let mut parts: Vec<&str> = spec.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let key = parts.pop().ok_or_else(|| format!("empty shortcut {spec:?}"))?;
    let mut mods = Shortcut::None;
    for m in parts {
        mods |= match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Shortcut::Ctrl,
            "shift" => Shortcut::Shift,
            "alt" | "option" | "opt" => Shortcut::Alt,
            "super" | "meta" | "cmd" | "command" => Shortcut::Meta,
            "mod" | "primary" | "cmdorctrl" | "ctrlorcmd" | "commandorcontrol" => PRIMARY,
            other => return Err(format!("unknown modifier {other:?} in {spec:?} (use Mod, Ctrl, Shift, Alt, Super)")),
        };
    }
    let lower = key.to_ascii_lowercase();
    let key = match lower.as_str() {
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "backspace" => Key::BackSpace,
        "delete" | "del" => Key::Delete,
        "space" => Key::from_char(' '),
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        f if f.len() >= 2 && f.starts_with('f') && f[1..].parse::<i32>().is_ok_and(|n| (1..=12).contains(&n)) => {
            Key::from_i32(Key::F1.bits() + f[1..].parse::<i32>().unwrap() - 1)
        }
        c if c.chars().count() == 1 => Key::from_char(c.chars().next().unwrap()),
        other => return Err(format!("unknown key {other:?} in {spec:?}")),
    };
    Ok(Keys { mods, key })
}

impl Keys {
    /// True when the current FLTK event is this key with exactly these modifiers.
    pub fn matches_event(&self) -> bool {
        self.matches(app::event_key(), app::event_state())
    }

    pub fn matches(&self, key: Key, state: Shortcut) -> bool {
        let mask = Shortcut::Ctrl | Shortcut::Shift | Shortcut::Alt | Shortcut::Meta;
        let key = if (Key::from_char('A').bits()..=Key::from_char('Z').bits()).contains(&key.bits()) {
            Key::from_i32(key.bits() + 32)
        } else {
            key
        };
        (state & mask) == self.mods && key == self.key
    }
}

/// `spec` as this platform writes shortcuts, for menus and the F1 help:
/// `Mod` becomes Ctrl (Cmd on macOS); macOS uses its symbols in its order
/// (⌃⌥⇧⌘, "⌘,", "⇧⌘P"). Text that is not a shortcut ("Up/Down", "Enter")
/// is returned as is. On Linux and Windows every spec without `Mod` is
/// unchanged.
pub fn label(spec: &str) -> String {
    let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return spec.to_string();
    }
    let (mods, key) = parts.split_at(parts.len() - 1);
    let mut set = Shortcut::None;
    for m in mods {
        set |= match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Shortcut::Ctrl,
            "shift" => Shortcut::Shift,
            "alt" | "option" | "opt" => Shortcut::Alt,
            "super" | "meta" | "cmd" | "command" => Shortcut::Meta,
            "mod" | "primary" | "cmdorctrl" | "ctrlorcmd" | "commandorcontrol" => PRIMARY,
            _ => return spec.to_string(),
        };
    }
    let key = key[0];
    if OS == Os::MacOs {
        let mut out = String::new();
        for (m, sym) in [(Shortcut::Ctrl, "⌃"), (Shortcut::Alt, "⌥"), (Shortcut::Shift, "⇧"), (Shortcut::Meta, "⌘")] {
            if set.contains(m) {
                out.push_str(sym);
            }
        }
        out.push_str(&upper_letter(key));
        return out;
    }
    if !mods.iter().any(|m| matches!(m.to_ascii_lowercase().as_str(), "mod" | "primary" | "cmdorctrl" | "ctrlorcmd" | "commandorcontrol")) {
        return spec.to_string(); // unchanged: no portable modifier in it
    }
    let mut out: Vec<&str> = Vec::new();
    let meta = if OS == Os::Windows { "Win" } else { "Super" };
    for (m, name) in [(Shortcut::Ctrl, "Ctrl"), (Shortcut::Alt, "Alt"), (Shortcut::Shift, "Shift"), (Shortcut::Meta, meta)] {
        if set.contains(m) {
            out.push(name);
        }
    }
    out.push(key);
    out.join("+")
}

/// "p" → "P" (macOS menus show letter keys in capitals); other keys as is.
fn upper_letter(key: &str) -> String {
    let mut c = key.chars();
    match (c.next(), c.next()) {
        (Some(ch), None) if ch.is_alphabetic() => ch.to_uppercase().collect(),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shortcuts() {
        let k = parse("Ctrl+Shift+P").unwrap();
        assert_eq!(k.mods, Shortcut::Ctrl | Shortcut::Shift);
        assert_eq!(k.key, Key::from_char('p'));
        assert!(k.matches(Key::from_char('p'), Shortcut::Ctrl | Shortcut::Shift));
        assert!(!k.matches(Key::from_char('p'), Shortcut::Ctrl));
        assert_eq!(parse("F5").unwrap().key, Key::F5);
        assert!(parse("Hyper+X").is_err());
    }

    #[test]
    fn primary_modifier_is_portable() {
        let k = parse("Mod+,").unwrap();
        assert_eq!(k.mods, PRIMARY);
        assert_eq!(parse("CmdOrCtrl+Shift+P").unwrap().mods, PRIMARY | Shortcut::Shift);
        assert!(is_primary(PRIMARY | Shortcut::Shift));
        match OS {
            Os::MacOs => {
                assert_eq!(PRIMARY, Shortcut::Meta);
                assert_eq!(label("Mod+,"), "⌘,");
                assert_eq!(label("Mod+Shift+p"), "⇧⌘P");
                assert_eq!(label("Ctrl+Tab"), "⌃Tab");
            }
            _ => {
                assert_eq!(PRIMARY, Shortcut::Ctrl);
                assert_eq!(label("Mod+,"), "Ctrl+,");
                assert_eq!(label("Mod+Shift+P"), "Ctrl+Shift+P");
                // Specs without Mod are shown exactly as written.
                assert_eq!(label("Ctrl+Tab"), "Ctrl+Tab");
                assert_eq!(label("Shift+Tab"), "Shift+Tab");
            }
        }
        assert_eq!(label("Up/Down"), "Up/Down");
        assert_eq!(label("F1"), "F1");
        assert_eq!(label("Ctrl++"), "Ctrl++");
    }
}
