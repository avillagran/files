//! Explicit trusted-coordinator stage metadata application; not approval.
use super::*;
use crate::directory_sync_metadata::Values;
#[derive(Debug)]
pub struct StageMetadataFailure {
    pub cause: io::Error,
    /// A metadata syscall was attempted; false is not a general FS freshness proof.
    pub mutation_attempted: bool,
}
impl std::fmt::Display for StageMetadataFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for StageMetadataFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
/// Exact historical metadata readback, not data verification or durability.
#[derive(Debug)]
pub struct AppliedStageMetadata {
    identity: Identity,
    mount: u64,
    revision: Revision,
    digest: [u8; 32],
}
impl AppliedStageMetadata {
    pub fn identity(&self) -> Identity {
        self.identity
    }
    pub fn mount(&self) -> u64 {
        self.mount
    }
    pub fn revision(&self) -> Revision {
        self.revision
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Point {
    BeforeMutation,
    Prepared,
    Owner,
    Writable,
    Attributes,
    Mode,
    Times,
    Verified,
}
impl Inventory {
    /// Apply only to staged objects after coordinator approval/journal intent.
    /// No inventory advances; recapture and global content/metadata confirmation
    /// are required. Errors after mutation preserve partial objects, never undo.
    pub fn apply_staged_metadata(
        &self,
        parent: &File,
        name: &OsStr,
        index: usize,
        desired: &Values,
        cancel: &AtomicBool,
    ) -> Result<AppliedStageMetadata, StageMetadataFailure> {
        self.apply_stage_inner(parent, name, index, desired, cancel, None, |_| Ok(()))
    }
    pub fn apply_staged_metadata_controlled(
        &self,
        parent: &File,
        name: &OsStr,
        index: usize,
        desired: &Values,
        control: &Control,
    ) -> Result<AppliedStageMetadata, StageMetadataFailure> {
        self.apply_stage_inner(
            parent,
            name,
            index,
            desired,
            control.cancellation_flag(),
            Some(control),
            |_| Ok(()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_stage_inner(
        &self,
        root_parent: &File,
        root_name: &OsStr,
        index: usize,
        desired: &Values,
        cancel: &AtomicBool,
        control: Option<&Control>,
        mut hook: impl FnMut(Point) -> io::Result<()>,
    ) -> Result<AppliedStageMetadata, StageMetadataFailure> {
        let mut attempted = false;
        let result = (|| {
            check_control(cancel, control)?;
            drop(desired.encode()?); // Validate the complete bounded raw recipe first.
            let old = self.metadata_values_for_sync(root_parent, root_name, index, cancel)?;
            let e = &self.entries[index];
            if desired.kind() != e.identity.kind || desired.uid() != unsafe { libc::geteuid() } {
                return Err(invalid("Tipo/propietario staged incompatible"));
            }
            // Linux has no safe FD xattr operation on an O_PATH symlink. Never
            // mutate attributes through a replaceable path as a fallback.
            if e.identity.kind == Kind::Symlink
                && (old.attributes() != desired.attributes() || desired.mode() != 0o777)
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Metadatos xattr/modo de symlink requieren una operación FD segura",
                ));
            }
            let (parent, pin, name) = self.pin_sync_path(root_parent, root_name, index, cancel)?;
            let file = if e.identity.kind == Kind::Symlink {
                pin
            } else {
                data(
                    &parent,
                    &name,
                    e.identity,
                    e.identity.kind == Kind::Directory,
                )?
            };
            let guard = || -> io::Result<()> {
                check_control(cancel, control)?;
                let m = file.metadata()?;
                if id(&m)? != e.identity
                    || m.uid() != unsafe { libc::geteuid() }
                    || m.size() != e.revision.size
                    || m.nlink() != e.revision.links
                    || mount_id(&file, None)? != self.mounts.as_ref().expect("sync")[index]
                {
                    return Err(invalid("Objeto staged cambió durante metadatos"));
                }
                Ok(())
            };
            hook(Point::Prepared)?;
            stable(&parent, &name, &file, e.identity, e.revision)?;
            guard()?;
            if old.gid() != desired.gid() {
                hook(Point::BeforeMutation)?;
                guard()?;
                attempted = true;
                if unsafe {
                    libc::fchownat(
                        file.as_raw_fd(),
                        c"".as_ptr(),
                        desired.uid(),
                        desired.gid(),
                        libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            hook(Point::Owner)?;
            guard()?;
            if e.identity.kind != Kind::Symlink {
                hook(Point::BeforeMutation)?;
                guard()?;
                attempted = true;
                if unsafe {
                    libc::fchmod(
                        file.as_raw_fd(),
                        if e.identity.kind == Kind::Directory {
                            0o700
                        } else {
                            0o600
                        },
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                hook(Point::Writable)?;
                guard()?;
                let mut attrs = Vec::new();
                attributes_with(&file, None, cancel, |name, value| {
                    attrs.push((name, value));
                    Ok(())
                })?;
                for (name, _) in attrs.iter().filter(|(name, _)| {
                    !desired
                        .attributes()
                        .iter()
                        .any(|a| a.name() == name.as_bytes())
                }) {
                    guard()?;
                    hook(Point::BeforeMutation)?;
                    guard()?;
                    attempted = true;
                    if unsafe { libc::fremovexattr(file.as_raw_fd(), name.as_ptr()) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                // Access ACL can remove owner write permission: install it last.
                for acl in [false, true] {
                    for a in desired
                        .attributes()
                        .iter()
                        .filter(|a| (a.name() == b"system.posix_acl_access") == acl)
                    {
                        guard()?;
                        hook(Point::BeforeMutation)?;
                        guard()?;
                        attempted = true;
                        let name = c(a.name())?;
                        if unsafe {
                            libc::fsetxattr(
                                file.as_raw_fd(),
                                name.as_ptr(),
                                a.value().as_ptr().cast(),
                                a.value().len(),
                                0,
                            )
                        } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                    }
                }
            }
            hook(Point::Attributes)?;
            guard()?;
            if e.identity.kind != Kind::Symlink {
                hook(Point::BeforeMutation)?;
                guard()?;
                attempted = true;
                if unsafe { libc::fchmod(file.as_raw_fd(), desired.mode()) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            hook(Point::Mode)?;
            guard()?;
            let times = [
                libc::timespec {
                    tv_sec: desired.access().seconds,
                    tv_nsec: i64::from(desired.access().nanos),
                },
                libc::timespec {
                    tv_sec: desired.modified().seconds,
                    tv_nsec: i64::from(desired.modified().nanos),
                },
            ];
            hook(Point::BeforeMutation)?;
            guard()?;
            attempted = true;
            if unsafe {
                libc::utimensat(
                    file.as_raw_fd(),
                    c"".as_ptr(),
                    times.as_ptr(),
                    libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            hook(Point::Times)?;
            guard()?;
            let before = file.metadata()?;
            let after = revision(&before)?;
            stable(&parent, &name, &file, e.identity, after)?;
            let named = if e.identity.kind == Kind::Symlink {
                let mut path = format!("/proc/self/fd/{}/", parent.as_raw_fd()).into_bytes();
                path.extend_from_slice(name.as_bytes());
                Some(c(&path)?)
            } else {
                None
            };
            let mut attrs = Vec::new();
            attributes_with(&file, named.as_ref(), cancel, |n, v| {
                attrs.push((n.as_bytes().to_vec(), v));
                Ok(())
            })?;
            stable(&parent, &name, &file, e.identity, after)?;
            let mut entry = Entry {
                relative: Vec::new(),
                identity: e.identity,
                revision: after,
                access: e.access,
                content_digest: e.content_digest,
                metadata_digest: e.metadata_digest,
            };
            entry.access = AccessTime {
                seconds: before.atime(),
                nanos: u32::try_from(before.atime_nsec()).map_err(|_| invalid("Atime inválido"))?,
            };
            let actual = Values::captured(&entry, entry.access, attrs)?;
            if &actual != desired {
                return Err(invalid("Readback staged no reproduce los valores exactos"));
            }
            hook(Point::Verified)?;
            guard()?;
            super::sync_metadata::pin_entries_after(
                root_parent,
                root_name,
                &self.entries,
                self.mounts.as_ref().expect("sync"),
                index,
                cancel,
                Some(after),
            )?;
            stable(&parent, &name, &file, e.identity, after)?;
            Ok(AppliedStageMetadata {
                identity: e.identity,
                mount: self.mounts.as_ref().expect("sync")[index],
                revision: after,
                digest: actual.evidence_digest(),
            })
        })();
        result.map_err(|cause| StageMetadataFailure {
            cause,
            mutation_attempted: attempted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{PermissionsExt, symlink},
        path::{Path, PathBuf},
        sync::atomic::AtomicU64,
    };
    struct Fixture {
        root: PathBuf,
        parent: File,
    }
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "files-sync-apply-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let parent = File::open(&root).unwrap();
            Self { root, parent }
        }
        fn capture(&self, name: &str) -> Inventory {
            capture_for_sync(
                &self.parent,
                OsStr::new(name),
                4096,
                &AtomicBool::new(false),
            )
            .unwrap()
        }
        fn desired(&self) -> Values {
            self.capture("source")
                .metadata_values_for_sync(
                    &self.parent,
                    OsStr::new("source"),
                    0,
                    &AtomicBool::new(false),
                )
                .unwrap()
        }
        fn attr(&self, path: &str, name: &[u8], value: &[u8]) {
            set_attr(&self.root.join(path), name, value);
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn set_attr(path: &Path, name: &[u8], value: &[u8]) {
        assert_eq!(
            unsafe {
                libc::setxattr(
                    c(path.as_os_str().as_bytes()).unwrap().as_ptr(),
                    c(name).unwrap().as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            },
            0
        );
    }
    fn files(f: &Fixture) {
        for name in ["source", "stage"] {
            fs::write(f.root.join(name), b"payload").unwrap();
        }
        fs::set_permissions(f.root.join("source"), fs::Permissions::from_mode(0o640)).unwrap();
    }
    #[test]
    fn stage_metadata_reproduces_raw_attributes_and_removes_inherited_extras() {
        let f = Fixture::new();
        files(&f);
        f.attr("source", b"user.raw\xff", &[0, 255, 10]);
        f.attr("source", b"user.empty", b"");
        f.attr("stage", b"user.extra", b"remove");
        let desired = f.desired();
        let stage = f.capture("stage");
        let before = stage.entries[0].revision;
        let receipt = stage
            .apply_staged_metadata(
                &f.parent,
                OsStr::new("stage"),
                0,
                &desired,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(receipt.digest(), desired.evidence_digest());
        assert_eq!(receipt.identity(), stage.entries[0].identity);
        assert_eq!(receipt.mount(), stage.mounts.as_ref().unwrap()[0]);
        assert_eq!(stage.entries[0].revision, before);
        assert!(
            stage
                .verify_for_sync(
                    &f.parent,
                    OsStr::new("stage"),
                    4096,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        let fresh = f.capture("stage");
        assert_eq!(
            fresh
                .metadata_values_for_sync(
                    &f.parent,
                    OsStr::new("stage"),
                    0,
                    &AtomicBool::new(false)
                )
                .unwrap(),
            desired
        );
        assert_eq!(fs::read(f.root.join("stage")).unwrap(), b"payload");
    }
    fn acl() -> Vec<u8> {
        let mut v = 2u32.to_le_bytes().to_vec();
        for (tag, perms, id) in [
            (1u16, 7u16, u32::MAX),
            (2, 4, unsafe { libc::geteuid() } + 1),
            (4, 5, u32::MAX),
            (16, 5, u32::MAX),
            (32, 1, u32::MAX),
        ] {
            v.extend_from_slice(&tag.to_le_bytes());
            v.extend_from_slice(&perms.to_le_bytes());
            v.extend_from_slice(&id.to_le_bytes());
        }
        v
    }
    #[test]
    fn stage_metadata_reproduces_access_default_acl_and_final_modes() {
        for directory in [false, true] {
            let f = Fixture::new();
            if directory {
                fs::create_dir(f.root.join("source")).unwrap();
                fs::create_dir(f.root.join("stage")).unwrap();
            } else {
                files(&f);
            }
            f.attr("source", b"system.posix_acl_access", &acl());
            if directory {
                f.attr("source", b"system.posix_acl_default", &acl());
            }
            f.attr("source", b"user.raw", b"value");
            let desired = f.desired();
            let stage = f.capture("stage");
            stage
                .apply_staged_metadata(
                    &f.parent,
                    OsStr::new("stage"),
                    0,
                    &desired,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                f.capture("stage")
                    .metadata_values_for_sync(
                        &f.parent,
                        OsStr::new("stage"),
                        0,
                        &AtomicBool::new(false)
                    )
                    .unwrap(),
                desired
            );
        }
    }
    #[test]
    fn stage_metadata_symlink_times_do_not_follow_or_modify_target() {
        let f = Fixture::new();
        fs::write(f.root.join("target"), b"untouched").unwrap();
        symlink("missing", f.root.join("source")).unwrap();
        symlink("target", f.root.join("stage")).unwrap();
        let desired = f.desired();
        let stage = f.capture("stage");
        let target = fs::symlink_metadata(f.root.join("target")).unwrap();
        stage
            .apply_staged_metadata(
                &f.parent,
                OsStr::new("stage"),
                0,
                &desired,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            fs::read_link(f.root.join("stage")).unwrap(),
            Path::new("target")
        );
        let after = fs::symlink_metadata(f.root.join("target")).unwrap();
        assert_eq!(revision(&target).unwrap(), revision(&after).unwrap());
        assert_eq!(fs::read(f.root.join("target")).unwrap(), b"untouched");
    }
    #[test]
    fn stage_metadata_failure_boundaries_preserve_partial_object_and_original_snapshot() {
        for point in 0..7 {
            let f = Fixture::new();
            files(&f);
            f.attr("source", b"user.a", b"actual");
            let desired = f.desired();
            let stage = f.capture("stage");
            let old = stage.entries[0].revision;
            let result = stage.apply_stage_inner(
                &f.parent,
                OsStr::new("stage"),
                0,
                &desired,
                &AtomicBool::new(false),
                None,
                |at| {
                    let n = match at {
                        Point::BeforeMutation => 99,
                        Point::Prepared => 0,
                        Point::Owner => 1,
                        Point::Writable => 2,
                        Point::Attributes => 3,
                        Point::Mode => 4,
                        Point::Times => 5,
                        Point::Verified => 6,
                    };
                    if n == point {
                        Err(io::Error::other("metadata fault"))
                    } else {
                        Ok(())
                    }
                },
            );
            let err = result.unwrap_err();
            assert_eq!(err.mutation_attempted, point >= 2);
            assert_eq!(stage.entries[0].revision, old);
            assert!(f.root.join("stage").exists());
            assert_eq!(fs::read(f.root.join("stage")).unwrap(), b"payload");
        }
    }
    #[test]
    fn stage_metadata_rejects_stale_or_wrong_kind_or_symlink_xattrs_before_mutation() {
        for case in 0..4 {
            let f = Fixture::new();
            files(&f);
            let mut desired = f.desired();
            if case == 1 {
                fs::remove_file(f.root.join("stage")).unwrap();
                fs::create_dir(f.root.join("stage")).unwrap();
            }
            if case == 2 {
                fs::remove_file(f.root.join("stage")).unwrap();
                symlink("missing", f.root.join("stage")).unwrap();
                let mut b = desired.encode().unwrap();
                b[0] = 3;
                b[1..5].copy_from_slice(&0o777u32.to_le_bytes());
                desired = Values::decode(&b).unwrap();
                // Add an otherwise well-formed raw attribute to this desired link.
                let n = b"user.a";
                let mut b = desired.encode().unwrap();
                b[37..39].copy_from_slice(&1u16.to_le_bytes());
                b.extend_from_slice(&(n.len() as u16).to_le_bytes());
                b.extend_from_slice(&1u32.to_le_bytes());
                b.extend_from_slice(n);
                b.push(1);
                desired = Values::decode(&b).unwrap();
            }
            if case == 3 {
                let mut bytes = desired.encode().unwrap();
                bytes[5..9]
                    .copy_from_slice(&unsafe { libc::geteuid() }.wrapping_add(1).to_le_bytes());
                desired = Values::decode(&bytes).unwrap();
            }
            let stage = f.capture("stage");
            if case == 0 {
                fs::write(f.root.join("stage"), b"changed").unwrap();
            }
            let err = stage
                .apply_staged_metadata(
                    &f.parent,
                    OsStr::new("stage"),
                    0,
                    &desired,
                    &AtomicBool::new(false),
                )
                .unwrap_err();
            assert!(!err.mutation_attempted);
        }
    }
    #[test]
    fn stage_metadata_name_substitution_does_not_modify_replacement() {
        let f = Fixture::new();
        files(&f);
        let desired = f.desired();
        let stage = f.capture("stage");
        let err = stage
            .apply_stage_inner(
                &f.parent,
                OsStr::new("stage"),
                0,
                &desired,
                &AtomicBool::new(false),
                None,
                |at| {
                    if matches!(at, Point::Prepared) {
                        fs::rename(f.root.join("stage"), f.root.join("kept")).unwrap();
                        fs::write(f.root.join("stage"), b"foreign").unwrap();
                    }
                    Ok(())
                },
            )
            .unwrap_err();
        assert!(!err.mutation_attempted);
        assert_eq!(fs::read(f.root.join("stage")).unwrap(), b"foreign");
        assert_eq!(fs::read(f.root.join("kept")).unwrap(), b"payload");
    }
    #[test]
    fn stage_metadata_late_name_substitution_modifies_only_held_inode_and_returns_no_receipt() {
        let f = Fixture::new();
        files(&f);
        let desired = f.desired();
        let stage = f.capture("stage");
        let mut replacement = None;
        let err = stage
            .apply_stage_inner(
                &f.parent,
                OsStr::new("stage"),
                0,
                &desired,
                &AtomicBool::new(false),
                None,
                |at| {
                    if matches!(at, Point::Writable) {
                        fs::rename(f.root.join("stage"), f.root.join("kept")).unwrap();
                        fs::write(f.root.join("stage"), b"foreign").unwrap();
                        replacement =
                            Some(revision(&fs::metadata(f.root.join("stage")).unwrap()).unwrap());
                    }
                    Ok(())
                },
            )
            .unwrap_err();
        assert!(err.mutation_attempted);
        assert_eq!(
            revision(&fs::metadata(f.root.join("stage")).unwrap()).unwrap(),
            replacement.unwrap()
        );
        assert_eq!(fs::read(f.root.join("stage")).unwrap(), b"foreign");
        assert_eq!(fs::read(f.root.join("kept")).unwrap(), b"payload");
    }
    #[test]
    fn stage_metadata_control_pause_resume_and_cancellation_preserve_ownership() {
        use std::{sync::mpsc, time::Duration};
        for cancelled in [false, true] {
            let f = Fixture::new();
            files(&f);
            let desired = f.desired();
            let stage = f.capture("stage");
            let control = Control::default();
            control.pause();
            let worker_control = control.clone();
            let parent = &f.parent;
            let inventory = &stage;
            std::thread::scope(|scope| {
                let (entered_tx, entered_rx) = mpsc::channel();
                let (done_tx, done_rx) = mpsc::channel();
                let worker = scope.spawn(move || {
                    entered_tx.send(()).unwrap();
                    done_tx
                        .send(inventory.apply_staged_metadata_controlled(
                            parent,
                            OsStr::new("stage"),
                            0,
                            &desired,
                            &worker_control,
                        ))
                        .unwrap();
                });
                entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                if cancelled {
                    control.cancel();
                } else {
                    control.resume();
                }
                let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                if cancelled {
                    let err = result.unwrap_err();
                    assert_eq!(err.cause.kind(), io::ErrorKind::Interrupted);
                    assert!(!err.mutation_attempted);
                } else {
                    result.unwrap();
                }
                worker.join().unwrap();
            });
        }
        let f = Fixture::new();
        files(&f);
        f.attr("source", b"user.new", b"retained");
        let desired = f.desired();
        let stage = f.capture("stage");
        let cancel = AtomicBool::new(false);
        let err = stage
            .apply_stage_inner(
                &f.parent,
                OsStr::new("stage"),
                0,
                &desired,
                &cancel,
                None,
                |at| {
                    if matches!(at, Point::Attributes) {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    Ok(())
                },
            )
            .unwrap_err();
        assert!(err.mutation_attempted);
        assert_eq!(err.cause.kind(), io::ErrorKind::Interrupted);
        assert!(f.root.join("stage").exists());
    }
}
