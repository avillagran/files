use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);
impl Color {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#')?;
        if s.len() != 6 || !s.is_ascii() {
            return None;
        }
        Some(Self(
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        ))
    }
}
#[derive(Clone, Debug)]
pub struct Palette {
    pub background: Color,
    pub darker_background: Color,
    pub lighter_background: Color,
    pub selection: Color,
    pub muted: Color,
    pub foreground: Color,
    pub bright_foreground: Color,
    pub dark_foreground: Color,
    pub accent: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            background: Color(5, 24, 46),
            darker_background: Color(2, 12, 23),
            lighter_background: Color(10, 37, 64),
            selection: Color(19, 78, 90),
            muted: Color(42, 107, 120),
            foreground: Color(246, 220, 172),
            bright_foreground: Color(246, 220, 172),
            dark_foreground: Color(63, 143, 138),
            accent: Color(250, 169, 104),
            blue: Color(63, 143, 138),
            magenta: Color(63, 143, 138),
            cyan: Color(140, 191, 184),
            green: Color(2, 131, 145),
            yellow: Color(233, 123, 60),
            red: Color(248, 85, 37),
        }
    }
}
impl Palette {
    pub fn parse(text: &str) -> Result<Self, String> {
        let map: HashMap<String, toml::Value> = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut p = Self::default();
        macro_rules! fields {($($f:ident),*)=>{$(if let Some(v)=map.get(stringify!($f)){p.$f=Color::parse(v.as_str().ok_or("Color inválido")?).ok_or("Color inválido")?;})*}}
        fields!(
            background,
            darker_background,
            lighter_background,
            selection,
            muted,
            foreground,
            bright_foreground,
            dark_foreground,
            accent,
            blue,
            magenta,
            cyan,
            green,
            yellow,
            red
        );
        if !map.contains_key("background") || !map.contains_key("foreground") {
            return Err("Faltan background/foreground".into());
        }
        Ok(p)
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Corners {
    #[default]
    Omarchy,
    Square,
    Rounded,
}
/// Compositor-managed modes. No overlay, floating rule or panel role is implied.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    #[default]
    Normal,
    Fullscreen,
}
impl WindowMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Fullscreen => "fullscreen",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Fullscreen => "Pantalla completa",
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub corners: Corners,
    #[serde(default)]
    pub window_mode: WindowMode,
    /// Dot-prefixed names stay hidden until the user explicitly enables them.
    #[serde(default)]
    pub show_hidden: bool,
    /// Optional MC-style operation strip; compact browser remains the default.
    #[serde(default)]
    pub operation_bar: bool,
}
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
pub fn config_dir() -> PathBuf {
    // Isolate application fixtures without changing fontconfig's XDG resolution.
    if let Some(path) = std::env::var_os("FILES_CONFIG_DIR") {
        return path.into();
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("files")
}
pub fn theme_path() -> PathBuf {
    if let Some(p) = std::env::var_os("FILES_THEME") {
        return p.into();
    }
    let p = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("omarchy/current/theme/colors.toml");
    if p.exists() {
        p
    } else {
        config_dir().join("theme.toml")
    }
}
pub fn rounding(c: Corners) -> i32 {
    match c {
        Corners::Square => 0,
        Corners::Rounded => 1,
        Corners::Omarchy => std::process::Command::new("hyprctl")
            .args(["getoption", "decoration:rounding", "-j"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.split_once("\"int\"").map(|(_, r)| {
                    r.trim_start_matches([' ', ':'])
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                })
            })
            .and_then(|n| n.parse::<u32>().ok())
            .map(|n| i32::from(n > 0))
            .unwrap_or(0),
    }
}
pub fn parse_preferences(s: &str) -> Result<Preferences, String> {
    toml::from_str(s).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_bar_defaults_off_and_requires_boolean() {
        assert!(!parse_preferences("").unwrap().operation_bar);
        assert!(
            parse_preferences("operation_bar=true")
                .unwrap()
                .operation_bar
        );
        assert!(parse_preferences("operation_bar='true'").is_err());
    }
    #[test]
    fn operation_bar_config_preserves_hidden_window_and_unknown_values() {
        let dir = crate::persistence::test_dir("operation-bar-config");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "show_hidden=true\nwindow_mode='fullscreen'\n[future]\nvalue=42\n",
        )
        .unwrap();
        for value in [true, false] {
            crate::persistence::update_config_values(&path, &[("operation_bar", value.into())])
                .unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            let prefs = parse_preferences(&text).unwrap();
            assert_eq!(prefs.operation_bar, value);
            assert!(prefs.show_hidden);
            assert_eq!(prefs.window_mode, WindowMode::Fullscreen);
            assert!(text.contains("value = 42"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn hidden_files_default_and_explicit_boolean_values() {
        for text in ["", "corners='square'", "show_hidden=false"] {
            assert!(!parse_preferences(text).unwrap().show_hidden);
        }
        assert!(parse_preferences("show_hidden=true").unwrap().show_hidden);
        for text in [
            "show_hidden='true'",
            "show_hidden=1",
            "show_hidden=[]",
            "show_hidden={value=true}",
        ] {
            assert!(parse_preferences(text).is_err(), "accepted {text}");
        }
    }

    #[test]
    fn hidden_files_boolean_snapshot_preserves_unrelated_preferences() {
        let directory = crate::persistence::test_dir("hidden-preferences");
        let path = directory.join("config.toml");
        std::fs::write(
            &path,
            "corners='square'\nwindow_mode='normal'\nshow_hidden=false\neffects=false\n[future]\nvalue=42\n",
        )
        .unwrap();
        for enabled in [true, false] {
            crate::persistence::update_config_values(
                &path,
                &[
                    ("corners", "rounded".into()),
                    ("window_mode", "fullscreen".into()),
                    ("show_hidden", enabled.into()),
                ],
            )
            .unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            let preferences = parse_preferences(&text).unwrap();
            assert_eq!(preferences.show_hidden, enabled);
            assert_eq!(preferences.corners, Corners::Rounded);
            assert_eq!(preferences.window_mode, WindowMode::Fullscreen);
            let table: toml::Table = toml::from_str(&text).unwrap();
            assert_eq!(table["show_hidden"].as_bool(), Some(enabled));
            assert_eq!(table["effects"].as_bool(), Some(false));
            assert_eq!(table["future"]["value"].as_integer(), Some(42));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn window_mode_default_and_explicit_values() {
        assert_eq!(
            parse_preferences("").unwrap().window_mode,
            WindowMode::Normal
        );
        assert_eq!(
            parse_preferences("corners='square'").unwrap().window_mode,
            WindowMode::Normal
        );
        for (key, mode) in [
            ("normal", WindowMode::Normal),
            ("fullscreen", WindowMode::Fullscreen),
        ] {
            assert_eq!(
                parse_preferences(&format!("window_mode='{key}'"))
                    .unwrap()
                    .window_mode,
                mode
            );
        }
        assert!(parse_preferences("window_mode='overlay'").is_err());
    }
    #[test]
    fn parse_palette() {
        let p =
            Palette::parse("background='#010203'\nforeground='#ffffff'\naccent='#abcdef'").unwrap();
        assert_eq!(p.background, Color(1, 2, 3));
        assert!(Palette::parse("background='oops'").is_err());
        assert!(Color::parse("#💜xx").is_none());
    }
}
