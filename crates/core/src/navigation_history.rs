//! Bounded, lossless requested-location history; no I/O or directory probes.
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
pub const MAX_LOCATIONS: usize = 64;
pub const MAX_PATH_BYTES: usize = 4096;
#[derive(Default, Debug)]
pub struct History {
    back: VecDeque<PathBuf>,
    forward: VecDeque<PathBuf>,
}
pub fn validate(path: &Path) -> Result<(), &'static str> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || bytes.is_empty() || bytes.len() > MAX_PATH_BYTES || bytes.contains(&0)
    {
        return Err("Ruta inválida: debe ser absoluta y no superar 4096 bytes");
    }
    Ok(())
}
impl History {
    pub fn paths(&self) -> (&VecDeque<PathBuf>, &VecDeque<PathBuf>) {
        (&self.back, &self.forward)
    }
    pub fn restore(
        back: VecDeque<PathBuf>,
        forward: VecDeque<PathBuf>,
    ) -> Result<Self, &'static str> {
        if back.len().saturating_add(forward.len()) > MAX_LOCATIONS {
            return Err("Historial de sesión demasiado largo");
        }
        for path in back.iter().chain(&forward) {
            validate(path)?;
        }
        Ok(Self { back, forward })
    }
    /// Conservative charge for owned path payloads and deque/container overhead.
    pub fn retained_bytes(&self) -> usize {
        4096 + self
            .back
            .iter()
            .chain(&self.forward)
            .map(|p| p.as_os_str().len() + 128)
            .sum::<usize>()
            + (self.back.capacity() + self.forward.capacity()) * std::mem::size_of::<PathBuf>()
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn visit(&mut self, current: &Path, next: &Path) -> Result<bool, &'static str> {
        validate(next)?;
        validate(current)?;
        if current == next {
            return Ok(false);
        }
        self.forward.clear();
        if self.back.len() == MAX_LOCATIONS {
            self.back.pop_front();
        }
        self.back.push_back(current.to_path_buf());
        Ok(true)
    }
    pub fn travel(
        &mut self,
        current: &Path,
        forward: bool,
    ) -> Result<Option<PathBuf>, &'static str> {
        validate(current)?;
        let (source, dest) = if forward {
            (&mut self.forward, &mut self.back)
        } else {
            (&mut self.back, &mut self.forward)
        };
        let Some(target) = source.pop_back() else {
            return Ok(None);
        };
        dest.push_back(current.to_path_buf());
        debug_assert!(self.back.len() + self.forward.len() <= MAX_LOCATIONS);
        Ok(Some(target))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn back_forward_branch_duplicate_and_limit_are_observable() {
        let mut h = History::default();
        let mut current = PathBuf::from("/start");
        for n in 0..100 {
            let next = PathBuf::from(format!("/folder{n}"));
            h.visit(&current, &next).unwrap();
            current = next;
        }
        assert_eq!(h.back.len(), 64);
        assert!(!h.can_forward());
        let p = h.travel(&current, false).unwrap().unwrap();
        assert_eq!(p, Path::new("/folder98"));
        current = p;
        assert!(h.can_forward());
        assert!(!h.visit(&current, &current).unwrap());
        assert!(h.can_forward());
        let p = h.travel(&current, true).unwrap().unwrap();
        assert_eq!(p, Path::new("/folder99"));
        h.travel(&p, false).unwrap();
        h.visit(Path::new("/folder98"), Path::new("/branch"))
            .unwrap();
        assert!(!h.can_forward());
        let mut count = 0;
        let mut path = PathBuf::from("/branch");
        while let Some(p) = h.travel(&path, false).unwrap() {
            path = p;
            count += 1;
        }
        assert_eq!(count, 64);
        assert!(!h.can_back());
    }
    #[test]
    fn invalid_requests_leave_history_unchanged_and_bounds_precede_clone() {
        let mut h = History::default();
        h.visit(Path::new("/a"), Path::new("/b")).unwrap();
        for bad in [
            "relative".into(),
            format!("/{}", "a".repeat(4096)),
            "/a\0b".into(),
        ] {
            assert!(h.visit(Path::new("/b"), Path::new(&bad)).is_err());
            assert_eq!(h.back.len(), 1);
        }
        assert!(validate(Path::new(&format!("/{}", "a".repeat(4095)))).is_ok());
        assert!(h.travel(Path::new("relative"), false).is_err());
        assert_eq!(h.back.len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn raw_paths_survive_roundtrip_without_conversion() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let a = Path::new(OsStr::from_bytes(b"/a\xff"));
        let b = Path::new(OsStr::from_bytes(b"/b\xfe"));
        let mut h = History::default();
        h.visit(a, b).unwrap();
        assert_eq!(h.travel(b, false).unwrap().unwrap(), a);
        assert_eq!(h.travel(a, true).unwrap().unwrap(), b);
    }
}
