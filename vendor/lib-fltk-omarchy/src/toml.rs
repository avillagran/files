//! Minimal reader for the TOML subset Omarchy writes in `colors.toml` and
//! `shell.toml`: `[section]` headers and `key = value` lines with strings,
//! numbers and booleans. Keys are flattened to `"section.key"`.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Bool(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Str(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            },
            Value::Num(n) => Some(*n != 0.0),
        }
    }
}

pub type Table = HashMap<String, Value>;

/// Tolerant parse: malformed lines are skipped (initial load, fallbacks).
pub fn parse(text: &str) -> Table {
    parse_with(text, false).unwrap_or_default()
}

/// A line that is not valid TOML for the subset Omarchy writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Strict parse: any malformed line (unterminated string or section, a
/// line without `=`, an empty key, a bare value that is not a number or
/// boolean, text after a closing quote) is an error. A file cut in the
/// middle of a write fails here instead of loading half a theme.
pub fn parse_strict(text: &str) -> Result<Table, ParseError> {
    parse_with(text, true)
}

fn parse_with(text: &str, strict: bool) -> Result<Table, ParseError> {
    let mut out = Table::new();
    let mut section = String::new();
    for (n, raw) in text.lines().enumerate() {
        let err = |m: &str| Err(ParseError { line: n + 1, message: m.to_string() });
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            match line.find(']') {
                Some(end) => {
                    let name = line[1..end].trim();
                    let tail = line[end + 1..].trim();
                    if strict && (name.is_empty() || !(tail.is_empty() || tail.starts_with('#'))) {
                        return err("malformed section header");
                    }
                    section = name.to_string();
                }
                None if strict => return err("unterminated section header"),
                None => {}
            }
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            if strict {
                return err("expected `key = value`");
            }
            continue;
        };
        let key = key.trim().trim_matches('"');
        if strict && key.is_empty() {
            return err("empty key");
        }
        let rest = rest.trim();
        let value = if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
            let body = &rest[1..];
            match body.find(q) {
                Some(end) => {
                    let tail = body[end + 1..].trim();
                    if strict && !(tail.is_empty() || tail.starts_with('#')) {
                        return err("text after the closing quote");
                    }
                    Value::Str(body[..end].to_string())
                }
                None if strict => return err("unterminated string"),
                None => Value::Str(body.to_string()),
            }
        } else if rest.starts_with('[') {
            match rest.find(']') {
                Some(end) => Value::Str(rest[..=end].to_string()),
                None if strict => return err("unterminated array"),
                None => Value::Str(rest.to_string()),
            }
        } else {
            let bare = rest.split('#').next().unwrap_or("").trim();
            match bare {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => match bare.parse::<f64>() {
                    Ok(n) => Value::Num(n),
                    Err(_) if strict => return err(if bare.is_empty() { "missing value" } else { "value is not a string, number or boolean" }),
                    Err(_) => Value::Str(bare.to_string()),
                },
            }
        };
        let full = if section.is_empty() { key.to_string() } else { format!("{section}.{key}") };
        out.insert(full, value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_strings_numbers_comments() {
        let t = parse("top = \"#e68e0d\"  # c\n[font]\nbase-size = 12 # x\n[menu]\nborder = \"hyprland.active-border\"\nflag = true\nalpha = 0.95\n");
        assert_eq!(t["top"].as_str(), Some("#e68e0d"));
        assert_eq!(t["font.base-size"].as_f64(), Some(12.0));
        assert_eq!(t["menu.border"].as_str(), Some("hyprland.active-border"));
        assert_eq!(t["menu.flag"].as_bool(), Some(true));
        assert_eq!(t["menu.alpha"].as_f64(), Some(0.95));
    }

    #[test]
    fn strict_rejects_cut_and_malformed_lines() {
        assert!(parse_strict("accent = \"#e68e0d\" # ok\n[font]\nbase-size = 12\nflag = true\n").is_ok());
        assert_eq!(parse_strict("accent = \"#e68e0d\"\nbackground = \"#12").unwrap_err().line, 2);
        assert!(parse_strict("[controls\n").is_err());
        assert!(parse_strict("accent\n").is_err());
        assert!(parse_strict("a = \"x\" y\n").is_err());
        assert!(parse_strict("a = #121212\n").is_err());
        assert!(parse_strict("a =\n").is_err());
        // The tolerant parser keeps what it can.
        assert_eq!(parse("accent = \"#e68e0d\"\nbackground = \"#12")["accent"].as_str(), Some("#e68e0d"));
    }
}
