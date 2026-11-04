//! Bounded raw path marks, independent of provisional row indices and display order.
use files_core::search::Hit;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
pub const MAX: usize = 128;
pub const LIMIT: usize = 64 * 1024;
#[derive(Default)]
pub struct Marks {
    paths: BTreeSet<PathBuf>,
    anchor: Option<PathBuf>,
    pub explicit: bool,
    pub pruned: usize,
}
impl Marks {
    fn prepare<'a>(
        paths: impl Iterator<Item = &'a Path>,
        anchor: Option<&Path>,
    ) -> Result<Self, &'static str> {
        let mut borrowed = BTreeSet::new();
        let mut bytes = 4096usize;
        for p in paths {
            if p.as_os_str().len() > 4096 {
                return Err("Ruta de resultado fuera de límite");
            }
            if borrowed.insert(p) {
                bytes = bytes.saturating_add(p.as_os_str().len() + 128);
            }
            if borrowed.len() > MAX || bytes > LIMIT {
                return Err("Marcas: máximo128 resultados y64KiB; reduce el rango");
            }
        }
        if anchor.is_some_and(|p| p.as_os_str().len() > 4096) {
            return Err("Ancla de resultado fuera de límite");
        }
        Ok(Self {
            paths: borrowed.into_iter().map(Path::to_path_buf).collect(),
            anchor: anchor.map(Path::to_path_buf),
            explicit: true,
            pruned: 0,
        })
    }
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.paths.iter().map(PathBuf::as_path)
    }
    pub fn anchor(&self) -> Option<&Path> {
        self.anchor.as_deref()
    }
    pub fn len(&self) -> usize {
        self.paths.len()
    }
    pub fn contains(&self, path: &Path) -> bool {
        self.paths.contains(path)
    }
    pub fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
        self.explicit = true;
        self.pruned = 0;
    }
    pub fn select(
        &mut self,
        hits: &[Hit],
        visual: &[usize],
        focused: usize,
        target: usize,
        modifiers: (bool, bool, bool),
    ) -> Result<(), &'static str> {
        let (toggle, extend, add) = modifiers;
        let path = hits
            .get(target)
            .ok_or("Resultado ya no disponible")?
            .relative
            .as_path();
        let next = if extend {
            let anchor = self
                .anchor()
                .or_else(|| hits.get(focused).map(|h| h.relative.as_path()));
            let a = visual
                .iter()
                .position(|&i| {
                    hits.get(i)
                        .is_some_and(|h| Some(h.relative.as_path()) == anchor)
                })
                .unwrap_or(0);
            let b = visual
                .iter()
                .position(|&i| i == target)
                .ok_or("Resultado fuera del orden visible")?;
            let range = visual[a.min(b)..=a.max(b)]
                .iter()
                .filter_map(|&i| hits.get(i).map(|h| h.relative.as_path()));
            if add {
                Self::prepare(self.paths().chain(range), anchor)?
            } else {
                Self::prepare(range, anchor)?
            }
        } else if toggle {
            let already = self.contains(path);
            Self::prepare(
                self.paths()
                    .filter(|p| *p != path)
                    .chain((!already).then_some(path)),
                Some(path),
            )?
        } else {
            Self::prepare(std::iter::once(path), Some(path))?
        };
        *self = next;
        Ok(())
    }
    pub fn all(
        &mut self,
        hits: &[Hit],
        visual: &[usize],
        focused: usize,
    ) -> Result<(), &'static str> {
        let next = Self::prepare(
            visual
                .iter()
                .filter_map(|&i| hits.get(i).map(|h| h.relative.as_path())),
            hits.get(focused).map(|h| h.relative.as_path()),
        )?;
        *self = next;
        Ok(())
    }
    pub fn reconcile(&mut self, hits: &[Hit]) {
        let mut available = hits
            .iter()
            .map(|h| h.relative.as_path())
            .collect::<Vec<_>>();
        available.sort_unstable();
        let before = self.len();
        self.paths
            .retain(|p| available.binary_search(&p.as_path()).is_ok());
        self.pruned += before - self.len();
        if self
            .anchor
            .as_ref()
            .is_some_and(|p| available.binary_search(&p.as_path()).is_err())
        {
            self.anchor = None;
        }
    }
    #[cfg(test)]
    pub fn indices(&self, hits: &[Hit], focused: usize) -> Vec<usize> {
        if self.explicit {
            hits.iter()
                .enumerate()
                .filter_map(|(i, h)| self.contains(&h.relative).then_some(i))
                .collect()
        } else if focused < hits.len() {
            vec![focused]
        } else {
            Vec::new()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hit(p: &str) -> Hit {
        Hit {
            relative: p.into(),
            kind: files_core::listing::Kind::File,
            content: None,
            snippet: None,
        }
    }
    #[test]
    fn visual_range_toggle_and_reorder_keep_paths_not_indices() {
        let hits = [hit("z"), hit("a"), hit("m")];
        let mut s = Marks::default();
        s.select(&hits, &[1, 2, 0], 0, 1, (false, false, false))
            .unwrap();
        s.select(&hits, &[1, 2, 0], 1, 0, (false, true, false))
            .unwrap();
        assert_eq!(s.len(), 3);
        s.select(&hits, &[0, 2, 1], 0, 2, (true, false, false))
            .unwrap();
        assert_eq!(s.len(), 2);
        s.reconcile(&[hit("a"), hit("z")]);
        assert_eq!(s.indices(&[hit("a"), hit("z")], 0), [0, 1]);
        s.reconcile(&[]);
        assert!(s.explicit && s.indices(&[hit("other")], 0).is_empty());
        assert_eq!(s.pruned, 2);
    }
    #[test]
    fn limits_reject_whole_range_and_all_without_replacing_previous_marks() {
        let hits = (0..129)
            .map(|i| hit(&format!("file{i}")))
            .collect::<Vec<_>>();
        let v = (0..129).collect::<Vec<_>>();
        let mut s = Marks::default();
        s.select(&hits, &v, 0, 0, (false, false, false)).unwrap();
        assert!(s.all(&hits, &v, 0).is_err());
        assert_eq!(s.indices(&hits, 0), [0]);
        assert!(s.select(&hits, &v, 0, 128, (false, true, false)).is_err());
        assert_eq!(s.indices(&hits, 0), [0]);
        let long = (0..20)
            .map(|i| hit(&format!("{i}{}", "x".repeat(4000))))
            .collect::<Vec<_>>();
        assert!(s.all(&long, &(0..20).collect::<Vec<_>>(), 0).is_err());
        assert_eq!(s.indices(&hits, 0), [0]);
        s.clear();
        assert!(s.indices(&hits, 0).is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn raw_paths_survive_final_reorder_and_missing_paths_are_explicitly_pruned() {
        use std::os::unix::ffi::OsStringExt;
        let mut raw = hit("placeholder");
        raw.relative = PathBuf::from(std::ffi::OsString::from_vec(b"d/raw-\xff".to_vec()));
        let mut s = Marks::default();
        s.select(std::slice::from_ref(&raw), &[0], 0, 0, (true, false, false))
            .unwrap();
        let hits = [hit("new"), raw];
        s.reconcile(&hits);
        assert_eq!(s.indices(&hits, 0), [1]);
        s.reconcile(&[hit("replacement")]);
        assert_eq!(s.pruned, 1);
        assert!(s.indices(&[hit("replacement")], 0).is_empty());
    }
}
