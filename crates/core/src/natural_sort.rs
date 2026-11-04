//! Clave cacheable: normaliza una sola vez y compara dígitos sin convertir a enteros.
use std::{
    borrow::Cow,
    cmp::Ordering,
    ffi::{OsStr, OsString},
};

/// Para ordenar índices de un modelo inmutable sin duplicar sus nombres.
/// Los nombres ASCII que ya están en minúsculas no requieren asignaciones.
#[derive(Debug, Eq, PartialEq)]
pub struct NaturalKeyRef<'a> {
    folded: Cow<'a, str>,
    original: &'a OsStr,
}

impl<'a> NaturalKeyRef<'a> {
    pub fn new(name: &'a OsStr) -> Self {
        let folded = match name.to_str() {
            Some(text) if text.is_ascii() && !text.bytes().any(|b| b.is_ascii_uppercase()) => {
                Cow::Borrowed(text)
            }
            _ => Cow::Owned(name.to_string_lossy().to_lowercase()),
        };
        Self {
            folded,
            original: name,
        }
    }
}

impl Ord for NaturalKeyRef<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(self.folded.as_bytes(), other.folded.as_bytes())
            .then_with(|| self.original.cmp(other.original))
    }
}
impl PartialOrd for NaturalKeyRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct NaturalKey {
    folded: String,
    original: OsString,
}

impl NaturalKey {
    pub fn new(name: &OsStr) -> Self {
        Self {
            folded: name.to_string_lossy().to_lowercase(),
            original: name.to_owned(),
        }
    }
}

fn compare(a: &[u8], b: &[u8]) -> Ordering {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (start_i, start_j) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let mut ai = start_i;
            let mut bj = start_j;
            while ai < i && a[ai] == b'0' {
                ai += 1;
            }
            while bj < j && b[bj] == b'0' {
                bj += 1;
            }
            let order = (i - ai)
                .cmp(&(j - bj))
                .then_with(|| a[ai..i].cmp(&b[bj..j]))
                .then_with(|| (i - start_i).cmp(&(j - start_j)));
            if order != Ordering::Equal {
                return order;
            }
        } else {
            let order = a[i].cmp(&b[j]);
            if order != Ordering::Equal {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

impl Ord for NaturalKey {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(self.folded.as_bytes(), other.folded.as_bytes())
            .then_with(|| self.original.cmp(&other.original))
    }
}
impl PartialOrd for NaturalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_keys_match_owned_order_without_copying_ascii_names() {
        let names = [
            "file2.txt",
            "file02.txt",
            "file10.txt",
            "A",
            "a",
            "Álbum2",
            "álbum10",
            "İ",
            "ß",
            "ss",
            "",
        ];
        for a in names {
            for b in names {
                let a = OsStr::new(a);
                let b = OsStr::new(b);
                assert_eq!(
                    NaturalKeyRef::new(a).cmp(&NaturalKeyRef::new(b)),
                    NaturalKey::new(a).cmp(&NaturalKey::new(b))
                );
            }
        }
        let original = OsStr::new("file123.txt");
        let key = NaturalKeyRef::new(original);
        assert!(matches!(key.folded, Cow::Borrowed(_)));
        assert!(std::ptr::eq(key.original, original));
        assert!(matches!(
            NaturalKeyRef::new(OsStr::new("FILE123.txt")).folded,
            Cow::Owned(_)
        ));
    }
    fn key(name: &str) -> NaturalKey {
        NaturalKey::new(OsStr::new(name))
    }
    #[test]
    fn numeric_order_without_integer_overflow() {
        assert!(key("file2") < key("file10"));
        assert!(key("file9") < key("file99999999999999999999999999999999999"));
        assert!(key("file2") < key("file02"));
        assert!(key("file0") < key("file00"));
        assert!(key("file2part9") < key("file2part10"));
    }
    #[test]
    fn unicode_case_and_stable_ties() {
        assert!(key("Álbum2") < key("álbum10"));
        assert_ne!(key("A"), key("a"));
        assert_eq!(
            key("A").cmp(&key("a")),
            OsStr::new("A").cmp(OsStr::new("a"))
        );
    }
    #[test]
    fn order_is_total_and_transitive() {
        let keys: Vec<_> = [
            "", "a", "A", "0", "00", "01", "1", "2", "10", "a01", "a1", "á2", "á10", "ß", "ss",
        ]
        .iter()
        .map(|s| key(s))
        .collect();
        for a in &keys {
            for b in &keys {
                assert_eq!(a.cmp(b), b.cmp(a).reverse());
                assert_eq!(a == b, a.cmp(b) == Ordering::Equal);
                for c in &keys {
                    if a <= b && b <= c {
                        assert!(a <= c);
                    }
                }
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn invalid_utf8_names_keep_distinct_identity() {
        use std::os::unix::ffi::OsStrExt;
        let a = NaturalKey::new(OsStr::from_bytes(b"file\xff"));
        let b = NaturalKey::new(OsStr::from_bytes(b"file\xfe"));
        assert_ne!(a.cmp(&b), Ordering::Equal);
        let ra = NaturalKeyRef::new(OsStr::from_bytes(b"file\xff"));
        let rb = NaturalKeyRef::new(OsStr::from_bytes(b"file\xfe"));
        assert_eq!(ra.cmp(&rb), a.cmp(&b));
    }
}
