//! Descriptor caller chooses occupancy. Numeric basenames are pure, bounded and lossless-prefix.
#![cfg(unix)]
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt};
pub const MAX_CANDIDATES: u16 = 1000;
pub const MAX_NAME: usize = 255;
pub struct Names<'a> {
    original: &'a OsStr,
    directory: bool,
    limit: usize,
    index: u16,
    bytes: [u8; MAX_NAME],
    len: usize,
}
impl<'a> Names<'a> {
    pub fn new(original: &'a OsStr, directory: bool, limit: usize) -> io::Result<Self> {
        let raw = original.as_bytes();
        if raw.is_empty()
            || raw == b"."
            || raw == b".."
            || raw.contains(&0)
            || raw.contains(&b'/')
            || !(1..=MAX_NAME).contains(&limit)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Nombre de destino inválido",
            ));
        }
        Ok(Self {
            original,
            directory,
            limit,
            index: 1,
            bytes: [0; MAX_NAME],
            len: 0,
        })
    }
    pub fn name(&self) -> &OsStr {
        if self.index == 1 {
            self.original
        } else {
            OsStr::from_bytes(&self.bytes[..self.len])
        }
    }
    pub fn changed(&self) -> bool {
        self.index > 1
    }
    pub fn advance(&mut self) -> io::Result<()> {
        if self.index == MAX_CANDIDATES {
            return Err(io::Error::other(
                "Conservar ambos: se agotaron1000 nombres; elige otro destino",
            ));
        }
        let index = self.index + 1;
        let mut digits = [0u8; 4];
        let mut number = index;
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (number % 10) as u8;
            number /= 10;
            if number == 0 {
                break;
            }
        }
        let suffix = 3 + digits.len() - start;
        let raw = self.original.as_bytes();
        let extension = if self.directory {
            raw.len()
        } else {
            [
                b".tar.gz".as_slice(),
                b".tar.bz2",
                b".tar.xz",
                b".tar.zst",
                b".tar.lzma",
            ]
            .into_iter()
            .find(|ext| {
                raw.len() > ext.len() && raw[raw.len() - ext.len()..].eq_ignore_ascii_case(ext)
            })
            .map_or_else(
                || {
                    raw.iter()
                        .rposition(|&b| b == b'.')
                        .filter(|&i| i > 0 && i + 1 < raw.len())
                        .unwrap_or(raw.len())
                },
                |ext| raw.len() - ext.len(),
            )
        };
        let stem = &raw[..extension];
        let ext = &raw[extension..];
        let available = self
            .limit
            .checked_sub(suffix + ext.len())
            .filter(|&n| n > 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "El sufijo no cabe conservando la extensión; elige un nombre más corto",
                )
            })?;
        let mut kept = stem.len().min(available);
        if let Ok(text) = std::str::from_utf8(stem) {
            while !text.is_char_boundary(kept) {
                kept -= 1;
            }
        }
        if kept == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "El sufijo no cabe conservando UTF8; elige un nombre más corto",
            ));
        }
        // Commit only after all bounds and UTF8 boundaries are checked.
        self.bytes[..kept].copy_from_slice(&stem[..kept]);
        self.bytes[kept..kept + 2].copy_from_slice(b" (");
        let end = kept + 2 + digits.len() - start;
        self.bytes[kept + 2..end].copy_from_slice(&digits[start..]);
        self.bytes[end] = b')';
        self.bytes[end + 1..end + 1 + ext.len()].copy_from_slice(ext);
        self.len = end + 1 + ext.len();
        self.index = index;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_dotfiles_compounds_and_directories_keep_expected_bytes() {
        for (original, dir, expected) in [
            ("file.txt", false, "file (2).txt"),
            ("archive.TAR.GZ", false, "archive (2).TAR.GZ"),
            (".bashrc", false, ".bashrc (2)"),
            ("folder.txt", true, "folder.txt (2)"),
        ] {
            let mut n = Names::new(OsStr::new(original), dir, 255).unwrap();
            assert!(!n.changed());
            assert_eq!(n.name(), original);
            n.advance().unwrap();
            assert_eq!(n.name(), expected);
            assert!(n.changed());
        }
        let original = OsStr::from_bytes(b"raw-\xff.txt");
        let mut n = Names::new(original, false, 255).unwrap();
        n.advance().unwrap();
        assert_eq!(n.name().as_bytes(), b"raw-\xff (2).txt");
    }
    #[test]
    fn truncation_preserves_valid_utf8_extension_and_explicit_parent_path_budget() {
        let original = format!("{}.txt", "ñ".repeat(125));
        let mut n = Names::new(OsStr::new(&original), false, 255).unwrap();
        n.advance().unwrap();
        assert!(n.name().as_bytes().len() <= 255);
        assert!(n.name().to_str().unwrap().ends_with(" (2).txt"));
        let mut n = Names::new(OsStr::new("long.txt"), false, 10).unwrap();
        n.advance().unwrap();
        assert_eq!(n.name(), "lo (2).txt");
        let mut n = Names::new(OsStr::new("a.verylong"), false, 9).unwrap();
        assert!(n.advance().is_err());
        assert_eq!(n.name(), "a.verylong");
    }
    #[test]
    fn candidate_limit_is_exact_and_failure_preserves_last_name() {
        let mut n = Names::new(OsStr::new("a"), false, 255).unwrap();
        for _ in 1..MAX_CANDIDATES {
            n.advance().unwrap();
        }
        assert_eq!(n.name(), "a (1000)");
        assert!(n.advance().is_err());
        assert_eq!(n.name(), "a (1000)");
    }
}
