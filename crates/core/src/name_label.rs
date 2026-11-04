//! Display-only, single-line labels. Never use these strings as filesystem paths.
use std::{borrow::Cow, ffi::OsStr, fmt::Write};
fn escaped(c: char) -> bool {
    c == '\\' || c.is_control() || matches!(c, '\u{2028}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}
fn append(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if escaped(c) && c.is_ascii() => {
                write!(out, "\\x{:02X}", c as u32).unwrap();
            }
            c if escaped(c) => {
                write!(out, "\\u{{{:X}}}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
}
pub fn label(name: &OsStr) -> Cow<'_, str> {
    bytes_label(name.as_encoded_bytes())
}
pub fn bytes_label(input: &[u8]) -> Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(input)
        && !text.chars().any(escaped)
    {
        return Cow::Borrowed(text);
    }
    let mut bytes = input;
    let mut out = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                append(&mut out, text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // valid_up_to is guaranteed to delimit a valid UTF-8 prefix.
                append(&mut out, std::str::from_utf8(&bytes[..valid]).unwrap());
                let invalid = error.error_len().unwrap_or(bytes.len() - valid);
                for byte in &bytes[valid..valid + invalid] {
                    write!(&mut out, "\\x{byte:02X}").unwrap();
                }
                bytes = &bytes[valid + invalid..];
            }
        }
    }
    Cow::Owned(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_unicode_borrows_and_controls_cannot_create_rows_or_bidi_overrides() {
        assert!(matches!(
            label(OsStr::new("café 日本語 👩‍💻.txt")),
            Cow::Borrowed(_)
        ));
        assert_eq!(
            label(OsStr::new("a\n\r\t\x01\x7f\u{85}\u{2028}\u{202e}\u{2066}b")),
            "a\\n\\r\\t\\x01\\x7F\\u{85}\\u{2028}\\u{202E}\\u{2066}b"
        );
        assert_ne!(label(OsStr::new("a\n")), label(OsStr::new("a\\n")));
    }
    #[cfg(unix)]
    #[test]
    fn invalid_bytes_are_distinct_from_literal_escapes_and_replacement_characters() {
        use std::os::unix::ffi::OsStrExt;
        for bytes in [b"raw\xff\n.txt".as_slice(), b"\xf0\x9f", b"a\xe2\x28\xa1z"] {
            let value = label(OsStr::from_bytes(bytes));
            assert!(value.len() <= bytes.len() * 4);
            assert!(!value.chars().any(char::is_control));
        }
        assert_eq!(
            label(OsStr::from_bytes(b"raw\xff\n.txt")),
            "raw\\xFF\\n.txt"
        );
        assert_ne!(label(OsStr::from_bytes(b"\xff")), label(OsStr::new("�")));
        assert_ne!(
            label(OsStr::from_bytes(b"\xff")),
            label(OsStr::new("\\xFF"))
        );
    }
}
