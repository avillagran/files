//! A bounded lossless selection snapshot, reused across coalesced refreshes.
use std::ffi::{OsStr, OsString};
const LIMIT: usize = 8 * 1024 * 1024;
const FIXED: usize = 64 * 1024;
pub struct Snapshot {
    selected: Vec<OsString>,
    focus: Option<OsString>,
    anchor: Option<OsString>,
    charge: usize,
}
impl Snapshot {
    pub fn capture<'a>(
        names: impl Iterator<Item = &'a OsStr> + Clone,
        focus: Option<&OsStr>,
    ) -> Result<Self, &'static str> {
        Self::capture_preserving_anchor(names, focus, focus)
    }
    pub fn capture_preserving_anchor<'a>(
        names: impl Iterator<Item = &'a OsStr> + Clone,
        focus: Option<&OsStr>,
        anchor: Option<&OsStr>,
    ) -> Result<Self, &'static str> {
        Self::capture_with_anchor(names, focus, anchor, LIMIT).map_err(|error| {
            if error == "Selección excede el presupuesto disponible; reduce selección y reintenta"
                || error == "Selección excede el presupuesto disponible"
            {
                "Actualización pendiente: selección excede8MiB; reduce selección y reintenta Ctrl+R"
            } else {
                error
            }
        })
    }
    pub fn estimate<'a>(
        names: impl Iterator<Item = &'a OsStr>,
        focus: Option<&OsStr>,
        anchor: Option<&OsStr>,
        limit: usize,
    ) -> Result<usize, &'static str> {
        if limit < FIXED {
            return Err("Presupuesto de selección insuficiente");
        }
        let mut bytes = FIXED;
        let mut count = 0usize;
        for name in names {
            count += 1;
            if count > 100_000 || name.len() > 4096 {
                return Err(
                    "Selección demasiado grande para actualizar sin perderla; reduce selección y reintenta Ctrl+R",
                );
            }
            bytes = bytes
                .checked_add(std::mem::size_of::<OsString>() + 32)
                .and_then(|n| n.checked_add(name.len()))
                .filter(|n| *n <= limit)
                .ok_or(
                    "Selección excede el presupuesto disponible; reduce selección y reintenta",
                )?;
        }
        for focus in [focus, anchor].into_iter().flatten() {
            bytes = bytes
                .checked_add(focus.len() + 32)
                .filter(|n| *n <= limit)
                .ok_or("Selección excede el presupuesto disponible")?;
            if focus.len() > 4096 {
                return Err("Nombre enfocado demasiado largo para actualizar");
            }
        }
        Ok(bytes)
    }
    pub fn capture_with_anchor<'a>(
        names: impl Iterator<Item = &'a OsStr> + Clone,
        focus: Option<&OsStr>,
        anchor: Option<&OsStr>,
        limit: usize,
    ) -> Result<Self, &'static str> {
        let bytes = Self::estimate(names.clone(), focus, anchor, limit)?;
        let count = names.clone().count();
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(count)
            .map_err(|_| "No hay memoria para preservar la selección")?;
        selected.extend(names.map(OsStr::to_os_string));
        selected.sort_unstable();
        selected.dedup();
        Ok(Self {
            selected,
            focus: focus.map(OsStr::to_os_string),
            anchor: anchor.map(OsStr::to_os_string),
            charge: bytes,
        })
    }
    pub fn names(&self) -> impl Iterator<Item = &OsStr> + Clone {
        self.selected.iter().map(OsString::as_os_str)
    }
    pub fn focus(&self) -> Option<&OsStr> {
        self.focus.as_deref()
    }
    pub fn anchor(&self) -> Option<&OsStr> {
        self.anchor.as_deref()
    }
    pub fn retained_bytes(&self) -> usize {
        self.charge
    }
    pub fn anchored(&self, name: &OsStr) -> bool {
        self.anchor.as_deref() == Some(name)
    }
    pub fn contains(&self, name: &OsStr) -> bool {
        self.selected
            .binary_search_by(|n| n.as_os_str().cmp(name))
            .is_ok()
    }
    pub fn focused(&self, name: &OsStr) -> bool {
        self.focus.as_deref() == Some(name)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_snapshot_still_respects_fixed_charge_and_anchor() {
        for cap in [0, FIXED - 1] {
            assert!(Snapshot::estimate(std::iter::empty(), None, None, cap).is_err());
        }
        assert_eq!(
            Snapshot::estimate(std::iter::empty(), None, None, FIXED).unwrap(),
            FIXED
        );
        let s = Snapshot::capture_with_anchor(
            [OsStr::new("a"), OsStr::new("b")].into_iter(),
            Some(OsStr::new("b")),
            Some(OsStr::new("a")),
            LIMIT,
        )
        .unwrap();
        assert!(s.anchored(OsStr::new("a")) && s.focused(OsStr::new("b")));
    }
    #[test]
    fn bounds_checked_before_cloning_and_lossless_membership() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let raw = OsStr::from_bytes(b"raw-\xff");
            let names = [OsStr::new("second"), raw];
            let snap = Snapshot::capture(names.into_iter(), Some(raw)).unwrap();
            assert!(snap.contains(raw) && snap.contains(OsStr::new("second")));
            assert!(snap.focused(raw));
        }
        assert!(Snapshot::capture(std::iter::repeat_n(OsStr::new("a"), 100_000), None).is_ok());
        assert!(Snapshot::capture(std::iter::repeat_n(OsStr::new("a"), 100_001), None).is_err());
        let name = "n".repeat(4096);
        assert!(Snapshot::capture(std::iter::repeat_n(OsStr::new(&name), 2048), None).is_err());
    }
}
