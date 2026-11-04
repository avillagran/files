//! Borrowed admission plan for multi-parent transfers. No filesystem I/O.
//! This is not a copy/move implementation or permission to delete sources.
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};
pub const MAX_ROOTS: usize = 128;
pub const MAX_PARENTS: usize = 32;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_AGGREGATE_BYTES: usize = 1024 * 1024;
fn ambiguous_mount(device: u64, mount: u64, other_device: u64, other_mount: u64) -> bool {
    device == other_device && mount != other_mount
}
fn directory_mount_ambiguous(root_mount: u64, parent_mount: u64) -> bool {
    root_mount != parent_mount
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooManyRoots,
    TooManyParents,
    InvalidPath { index: usize },
    TooManyBytes,
    DuplicateDestination { first: usize, second: usize },
    OverlappingRoots { first: usize, second: usize },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "La selección está vacía"),
            Self::TooManyRoots => write!(f, "Máximo 128 elementos por lote"),
            Self::TooManyParents => write!(f, "Máximo 32 carpetas de origen por lote"),
            Self::InvalidPath { index } => {
                write!(f, "Ruta de origen inválida en elemento {}", index + 1)
            }
            Self::TooManyBytes => write!(f, "Las rutas superan el presupuesto conjunto"),
            Self::DuplicateDestination { first, second } => write!(
                f,
                "Los elementos {} y {} tienen el mismo nombre de destino",
                first + 1,
                second + 1
            ),
            Self::OverlappingRoots { first, second } => write!(
                f,
                "Los elementos {} y {} se solapan como ancestro y descendiente",
                first + 1,
                second + 1
            ),
        }
    }
}
impl std::error::Error for Error {}
#[derive(Debug)]
pub struct Group<'a> {
    pub parent: &'a Path,
    pub indices: Vec<usize>,
}
/// Two passes: reject count, individual and aggregate lengths before building
/// any map/vector. Returned indices borrow caller paths and preserve input order.
pub fn plan(paths: &[PathBuf]) -> Result<Vec<Group<'_>>, Error> {
    if paths.is_empty() {
        return Err(Error::Empty);
    }
    if paths.len() > MAX_ROOTS {
        return Err(Error::TooManyRoots);
    }
    let mut total = 0usize;
    let mut parents: [Option<&Path>; MAX_PARENTS] = [None; MAX_PARENTS];
    let mut parent_count = 0;
    for (index, path) in paths.iter().enumerate() {
        let raw = path.as_os_str().as_encoded_bytes();
        if !path.is_absolute()
            || raw.is_empty()
            || raw.len() > MAX_PATH_BYTES
            || raw.contains(&0)
            || path.file_name().is_none()
            || path.parent().is_none()
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(Error::InvalidPath { index });
        }
        total = total
            .checked_add(raw.len())
            .filter(|n| *n <= MAX_AGGREGATE_BYTES)
            .ok_or(Error::TooManyBytes)?;
        let parent = path.parent().expect("preflight");
        if !parents[..parent_count].contains(&Some(parent)) {
            if parent_count == MAX_PARENTS {
                return Err(Error::TooManyParents);
            }
            parents[parent_count] = Some(parent);
            parent_count += 1;
        }
        for (prior, other) in paths[..index].iter().enumerate() {
            if path != other && (path.starts_with(other) || other.starts_with(path)) {
                return Err(Error::OverlappingRoots {
                    first: prior,
                    second: index,
                });
            }
        }
    }
    let mut names: HashMap<&OsStr, usize> = HashMap::with_capacity(paths.len());
    let mut groups: Vec<Group<'_>> = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let name = path.file_name().expect("preflight");
        if let Some(first) = names.insert(name, index) {
            return Err(Error::DuplicateDestination {
                first,
                second: index,
            });
        }
        let parent = path.parent().expect("preflight");
        if let Some(group) = groups.iter_mut().find(|g| g.parent == parent) {
            group.indices.push(index);
        } else {
            if groups.len() == MAX_PARENTS {
                return Err(Error::TooManyParents);
            }
            groups.push(Group {
                parent,
                indices: vec![index],
            });
        }
    }
    Ok(groups)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_32_parents_and_128_roots_in_stable_order() {
        let paths: Vec<_> = (0..128)
            .map(|i| PathBuf::from(format!("/p{}/item{i}", i % 32)))
            .collect();
        let g = plan(&paths).unwrap();
        assert_eq!(g.len(), 32);
        assert_eq!(g[0].indices, vec![0, 32, 64, 96]);
        assert_eq!(plan(&paths[..129.min(paths.len())]).unwrap().len(), 32);
        let mut many = paths.clone();
        many.push("/p/new".into());
        assert_eq!(plan(&many).unwrap_err(), Error::TooManyRoots);
        let p: Vec<_> = (0..33)
            .map(|i| PathBuf::from(format!("/p{i}/item{i}")))
            .collect();
        assert_eq!(plan(&p).unwrap_err(), Error::TooManyParents);
    }
    #[test]
    fn rejects_same_destination_before_execution() {
        let paths = vec![
            PathBuf::from("/one/a"),
            PathBuf::from("/two/b"),
            PathBuf::from("/two/a"),
        ];
        assert_eq!(
            plan(&paths).unwrap_err(),
            Error::DuplicateDestination {
                first: 0,
                second: 2
            }
        );
    }
    #[test]
    fn rejects_overlapping_roots_regardless_of_order() {
        for paths in [
            vec![PathBuf::from("/a/dir"), PathBuf::from("/a/dir/child")],
            vec![PathBuf::from("/a/dir/child"), PathBuf::from("/a/dir")],
        ] {
            assert_eq!(
                plan(&paths).unwrap_err(),
                Error::OverlappingRoots {
                    first: 0,
                    second: 1
                }
            );
        }
        assert!(plan(&["/a/dir".into(), "/a/dir2/child".into()]).is_ok());
    }
    #[test]
    fn same_device_different_mount_is_ambiguous_even_for_unrelated_names() {
        assert!(ambiguous_mount(10, 7, 10, 8));
        assert!(!ambiguous_mount(10, 7, 10, 7));
        assert!(!ambiguous_mount(10, 7, 11, 8));
        // A submount may bind a subtree even when its device differs from the
        // selected ancestor's device. Device inequality is not independence.
        assert!(directory_mount_ambiguous(7, 8));
        assert!(!directory_mount_ambiguous(7, 7));
    }
    #[test]
    fn invalid_and_oversized_paths_are_rejected() {
        assert_eq!(plan(&[]).unwrap_err(), Error::Empty);
        assert_eq!(
            plan(&["relative/file".into()]).unwrap_err(),
            Error::InvalidPath { index: 0 }
        );
        assert_eq!(
            plan(&["/a/../b".into()]).unwrap_err(),
            Error::InvalidPath { index: 0 }
        );
        assert_eq!(
            plan(&[PathBuf::from(format!("/{}", "x".repeat(4096)))]).unwrap_err(),
            Error::InvalidPath { index: 0 }
        );
    }
    #[cfg(unix)]
    #[test]
    fn preserves_raw_names_and_compares_without_loss() {
        use std::os::unix::ffi::OsStringExt;
        let mut a = PathBuf::from("/one");
        a.push(std::ffi::OsString::from_vec(vec![0xff]));
        let mut b = PathBuf::from("/two");
        b.push(std::ffi::OsString::from_vec(vec![0xfe]));
        let paths = vec![a, b];
        let groups = plan(&paths).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            paths[groups[0].indices[0]]
                .file_name()
                .unwrap()
                .as_encoded_bytes(),
            &[0xff]
        );
    }
}

