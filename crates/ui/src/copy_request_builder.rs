//! Build local copy requests only after bounding the entire borrowed selection.
//! No filesystem mutation or canonicalization; the engine revalidates on execution.
use files_core::copy_engine::{CopyOptions, CopyRequest};
use std::{
    ffi::OsStr,
    path::{Component, Path},
};

const MAX_REQUESTS: usize = 4096;
const MAX_PATH: usize = 4096;
const MAX_PATH_TOTAL: usize = 4 * 1024 * 1024;

/// Bounds borrowed input without allocating paths or cloning any names.
pub fn preflight<'a>(
    source_parent: &Path,
    destination_parent: &Path,
    names: impl Iterator<Item = &'a OsStr>,
    options: &CopyOptions,
) -> Result<usize, &'static str> {
    preflight_pairs(
        source_parent,
        destination_parent,
        names.map(|n| (n, n)),
        options,
    )
}
pub(crate) fn preflight_pairs<'a>(
    source_parent: &Path,
    destination_parent: &Path,
    pairs: impl Iterator<Item = (&'a OsStr, &'a OsStr)>,
    options: &CopyOptions,
) -> Result<usize, &'static str> {
    if !source_parent.is_absolute() || !destination_parent.is_absolute() {
        return Err("Las carpetas de copia deben tener rutas absolutas");
    }
    if !(1..=MAX_REQUESTS).contains(&options.max_requests)
        || !(1..=MAX_PATH_TOTAL).contains(&options.max_request_path_bytes)
    {
        return Err("Límites de selección de copia no válidos");
    }
    let mut count = 0usize;
    let mut total = 0usize;
    for (source, target) in pairs {
        count += 1;
        if count > options.max_requests {
            return Err("Demasiados elementos seleccionados para copiar");
        }
        for (parent, name) in [(source_parent, source), (destination_parent, target)] {
            let mut parts = Path::new(name).components();
            if !matches!(parts.next(),Some(Component::Normal(part))if part==name)
                || parts.next().is_some()
                || name.as_encoded_bytes().contains(&0)
            {
                return Err("Nombre de archivo no válido para copiar");
            }
            let size = parent
                .as_os_str()
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_add(name.len()))
                .ok_or("Ruta demasiado larga para copiar")?;
            if size > MAX_PATH {
                return Err("Ruta demasiado larga para copiar");
            }
            total = total
                .checked_add(size)
                .ok_or("Selección demasiado grande para copiar")?;
            if total > options.max_request_path_bytes {
                return Err("Selección demasiado grande para copiar");
            }
        }
    }
    if count == 0 {
        return Err("Selecciona al menos un archivo o carpeta");
    }
    Ok(count)
}
/// Alternate names are bound to the captured Arc selection, never displayed labels.
/// Preflight all source/target lengths and duplicate targets before path clones.
pub fn build_named(
    source: &Path,
    target: &Path,
    names: &std::sync::Arc<[std::ffi::OsString]>,
    plan: &crate::copy_names::Plan,
    options: &CopyOptions,
) -> Result<Vec<CopyRequest>, &'static str> {
    if !plan.same_selection(names) {
        return Err("El plan pertenece a otra selección");
    }
    let pairs = || {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_os_str(), plan.target(i).expect("captured name")))
    };
    let count = preflight_pairs(source, target, pairs(), options)?;
    let mut targets: Vec<&OsStr> = pairs().map(|(_, t)| t).collect();
    targets.sort_unstable();
    if targets.windows(2).any(|w| w[0] == w[1]) {
        return Err("Dos elementos tienen el mismo nombre de destino");
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| "No hay memoria para preparar copia")?;
    for (s, t) in pairs() {
        result.push(CopyRequest {
            source: source.join(s),
            destination: target.join(t),
        });
    }
    Ok(result)
}

