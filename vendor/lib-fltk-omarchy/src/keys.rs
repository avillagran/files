//! Human-readable shortcuts: "Ctrl+S", "Ctrl+Shift+P", "Alt+Enter", "F5", "Escape".

use fltk::{app, enums::{Key, Shortcut}};

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
            "alt" => Shortcut::Alt,
            "super" | "meta" | "cmd" => Shortcut::Meta,
            other => return Err(format!("unknown modifier {other:?} in {spec:?} (use Ctrl, Shift, Alt, Super)")),
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
}
