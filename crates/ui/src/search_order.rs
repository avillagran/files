//! Display-only ordering over bounded immutable search hits. No path/RGB copies.
use files_core::{listing::Kind, natural_sort::NaturalKeyRef, search::Hit};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Path,
    Name,
    Kind,
}
impl Order {
    pub fn label(self) -> &'static str {
        match self {
            Self::Path => "Ruta",
            Self::Name => "Nombre",
            Self::Kind => "Tipo",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Path => Self::Name,
            Self::Name => Self::Kind,
            Self::Kind => Self::Path,
        }
    }
}
#[derive(Default)]
pub struct Index {
    pub order: Order,
    indices: Vec<usize>,
    source: Option<(usize, usize, Order)>,
}
impl Index {
    pub fn invalidate(&mut self) {
        self.source = None;
    }
    pub fn cycle(&mut self) {
        self.order = self.order.next();
        self.invalidate();
    }
    pub fn visible<'a>(&'a self, hits: &[Hit]) -> &'a [usize] {
        if self.source == Some((hits.as_ptr() as usize, hits.len(), self.order)) {
            &self.indices
        } else {
            &[]
        }
    }
    pub fn sync(&mut self, hits: &[Hit]) {
        let tag = (hits.as_ptr() as usize, hits.len(), self.order);
        if self.source == Some(tag) {
            return;
        }
        self.indices.clear();
        // Real reports/batches are protocol bounded. Fail closed for an invalid local producer too.
        if hits.len() > files_core::search::MAX_RESULTS {
            self.source = None;
            return;
        }
        self.indices.extend(0..hits.len());
        if self.order == Order::Path {
            self.indices
                .sort_unstable_by(|&a, &b| hits[a].relative.cmp(&hits[b].relative));
        } else {
            // One folded key per basename per accepted population/order change, never per paint/comparison.
            let keys: Vec<_> = hits
                .iter()
                .map(|h| {
                    NaturalKeyRef::new(h.relative.file_name().unwrap_or(h.relative.as_os_str()))
                })
                .collect();
            let kind = |k| match k {
                Kind::Directory => 0,
                Kind::File => 1,
                Kind::Symlink => 2,
                Kind::Other => 3,
            };
            self.indices.sort_unstable_by(|&a, &b| {
                let grouping = if self.order == Order::Kind {
                    kind(hits[a].kind).cmp(&kind(hits[b].kind))
                } else {
                    std::cmp::Ordering::Equal
                };
                grouping
                    .then_with(|| keys[a].cmp(&keys[b]))
                    .then_with(|| hits[a].relative.cmp(&hits[b].relative))
            });
        }
        self.source = Some(tag);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hit(path: &str, kind: Kind) -> Hit {
        Hit {
            relative: path.into(),
            kind,
            content: None,
            snippet: None,
        }
    }
    #[test]
    fn natural_name_kind_and_path_have_deterministic_ties_without_changing_hits() {
        let hits = vec![
            hit("z/file10", Kind::File),
            hit("a/file2", Kind::File),
            hit("b/file2", Kind::Directory),
            hit("link", Kind::Symlink),
        ];
        let mut index = Index::default();
        index.sync(&hits);
        assert_eq!(index.visible(&hits), [1, 2, 3, 0]);
        index.cycle();
        index.sync(&hits);
        assert_eq!(index.visible(&hits), [1, 2, 0, 3]);
        index.cycle();
        index.sync(&hits);
        assert_eq!(index.visible(&hits), [2, 1, 0, 3]);
        assert_eq!(hits[0].relative, std::path::Path::new("z/file10"));
        index.invalidate();
        assert!(index.visible(&hits).is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn lossy_display_equal_raw_names_keep_byte_tiebreak_and_population_cap() {
        use std::os::unix::ffi::OsStrExt;
        let mut hits = vec![hit("a", Kind::File), hit("b", Kind::File)];
        hits[0].relative = std::ffi::OsStr::from_bytes(b"raw-\xff").into();
        hits[1].relative = std::ffi::OsStr::from_bytes(b"raw-\xfe").into();
        let mut index = Index::default();
        index.cycle();
        index.sync(&hits);
        assert_eq!(index.visible(&hits), [1, 0]);
        hits.resize(files_core::search::MAX_RESULTS + 1, hit("x", Kind::File));
        index.sync(&hits);
        assert!(index.visible(&hits).is_empty());
    }
}