/// Provisional read-only proofs. Parent `..` traversal does not establish
/// independence across all bind mounts. Never use this as authorization for a
/// copy/move until mount topology checks and effective-FD validation are wired.
#[derive(Debug)]
pub struct CapturedGroup {
    pub source: std::sync::Arc<crate::selection_source::Source>,
    pub indices: Vec<usize>,
}
#[derive(Debug)]
pub struct Captured {
    groups: Vec<CapturedGroup>,
    retained_bytes: usize,
    mounts: Vec<(u64, Vec<u64>)>,
    ancestors: Vec<Vec<crate::selection_source::ObjectIdentity>>,
}
impl Captured {
    fn validate_destination_mount(&self, mount: u64) -> std::io::Result<()> {
        for (group_index, group) in self.groups.iter().enumerate() {
            for i in 0..group.indices.len() {
                let identity = group
                    .source
                    .root_identity(i)
                    .ok_or_else(|| std::io::Error::other("Firma raíz ausente"))?;
                if identity.directory
                    && directory_mount_ambiguous(self.mounts[group_index].1[i], mount)
                {
                    return Err(std::io::Error::other(
                        "Directorio y destino en montajes distintos; posible alias",
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn groups(&self) -> &[CapturedGroup] {
        &self.groups
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    /// Revalidate the captured bindings/topology on the worker immediately
    /// before preparation; effective motor FDs still require their own proof.
    pub fn verify(&self, cancel: &std::sync::atomic::AtomicBool) -> std::io::Result<()> {
        for (index, group) in self.groups.iter().enumerate() {
            group.source.verify(cancel)?;
            if group.source.mount_ids(cancel)? != self.mounts[index]
                || group.source.parent_ancestors(cancel)? != self.ancestors[index]
            {
                return Err(std::io::Error::other(
                    "Topología de origen cambió; revisa el lote",
                ));
            }
            group.source.verify(cancel)?;
        }
        Ok(())
    }
    /// Revalidate only roots still pending, using their ORIGINAL identity.
    /// A completed root may no longer exist at the old name. No root is
    /// silently recaptured; caller owns the confirmed/uncertain ledger.
    pub fn verify_pending(
        &self,
        pending: &[bool],
        cancel: &std::sync::atomic::AtomicBool,
    ) -> std::io::Result<()> {
        let count: usize = self.groups.iter().map(|g| g.indices.len()).sum();
        if pending.len() != count {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Ledger multipadre incompatible",
            ));
        }
        for (group_index, group) in self.groups.iter().enumerate() {
            let local: Vec<usize> = group
                .indices
                .iter()
                .enumerate()
                .filter_map(|(i, &original)| pending[original].then_some(i))
                .collect();
            if local.is_empty() {
                continue;
            }
            group.source.verify_subset(
                local.iter().map(|&i| group.source.names()[i].as_os_str()),
                cancel,
            )?;
            let (parent, mounts) = group.source.mount_ids_subset(&local, cancel)?;
            if parent != self.mounts[group_index].0
                || local
                    .iter()
                    .zip(mounts.iter())
                    .any(|(&i, &mount)| self.mounts[group_index].1[i] != mount)
                || group.source.parent_ancestors(cancel)? != self.ancestors[group_index]
            {
                return Err(std::io::Error::other(
                    "Topología pendiente cambió; revisa el lote",
                ));
            }
            group.source.verify_subset(
                local.iter().map(|&i| group.source.names()[i].as_os_str()),
                cancel,
            )?;
        }
        Ok(())
    }
    pub fn fd_count(&self) -> usize {
        self.groups.iter().map(|g| g.source.fd_count()).sum()
    }
    /// Verification/ancestry uses at most two additional temporary descriptors.
    pub fn fd_reservation(&self) -> usize {
        self.fd_count().saturating_add(2)
    }
    /// Compare the descriptors the engine actually captured with this group's
    /// original proof. Engine still validates pathname binding at execution.
    pub fn verify_effective(
        &self,
        group: usize,
        parent: &std::fs::File,
        name: &OsStr,
        root: &std::fs::File,
    ) -> std::io::Result<()> {
        self.groups
            .get(group)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Grupo de origen obsoleto")
            })?
            .source
            .verify_captured(parent, name, root)
    }
}
/// Same borrowed reservation used by capture; includes source/topology scratch.
pub fn capture_reservation(paths: &[PathBuf]) -> Result<(usize, usize), Error> {
    let groups = plan(paths)?;
    let mut reserve = 128usize * 1024;
    for g in &groups {
        reserve = reserve
            .checked_add(g.parent.as_os_str().len().saturating_mul(2) + 1024)
            .ok_or(Error::TooManyBytes)?;
        for &i in &g.indices {
            reserve = reserve
                .checked_add(paths[i].file_name().expect("plan").len().saturating_mul(2) + 256)
                .ok_or(Error::TooManyBytes)?;
        }
    }
    if reserve > 2 * 1024 * 1024 {
        return Err(Error::TooManyBytes);
    }
    Ok((reserve, paths.len() + groups.len() + 2))
}
pub fn capture_provisional(
    paths: &[PathBuf],
    cancel: &std::sync::atomic::AtomicBool,
) -> std::io::Result<Captured> {
    use std::{
        io,
        sync::{Arc, atomic::Ordering},
    };
    let groups = plan(paths).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    // Estimate all input names, parent copies, descriptor records and scratch
    // before creating any source; capacities are checked again below.
    let reserve = capture_reservation(paths)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
        .0;
    let mut captured = Vec::with_capacity(groups.len());
    let mut retained =
        std::mem::size_of::<Captured>() + groups.len() * std::mem::size_of::<CapturedGroup>();
    for group in groups {
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Captura multipadre cancelada",
            ));
        }
        let names: Arc<[std::ffi::OsString]> = group
            .indices
            .iter()
            .map(|&i| paths[i].file_name().expect("plan").to_os_string())
            .collect::<Vec<_>>()
            .into();
        let source = Arc::new(crate::selection_source::Source::capture(
            group.parent.to_path_buf(),
            names,
            cancel,
        )?);
        retained = retained
            .checked_add(source.retained_bytes())
            .and_then(|n| n.checked_add(group.indices.capacity() * std::mem::size_of::<usize>()))
            .filter(|n| *n <= reserve)
            .ok_or_else(|| io::Error::other("Capacidad multipadre excede reserva"))?;
        captured.push(CapturedGroup {
            source,
            indices: group.indices,
        });
    }
    let mut roots = Vec::with_capacity(paths.len());
    let mut mounts = Vec::with_capacity(captured.len());
    for (group_index, group) in captured.iter().enumerate() {
        group.source.verify(cancel)?;
        let (parent_mount, root_mounts) = group.source.mount_ids(cancel)?;
        for (i, &root_mount) in root_mounts.iter().enumerate() {
            let identity = group
                .source
                .root_identity(i)
                .ok_or_else(|| io::Error::other("Identidad raíz no disponible"))?;
            if roots.iter().any(
                |(_, previous, _): &(usize, crate::selection_source::ObjectIdentity, u64)| {
                    previous.device == identity.device && previous.inode == identity.inode
                },
            ) {
                return Err(io::Error::other(
                    "Raíces comparten identidad física; lote no admitido",
                ));
            }
            roots.push((group_index, identity, root_mount));
        }
        mounts.push((parent_mount, root_mounts));
    }
    let mut parents = Vec::with_capacity(captured.len());
    let mut ancestor_snapshot = Vec::with_capacity(captured.len());
    for (index, group) in captured.iter().enumerate() {
        let ancestors = group.source.parent_ancestors(cancel)?;
        let parent = *ancestors
            .first()
            .ok_or_else(|| io::Error::other("Padre no disponible"))?;
        if parents
            .iter()
            .any(|p: &crate::selection_source::ObjectIdentity| {
                p.device == parent.device && p.inode == parent.inode
            })
        {
            return Err(io::Error::other(
                "Padres son aliases físicos; lote no admitido",
            ));
        }
        for (prior, other) in parents.iter().enumerate() {
            if ambiguous_mount(
                other.device,
                mounts[prior].0,
                parent.device,
                mounts[index].0,
            ) {
                return Err(io::Error::other(
                    "Mismo dispositivo en montajes distintos; alias ambiguo",
                ));
            }
        }
        if roots.iter().any(|(_, root, mount)| {
            root.directory && directory_mount_ambiguous(*mount, mounts[index].0)
        }) {
            return Err(io::Error::other(
                "Directorio seleccionado y padre en montajes distintos; alias ambiguo",
            ));
        }
        parents.push(parent);
        if roots.iter().any(|(other, root, _)| {
            *other != index
                && root.directory
                && ancestors
                    .iter()
                    .any(|a| a.device == root.device && a.inode == root.inode)
        }) {
            return Err(io::Error::other(
                "Raíces se solapan físicamente; lote no admitido",
            ));
        }
        ancestor_snapshot.push(ancestors);
    }
    for (index, group) in captured.iter().enumerate() {
        group.source.verify(cancel)?;
        if group.source.mount_ids(cancel)? != mounts[index] {
            return Err(io::Error::other(
                "Montaje cambió durante captura; lote no admitido",
            ));
        }
    }
    retained = retained
        .checked_add(mounts.capacity() * std::mem::size_of::<(u64, Vec<u64>)>())
        .and_then(|n| {
            n.checked_add(
                mounts
                    .iter()
                    .map(|(_, v)| v.capacity() * std::mem::size_of::<u64>())
                    .sum::<usize>(),
            )
        })
        .and_then(|n| {
            n.checked_add(
                ancestor_snapshot.capacity()
                    * std::mem::size_of::<Vec<crate::selection_source::ObjectIdentity>>(),
            )
        })
        .and_then(|n| {
            n.checked_add(
                ancestor_snapshot
                    .iter()
                    .map(|v| {
                        v.capacity()
                            * std::mem::size_of::<crate::selection_source::ObjectIdentity>()
                    })
                    .sum::<usize>(),
            )
        })
        .filter(|n| *n <= reserve)
        .ok_or_else(|| io::Error::other("Topología excede reserva multipadre"))?;
    Ok(Captured {
        groups: captured,
        retained_bytes: retained,
        mounts,
        ancestors: ancestor_snapshot,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod capture_tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::symlink,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let p = crate::theme::home().join(".cache/files-test").join(format!(
                "multi-origin-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn captures_32_parents_and_all_roots_without_mutation() {
        let f = Fixture::new();
        let mut paths = Vec::new();
        for group in 0..32 {
            let dir = f.0.join(format!("p{group}"));
            fs::create_dir(&dir).unwrap();
            for n in 0..4 {
                let p = dir.join(format!("f{group}-{n}"));
                fs::write(&p, b"unchanged").unwrap();
                paths.push(p);
            }
        }
        let destination = f.0.join("destination");
        fs::create_dir(&destination).unwrap();
        let c = std::sync::Arc::new(capture_provisional(&paths, &AtomicBool::new(false)).unwrap());
        assert_eq!(c.groups.len(), 32);
        assert_eq!(c.groups.iter().map(|g| g.indices.len()).sum::<usize>(), 128);
        assert!(c.retained_bytes < 2 * 1024 * 1024);
        assert!(paths.iter().all(|p| fs::read(p).unwrap() == b"unchanged"));
        c.verify(&AtomicBool::new(false)).unwrap();
        let prepared = prepare_move_batch(
            c.clone(),
            &destination,
            256,
            2 * 1024 * 1024,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(prepared.tokens.len(), 32);
        assert!(prepared.fd_count() <= 256);
        assert!(paths.iter().all(|p| p.exists()));
        drop(prepared);
        let parent = fs::File::open(paths[0].parent().unwrap()).unwrap();
        let root = fs::File::open(&paths[0]).unwrap();
        c.verify_effective(0, &parent, paths[0].file_name().unwrap(), &root)
            .unwrap();
        let retained = paths[0].with_file_name("preserved");
        fs::rename(&paths[0], &retained).unwrap();
        fs::write(&paths[0], b"replacement").unwrap();
        let replacement = fs::File::open(&paths[0]).unwrap();
        assert!(
            c.verify_effective(0, &parent, paths[0].file_name().unwrap(), &replacement)
                .is_err()
        );
        assert!(c.verify(&AtomicBool::new(false)).is_err());
        let mut pending = vec![true; paths.len()];
        pending[0] = false;
        c.verify_pending(&pending, &AtomicBool::new(false)).unwrap();
        fs::write(&paths[1], b"changed remaining").unwrap();
        assert!(c.verify_pending(&pending, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(&retained).unwrap(), b"unchanged");
        assert_eq!(
            capture_provisional(&paths, &AtomicBool::new(true))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn rejects_alias_parents_hardlinks_and_physical_ancestors() {
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(a.join("one"), b"bytes").unwrap();
        fs::write(a.join("two"), b"two").unwrap();
        symlink(&f.0, f.0.join("alias")).unwrap();
        let alias = vec![a.join("one"), f.0.join("alias/a/two")];
        assert!(
            capture_provisional(&alias, &AtomicBool::new(false))
                .unwrap_err()
                .to_string()
                .contains("aliases")
        );
        fs::hard_link(a.join("one"), b.join("other")).unwrap();
        let links = vec![a.join("one"), b.join("other")];
        assert!(
            capture_provisional(&links, &AtomicBool::new(false))
                .unwrap_err()
                .to_string()
                .contains("identidad física")
        );
        fs::create_dir(a.join("dir")).unwrap();
        fs::write(a.join("dir/child"), b"c").unwrap();
        let nested = vec![a.join("dir"), f.0.join("alias/a/dir/child")];
        assert!(
            capture_provisional(&nested, &AtomicBool::new(false))
                .unwrap_err()
                .to_string()
                .contains("solapan físicamente")
        );
        assert_eq!(fs::read(a.join("one")).unwrap(), b"bytes");
    }
    #[test]
    fn prepared_batch_retries_only_pending_original_proofs_after_cancel() {
        use std::sync::Arc;
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        let d = f.0.join("dest");
        for path in [&a, &b, &d] {
            fs::create_dir(path).unwrap();
        }
        let paths = vec![a.join("one"), b.join("two")];
        fs::write(&paths[0], b"one").unwrap();
        fs::write(&paths[1], b"two").unwrap();
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(capture_provisional(&paths, &cancel).unwrap());
        assert!(prepare_move_batch(proof.clone(), &d, 1, 2 * 1024 * 1024, &cancel).is_err());
        assert!(paths.iter().all(|p| p.exists()));
        let batch = prepare_move_batch(proof, &d, 256, 2 * 1024 * 1024, &cancel).unwrap();
        assert_eq!(batch.tokens.len(), 2);
        batch.verify(&cancel).unwrap();
        batch.tokens[0]
            .execute(&crate::copy_engine::Control::default(), |_| {})
            .unwrap();
        let stop = crate::copy_engine::Control::default();
        stop.cancel();
        assert!(batch.tokens[1].execute(&stop, |_| {}).is_err());
        assert!(!paths[0].exists() && paths[1].exists());
        let pending = [false, true];
        batch.verify_pending(&pending, &cancel).unwrap();
        batch.tokens[1]
            .execute(&crate::copy_engine::Control::default(), |_| {})
            .unwrap();
        assert_eq!(fs::read(d.join("one")).unwrap(), b"one");
        assert_eq!(fs::read(d.join("two")).unwrap(), b"two");
        assert!(!paths[0].exists() && !paths[1].exists());
    }
    #[test]
    fn partial_same_parent_retry_skips_confirmed_and_rejects_replacement() {
        use std::sync::Arc;
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        let d = f.0.join("dest");
        for p in [&a, &b, &d] {
            fs::create_dir(p).unwrap();
        }
        let paths = vec![a.join("one"), a.join("two"), b.join("three")];
        for (i, p) in paths.iter().enumerate() {
            fs::write(p, [i as u8]).unwrap();
        }
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(capture_provisional(&paths, &cancel).unwrap());
        let batch = prepare_move_batch(proof, &d, 256, 2 * 1024 * 1024, &cancel).unwrap();
        batch.tokens[0]
            .execute_subset(
                &[true, false],
                &crate::copy_engine::Control::default(),
                |_| {},
            )
            .unwrap();
        let stop = crate::copy_engine::Control::default();
        stop.cancel();
        assert!(
            batch.tokens[0]
                .execute_subset(&[false, true], &stop, |_| {})
                .is_err()
        );
        let pending = [false, true, true];
        batch.verify_pending(&pending, &cancel).unwrap();
        batch.tokens[0]
            .execute_subset(
                &[false, true],
                &crate::copy_engine::Control::default(),
                |_| {},
            )
            .unwrap();
        batch
            .verify_pending(&[false, false, true], &cancel)
            .unwrap();
        let preserved = b.join("preserved");
        fs::rename(&paths[2], &preserved).unwrap();
        fs::write(&paths[2], b"actor").unwrap();
        assert!(
            batch
                .verify_pending(&[false, false, true], &cancel)
                .is_err()
        );
        assert_eq!(fs::read(d.join("one")).unwrap(), [0]);
        assert_eq!(fs::read(d.join("two")).unwrap(), [1]);
        assert_eq!(fs::read(&preserved).unwrap(), [2]);
        assert_eq!(fs::read(&paths[2]).unwrap(), b"actor");
    }
    #[test]
    fn logical_batch_cancels_mid_group_then_retries_without_recapturing() {
        use std::sync::Arc;
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        let d = f.0.join("dest");
        for p in [&a, &b, &d] {
            fs::create_dir(p).unwrap();
        }
        let paths = vec![a.join("one"), a.join("two"), b.join("three")];
        for (i, p) in paths.iter().enumerate() {
            fs::write(p, [i as u8]).unwrap();
        }
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(capture_provisional(&paths, &cancel).unwrap());
        let batch = prepare_move_batch(proof, &d, 256, 2 * 1024 * 1024, &cancel).unwrap();
        let mut ledger = batch.new_ledger();
        let control = crate::copy_engine::Control::default();
        assert!(
            batch
                .execute_pending(&mut ledger, &control, |_| control.cancel())
                .is_err()
        );
        assert_eq!(
            ledger.states(),
            &[RootState::Confirmed, RootState::Pending, RootState::Pending]
        );
        assert!(!paths[0].exists() && paths[1].exists() && paths[2].exists());
        batch
            .execute_pending(&mut ledger, &crate::copy_engine::Control::default(), |_| {})
            .unwrap();
        assert_eq!(
            ledger.states(),
            &[
                RootState::Confirmed,
                RootState::Confirmed,
                RootState::Confirmed
            ]
        );
        assert_eq!(fs::read(d.join("one")).unwrap(), [0]);
        assert_eq!(fs::read(d.join("two")).unwrap(), [1]);
        assert_eq!(fs::read(d.join("three")).unwrap(), [2]);
    }
    #[test]
    fn logical_batch_retry_rejects_replaced_pending_root() {
        use std::sync::Arc;
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        let d = f.0.join("dest");
        for p in [&a, &b, &d] {
            fs::create_dir(p).unwrap();
        }
        let paths = vec![a.join("one"), a.join("two"), b.join("three")];
        for (i, p) in paths.iter().enumerate() {
            fs::write(p, [i as u8]).unwrap();
        }
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(capture_provisional(&paths, &cancel).unwrap());
        let batch = prepare_move_batch(proof, &d, 256, 2 * 1024 * 1024, &cancel).unwrap();
        let mut ledger = batch.new_ledger();
        let control = crate::copy_engine::Control::default();
        assert!(
            batch
                .execute_pending(&mut ledger, &control, |_| control.cancel())
                .is_err()
        );
        let preserved = a.join("preserved");
        fs::rename(&paths[1], &preserved).unwrap();
        fs::write(&paths[1], b"actor").unwrap();
        assert!(
            batch
                .execute_pending(&mut ledger, &crate::copy_engine::Control::default(), |_| {})
                .is_err()
        );
        assert_eq!(
            ledger.states(),
            &[RootState::Confirmed, RootState::Pending, RootState::Pending]
        );
        assert_eq!(fs::read(d.join("one")).unwrap(), [0]);
        assert_eq!(fs::read(&preserved).unwrap(), [1]);
        assert_eq!(fs::read(&paths[1]).unwrap(), b"actor");
        assert_eq!(fs::read(&paths[2]).unwrap(), [2]);
    }
    #[test]
    fn ledger_cannot_be_reused_with_same_proof_and_another_destination() {
        use std::sync::Arc;
        let f = Fixture::new();
        let a = f.0.join("a");
        let d1 = f.0.join("d1");
        let d2 = f.0.join("d2");
        for p in [&a, &d1, &d2] {
            fs::create_dir(p).unwrap();
        }
        let paths = vec![a.join("one"), a.join("two")];
        fs::write(&paths[0], b"one").unwrap();
        fs::write(&paths[1], b"two").unwrap();
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(capture_provisional(&paths, &cancel).unwrap());
        let first = prepare_move_batch(proof.clone(), &d1, 256, 2 * 1024 * 1024, &cancel).unwrap();
        let other = prepare_move_batch(proof, &d2, 256, 2 * 1024 * 1024, &cancel).unwrap();
        let mut ledger = first.new_ledger();
        let control = crate::copy_engine::Control::default();
        assert!(
            first
                .execute_pending(&mut ledger, &control, |_| control.cancel())
                .is_err()
        );
        assert_eq!(ledger.states(), &[RootState::Confirmed, RootState::Pending]);
        assert!(
            other
                .execute_pending(&mut ledger, &crate::copy_engine::Control::default(), |_| {})
                .is_err()
        );
        assert!(d1.join("one").exists() && paths[1].exists());
        assert!(!d2.join("one").exists() && !d2.join("two").exists());
    }
    #[test]
    fn logical_batch_moves_128_roots_from_32_parents_as_one_ledger() {
        use std::sync::Arc;
        let f = Fixture::new();
        let destination = f.0.join("destination");
        fs::create_dir(&destination).unwrap();
        let mut paths = Vec::new();
        for group in 0..32 {
            let dir = f.0.join(format!("group{group}"));
            fs::create_dir(&dir).unwrap();
            for item in 0..4 {
                let p = dir.join(format!("file{group}-{item}"));
                fs::write(&p, [group as u8, item as u8]).unwrap();
                paths.push(p);
            }
        }
        let cancel = AtomicBool::new(false);
        let proof = Arc::new(capture_provisional(&paths, &cancel).unwrap());
        let batch = prepare_move_batch(proof, &destination, 256, 2 * 1024 * 1024, &cancel).unwrap();
        let mut ledger = batch.new_ledger();
        batch
            .execute_pending(&mut ledger, &crate::copy_engine::Control::default(), |_| {})
            .unwrap();
        assert!(ledger.states().iter().all(|s| *s == RootState::Confirmed));
        for (i, path) in paths.iter().enumerate() {
            assert!(!path.exists());
            assert_eq!(
                fs::read(destination.join(path.file_name().unwrap())).unwrap(),
                [(i / 4) as u8, (i % 4) as u8]
            );
        }
    }
}

/// Same-filesystem per-parent tokens, admitted as one logical batch. Preparation
/// is read-only and does not imply a global atomic rename or EXDEV support.
#[derive(Debug)]
pub struct PreparedMoveBatch {
    proof: std::sync::Arc<Captured>,
    batch_id: std::sync::Arc<()>,
    tokens: Vec<crate::move_engine::PreparedMove>,
    metadata_bytes: usize,
    fd_count: usize,
    destination_identity: (u64, u64, u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootState {
    Pending,
    Confirmed,
    Uncertain,
}
#[derive(Debug)]
pub struct MoveLedger {
    proof: std::sync::Arc<Captured>,
    batch_id: std::sync::Arc<()>,
    states: Vec<RootState>,
}
impl MoveLedger {
    pub fn states(&self) -> &[RootState] {
        &self.states
    }
}
#[derive(Debug)]
pub enum BatchFailure {
    Review(std::io::Error),
    Engine {
        group: usize,
        error: Box<crate::copy_engine::CopyError>,
    },
}
impl PreparedMoveBatch {
    /// Borrow the exact engine requests captured during review. The caller may
    /// display or account for them without reconstructing paths from labels.
    pub fn requests(&self) -> impl Iterator<Item = &crate::copy_engine::CopyRequest> {
        self.tokens.iter().flat_map(|token| token.paths().iter())
    }
    pub fn request_at_original(&self, original: usize) -> Option<&crate::copy_engine::CopyRequest> {
        self.proof
            .groups()
            .iter()
            .zip(&self.tokens)
            .find_map(|(group, token)| {
                group
                    .indices
                    .iter()
                    .position(|index| *index == original)
                    .map(|index| &token.paths()[index])
            })
    }
    pub fn proof(&self) -> &std::sync::Arc<Captured> {
        &self.proof
    }
    pub fn metadata_bytes(&self) -> usize {
        self.metadata_bytes
    }
    pub fn fd_count(&self) -> usize {
        self.fd_count
    }
    pub fn new_ledger(&self) -> MoveLedger {
        let count = self.proof.groups().iter().map(|g| g.indices.len()).sum();
        MoveLedger {
            proof: self.proof.clone(),
            batch_id: self.batch_id.clone(),
            states: vec![RootState::Pending; count],
        }
    }
    /// One logical batch, sequential groups, no global rollback. A root becomes
    /// Confirmed only from the engine's post-rename identity-verified callback.
    /// Uncertain publication blocks retry; it must be inspected, never replayed.
    pub fn execute_pending(
        &self,
        ledger: &mut MoveLedger,
        control: &crate::copy_engine::Control,
        progress: impl FnMut(crate::copy_engine::CopyProgress<'_>),
    ) -> Result<(), BatchFailure> {
        self.execute_pending_reports(ledger, control, progress, |_| {})
    }
    /// Borrow each effective per-group report, including a failed group, without
    /// retaining 32 full reports or inferring paths from the requested strings.
    pub fn execute_pending_reports(
        &self,
        ledger: &mut MoveLedger,
        control: &crate::copy_engine::Control,
        mut progress: impl FnMut(crate::copy_engine::CopyProgress<'_>),
        mut observed: impl FnMut(&crate::copy_engine::CopyReport),
    ) -> Result<(), BatchFailure> {
        if !std::sync::Arc::ptr_eq(&self.proof, &ledger.proof)
            || !std::sync::Arc::ptr_eq(&self.batch_id, &ledger.batch_id)
            || ledger.states.contains(&RootState::Uncertain)
        {
            return Err(BatchFailure::Review(std::io::Error::other(
                "Ledger ajeno o publicación no confirmada; revisa antes de reintentar",
            )));
        }
        for (group_index, (group, token)) in
            self.proof.groups().iter().zip(&self.tokens).enumerate()
        {
            let pending: Vec<bool> = ledger
                .states
                .iter()
                .map(|s| *s == RootState::Pending)
                .collect();
            self.verify_pending(&pending, control.cancellation_flag())
                .map_err(BatchFailure::Review)?;
            let local: Vec<bool> = group.indices.iter().map(|&i| pending[i]).collect();
            if !local.iter().any(|p| *p) {
                continue;
            }
            let mut unknown_callback = false;
            let result = token.execute_subset(&local, control, |event| {
                if let Some(index) = token.paths().iter().position(|r| r.source == event.source) {
                    ledger.states[group.indices[index]] = RootState::Confirmed;
                } else {
                    unknown_callback = true;
                }
                progress(event);
            });
            match &result {
                Ok(report) => observed(report),
                Err(error) => observed(&error.report),
            }
            if unknown_callback {
                for &original in &group.indices {
                    if ledger.states[original] == RootState::Pending {
                        ledger.states[original] = RootState::Uncertain;
                    }
                }
                return Err(BatchFailure::Review(std::io::Error::other(
                    "Resultado de movimiento no identificado; inspecciona el lote",
                )));
            }
            if let Err(error) = result {
                if error.cleanup_unresolved_entries > 0
                    && let Some(&original) = group
                        .indices
                        .iter()
                        .find(|&&i| ledger.states[i] == RootState::Pending)
                {
                    ledger.states[original] = RootState::Uncertain;
                }
                return Err(BatchFailure::Engine {
                    group: group_index,
                    error: Box::new(error),
                });
            }
        }
        Ok(())
    }
    /// Recheck original proof and all captured engine FDs before execution.
    pub fn verify(&self, cancel: &std::sync::atomic::AtomicBool) -> std::io::Result<()> {
        self.proof.verify(cancel)?;
        for (group, token) in self.proof.groups().iter().zip(&self.tokens) {
            if token.destination_mount_identity()? != self.destination_identity {
                return Err(std::io::Error::other("Destino cambió desde la revisión"));
            }
            token.verify_selection(&group.source, cancel)?;
        }
        self.proof.verify(cancel)
    }
    pub fn verify_pending(
        &self,
        pending: &[bool],
        cancel: &std::sync::atomic::AtomicBool,
    ) -> std::io::Result<()> {
        self.proof.verify_pending(pending, cancel)?;
        for (group, token) in self.proof.groups().iter().zip(&self.tokens) {
            if token.destination_mount_identity()? != self.destination_identity {
                return Err(std::io::Error::other("Destino cambió desde la revisión"));
            }
            let indices: Vec<usize> = group
                .indices
                .iter()
                .enumerate()
                .filter_map(|(i, &original)| pending[original].then_some(i))
                .collect();
            token.verify_selection_indices(&group.source, &indices, cancel)?;
        }
        self.proof.verify_pending(pending, cancel)
    }
}
/// Reserve the entire logical batch before opening a single new engine FD.
/// Available budget is supplied by the caller after accounting for queue,
/// review, unacknowledged results, other workers and scratch.
pub fn prepare_move_batch(
    proof: std::sync::Arc<Captured>,
    destination: &Path,
    available_fds: usize,
    available_metadata: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> std::io::Result<PreparedMoveBatch> {
    use std::{io, sync::atomic::Ordering};
    if !destination.is_absolute() || destination.as_os_str().len() > MAX_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Destino multipadre inválido",
        ));
    }
    let mut roots = 0usize;
    let mut bytes = proof.retained_bytes().saturating_add(64 * 1024);
    for group in proof.groups() {
        bytes = bytes
            .checked_add(8192)
            .ok_or_else(|| io::Error::other("Presupuesto multipadre excedido"))?;
        for name in group.source.names().iter() {
            roots += 1;
            let to = destination
                .as_os_str()
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_add(name.len()))
                .filter(|n| *n <= MAX_PATH_BYTES)
                .ok_or_else(|| io::Error::other("Ruta destino supera4096B"))?;
            let from = group
                .source
                .parent()
                .as_os_str()
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_add(name.len()))
                .ok_or_else(|| io::Error::other("Ruta origen supera4096B"))?;
            bytes = bytes
                .checked_add((from + to).saturating_mul(2) + 512)
                .ok_or_else(|| io::Error::other("Presupuesto multipadre excedido"))?;
        }
    }
    let fd_reservation = proof
        .fd_reservation()
        .checked_add(roots)
        .and_then(|n| n.checked_add(proof.groups().len() * 2))
        .ok_or_else(|| io::Error::other("Descriptores multipadre excedidos"))?;
    if fd_reservation > available_fds || bytes > available_metadata {
        return Err(io::Error::other(
            "La revisión multipadre excede presupuesto o descriptores; espera o reduce lote",
        ));
    }
    proof.verify(cancel)?;
    let mut tokens = Vec::with_capacity(proof.groups().len());
    let mut destination_identity = None;
    for group in proof.groups() {
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Revisión multipadre cancelada",
            ));
        }
        let paths = group
            .source
            .names()
            .iter()
            .map(|name| crate::copy_engine::CopyRequest {
                source: group.source.parent().join(name),
                destination: destination.join(name),
            })
            .collect();
        let token = crate::move_engine::PreparedMove::capture(paths, cancel)?;
        let identity = token.destination_mount_identity()?;
        if destination_identity.is_some_and(|old| old != identity) {
            return Err(io::Error::other(
                "Destino cambiado entre grupos; lote no admitido",
            ));
        }
        proof.validate_destination_mount(identity.2)?;
        destination_identity = Some(identity);
        token.verify_selection(&group.source, cancel)?;
        tokens.push(token);
    }
    let result = PreparedMoveBatch {
        proof,
        batch_id: std::sync::Arc::new(()),
        tokens,
        metadata_bytes: bytes,
        fd_count: fd_reservation,
        destination_identity: destination_identity
            .ok_or_else(|| io::Error::other("Destino ausente"))?,
    };
    result.verify(cancel)?;
    Ok(result)
}
