//! Captured selection indices -> explicit destination basenames. No filesystem I/O.
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    path::{Component, Path},
    sync::Arc,
};
pub const MAX_OVERRIDES: usize = 128;
pub const MAX_NAME_BYTES: usize = 255;
#[derive(Debug)]
pub struct Plan {
    names: Arc<[OsString]>,
    overrides: BTreeMap<usize, OsString>,
}
pub fn validate(name: &str) -> Result<(), &'static str> {
    let mut parts = Path::new(name).components();
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name.chars().any(char::is_control)
        || name.contains('/')
        || !matches!(parts.next(),Some(Component::Normal(n))if n==name)
        || parts.next().is_some()
    {
        return Err("Nombre:1..255bytes, sin separadores, controles, . o ..");
    }
    Ok(())
}
impl Plan {
    pub fn new(names: Arc<[OsString]>) -> Self {
        Self {
            names,
            overrides: BTreeMap::new(),
        }
    }
    pub fn names(&self) -> &Arc<[OsString]> {
        &self.names
    }
    pub fn target(&self, index: usize) -> Option<&OsStr> {
        self.overrides
            .get(&index)
            .map(OsString::as_os_str)
            .or_else(|| self.names.get(index).map(OsString::as_os_str))
    }
    pub fn modified(&self) -> usize {
        self.overrides.len()
    }
    pub fn reset(&mut self, index: usize) {
        self.overrides.remove(&index);
    }
    pub fn set(&mut self, index: usize, name: &str) -> Result<(), &'static str> {
        let original = self.names.get(index).ok_or("Elemento de copia obsoleto")?;
        validate(name)?;
        if original == name {
            self.overrides.remove(&index);
            return Ok(());
        }
        if !self.overrides.contains_key(&index) && self.overrides.len() == MAX_OVERRIDES {
            return Err("Máximo128 nombres alternativos por copia");
        }
        self.overrides.insert(index, OsString::from(name));
        Ok(())
    }
    pub fn same_selection(&self, names: &Arc<[OsString]>) -> bool {
        Arc::ptr_eq(&self.names, names)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_validate_before_mutation_and_captured_selection_is_distinct() {
        let names: Arc<[OsString]> = vec!["a".into(), "b".into()].into();
        let mut p = Plan::new(names.clone());
        p.set(0, "copia ñ.txt").unwrap();
        for n in ["", ".", "..", "../a", "a/b", "a\0b", "a\nb"] {
            assert!(p.set(0, n).is_err());
        }
        assert_eq!(p.target(0), Some(OsStr::new("copia ñ.txt")));
        assert!(p.set(8, "x").is_err());
        assert!(p.same_selection(&names));
        assert!(!p.same_selection(&vec!["a".into(), "b".into()].into()));
        p.set(0, "a").unwrap();
        assert_eq!(p.modified(), 0);
    }
    #[test]
    fn override_count_and_utf8_byte_limit_are_explicit() {
        let names: Arc<[OsString]> = (0..129)
            .map(|i| format!("a{i}").into())
            .collect::<Vec<_>>()
            .into();
        let mut p = Plan::new(names);
        for i in 0..128 {
            p.set(i, &format!("b{i}")).unwrap();
        }
        assert!(p.set(128, "c").is_err());
        assert!(p.set(0, &"é".repeat(128)).is_err());
        p.reset(5);
        p.set(128, "c").unwrap();
        assert_eq!(p.modified(), 128);
    }
    #[cfg(unix)]
    #[test]
    fn literal_backslash_is_not_decoded_or_treated_as_unix_separator() {
        let mut p = Plan::new(vec!["original".into()].into());
        p.set(0, r"literal\xFF").unwrap();
        assert_eq!(p.target(0), Some(OsStr::new(r"literal\xFF")));
    }
}
