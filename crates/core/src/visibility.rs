//! Filename visibility without filesystem I/O or lossy Unicode conversion.

use std::ffi::OsStr;

/// Linux/Unix dotfile convention, also used consistently for names on other hosts.
/// This does not query platform-specific hidden attributes.
pub fn is_hidden(name: &OsStr) -> bool {
    name.as_encoded_bytes().first() == Some(&b'.')
}

pub fn accepts(name: &OsStr, show_hidden: bool) -> bool {
    show_hidden || !is_hidden(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_prefix_is_the_only_filename_rule() {
        for name in [".foo", ".", "..", ".日本語", ".😀"] {
            assert!(is_hidden(OsStr::new(name)), "{name}");
            assert!(!accepts(OsStr::new(name), false));
            assert!(accepts(OsStr::new(name), true));
        }
        for name in ["", "foo", "foo.bar", "日本語", "😀", "．foo"] {
            assert!(!is_hidden(OsStr::new(name)), "{name}");
            assert!(accepts(OsStr::new(name), false));
            assert!(accepts(OsStr::new(name), true));
        }
    }

    #[cfg(unix)]
    #[test]
    fn invalid_utf8_identity_is_preserved() {
        use std::os::unix::ffi::OsStrExt;
        let hidden = OsStr::from_bytes(b".\xffname");
        let visible = OsStr::from_bytes(b"\xff.name");
        assert!(is_hidden(hidden));
        assert!(!accepts(hidden, false));
        assert!(accepts(hidden, true));
        assert!(!is_hidden(visible));
        assert!(accepts(visible, false));
        assert_eq!(hidden.as_bytes(), b".\xffname");
        assert_eq!(visible.as_bytes(), b"\xff.name");
    }
}
