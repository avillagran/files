//! Bounded clipboard selection identity. Capture/verify on a worker, never UI.
//! Not a content snapshot: changes after verification remain subject to the
//! executing copy/move engine's own descriptor/identity checks.
use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub const MAX_NAMES: usize = 128;
pub const MAX_PATH: usize = 4096;
pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectIdentity {
    pub device: u64,
    pub inode: u64,
    pub directory: bool,
}
fn invalid(s: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
fn checkpoint(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Captura de archivos cancelada",
        ))
    } else {
        Ok(())
    }
}
fn preflight(parent: &std::path::Path, names: &[OsString]) -> io::Result<()> {
    let p = parent.as_os_str().as_encoded_bytes();
    if !parent.is_absolute()
        || p.len() > MAX_PATH
        || p.contains(&0)
        || names.is_empty()
        || names.len() > MAX_NAMES
    {
        return Err(invalid(
            "Portapapeles: ruta o cantidad fuera de límite (128)",
        ));
    }
    let mut bytes = 0usize;
    let mut seen = std::collections::HashSet::with_capacity(names.len());
    for name in names {
        let b = name.as_encoded_bytes();
        if b.is_empty()
            || b.len() > MAX_PATH
            || b.contains(&0)
            || b.contains(&b'/')
            || b == b"."
            || b == b".."
            || !seen.insert(name.as_os_str())
        {
            return Err(invalid("Portapapeles: nombre inválido o repetido"));
        }
        let path_bytes = p
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_add(b.len()))
            .filter(|n| *n <= MAX_PATH)
            .ok_or_else(|| invalid("Portapapeles: ruta por elemento demasiado larga"))?;
        bytes = bytes
            .checked_add(path_bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| invalid("Portapapeles: rutas agregadas superan1MiB"))?;
    }
    Ok(())
}
#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        ffi::{CString, OsStr},
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Revision {
        dev: u64,
        ino: u64,
        kind: u32,
        bytes: i64,
        mtime: i64,
        mtime_ns: i64,
        ctime: i64,
        ctime_ns: i64,
        links: u64,
    }
    fn revision(s: &libc::stat) -> Revision {
        // libc nlink_t is u32 on Linux AArch64 and u64 on Linux x86_64.
        #[allow(clippy::useless_conversion)]
        let links = u64::from(s.st_nlink);
        Revision {
            dev: s.st_dev,
            ino: s.st_ino,
            kind: s.st_mode & libc::S_IFMT,
            bytes: s.st_size,
            mtime: s.st_mtime,
            mtime_ns: s.st_mtime_nsec,
            ctime: s.st_ctime,
            ctime_ns: s.st_ctime_nsec,
            links,
        }
    }
    fn c(s: &OsStr) -> io::Result<CString> {
        CString::new(s.as_bytes()).map_err(|_| invalid("NUL en portapapeles"))
    }
    fn directory(p: &std::path::Path) -> io::Result<File> {
        let fd = unsafe {
            libc::open(
                c(p.as_os_str())?.as_ptr(),
                libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn stat(fd: &File, name: Option<&OsStr>) -> io::Result<libc::stat> {
        let mut s = std::mem::MaybeUninit::uninit();
        let rc = if let Some(name) = name {
            unsafe {
                libc::fstatat(
                    fd.as_raw_fd(),
                    c(name)?.as_ptr(),
                    s.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            }
        } else {
            unsafe { libc::fstat(fd.as_raw_fd(), s.as_mut_ptr()) }
        };
        if rc != 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { s.assume_init() })
        }
    }
    pub struct Source {
        parent: PathBuf,
        names: Arc<[OsString]>,
        pin: File,
        identity: Revision,
        revisions: Vec<Revision>,
    }
    impl std::fmt::Debug for Source {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SelectionSource")
                .field("parent", &self.parent)
                .field("count", &self.names.len())
                .finish_non_exhaustive()
        }
    }
    impl Source {
        /// `statx` mount IDs on the pinned parent and each captured name.
        /// This is only a topology observation; callers must reject ambiguous
        /// cross-mount relationships and revalidate before any mutation.
        fn mount_id(fd: &File, name: Option<&OsStr>, expected: Revision) -> io::Result<u64> {
            let name = match name {
                Some(n) => c(n)?,
                None => c"".to_owned(),
            };
            let mut value = std::mem::MaybeUninit::<libc::statx>::zeroed();
            let flags = libc::AT_SYMLINK_NOFOLLOW
                | if name.as_bytes().is_empty() {
                    libc::AT_EMPTY_PATH
                } else {
                    0
                };
            let rc = unsafe {
                libc::statx(
                    fd.as_raw_fd(),
                    name.as_ptr(),
                    flags,
                    libc::STATX_BASIC_STATS | libc::STATX_MNT_ID,
                    value.as_mut_ptr(),
                )
            };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
            let value = unsafe { value.assume_init() };
            let device = libc::makedev(value.stx_dev_major, value.stx_dev_minor);
            if value.stx_mask & libc::STATX_MNT_ID == 0
                || value.stx_mnt_id == 0
                || device != expected.dev
                || value.stx_ino != expected.ino
                || u32::from(value.stx_mode) & libc::S_IFMT != expected.kind
            {
                return Err(io::Error::other(
                    "Identidad o montaje cambió durante captura",
                ));
            }
            Ok(value.stx_mnt_id)
        }
        pub fn mount_ids(&self, cancel: &AtomicBool) -> io::Result<(u64, Vec<u64>)> {
            checkpoint(cancel)?;
            let parent = Self::mount_id(&self.pin, None, self.identity)?;
            let mut roots = Vec::with_capacity(self.names.len());
            for (name, revision) in self.names.iter().zip(&self.revisions) {
                checkpoint(cancel)?;
                roots.push(Self::mount_id(&self.pin, Some(name), *revision)?);
            }
            checkpoint(cancel)?;
            Ok((parent, roots))
        }
        pub fn mount_ids_subset(
            &self,
            indices: &[usize],
            cancel: &AtomicBool,
        ) -> io::Result<(u64, Vec<u64>)> {
            checkpoint(cancel)?;
            let parent = Self::mount_id(&self.pin, None, self.identity)?;
            let mut roots = Vec::with_capacity(indices.len());
            for &i in indices {
                checkpoint(cancel)?;
                let name = self
                    .names
                    .get(i)
                    .ok_or_else(|| invalid("Índice de origen obsoleto"))?;
                let rev = *self
                    .revisions
                    .get(i)
                    .ok_or_else(|| invalid("Firma de origen obsoleta"))?;
                roots.push(Self::mount_id(&self.pin, Some(name), rev)?);
            }
            checkpoint(cancel)?;
            Ok((parent, roots))
        }
        pub fn root_identity(&self, index: usize) -> Option<ObjectIdentity> {
            self.revisions.get(index).map(|r| ObjectIdentity {
                device: r.dev,
                inode: r.ino,
                directory: r.kind == libc::S_IFDIR,
            })
        }
        /// Pinned parent and its ancestors, not a path-based canonicalization.
        /// A depth limit fails closed; callers must revalidate bindings after.
        pub fn parent_ancestors(&self, cancel: &AtomicBool) -> io::Result<Vec<ObjectIdentity>> {
            let dup = unsafe { libc::fcntl(self.pin.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
            if dup < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut dir = unsafe { File::from_raw_fd(dup) };
            let mut result = Vec::with_capacity(16);
            for _ in 0..128 {
                checkpoint(cancel)?;
                let current = revision(&stat(&dir, None)?);
                let identity = ObjectIdentity {
                    device: current.dev,
                    inode: current.ino,
                    directory: true,
                };
                if result.contains(&identity) {
                    return Err(io::Error::other("Ciclo de ancestros de origen"));
                }
                result.push(identity);
                let next = unsafe {
                    libc::openat(
                        dir.as_raw_fd(),
                        c"..".as_ptr(),
                        libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
                    )
                };
                if next < 0 {
                    return Err(io::Error::last_os_error());
                }
                let next = unsafe { File::from_raw_fd(next) };
                let parent = revision(&stat(&next, None)?);
                if parent.dev == current.dev && parent.ino == current.ino {
                    return Ok(result);
                }
                dir = next;
            }
            Err(io::Error::other("Ancestros de origen superan límite128"))
        }
        /// Borrowed worker-only handle; the Source retains ownership/binding.
        pub(crate) fn parent_handle(&self) -> &File {
            &self.pin
        }
        pub fn parent(&self) -> &std::path::Path {
            &self.parent
        }
        pub fn names(&self) -> &Arc<[OsString]> {
            &self.names
        }
        pub fn capture(
            parent: PathBuf,
            names: Arc<[OsString]>,
            cancel: &AtomicBool,
        ) -> io::Result<Self> {
            preflight(&parent, &names)?;
            checkpoint(cancel)?;
            let pin = directory(&parent)?;
            let identity = revision(&stat(&pin, None)?);
            let mut revisions = Vec::with_capacity(names.len());
            for name in names.iter() {
                checkpoint(cancel)?;
                let r = revision(&stat(&pin, Some(name))?);
                if !matches!(r.kind, libc::S_IFREG | libc::S_IFDIR | libc::S_IFLNK) {
                    return Err(invalid(
                        "Portapapeles: sólo archivos/carpetas/enlaces, no dispositivos/FIFO",
                    ));
                }
                revisions.push(r);
            }
            let result = Self {
                parent,
                names,
                pin,
                identity,
                revisions,
            };
            if result.retained_bytes() > 2 * MAX_BYTES {
                return Err(invalid("Portapapeles: capacidad retenida excesiva"));
            }
            result.verify(cancel)?;
            Ok(result)
        }
        /// One parent descriptor; no descriptor per selected entry. Parent path
        /// replacement is rejected even if the original descriptor remains live.
        /// Retained count; verification needs one additional temporary parent FD.
        pub fn fd_count(&self) -> usize {
            1
        }
        pub fn retained_bytes(&self) -> usize {
            std::mem::size_of::<Self>()
                + self.parent.capacity()
                + self.revisions.capacity() * std::mem::size_of::<Revision>()
                + self.names.len() * std::mem::size_of::<OsString>()
                + self.names.iter().map(|n| n.capacity()).sum::<usize>()
                + 512
        }
        pub fn verify(&self, cancel: &AtomicBool) -> io::Result<()> {
            self.verify_subset(self.names.iter().map(OsString::as_os_str), cancel)
        }
        pub fn verify_subset_controlled<'a>(
            &self,
            names: impl Iterator<Item = &'a OsStr>,
            control: &crate::copy_engine::Control,
        ) -> io::Result<()> {
            control.checkpoint()?;
            self.verify_subset(names, control.cancellation_flag())
        }
        pub fn verify_subset<'a>(
            &self,
            names: impl Iterator<Item = &'a OsStr>,
            cancel: &AtomicBool,
        ) -> io::Result<()> {
            checkpoint(cancel)?;
            let current = directory(&self.parent)?;
            let bound = revision(&stat(&current, None)?);
            let pinned = revision(&stat(&self.pin, None)?);
            let same_parent = |r: Revision| {
                r.dev == self.identity.dev
                    && r.ino == self.identity.ino
                    && r.kind == self.identity.kind
            };
            if !same_parent(bound) || !same_parent(pinned) {
                return Err(io::Error::other(
                    "La carpeta de origen fue sustituida; vuelve a copiar/cortar",
                ));
            }
            drop(current);
            for (count, name) in names.enumerate() {
                checkpoint(cancel)?;
                if count >= MAX_NAMES {
                    return Err(invalid("Subselección fuera de límite"));
                }
                let index = self
                    .names
                    .iter()
                    .position(|n| n == name)
                    .ok_or_else(|| invalid("Nombre fuera de selección capturada"))?;
                let expected = &self.revisions[index];
                if revision(&stat(&self.pin, Some(name))?) != *expected {
                    return Err(io::Error::other(
                        "Un origen cambió desde Copiar/Cortar; recaptura la selección",
                    ));
                }
            }
            // Recheck path binding after every name was observed; no atomic
            // snapshot or inode-conditional kernel rename is implied by this.
            let last = directory(&self.parent)?;
            if !same_parent(revision(&stat(&last, None)?)) {
                return Err(io::Error::other("Cambió la carpeta durante la revisión"));
            }
            checkpoint(cancel)
        }
        /// Compare the descriptors actually pinned by the move preparation,
        /// not a second pathname probe which could observe another occupant.
        pub fn verify_captured(&self, parent: &File, name: &OsStr, root: &File) -> io::Result<()> {
            let index = self
                .names
                .iter()
                .position(|n| n == name)
                .ok_or_else(|| invalid("La raíz no pertenece a la selección capturada"))?;
            let p = revision(&stat(parent, None)?);
            if p.dev != self.identity.dev
                || p.ino != self.identity.ino
                || p.kind != self.identity.kind
                || revision(&stat(root, None)?) != self.revisions[index]
            {
                return Err(io::Error::other(
                    "El archivo abierto no corresponde al origen capturado",
                ));
            }
            Ok(())
        }
    }
}
#[cfg(target_os = "linux")]
pub use platform::Source;
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct Source {
    parent: PathBuf,
    names: Arc<[OsString]>,
}
#[cfg(not(target_os = "linux"))]
impl Source {
    pub fn mount_ids(&self, _: &AtomicBool) -> io::Result<(u64, Vec<u64>)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de montaje pendiente",
        ))
    }
    pub fn mount_ids_subset(&self, _: &[usize], _: &AtomicBool) -> io::Result<(u64, Vec<u64>)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de montaje pendiente",
        ))
    }
    pub fn root_identity(&self, _: usize) -> Option<ObjectIdentity> {
        None
    }
    pub fn parent_ancestors(&self, _: &AtomicBool) -> io::Result<Vec<ObjectIdentity>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de ancestros pendiente",
        ))
    }
    pub fn verify_subset_controlled<'a>(
        &self,
        _: impl Iterator<Item = &'a std::ffi::OsStr>,
        _: &crate::copy_engine::Control,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de clipboard pendiente",
        ))
    }
    pub fn parent(&self) -> &std::path::Path {
        &self.parent
    }
    pub fn names(&self) -> &Arc<[OsString]> {
        &self.names
    }
    pub fn verify_captured(
        &self,
        _: &std::fs::File,
        _: &std::ffi::OsStr,
        _: &std::fs::File,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad del token de movimiento pendiente en esta plataforma",
        ))
    }
    pub fn capture(
        parent: PathBuf,
        names: Arc<[OsString]>,
        cancel: &AtomicBool,
    ) -> io::Result<Self> {
        preflight(&parent, &names)?;
        checkpoint(cancel)?;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de clipboard pendiente en esta plataforma",
        ))
    }
    pub fn verify_subset<'a>(
        &self,
        _: impl Iterator<Item = &'a std::ffi::OsStr>,
        _: &AtomicBool,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de clipboard pendiente en esta plataforma",
        ))
    }
    pub fn verify(&self, _: &AtomicBool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de clipboard pendiente en esta plataforma",
        ))
    }
    /// Retained count; verification needs one additional temporary parent FD.
    pub fn fd_count(&self) -> usize {
        0
    }
    pub fn retained_bytes(&self) -> usize {
        self.parent.capacity()
            + self
                .names
                .iter()
                .map(|n| n.capacity() + std::mem::size_of::<OsString>())
                .sum::<usize>()
            + 512
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::atomic::AtomicU64,
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let p = crate::theme::home().join(".cache/files-test").join(format!(
                "selection-source-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn flag() -> AtomicBool {
        AtomicBool::new(false)
    }
    #[test]
    fn raw_regular_directory_and_literal_symlink_capture_are_non_mutating() {
        let f = Fixture::new();
        let raw = OsString::from_vec(b"raw-\xff\n%00".to_vec());
        fs::write(f.0.join(&raw), b"original").unwrap();
        fs::create_dir(f.0.join("dir")).unwrap();
        symlink(&raw, f.0.join("link")).unwrap();
        let s = Source::capture(
            f.0.clone(),
            vec![raw.clone(), "dir".into(), "link".into()].into(),
            &flag(),
        )
        .unwrap();
        s.verify(&flag()).unwrap();
        assert_eq!(s.names()[0], raw);
        assert_eq!(s.fd_count(), 1);
        assert!(s.retained_bytes() < 2 * MAX_BYTES);
        assert_eq!(fs::read(f.0.join(raw)).unwrap(), b"original");
        assert_eq!(
            fs::read_link(f.0.join("link")).unwrap().as_os_str(),
            s.names()[0]
        );
    }
    #[test]
    fn replaced_same_name_and_modified_bytes_are_rejected_without_mutation() {
        let f = Fixture::new();
        let p = f.0.join("a");
        fs::write(&p, b"AAAA").unwrap();
        let a = Source::capture(f.0.clone(), vec!["a".into()].into(), &flag()).unwrap();
        fs::rename(&p, f.0.join("old")).unwrap();
        fs::write(&p, b"BBBB").unwrap();
        assert!(a.verify(&flag()).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"BBBB");
        let b = Source::capture(f.0.clone(), vec!["a".into()].into(), &flag()).unwrap();
        fs::write(&p, b"longer").unwrap();
        assert!(b.verify(&flag()).is_err());
    }
    #[test]
    fn replaced_parent_does_not_follow_the_substitute() {
        let f = Fixture::new();
        let p = f.0.join("parent");
        fs::create_dir(&p).unwrap();
        fs::write(p.join("a"), b"old").unwrap();
        let s = Source::capture(p.clone(), vec!["a".into()].into(), &flag()).unwrap();
        fs::rename(&p, f.0.join("preserved")).unwrap();
        fs::create_dir(&p).unwrap();
        fs::write(p.join("a"), b"new").unwrap();
        assert!(s.verify(&flag()).is_err());
        assert_eq!(fs::read(p.join("a")).unwrap(), b"new");
        assert_eq!(fs::read(f.0.join("preserved/a")).unwrap(), b"old");
    }
    #[test]
    fn bounds_duplicates_and_cancel_fail_before_io() {
        let path = PathBuf::from("/nonexistent-private-selection-source");
        for names in [
            vec![],
            vec!["../escape".into()],
            vec!["a".into(), "a".into()],
            vec!["x".into(); 129],
            vec![OsString::from("x".repeat(4097))],
        ] {
            assert_eq!(
                Source::capture(path.clone(), names.into(), &flag())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let yes = AtomicBool::new(true);
        assert_eq!(
            Source::capture(path, vec!["a".into()].into(), &yes)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn individual_path_and_aggregate_budget_are_separate() {
        let parent = PathBuf::from("/parent");
        let names = (0..128)
            .map(|i| OsString::from(format!("{i:03}-{}", "x".repeat(100))))
            .collect::<Vec<_>>();
        assert!(
            names
                .iter()
                .map(|n| parent.as_os_str().len() + 1 + n.len())
                .sum::<usize>()
                > MAX_PATH
        );
        preflight(&parent, &names).unwrap();
        let exact = PathBuf::from(format!("/{}", "p".repeat(3994)));
        preflight(&exact, &["x".repeat(100).into()]).unwrap();
        assert_eq!(
            preflight(&exact, &["x".repeat(101).into()])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn clipboard_proof_rejects_replacement_between_verify_and_native_capture() {
        use crate::{copy_engine::CopyRequest, move_engine::PreparedMove};
        let f = Fixture::new();
        let cancel = flag();
        let dest = f.0.join("dest");
        fs::create_dir(&dest).unwrap();
        fs::write(f.0.join("a"), b"A original").unwrap();
        let expected = Source::capture(f.0.clone(), vec!["a".into()].into(), &cancel).unwrap();
        let requests = || {
            vec![CopyRequest {
                source: f.0.join("a"),
                destination: dest.join("a"),
            }]
        };
        let original = PreparedMove::capture(requests(), &cancel).unwrap();
        original.verify_selection(&expected, &cancel).unwrap();
        drop(original);
        expected.verify(&cancel).unwrap();
        fs::rename(f.0.join("a"), f.0.join("preserved")).unwrap();
        fs::write(f.0.join("a"), b"B replacement").unwrap();
        let substitute = PreparedMove::capture(requests(), &cancel).unwrap();
        assert!(substitute.verify_selection(&expected, &cancel).is_err());
        assert_eq!(fs::read(f.0.join("a")).unwrap(), b"B replacement");
        assert_eq!(fs::read(f.0.join("preserved")).unwrap(), b"A original");
        assert!(!dest.join("a").exists());
    }
    #[test]
    fn subset_keeps_original_signature_and_ignores_consumed_roots() {
        let f = Fixture::new();
        let cancel = flag();
        for n in ["a", "b"] {
            fs::write(f.0.join(n), n).unwrap();
        }
        let s = Source::capture(f.0.clone(), vec!["a".into(), "b".into()].into(), &cancel).unwrap();
        fs::rename(f.0.join("a"), f.0.join("moved-a")).unwrap();
        assert!(s.verify(&cancel).is_err());
        s.verify_subset([std::ffi::OsStr::new("b")].into_iter(), &cancel)
            .unwrap();
        fs::write(f.0.join("b"), b"actor changed b").unwrap();
        assert!(
            s.verify_subset([std::ffi::OsStr::new("b")].into_iter(), &cancel)
                .is_err()
        );
        assert!(
            s.verify_subset([std::ffi::OsStr::new("foreign")].into_iter(), &cancel)
                .is_err()
        );
    }
}