/// The cloneable iterator must describe the same selection on both passes.
/// Parents must be absolute. Names remain lossless (including non-UTF8 names).
/// Length estimates include one separator even if a parent already ends in one.
pub fn build<'a>(
    source_parent: &Path,
    destination_parent: &Path,
    names: impl Iterator<Item = &'a OsStr> + Clone,
    options: &CopyOptions,
) -> Result<Vec<CopyRequest>, &'static str> {
    let count = preflight(source_parent, destination_parent, names.clone(), options)?;
    let mut requests = Vec::new();
    requests
        .try_reserve_exact(count)
        .map_err(|_| "No hay memoria para preparar la copia")?;
    for name in names {
        requests.push(CopyRequest {
            source: source_parent.join(name),
            destination: destination_parent.join(name),
        });
    }
    Ok(requests)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    fn make<'a>(
        names: impl Iterator<Item = &'a OsStr> + Clone,
        options: &CopyOptions,
    ) -> Result<Vec<CopyRequest>, &'static str> {
        build(Path::new("/source"), Path::new("/target"), names, options)
    }
    #[test]
    fn lossless_names_and_input_order_are_preserved() {
        let names = [OsStr::new("second.txt"), OsStr::from_bytes(b"raw-\xff")];
        let requests = make(names.into_iter(), &CopyOptions::default()).unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].destination, Path::new("/target/second.txt"));
        assert_eq!(requests[1].source.file_name(), Some(names[1]));
    }
    #[test]
    fn reject_empty_paths_traversal_and_nul() {
        for name in [
            "",
            ".",
            "..",
            "../outside",
            "/absolute",
            "a/b",
            "a/",
            "a\0b",
        ] {
            assert!(
                make([OsStr::new(name)].into_iter(), &CopyOptions::default()).is_err(),
                "{name:?}"
            );
        }
        assert!(make(std::iter::empty(), &CopyOptions::default()).is_err());
        assert!(
            build(
                Path::new("relative"),
                Path::new("/target"),
                [OsStr::new("x")].into_iter(),
                &CopyOptions::default()
            )
            .is_err()
        );
    }
    #[test]
    fn count_and_aggregate_bounds_apply_before_second_pass() {
        let mut options = CopyOptions {
            max_requests: 2,
            max_request_path_bytes: 36,
            ..Default::default()
        };
        let name = OsStr::new("x"); // 9 bytes per path, two paths per request
        assert_eq!(
            make(std::iter::repeat_n(name, 2), &options).unwrap().len(),
            2
        );
        assert!(make(std::iter::repeat_n(name, 3), &options).is_err());
        options.max_request_path_bytes = 35;
        assert!(make(std::iter::repeat_n(name, 2), &options).is_err());
        // An unbounded selection is rejected after at most max_requests+1 names.
        assert!(make(std::iter::repeat(name), &options).is_err());
    }
    #[test]
    fn per_path_and_configured_limits_are_enforced() {
        let long = "x".repeat(MAX_PATH);
        assert!(make([OsStr::new(&long)].into_iter(), &CopyOptions::default()).is_err());
        for options in [
            CopyOptions {
                max_requests: MAX_REQUESTS + 1,
                ..Default::default()
            },
            CopyOptions {
                max_request_path_bytes: MAX_PATH_TOTAL + 1,
                ..Default::default()
            },
        ] {
            assert!(make([OsStr::new("x")].into_iter(), &options).is_err());
        }
    }
    #[test]
    fn alternate_names_preserve_raw_source_and_reject_duplicate_or_stale_plan() {
        use crate::copy_names::Plan;
        use std::sync::Arc;
        let names: Arc<[std::ffi::OsString]> =
            vec![OsStr::from_bytes(b"a-\xff").to_os_string(), "b.txt".into()].into();
        let mut p = Plan::new(names.clone());
        p.set(0, "copie ñ.txt").unwrap();
        let r = build_named(
            Path::new("/source"),
            Path::new("/target"),
            &names,
            &p,
            &CopyOptions::default(),
        )
        .unwrap();
        assert_eq!(
            r[0].source.as_os_str().as_encoded_bytes(),
            b"/source/a-\xff"
        );
        assert_eq!(r[0].destination, Path::new("/target/copie ñ.txt"));
        p.set(1, "copie ñ.txt").unwrap();
        assert!(
            build_named(
                Path::new("/source"),
                Path::new("/target"),
                &names,
                &p,
                &CopyOptions::default()
            )
            .is_err()
        );
        let other: Arc<[std::ffi::OsString]> = names.to_vec().into();
        assert!(
            build_named(
                Path::new("/source"),
                Path::new("/target"),
                &other,
                &p,
                &CopyOptions::default()
            )
            .is_err()
        );
    }
}
