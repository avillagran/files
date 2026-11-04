//! Native same-filesystem moves only. Prepared descriptors pin reviewed objects;
//! EXDEV never falls back to copy/delete. Per-root atomic publication, no journal.
use crate::copy_engine::{Control, CopyError, CopyMethod, CopyProgress, CopyReport, CopyRequest};
use std::{
    io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub const MAX_ROOTS: usize = 128;
pub const MAX_PATH_BYTES: usize = 1024 * 1024;
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn check(flag: &AtomicBool) -> io::Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Revisión de movimiento cancelada",
        ))
    } else {
        Ok(())
    }
}
fn error(cause: io::Error, report: CopyReport) -> CopyError {
    CopyError {
        cause,
        report: Box::new(report),
        rescue_paths: Vec::new(),
        cleanup_errors: Vec::new(),
        rescue_paths_omitted: 0,
        cleanup_errors_omitted: 0,
        cleanup_unresolved_entries: 0,
        details_truncated: 0,
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        collections::HashSet,
        ffi::{CString, OsStr},
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::PathBuf,
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Identity {
        device: u64,
        inode: u64,
        kind: u32,
    }
    fn identity(s: &libc::stat) -> Identity {
        Identity {
            device: s.st_dev,
            inode: s.st_ino,
            kind: s.st_mode & libc::S_IFMT,
        }
    }
    fn c(p: &OsStr) -> io::Result<CString> {
        CString::new(p.as_bytes()).map_err(|_| invalid("NUL en ruta de movimiento"))
    }
    fn file(fd: libc::c_int) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn directory(path: &Path) -> io::Result<File> {
        file(unsafe {
            libc::open(
                c(path.as_os_str())?.as_ptr(),
                libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn child(parent: &File, name: &OsStr) -> io::Result<File> {
        file(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                c(name)?.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn stat(file: &File) -> io::Result<libc::stat> {
        let mut s = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(file.as_raw_fd(), s.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { s.assume_init() })
    }
    fn at(parent: &File, name: &OsStr) -> io::Result<Identity> {
        let mut s = std::mem::MaybeUninit::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                c(name)?.as_ptr(),
                s.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(identity(&unsafe { s.assume_init() }))
    }
    fn absent(parent: &File, name: &OsStr) -> io::Result<()> {
        match at(parent, name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "El destino ya existe; origen conservado",
            )),
        }
    }
    fn rename(source: &File, from: &OsStr, destination: &File, to: &OsStr) -> io::Result<()> {
        if unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                source.as_raw_fd(),
                c(from)?.as_ptr(),
                destination.as_raw_fd(),
                c(to)?.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    fn parts(path: &Path) -> io::Result<(&Path, &OsStr)> {
        crate::navigation_history::validate(path).map_err(invalid)?;
        let parent = path
            .parent()
            .ok_or_else(|| invalid("Movimiento requiere padre"))?;
        let name = path
            .file_name()
            .ok_or_else(|| invalid("Movimiento requiere nombre"))?;
        if name.as_bytes() == b"." || name.as_bytes() == b".." {
            return Err(invalid("Nombre inválido"));
        }
        Ok((parent, name))
    }
    fn ancestors_outside(
        destination: &File,
        root: Identity,
        checkpoint: impl Fn() -> io::Result<()>,
    ) -> io::Result<()> {
        let mut current = destination.try_clone()?;
        for _ in 0..256 {
            checkpoint()?;
            let id = identity(&stat(&current)?);
            if id == root {
                return Err(invalid("El destino está dentro del origen; no se movió"));
            }
            let parent = file(unsafe {
                libc::openat(
                    current.as_raw_fd(),
                    c(OsStr::new(".."))?.as_ptr(),
                    libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            })?;
            if identity(&stat(&parent)?) == id {
                return Ok(());
            }
            current = parent;
        }
        Err(invalid(
            "No se pudo verificar ancestros del destino dentro del límite256",
        ))
    }
    // Same allocation policy as copy report paths: Vec capacity4097 -> PathBuf.
    fn current_path(file: &File) -> io::Result<PathBuf> {
        let expected = identity(&stat(file)?);
        let proc = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
        let mut bytes: Vec<u8> = Vec::with_capacity(4097);
        let n = unsafe { libc::readlink(proc.as_ptr(), bytes.as_mut_ptr().cast(), 4097) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n > 4096 {
            return Err(invalid("Ruta efectiva de movimiento supera4096B"));
        }
        unsafe {
            bytes.set_len(n as usize);
        }
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.dev() != expected.device
            || meta.ino() != expected.inode
            || meta.mode() & libc::S_IFMT != expected.kind
        {
            return Err(io::Error::other(
                "Ruta efectiva cambió al resolver descriptor",
            ));
        }
        Ok(path)
    }
    #[derive(Debug)]
    pub struct PreparedMove {
        requests: Vec<CopyRequest>,
        source: File,
        destination: File,
        roots: Vec<File>,
        identities: Vec<Identity>,
    }
    impl PreparedMove {
        /// Identity and mount ID of the actual pinned destination descriptor.
        pub fn destination_mount_identity(&self) -> io::Result<(u64, u64, u64)> {
            let before = identity(&stat(&self.destination)?);
            let mut value = std::mem::MaybeUninit::<libc::statx>::zeroed();
            let rc = unsafe {
                libc::statx(
                    self.destination.as_raw_fd(),
                    c"".as_ptr(),
                    libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                    libc::STATX_BASIC_STATS | libc::STATX_MNT_ID,
                    value.as_mut_ptr(),
                )
            };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
            let value = unsafe { value.assume_init() };
            if value.stx_mask & libc::STATX_MNT_ID == 0
                || value.stx_mnt_id == 0
                || libc::makedev(value.stx_dev_major, value.stx_dev_minor) != before.device
                || value.stx_ino != before.inode
                || identity(&stat(&self.destination)?) != before
            {
                return Err(io::Error::other(
                    "Destino o montaje cambió durante revisión",
                ));
            }
            Ok((before.device, before.inode, value.stx_mnt_id))
        }
        pub fn verify_selection_indices(
            &self,
            expected: &crate::selection_source::Source,
            indices: &[usize],
            cancel: &AtomicBool,
        ) -> io::Result<()> {
            for &index in indices {
                check(cancel)?;
                let request = self
                    .requests
                    .get(index)
                    .ok_or_else(|| invalid("Índice de movimiento obsoleto"))?;
                let root = self
                    .roots
                    .get(index)
                    .ok_or_else(|| invalid("Descriptor de movimiento obsoleto"))?;
                expected.verify_captured(
                    &self.source,
                    request
                        .source
                        .file_name()
                        .ok_or_else(|| invalid("Falta nombre origen"))?,
                    root,
                )?;
            }
            check(cancel)
        }
        pub fn verify_selection(
            &self,
            expected: &crate::selection_source::Source,
            cancel: &AtomicBool,
        ) -> io::Result<()> {
            for (request, root) in self.requests.iter().zip(&self.roots) {
                check(cancel)?;
                expected.verify_captured(
                    &self.source,
                    request
                        .source
                        .file_name()
                        .ok_or_else(|| invalid("Falta nombre origen"))?,
                    root,
                )?;
            }
            Ok(())
        }
        pub fn verify_selection_controlled(
            &self,
            expected: &crate::selection_source::Source,
            control: &Control,
        ) -> io::Result<()> {
            self.verify_selection_controlled_with(expected, control, |_| {})
        }
        fn verify_selection_controlled_with(
            &self,
            expected: &crate::selection_source::Source,
            control: &Control,
            mut after: impl FnMut(usize),
        ) -> io::Result<()> {
            for (i, (request, root)) in self.requests.iter().zip(&self.roots).enumerate() {
                control.checkpoint()?;
                expected.verify_captured(
                    &self.source,
                    request
                        .source
                        .file_name()
                        .ok_or_else(|| invalid("Falta nombre origen"))?,
                    root,
                )?;
                after(i);
            }
            control.checkpoint()
        }
        pub fn capture(requests: Vec<CopyRequest>, cancel: &AtomicBool) -> io::Result<Self> {
            check(cancel)?;
            if requests.is_empty()
                || requests.len() > MAX_ROOTS
                || requests.capacity() > MAX_ROOTS * 2
            {
                return Err(invalid("Movimiento admite1..128 raíces; reduce el lote"));
            }
            let mut bytes = 0usize;
            let mut capacity = 0usize;
            for r in &requests {
                check(cancel)?;
                for p in [&r.source, &r.destination] {
                    parts(p)?;
                    bytes = bytes
                        .checked_add(p.as_os_str().len())
                        .filter(|n| *n <= MAX_PATH_BYTES)
                        .ok_or_else(|| invalid("Rutas de movimiento exceden1MiB"))?;
                    capacity = capacity
                        .checked_add(p.capacity())
                        .filter(|n| *n <= 2 * MAX_PATH_BYTES)
                        .ok_or_else(|| invalid("Capacidad de rutas excede2MiB"))?;
                    if p.capacity() > 8192 {
                        return Err(invalid("Capacidad de ruta excede8192B"));
                    }
                }
            }
            let (sp, _) = parts(&requests[0].source)?;
            let (dp, _) = parts(&requests[0].destination)?;
            if requests
                .iter()
                .any(|r| r.source.parent() != Some(sp) || r.destination.parent() != Some(dp))
            {
                return Err(invalid(
                    "Un movimiento debe compartir padres origen y destino",
                ));
            }
            let mut from = HashSet::with_capacity(requests.len());
            let mut to = HashSet::with_capacity(requests.len());
            for r in &requests {
                if !from.insert(r.source.file_name().unwrap())
                    || !to.insert(r.destination.file_name().unwrap())
                {
                    return Err(invalid("Origen/destino duplicado en el lote"));
                }
            }
            let source = directory(sp)?;
            let destination = directory(dp)?;
            let src_id = identity(&stat(&source)?);
            let dst_id = identity(&stat(&destination)?);
            if src_id.device != dst_id.device {
                return Err(io::Error::from_raw_os_error(libc::EXDEV));
            }
            let mut roots = Vec::with_capacity(requests.len());
            let mut identities = Vec::with_capacity(requests.len());
            for r in &requests {
                check(cancel)?;
                let from = r.source.file_name().unwrap();
                let to = r.destination.file_name().unwrap();
                if src_id == dst_id && from == to {
                    return Err(invalid("Origen y destino son el mismo elemento"));
                }
                absent(&destination, to)?;
                let root = child(&source, from)?;
                let id = identity(&stat(&root)?);
                if !matches!(id.kind, libc::S_IFREG | libc::S_IFDIR | libc::S_IFLNK) {
                    return Err(invalid("Tipo de origen no soportado para mover"));
                }
                if at(&source, from)? != id {
                    return Err(io::Error::other("El origen cambió durante la revisión"));
                }
                if id.kind == libc::S_IFDIR {
                    ancestors_outside(&destination, id, || check(cancel))?;
                }
                roots.push(root);
                identities.push(id);
            }
            Ok(Self {
                requests,
                source,
                destination,
                roots,
                identities,
            })
        }
        pub fn paths(&self) -> &Vec<CopyRequest> {
            &self.requests
        }
        pub fn fd_count(&self) -> usize {
            self.roots.len() + 2
        }
        pub fn metadata_bytes(&self) -> usize {
            8192 + self.roots.capacity() * std::mem::size_of::<File>()
                + self.identities.capacity() * std::mem::size_of::<Identity>()
        }
        pub fn execute(
            &self,
            control: &Control,
            progress: impl FnMut(CopyProgress<'_>),
        ) -> Result<CopyReport, CopyError> {
            self.execute_with(control, progress, rename)
        }
        fn execute_with(
            &self,
            control: &Control,
            progress: impl FnMut(CopyProgress<'_>),
            syscall: impl FnMut(&File, &OsStr, &File, &OsStr) -> io::Result<()>,
        ) -> Result<CopyReport, CopyError> {
            self.execute_subset_with(None, control, progress, syscall)
        }
        /// Execute exactly the selected still-pending roots against the same
        /// captured FDs. The caller owns and validates its confirmation ledger.
        pub fn execute_subset(
            &self,
            pending: &[bool],
            control: &Control,
            progress: impl FnMut(CopyProgress<'_>),
        ) -> Result<CopyReport, CopyError> {
            self.execute_subset_with(Some(pending), control, progress, rename)
        }
        fn execute_subset_with(
            &self,
            pending: Option<&[bool]>,
            control: &Control,
            mut progress: impl FnMut(CopyProgress<'_>),
            mut syscall: impl FnMut(&File, &OsStr, &File, &OsStr) -> io::Result<()>,
        ) -> Result<CopyReport, CopyError> {
            let mut report = CopyReport {
                auxiliary_accounted_peak_bytes: self.metadata_bytes(),
                ..Default::default()
            };
            if pending.is_some_and(|p| p.len() != self.requests.len()) {
                return Err(error(invalid("Ledger de movimiento incompatible"), report));
            }
            for (i, r) in self.requests.iter().enumerate() {
                if pending.is_some_and(|p| !p[i]) {
                    continue;
                }
                let mut published = false;
                let mut verified = false;
                let result = (|| {
                    control.checkpoint()?;
                    let from = r.source.file_name().unwrap();
                    let to = r.destination.file_name().unwrap();
                    let id = self.identities[i];
                    if identity(&stat(&self.roots[i])?) != id || at(&self.source, from)? != id {
                        return Err(io::Error::other(
                            "El origen cambió desde la revisión; no se movió",
                        ));
                    }
                    absent(&self.destination, to)?;
                    if id.kind == libc::S_IFDIR {
                        ancestors_outside(&self.destination, id, || control.checkpoint())?;
                    }
                    control.checkpoint()?;
                    if identity(&stat(&self.roots[i])?) != id || at(&self.source, from)? != id {
                        return Err(io::Error::other(
                            "El origen cambió después de verificar el destino",
                        ));
                    }
                    syscall(&self.source, from, &self.destination, to)?;
                    published = true;
                    if at(&self.destination, to)? != id {
                        return Err(io::Error::other(
                            "Identidad posterior al movimiento no confirmada; revisa ambas carpetas",
                        ));
                    }
                    verified = true;
                    report.completed_count += 1;
                    match id.kind {
                        libc::S_IFREG => report.files_copied += 1,
                        libc::S_IFDIR => report.directories_copied += 1,
                        _ => report.symlinks_copied += 1,
                    };
                    if report.completed.len() < 128 {
                        match current_path(&self.roots[i]) {
                            Ok(p) => report.completed.push(p),
                            Err(_) => report.completed_omitted += 1,
                        }
                    } else {
                        report.completed_omitted += 1;
                    }
                    progress(CopyProgress {
                        source: &r.source,
                        destination: &r.destination,
                        bytes_copied: 0,
                        entries_copied: report.completed_count,
                        method: CopyMethod::Rename,
                    });
                    Ok(())
                })();
                if let Err(cause) = result {
                    let mut failure = error(
                        if cause.raw_os_error() == Some(libc::EXDEV) {
                            io::Error::new(
                                io::ErrorKind::Unsupported,
                                "EXDEV: mover entre filesystems sigue pendiente; origen conservado, no se copió ni borró",
                            )
                        } else {
                            cause
                        },
                        report,
                    );
                    // Unknown post-rename races are not repaired by renaming an
                    // unverified replacement. Verify containing parents only.
                    if (published && !verified)
                        || at(&self.source, r.source.file_name().unwrap()).ok()
                            != Some(self.identities[i])
                    {
                        failure.cleanup_unresolved_entries = 1;
                        for parent in [&self.source, &self.destination] {
                            match current_path(parent) {
                                Ok(p) => failure.rescue_paths.push(p),
                                Err(_) => failure.rescue_paths_omitted += 1,
                            }
                        }
                    }
                    for parent in [&self.source, &self.destination] {
                        if let Ok(path) = current_path(parent) {
                            failure.report.affected_directories.push(path);
                        }
                    }
                    return Err(failure);
                }
            }
            for parent in [&self.source, &self.destination] {
                if let Ok(path) = current_path(parent) {
                    report.affected_directories.push(path);
                }
            }
            Ok(report)
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            fs,
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let p = crate::theme::home().join(format!(
                    ".cache/files-test/native-move-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(p.join("src")).unwrap();
                fs::create_dir(p.join("dst")).unwrap();
                Self(p)
            }
            fn requests(&self, names: &[&str]) -> Vec<CopyRequest> {
                names
                    .iter()
                    .map(|n| CopyRequest {
                        source: self.0.join("src").join(n),
                        destination: self.0.join("dst").join(n),
                    })
                    .collect()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        #[test]
        fn native_file_directory_symlink_preserve_identity_and_content() {
            let f = Fixture::new();
            fs::write(f.0.join("src/a"), b"bytes").unwrap();
            fs::create_dir(f.0.join("src/folder")).unwrap();
            fs::write(f.0.join("src/folder/inside"), b"nested").unwrap();
            std::os::unix::fs::symlink("a", f.0.join("src/link")).unwrap();
            let original = fs::symlink_metadata(f.0.join("src/a")).unwrap();
            let p = PreparedMove::capture(
                f.requests(&["a", "folder", "link"]),
                &AtomicBool::new(false),
            )
            .unwrap();
            let r = p.execute(&Control::default(), |_| {}).unwrap();
            assert_eq!(r.completed_count, 3);
            assert!(!f.0.join("src/a").exists());
            assert_eq!(fs::read(f.0.join("dst/a")).unwrap(), b"bytes");
            assert_eq!(fs::read(f.0.join("dst/folder/inside")).unwrap(), b"nested");
            assert_eq!(fs::read_link(f.0.join("dst/link")).unwrap(), Path::new("a"));
            let final_meta = fs::metadata(f.0.join("dst/a")).unwrap();
            assert_eq!(
                (original.ino(), original.mode(), original.mtime_nsec()),
                (final_meta.ino(), final_meta.mode(), final_meta.mtime_nsec())
            );
        }
        #[test]
        fn collisions_replacement_and_pre_cancel_leave_sources() {
            let f = Fixture::new();
            fs::write(f.0.join("src/a"), b"old").unwrap();
            let p = PreparedMove::capture(f.requests(&["a"]), &AtomicBool::new(false)).unwrap();
            fs::rename(f.0.join("src/a"), f.0.join("src/old")).unwrap();
            fs::write(f.0.join("src/a"), b"new").unwrap();
            assert!(p.execute(&Control::default(), |_| {}).is_err());
            assert_eq!(fs::read(f.0.join("src/a")).unwrap(), b"new");
            assert_eq!(fs::read(f.0.join("src/old")).unwrap(), b"old");
            let p = PreparedMove::capture(f.requests(&["a"]), &AtomicBool::new(false)).unwrap();
            fs::write(f.0.join("dst/a"), b"dest").unwrap();
            assert!(p.execute(&Control::default(), |_| {}).is_err());
            assert_eq!(fs::read(f.0.join("dst/a")).unwrap(), b"dest");
            assert!(PreparedMove::capture(f.requests(&["a"]), &AtomicBool::new(true)).is_err());
        }
        #[test]
        fn cancel_after_publication_keeps_completed_and_remaining() {
            let f = Fixture::new();
            for n in ["a", "b"] {
                fs::write(f.0.join("src").join(n), n).unwrap();
            }
            let p =
                PreparedMove::capture(f.requests(&["a", "b"]), &AtomicBool::new(false)).unwrap();
            let c = Control::default();
            let e = p.execute(&c, |_| c.cancel()).unwrap_err();
            assert_eq!(e.report.completed_count, 1);
            assert!(f.0.join("dst/a").exists() && f.0.join("src/b").exists());
            assert!(!f.0.join("src/a").exists() && !f.0.join("dst/b").exists());
        }
        #[test]
        fn pinned_parents_do_not_follow_replacements() {
            let f = Fixture::new();
            fs::write(f.0.join("src/a"), b"pinned").unwrap();
            let p = PreparedMove::capture(f.requests(&["a"]), &AtomicBool::new(false)).unwrap();
            fs::rename(f.0.join("src"), f.0.join("original-src")).unwrap();
            fs::create_dir(f.0.join("src")).unwrap();
            fs::write(f.0.join("src/a"), b"replacement").unwrap();
            fs::rename(f.0.join("dst"), f.0.join("original-dst")).unwrap();
            fs::create_dir(f.0.join("dst")).unwrap();
            p.execute(&Control::default(), |_| {}).unwrap();
            assert_eq!(fs::read(f.0.join("original-dst/a")).unwrap(), b"pinned");
            assert_eq!(fs::read(f.0.join("src/a")).unwrap(), b"replacement");
            assert!(!f.0.join("dst/a").exists());
        }
        #[test]
        fn self_descendant_alias_and_late_destination_relocation_are_rejected() {
            let f = Fixture::new();
            fs::create_dir(f.0.join("src/root")).unwrap();
            fs::create_dir(f.0.join("src/root/inside")).unwrap();
            let self_move = vec![CopyRequest {
                source: f.0.join("src/root"),
                destination: f.0.join("src/../src/root"),
            }];
            assert!(PreparedMove::capture(self_move, &AtomicBool::new(false)).is_err());
            let descendant = vec![CopyRequest {
                source: f.0.join("src/root"),
                destination: f.0.join("src/root/inside/new"),
            }];
            assert!(PreparedMove::capture(descendant, &AtomicBool::new(false)).is_err());
            let p = PreparedMove::capture(f.requests(&["root"]), &AtomicBool::new(false)).unwrap();
            fs::rename(f.0.join("dst"), f.0.join("src/root/relocated")).unwrap();
            assert!(p.execute(&Control::default(), |_| {}).is_err());
            assert!(f.0.join("src/root").exists());
        }
        #[test]
        fn injected_exdev_and_post_rename_race_never_delete() {
            let f = Fixture::new();
            fs::write(f.0.join("src/a"), b"keep").unwrap();
            let p = PreparedMove::capture(f.requests(&["a"]), &AtomicBool::new(false)).unwrap();
            let e = p
                .execute_with(
                    &Control::default(),
                    |_| {},
                    |_, _, _, _| Err(io::Error::from_raw_os_error(libc::EXDEV)),
                )
                .unwrap_err();
            assert_eq!(e.report.completed_count, 0);
            assert_eq!(fs::read(f.0.join("src/a")).unwrap(), b"keep");
            let e = p
                .execute_with(
                    &Control::default(),
                    |_| {},
                    |s, a, d, b| {
                        rename(s, a, d, b)?;
                        fs::rename(f.0.join("dst/a"), f.0.join("dst/preserved")).unwrap();
                        fs::write(f.0.join("dst/a"), b"actor").unwrap();
                        Ok(())
                    },
                )
                .unwrap_err();
            assert_eq!(e.cleanup_unresolved_entries, 1);
            assert_eq!(fs::read(f.0.join("dst/preserved")).unwrap(), b"keep");
            assert_eq!(fs::read(f.0.join("dst/a")).unwrap(), b"actor");
            assert_eq!(e.rescue_paths.len(), 2);
        }
        #[test]
        fn post_publication_unknown_is_not_cleared_by_source_hardlink() {
            let f = Fixture::new();
            fs::write(f.0.join("src/a"), b"keep").unwrap();
            let p = PreparedMove::capture(f.requests(&["a"]), &AtomicBool::new(false)).unwrap();
            let e = p
                .execute_with(
                    &Control::default(),
                    |_| {},
                    |s, a, d, b| {
                        rename(s, a, d, b)?;
                        fs::rename(f.0.join("dst/a"), f.0.join("dst/preserved")).unwrap();
                        fs::hard_link(f.0.join("dst/preserved"), f.0.join("src/a")).unwrap();
                        fs::write(f.0.join("dst/a"), b"actor").unwrap();
                        Ok(())
                    },
                )
                .unwrap_err();
            assert_eq!(e.cleanup_unresolved_entries, 1);
            assert_eq!(e.rescue_paths.len(), 2);
            assert_eq!(e.report.completed_count, 0);
            assert_eq!(fs::read(f.0.join("src/a")).unwrap(), b"keep");
            assert_eq!(fs::read(f.0.join("dst/a")).unwrap(), b"actor");
        }
        #[test]
        fn clipboard_cancel_between_verified_roots_keeps_interrupted_and_no_mutation() {
            let f = Fixture::new();
            for n in ["a", "b"] {
                fs::write(f.0.join("src").join(n), n).unwrap();
            }
            let expected = crate::selection_source::Source::capture(
                f.0.join("src"),
                vec!["a".into(), "b".into()].into(),
                &AtomicBool::new(false),
            )
            .unwrap();
            let token =
                PreparedMove::capture(f.requests(&["a", "b"]), &AtomicBool::new(false)).unwrap();
            let control = Control::default();
            let mut seen = Vec::new();
            let e = token
                .verify_selection_controlled_with(&expected, &control, |i| {
                    seen.push(i);
                    control.cancel();
                })
                .unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::Interrupted);
            assert_eq!(seen, [0]);
            assert!(f.0.join("src/a").exists() && f.0.join("src/b").exists());
            assert!(!f.0.join("dst/a").exists() && !f.0.join("dst/b").exists());
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::PreparedMove;
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct PreparedMove {
    requests: Vec<CopyRequest>,
}
#[cfg(not(target_os = "linux"))]
impl PreparedMove {
    pub fn destination_mount_identity(&self) -> io::Result<(u64, u64, u64)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de destino pendiente",
        ))
    }
    pub fn execute_subset(
        &self,
        _: &[bool],
        _: &Control,
        _: impl FnMut(CopyProgress<'_>),
    ) -> Result<CopyReport, CopyError> {
        Err(error(
            io::Error::new(io::ErrorKind::Unsupported, "Mover nativo pendiente"),
            CopyReport::default(),
        ))
    }
    pub fn verify_selection_indices(
        &self,
        _: &crate::selection_source::Source,
        _: &[usize],
        _: &AtomicBool,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de movimiento pendiente",
        ))
    }
    pub fn verify_selection(
        &self,
        _: &crate::selection_source::Source,
        _: &AtomicBool,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Mover nativo pendiente en esta plataforma",
        ))
    }
    pub fn verify_selection_controlled(
        &self,
        _: &crate::selection_source::Source,
        _: &Control,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Mover nativo pendiente",
        ))
    }
    pub fn capture(_: Vec<CopyRequest>, _: &AtomicBool) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Mover nativo pendiente en esta plataforma",
        ))
    }
    pub fn paths(&self) -> &Vec<CopyRequest> {
        &self.requests
    }
    pub fn fd_count(&self) -> usize {
        0
    }
    pub fn metadata_bytes(&self) -> usize {
        0
    }
    pub fn execute(
        &self,
        _: &Control,
        _: impl FnMut(CopyProgress<'_>),
    ) -> Result<CopyReport, CopyError> {
        Err(error(
            io::Error::new(
                io::ErrorKind::Unsupported,
                "Mover nativo pendiente en esta plataforma",
            ),
            CopyReport::default(),
        ))
    }
}
