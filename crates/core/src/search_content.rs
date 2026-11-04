//! Content query contract; no filesystem work or regular-expression engine.
pub const MAX_LITERAL: usize = 1024;
pub const MAX_FILE: u64 = 1024 * 1024;
pub const MAX_TOTAL: u64 = 64 * 1024 * 1024;
/// Conventional quoted escapes only; no path decoding is involved.
pub fn literal(value: &str) -> Result<String, &'static str> {
    let error = "content: usa texto o una frase entre comillas, máximo1024B; escapes \\\", \\\\, \\n, \\r, \\t";
    let quoted = value.starts_with('"');
    let value = if quoted {
        value
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or(error)?
    } else {
        value
    };
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(mut c) = chars.next() {
        if quoted && c == '\\' {
            c = match chars.next() {
                Some('"') => '"',
                Some('\\') => '\\',
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                _ => return Err(error),
            };
        } else if c == '"' || c.is_control() {
            return Err(error);
        }
        if c == '\0' || result.len() + c.len_utf8() > MAX_LITERAL {
            return Err(error);
        }
        result.push(c);
    }
    if result.is_empty() {
        return Err("content: escribe el texto que quieres buscar");
    }
    Ok(result)
}
fn is_false(value: &bool) -> bool {
    !*value
}
#[derive(Debug, Default, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    #[serde(default, skip_serializing_if = "is_false")]
    pub regex: bool,
    pub files: u64,
    pub bytes: u64,
    pub binary: u64,
    pub limited: u64,
    pub changed: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Match {
    pub line: u64,
    pub byte: u64,
    /// Present only in regex mode. Equal endpoints denote a zero-width match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex_end: Option<u64>,
}

impl Match {
    pub fn valid(self, regex: bool, bytes: u64) -> bool {
        if self.line == 0 || self.line > self.byte.saturating_add(1) || self.line > MAX_FILE + 1 {
            return false;
        }
        match (regex, self.regex_end) {
            (false, None) => self.byte < bytes && self.byte < MAX_FILE,
            (true, Some(end)) => self.byte <= end && end <= bytes && end <= MAX_FILE,
            _ => false,
        }
    }
    pub fn zero_width(self) -> bool {
        self.regex_end == Some(self.byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_quotes_escapes_unicode_and_limits() {
        assert_eq!(literal(r#""Hola mundo""#).unwrap(), "Hola mundo");
        assert_eq!(literal(r#""\"á\"\\\n\t""#).unwrap(), "\"á\"\\\n\t");
        for bad in ["", r#""""#, r#""unfinished"#, r#""x\u0000""#, "bad\0"] {
            assert!(literal(bad).is_err(), "{bad}");
        }
        assert!(literal(&"a".repeat(1024)).is_ok());
        assert!(literal(&"á".repeat(513)).is_err());
    }
}
