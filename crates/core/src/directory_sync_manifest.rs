//! Fresh read-only execution-manifest evidence. Roots/parents are pinned; leaf
//! revisions/content/metadata/topology are rewalked. No Apply token, staging,
//! journal or mutation. Never construct this from the installed dry-run report.
use crate::{
    directory_sync_wire::Intent,
    move_manifest::{Entry, Kind},
    selection_source::ObjectIdentity,
};
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub phase: u8,
    pub side: usize,
    pub entries: usize,
    pub payload_read: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub side: usize,
    pub index: usize,
}
#[derive(Debug)]
pub struct LinkGroup {
    pub device: u64,
    pub inode: u64,
    pub kind: Kind,
    pub observed_links: u64,
    pub occurrences: Vec<Location>,
    /// Number, not locations, of names outside both complete inventories.
    pub external_links: u64,
}
fn bad(s: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Captura de manifiesto cancelada",
        ))
    } else {
        Ok(())
    }
}
fn disjoint(roots: [ObjectIdentity; 2], ancestors: [&[ObjectIdentity]; 2]) -> io::Result<()> {
    if roots.iter().any(|r| !r.directory)
        || roots[0] == roots[1]
        || (0..2).any(|side| ancestors[side].contains(&roots[1 - side]))
    {
        return Err(bad(
            "Raíces coincidentes, enlazadas o contenidas físicamente",
        ));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::{
        exdev_source::{self, Inventory},
        relative_source::DirectoryIdentity,
        selection_source::Source,
    };
    use std::{
        collections::{HashMap, HashSet},
        ffi::OsString,
        path::{Path, PathBuf},
        sync::Arc,
    };
    struct Root {
        original: PathBuf,
        source: Source,
        inventory: Option<Inventory>,
        mounts: (u64, Vec<u64>),
        ancestors: Vec<ObjectIdentity>,
    }
    impl Root {
        fn pin(path: &Path, cancel: &AtomicBool) -> io::Result<Self> {
            let parent = path
                .parent()
                .ok_or_else(|| bad("La raíz de filesystem contiene cualquier otra raíz"))?;
            let name = path.file_name().ok_or_else(|| bad("Raíz sin nombre"))?;
            let names: Arc<[OsString]> = vec![name.to_owned()].into();
            let source = Source::capture(std::fs::canonicalize(parent)?, names, cancel)?;
            if !source.root_identity(0).is_some_and(|i| i.directory) {
                return Err(bad("La raíz no es un directorio sin enlace final"));
            }
            let mounts = source.mount_ids(cancel)?;
            let ancestors = source.parent_ancestors(cancel)?;
            Ok(Self {
                original: path.into(),
                source,
                inventory: None,
                mounts,
                ancestors,
            })
        }
        fn identity(&self) -> ObjectIdentity {
            self.source.root_identity(0).expect("one pinned root")
        }
        fn verify_binding(&self, cancel: &AtomicBool) -> io::Result<()> {
            check(cancel)?;
            let parent = std::fs::canonicalize(
                self.original
                    .parent()
                    .ok_or_else(|| bad("Raíz sin padre"))?,
            )?;
            if parent != self.source.parent() {
                return Err(bad("El alias del padre fue redirigido"));
            }
            self.source.verify(cancel)?;
            if self.source.mount_ids(cancel)? != self.mounts
                || self.source.parent_ancestors(cancel)? != self.ancestors
            {
                return Err(bad("Topología de raíz o ancestros cambió"));
            }
            // Reopen the current pathname, not just the held FD: a bind mount
            // over the parent can preserve dev/ino while changing mount binding.
            let named = Source::capture(parent, self.source.names().clone(), cancel)?;
            if named.root_identity(0) != Some(self.identity())
                || named.mount_ids(cancel)? != self.mounts
                || named.parent_ancestors(cancel)? != self.ancestors
            {
                return Err(bad(
                    "La ruta actual no corresponde al padre o montaje fijado",
                ));
            }
            if let Some(i) = &self.inventory {
                self.source.verify_captured(
                    self.source.parent_handle(),
                    &self.source.names()[0],
                    i.root(),
                )?;
                named.verify_captured(named.parent_handle(), &named.names()[0], i.root())?;
            }
            Ok(())
        }
    }
    /// Mutable execution data forked from original capture, preserving canonical
    /// symlink access evidence. This is not fresh approval or effect authority.
    pub struct NamespaceSnapshot {
        pub inventory: Inventory,
        pub parent: std::fs::File,
        pub name: OsString,
        pub payload_read: u64,
    }
    /// Not serializable and not a mutation token. The two pinned roots and their
    /// parents remain owned until drop. Leaves are acquired/revalidated by walks;
    /// future effects must reacquire and check them immediately before/after I/O.
    pub struct Prepared {
        intent: Intent,
        roots: [Root; 2],
        links: Vec<LinkGroup>,
        payload_read: u64,
    }
    /// A matching record plus revalidated live pins, not operator approval or a
    /// durable barrier. It cannot be decoded and borrows both owners.
    pub struct BoundRecord<'a> {
        prepared: &'a Prepared,
        record: &'a crate::directory_sync_record::Record,
    }
    impl BoundRecord<'_> {
        pub fn prepared(&self) -> &Prepared {
            self.prepared
        }
        pub fn record(&self) -> &crate::directory_sync_record::Record {
            self.record
        }
        pub fn evidence_key(
            &self,
            job: [u8; 16],
        ) -> io::Result<crate::directory_sync_record::EvidenceKey> {
            crate::directory_sync_record::EvidenceKey::new(job, self.record.seal())
        }
    }
    impl Prepared {
        pub(crate) fn record_root(
            &self,
            side: usize,
        ) -> Option<crate::directory_sync_record::RootHeader<'_>> {
            let r = self.roots.get(side)?;
            Some(crate::directory_sync_record::RootHeader {
                original: r.original.as_os_str().as_encoded_bytes(),
                parent: r.source.parent().as_os_str().as_encoded_bytes(),
                parent_mount: r.mounts.0,
                ancestors: &r.ancestors,
            })
        }
        pub fn namespace_snapshot(
            &self,
            side: usize,
            read: u64,
            cancel: &AtomicBool,
        ) -> io::Result<NamespaceSnapshot> {
            self.namespace_snapshot_inner(side, read, cancel, None)
        }
        pub fn namespace_snapshot_controlled(
            &self,
            side: usize,
            read: u64,
            control: &crate::copy_engine::Control,
        ) -> io::Result<NamespaceSnapshot> {
            self.namespace_snapshot_inner(side, read, control.cancellation_flag(), Some(control))
        }
        fn namespace_snapshot_inner(
            &self,
            side: usize,
            read: u64,
            cancel: &AtomicBool,
            control: Option<&crate::copy_engine::Control>,
        ) -> io::Result<NamespaceSnapshot> {
            if let Some(control) = control {
                control.checkpoint()?;
            }
            let root = self
                .roots
                .get(side)
                .ok_or_else(|| bad("Side de snapshot inválido"))?;
            root.verify_binding(cancel)?;
            let (inventory, payload_read) =
                root.inventory.as_ref().expect("complete").fork_metadata(
                    root.source.parent_handle(),
                    &root.source.names()[0],
                    read,
                    cancel,
                    control,
                )?;
            let parent = root.source.parent_handle().try_clone()?;
            root.verify_binding(cancel)?;
            Ok(NamespaceSnapshot {
                inventory,
                parent,
                name: root.source.names()[0].clone(),
                payload_read,
            })
        }
        pub fn record(&self) -> io::Result<crate::directory_sync_record::Record> {
            crate::directory_sync_record::Record::from_prepared(self)
        }
        pub fn bind_record<'a>(
            &'a self,
            record: &'a crate::directory_sync_record::Record,
            cancel: &AtomicBool,
        ) -> io::Result<BoundRecord<'a>> {
            check(cancel)?;
            if self.record()?.bytes() != record.bytes() {
                return Err(bad("Registro distinto de la captura fijada"));
            }
            self.verify(cancel)?;
            Ok(BoundRecord {
                prepared: self,
                record,
            })
        }
        pub fn intent(&self) -> &Intent {
            &self.intent
        }
        pub fn entries(&self, side: usize) -> Option<&[Entry]> {
            Some(self.roots.get(side)?.inventory.as_ref()?.entries())
        }
        pub fn mount_ids(&self, side: usize) -> Option<&[u64]> {
            self.roots.get(side)?.inventory.as_ref()?.mounts()
        }
        pub fn relative(&self, side: usize, index: usize) -> Option<&[u8]> {
            let root = self.roots.get(side)?;
            let entry = root.inventory.as_ref()?.entries().get(index)?;
            let prefix = root.source.names()[0].as_encoded_bytes().len();
            if entry.relative.len() == prefix {
                Some(&[])
            } else {
                entry.relative.get(prefix + 1..)
            }
        }
        pub(crate) fn metadata_values(
            &self,
            location: Location,
            cancel: &AtomicBool,
        ) -> io::Result<crate::directory_sync_metadata::Values> {
            check(cancel)?;
            let root = self
                .roots
                .get(location.side)
                .ok_or_else(|| bad("Lado inválido"))?;
            root.verify_binding(cancel)?;
            let values = root
                .inventory
                .as_ref()
                .expect("complete inventory")
                .metadata_values_for_sync(
                    root.source.parent_handle(),
                    &root.source.names()[0],
                    location.index,
                    cancel,
                )?;
            root.verify_binding(cancel)?;
            Ok(values)
        }
        pub fn links(&self) -> &[LinkGroup] {
            &self.links
        }
        pub fn payload_read(&self) -> u64 {
            self.payload_read
        }
        /// Two parent Source pins plus two Inventory root pins. Temporary walk/
        /// ancestry descriptors are separate; this is not a measured FD peak.
        pub fn retained_fd_count(&self) -> usize {
            4
        }
        pub fn verify(&self, cancel: &AtomicBool) -> io::Result<u64> {
            check(cancel)?;
            disjoint(
                [self.roots[0].identity(), self.roots[1].identity()],
                [&self.roots[0].ancestors, &self.roots[1].ancestors],
            )?;
            let mut payload_read = 0;
            for root in &self.roots {
                check(cancel)?;
                root.verify_binding(cancel)?;
                payload_read += root
                    .inventory
                    .as_ref()
                    .expect("complete prepared inventory")
                    .verify_for_sync(
                        root.source.parent_handle(),
                        &root.source.names()[0],
                        self.intent.read_limit - payload_read,
                        cancel,
                    )?;
                root.verify_binding(cancel)?;
            }
            check(cancel)?;
            Ok(payload_read)
        }
    }
    pub fn capture(
        mut intent: Intent,
        cancel: &AtomicBool,
        mut progress: impl FnMut(Progress) -> io::Result<()>,
    ) -> io::Result<Prepared> {
        intent.validate()?;
        check(cancel)?;
        // Normalize lexical separators/dots before splitting parent/name. '..'
        // was rejected by Intent; raw bytes and ancestor symlinks are retained.
        intent.left = intent.left.components().collect();
        intent.right = intent.right.components().collect();
        intent.validate()?;
        let mut roots = Vec::new();
        for (side, path) in [&intent.left, &intent.right].into_iter().enumerate() {
            progress(Progress {
                phase: 0,
                side,
                entries: 0,
                payload_read: 0,
            })?;
            check(cancel)?;
            let root = Root::pin(path, cancel)?;
            if intent.expected.is_some_and(|ids| {
                ids[side]
                    != DirectoryIdentity {
                        device: root.identity().device,
                        inode: root.identity().inode,
                    }
            }) {
                return Err(bad("Raíz capturada no corresponde a la intención"));
            }
            roots.push(root);
        }
        let mut roots: [Root; 2] = roots.try_into().unwrap_or_else(|_| unreachable!());
        disjoint(
            [roots[0].identity(), roots[1].identity()],
            [&roots[0].ancestors, &roots[1].ancestors],
        )?;
        let mut payload_read = 0;
        let mut count = 0;
        for (side, root) in roots.iter_mut().enumerate() {
            root.verify_binding(cancel)?;
            let inventory = exdev_source::capture_for_sync(
                root.source.parent_handle(),
                &root.source.names()[0],
                intent.read_limit - payload_read,
                cancel,
            )?;
            root.source.verify_captured(
                root.source.parent_handle(),
                &root.source.names()[0],
                inventory.root(),
            )?;
            payload_read += inventory.payload_read();
            count += inventory.entries().len();
            root.inventory = Some(inventory);
            progress(Progress {
                phase: 1,
                side,
                entries: count,
                payload_read,
            })?;
            check(cancel)?;
        }
        let mut directories = HashSet::new();
        let mut groups: HashMap<(u64, u64), LinkGroup> = HashMap::new();
        for (side, root) in roots.iter().enumerate() {
            for (index, e) in root
                .inventory
                .as_ref()
                .unwrap()
                .entries()
                .iter()
                .enumerate()
            {
                check(cancel)?;
                let id = (e.identity.device, e.identity.inode);
                if e.identity.kind == Kind::Directory {
                    if !directories.insert(id) {
                        return Err(bad("Alias de directorio entre inventarios"));
                    }
                } else if e.revision.links > 1 {
                    let g = groups.entry(id).or_insert_with(|| LinkGroup {
                        device: id.0,
                        inode: id.1,
                        kind: e.identity.kind,
                        observed_links: e.revision.links,
                        occurrences: Vec::new(),
                        external_links: 0,
                    });
                    if g.kind != e.identity.kind || g.observed_links != e.revision.links {
                        return Err(bad("Identidad hardlink incoherente"));
                    }
                    g.occurrences.push(Location { side, index });
                }
            }
        }
        let mut links: Vec<_> = groups.into_values().collect();
        links.sort_unstable_by_key(|g| (g.device, g.inode));
        for group in &mut links {
            group.external_links = group
                .observed_links
                .checked_sub(group.occurrences.len() as u64)
                .ok_or_else(|| bad("Hardlinks observados exceden nlink"))?;
        }
        let mut result = Prepared {
            intent,
            roots,
            links,
            payload_read,
        };
        for side in 0..2 {
            progress(Progress {
                phase: 2,
                side,
                entries: count,
                payload_read,
            })?;
            check(cancel)?;
            let root = &result.roots[side];
            root.verify_binding(cancel)?;
            payload_read += root.inventory.as_ref().unwrap().verify_for_sync(
                root.source.parent_handle(),
                &root.source.names()[0],
                result.intent.read_limit - payload_read,
                cancel,
            )?;
            root.verify_binding(cancel)?;
        }
        result.payload_read = payload_read;
        progress(Progress {
            phase: 3,
            side: 1,
            entries: count,
            payload_read,
        })?;
        // A consumer may change a root or cancel during the terminal update.
        for root in &result.roots {
            root.verify_binding(cancel)?;
        }
        check(cancel)?;
        Ok(result)
    }
}
#[cfg(target_os = "linux")]
pub use linux::{BoundRecord, NamespaceSnapshot, Prepared, capture};
#[cfg(not(target_os = "linux"))]
pub struct Prepared;
#[cfg(not(target_os = "linux"))]
pub fn capture(
    _: Intent,
    _: &AtomicBool,
    _: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Prepared> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Manifiesto de sincronización pendiente en este sistema",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        directory_sync::{Mode, Side},
        relative_source::DirectoryIdentity,
    };
    use std::{
        fs,
        os::unix::{
            ffi::OsStringExt,
            fs::{MetadataExt, PermissionsExt, symlink},
        },
        path::PathBuf,
        sync::atomic::AtomicU64,
    };
    struct Fixture {
        base: PathBuf,
        paths: [PathBuf; 2],
    }
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "files-sync-manifest-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let paths = [base.join("a"), base.join("b")];
            fs::create_dir_all(&paths[0]).unwrap();
            fs::create_dir(&paths[1]).unwrap();
            Self { base, paths }
        }
        fn intent(&self, read: u64) -> Intent {
            Intent {
                version: 1,
                id: 7,
                left: self.paths[0].clone(),
                right: self.paths[1].clone(),
                mode: Mode::Mirror(Side::Left),
                read_limit: read,
                expected: None,
            }
        }
        fn capture(&self, read: u64) -> Prepared {
            capture(self.intent(read), &AtomicBool::new(false), |_| Ok(()))
                .unwrap_or_else(|e| panic!("capture: {e}"))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn find(p: &Prepared, side: usize, path: &[u8]) -> usize {
        (0..p.entries(side).unwrap().len())
            .find(|&i| p.relative(side, i) == Some(path))
            .unwrap()
    }
    #[test]
    fn raw_complete_evidence_contains_revision_content_metadata_mounts_and_empty_dirs() {
        use sha2::{Digest, Sha256};
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("empty")).unwrap();
        let raw = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        fs::write(f.paths[0].join(&raw), b"data").unwrap();
        symlink(
            std::ffi::OsString::from_vec(b"outside\xff".to_vec()),
            f.paths[0].join("link"),
        )
        .unwrap();
        let before = fs::symlink_metadata(f.paths[0].join(&raw)).unwrap();
        let p = f.capture(8);
        assert_eq!(p.payload_read(), 8);
        assert_eq!(p.retained_fd_count(), 4);
        let e = &p.entries(0).unwrap()[find(&p, 0, b"raw\xff\n")];
        assert_eq!(e.identity.inode, before.ino());
        assert_eq!(e.revision.size, 4);
        assert_eq!(e.content_digest, <[u8; 32]>::from(Sha256::digest(b"data")));
        assert_ne!(e.metadata_digest, [0; 32]);
        assert_eq!(
            p.entries(0).unwrap()[find(&p, 0, b"empty")].identity.kind,
            Kind::Directory
        );
        assert_eq!(
            p.entries(0).unwrap()[find(&p, 0, b"link")].content_digest,
            <[u8; 32]>::from(Sha256::digest(b"outside\xff"))
        );
        for side in 0..2 {
            assert_eq!(
                p.mount_ids(side).unwrap().len(),
                p.entries(side).unwrap().len()
            );
            assert!(p.mount_ids(side).unwrap().iter().all(|&m| m > 0));
            assert_eq!(p.relative(side, 0), Some(b"".as_slice()));
        }
        assert!(p.entries(2).is_none());
        assert!(p.relative(0, usize::MAX).is_none());
        p.verify(&AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(f.paths[0].join(&raw)).unwrap(), b"data");
        assert!(!f.paths[1].join(&raw).exists());
    }
    #[test]
    fn extended_attribute_values_are_part_of_captured_metadata_digest() {
        let f = Fixture::new();
        let path = f.paths[0].join("file");
        fs::write(&path, b"data").unwrap();
        let before = f.capture(8);
        let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let value = b"attribute-value";
        assert_eq!(
            unsafe {
                libc::setxattr(
                    path.as_ptr(),
                    c"user.files-sync-proof".as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            },
            0
        );
        let after = f.capture(8);
        let a = &before.entries(0).unwrap()[find(&before, 0, b"file")];
        let b = &after.entries(0).unwrap()[find(&after, 0, b"file")];
        assert_eq!(a.content_digest, b.content_digest);
        assert_eq!(a.access, b.access);
        assert_ne!(a.metadata_digest, b.metadata_digest);
        assert!(before.verify(&AtomicBool::new(false)).is_err());
        after.verify(&AtomicBool::new(false)).unwrap();
    }
    #[test]
    fn regular_read_budget_is_shared_between_roots_and_rejects_before_large_body() {
        let f = Fixture::new();
        for p in &f.paths {
            fs::write(p.join("file"), b"12345").unwrap();
        }
        assert!(capture(f.intent(9), &AtomicBool::new(false), |_| Ok(())).is_err());
        assert!(capture(f.intent(19), &AtomicBool::new(false), |_| Ok(())).is_err());
        let p = f.capture(20);
        assert_eq!(p.payload_read(), 20);
        p.verify(&AtomicBool::new(false)).unwrap();
        let file = fs::File::create(f.paths[0].join("large")).unwrap();
        file.set_len(1024 * 1024 * 1024).unwrap();
        assert!(capture(f.intent(10), &AtomicBool::new(false), |_| Ok(())).is_err());
        assert_eq!(file.metadata().unwrap().len(), 1024 * 1024 * 1024);
    }
    #[test]
    fn hardlinks_across_roots_and_outside_are_recorded_without_double_counting_within_side() {
        let f = Fixture::new();
        fs::write(f.paths[0].join("first"), b"data").unwrap();
        fs::hard_link(f.paths[0].join("first"), f.paths[0].join("second")).unwrap();
        fs::hard_link(f.paths[0].join("first"), f.paths[1].join("cross")).unwrap();
        fs::hard_link(f.paths[0].join("first"), f.base.join("external")).unwrap();
        let p = f.capture(16);
        assert_eq!(p.payload_read(), 16);
        assert_eq!(p.links().len(), 1);
        let g = &p.links()[0];
        assert_eq!(
            (g.observed_links, g.occurrences.len(), g.external_links),
            (4, 3, 1)
        );
        for location in &g.occurrences {
            assert_eq!(
                p.entries(location.side).unwrap()[location.index]
                    .identity
                    .inode,
                g.inode
            );
        }
        p.verify(&AtomicBool::new(false)).unwrap();
        fs::remove_file(f.base.join("external")).unwrap();
        assert!(p.verify(&AtomicBool::new(false)).is_err());
    }
    #[test]
    fn later_content_metadata_namespace_and_symlink_changes_are_rejected() {
        for case in 0..5 {
            let f = Fixture::new();
            fs::create_dir(f.paths[0].join("nested")).unwrap();
            fs::write(f.paths[0].join("nested/file"), b"left").unwrap();
            symlink("target", f.paths[1].join("link")).unwrap();
            let p = f.capture(8);
            match case {
                0 => fs::write(f.paths[0].join("nested/file"), b"rite").unwrap(),
                1 => fs::set_permissions(
                    f.paths[0].join("nested/file"),
                    fs::Permissions::from_mode(0o600),
                )
                .unwrap(),
                2 => fs::write(f.paths[0].join("nested/new"), b"new").unwrap(),
                3 => {
                    fs::remove_file(f.paths[1].join("link")).unwrap();
                    symlink("other", f.paths[1].join("link")).unwrap();
                }
                _ => {
                    fs::rename(
                        f.paths[0].join("nested/file"),
                        f.paths[0].join("nested/old"),
                    )
                    .unwrap();
                    fs::write(f.paths[0].join("nested/file"), b"left").unwrap();
                }
            }
            assert!(p.verify(&AtomicBool::new(false)).is_err(), "case {case}");
        }
    }
    #[test]
    fn capture_detects_nested_changes_between_the_two_inventories() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("nested")).unwrap();
        fs::write(f.paths[0].join("nested/file"), b"left").unwrap();
        let mut changed = false;
        let result = capture(f.intent(16), &AtomicBool::new(false), |p| {
            if p.phase == 1 && p.side == 1 {
                changed = true;
                fs::write(f.paths[0].join("nested/file"), b"rite").unwrap();
            }
            Ok(())
        });
        assert!(changed && result.is_err());
        assert_eq!(fs::read(f.paths[0].join("nested/file")).unwrap(), b"rite");
    }
    #[test]
    fn ancestor_symlinks_are_supported_but_retargeting_invalidates_original_intent() {
        let f = Fixture::new();
        fs::create_dir_all(f.base.join("real/root")).unwrap();
        fs::write(f.base.join("real/root/file"), b"left").unwrap();
        symlink(f.base.join("real"), f.base.join("alias")).unwrap();
        let mut i = f.intent(8);
        i.left = f.base.join("alias/root");
        let p = capture(i, &AtomicBool::new(false), |_| Ok(()))
            .unwrap_or_else(|e| panic!("parent alias: {e}"));
        p.verify(&AtomicBool::new(false)).unwrap();
        fs::create_dir_all(f.base.join("other/root")).unwrap();
        fs::write(f.base.join("other/root/file"), b"rite").unwrap();
        fs::remove_file(f.base.join("alias")).unwrap();
        symlink(f.base.join("other"), f.base.join("alias")).unwrap();
        assert!(p.verify(&AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(f.base.join("real/root/file")).unwrap(), b"left");
    }
    #[test]
    fn root_and_parent_replacements_and_captured_id_mismatch_are_rejected() {
        for parent in [false, true] {
            let f = Fixture::new();
            let p = f.capture(0);
            if parent {
                fs::rename(&f.base, f.base.with_extension("old")).unwrap();
                fs::create_dir_all(&f.paths[0]).unwrap();
                fs::create_dir(&f.paths[1]).unwrap();
            } else {
                fs::rename(&f.paths[0], f.base.join("old-a")).unwrap();
                fs::create_dir(&f.paths[0]).unwrap();
            }
            assert!(p.verify(&AtomicBool::new(false)).is_err());
            if parent {
                fs::remove_dir_all(f.base.with_extension("old")).unwrap();
            }
        }
        let f = Fixture::new();
        let mut i = f.intent(0);
        i.expected = Some([
            DirectoryIdentity {
                device: 0,
                inode: 1,
            },
            DirectoryIdentity {
                device: 0,
                inode: 2,
            },
        ]);
        assert!(capture(i, &AtomicBool::new(false), |_| Ok(())).is_err());
    }
    #[test]
    fn overlap_final_link_special_object_and_terminal_root_swap_never_produce_manifest() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("nested")).unwrap();
        let mut i = f.intent(0);
        i.right = f.paths[0].join("nested");
        assert!(capture(i, &AtomicBool::new(false), |_| Ok(())).is_err());
        symlink(&f.paths[0], f.base.join("alias")).unwrap();
        let mut i = f.intent(0);
        i.right = f.base.join("alias");
        assert!(capture(i, &AtomicBool::new(false), |_| Ok(())).is_err());
        let fifo =
            std::ffi::CString::new(f.paths[0].join("pipe").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(capture(f.intent(0), &AtomicBool::new(false), |_| Ok(())).is_err());
        fs::remove_file(f.paths[0].join("pipe")).unwrap();
        assert!(
            capture(f.intent(0), &AtomicBool::new(false), |p| {
                if p.phase == 3 {
                    fs::rename(&f.paths[1], f.base.join("old-b")).unwrap();
                    fs::create_dir(&f.paths[1]).unwrap();
                }
                Ok(())
            })
            .is_err()
        );
    }
    #[test]
    fn cancellation_and_one_shot_consumer_failure_are_not_partial_success() {
        let f = Fixture::new();
        let cancel = AtomicBool::new(true);
        assert!(
            matches!(capture(f.intent(0),&cancel,|_|Ok(())),Err(e)if e.kind()==io::ErrorKind::Interrupted)
        );
        cancel.store(false, Ordering::Release);
        assert!(
            matches!(capture(f.intent(0),&cancel,|_|{cancel.store(true,Ordering::Release);Ok(())}),Err(e)if e.kind()==io::ErrorKind::Interrupted)
        );
        cancel.store(false, Ordering::Release);
        let mut failed = false;
        let result = capture(f.intent(0), &cancel, |p| {
            if p.phase == 1 && !failed {
                failed = true;
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "consumer"));
            }
            Ok(())
        });
        assert!(failed && matches!(result,Err(e)if e.kind()==io::ErrorKind::BrokenPipe));
    }
    #[test]
    fn both_full_retained_namespaces_include_root_without_losing_a_child() {
        let f = Fixture::new();
        for (side, path) in f.paths.iter().enumerate() {
            for n in 0..crate::directory_compare::MAX_ITEMS {
                fs::File::create(path.join(format!("{side}{n:04x}{}", "\u{1}".repeat(123))))
                    .unwrap();
            }
        }
        let p = f.capture(0);
        for side in 0..2 {
            assert_eq!(p.entries(side).unwrap().len(), 8193);
            assert_eq!(p.mount_ids(side).unwrap().len(), 8193);
            assert!(
                find(
                    &p,
                    side,
                    format!("{side}1fff{}", "\u{1}".repeat(123)).as_bytes()
                ) > 0
            );
        }
        assert_eq!(p.payload_read(), 0);
    }
    #[test]
    fn independent_roots_on_different_filesystems_are_allowed() {
        if !std::path::Path::new("/dev/shm").is_dir() {
            return;
        }
        let f = Fixture::new();
        let other = PathBuf::from("/dev/shm").join(f.base.file_name().unwrap());
        fs::create_dir(&other).unwrap();
        fs::write(other.join("file"), b"data").unwrap();
        let mut i = f.intent(8);
        i.right = other.clone();
        let result = capture(i, &AtomicBool::new(false), |_| Ok(()));
        fs::remove_dir_all(&other).unwrap();
        let p = result.unwrap_or_else(|e| panic!("independent cross-filesystem capture: {e}"));
        assert_ne!(p.mount_ids(0).unwrap()[0], p.mount_ids(1).unwrap()[0]);
        assert_eq!(p.payload_read(), 8);
    }
}
