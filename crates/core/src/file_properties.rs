//! Bounded local properties and explicit, non-recursive Linux permission changes.
//!
//! Capture pins the parent and final object (O_PATH|O_NOFOLLOW), including a
//! symlink itself. chmod uses fchmodat2(AT_EMPTY_PATH) on that same descriptor;
//! no reopening through the original pathname and no symlink-following fallback.
//! Requires Linux6.6+ support for descriptor-only chmod. Other platforms return
//! Unsupported. This is preparation for a worker/confirmation UI, not U04 done.
//!
//! There is no atomic “change mode only while this directory entry still names
//! this inode” syscall. The entry/owner are checked immediately before mutation;
//! a concurrent actor can move the pinned object afterwards. Mode/uid/gid must
//! also match the captured observation before applying a change; a stale dialog
//! is rejected. After a successful own change, callers may replace
//! snapshot.properties with ChangeReport.after to authorize a later confirmed
//! edit on the same pinned FDs. The descriptor will
//! never switch to a replacement inode, but the object may have a different path.
//! Modes affect all hardlinks to an inode. No ACL/xattrs/chown/recursive changes,
//! special-mode manipulation, privilege escalation or content snapshots.
//!
//! Primary contracts: https://man7.org/linux/man-pages/man2/chmod.2.html and
//! https://man7.org/linux/man-pages/man2/readlink.2.html (empty pathname + O_PATH).
use std::{
    ffi::OsString,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
const MAX_PATH_BYTES: usize = 4096;
const MAX_LINK_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    pub kind: Kind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub seconds: i64,
    pub nanoseconds: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Properties {
    pub identity: Identity,
    /// st_size (directory value is not a recursive content total).
    pub apparent_bytes: u64,
    /// st_blocks *512; no claim about compressed/shared physical ownership.
    pub allocated_bytes: u64,
    pub hardlinks: u64,
    pub uid: u32,
    pub gid: u32,
    /// Permissions plus existing special bits (07777) for read-only display.
    pub mode: u32,
    pub accessed: Timestamp,
    pub modified: Timestamp,
    /// Inode-status change time, NOT file creation/birth time.
    pub changed: Timestamp,
}
#[derive(Debug)]
pub struct Snapshot {
    pub properties: Properties,
    /// Raw target bytes from readlinkat on the pinned symlink FD, never followed.
    pub symlink_target: Option<OsString>,
    original: PathBuf,
    parent: File,
    object: File,
    name: OsString,
}
impl Snapshot {
    /// Historical capture pathname; do not present as a verified current path.
    pub fn original_path(&self) -> &Path {
        &self.original
    }
    pub fn current_path(&self) -> io::Result<PathBuf> {
        platform::current_path(self)
    }
}
#[derive(Debug)]
pub struct ChangeReport {
    /// The chmod syscall succeeded. A later diagnostic failure is a warning,
    /// never Err which could be mistaken for an unapplied operation.
    pub requested_mode: u32,
    pub after: Option<Properties>,
    pub warnings: Vec<String>,
}
pub fn capture(path: &Path) -> io::Result<Snapshot> {
    validate_path(path)?;
    platform::capture(path)
}
/// Internal writer primitive: caller must already own/pin and verify the object.
/// No pathname fallback or process-wide umask changes.
#[cfg(target_os = "linux")]
pub(crate) fn set_descriptor_mode(object: &std::fs::File, mode: u32) -> io::Result<()> {
    if mode & !0o777 != 0 {
        return Err(invalid("Modo FD fuera de0777"));
    }
    platform::apply_mode(object, mode)
}
pub fn chmod(snapshot: &Snapshot, mode: u32, cancel: &AtomicBool) -> io::Result<ChangeReport> {
    if mode & !0o777 != 0 {
        return Err(invalid(
            "Sólo se permiten permisos0777; sin setuid, setgid ni sticky",
        ));
    }
    check_cancel(cancel)?;
    platform::chmod(snapshot, mode, cancel)
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn validate_path(path: &Path) -> io::Result<()> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute()
        || bytes.len() > MAX_PATH_BYTES
        || bytes.contains(&0)
        || path.file_name().is_none()
        || bytes
            .split(|byte| *byte == b'/')
            .any(|part| matches!(part, b"." | b".."))
    {
        return Err(invalid(
            "Selecciona un objeto con ruta absoluta de hasta4096bytes, sin NUL/. /..",
        ));
    }
    Ok(())
}
fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Cambio de permisos cancelado",
        ))
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        ffi::{CString, OsStr},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::{OsStrExt, OsStringExt},
        },
    };
    fn c(name: &OsStr) -> io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|_| invalid("NUL en ruta de propiedades"))
    }
    fn from_fd(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn open_directory(path: &Path) -> io::Result<File> {
        let path = c(path.as_os_str())?;
        from_fd(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    }
    fn stat(object: &File) -> io::Result<libc::stat> {
        let mut stat = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(object.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { stat.assume_init() })
    }
    fn kind(mode: u32) -> Kind {
        match mode & libc::S_IFMT {
            libc::S_IFREG => Kind::File,
            libc::S_IFDIR => Kind::Directory,
            libc::S_IFLNK => Kind::Symlink,
            _ => Kind::Other,
        }
    }
    fn identity(stat: &libc::stat) -> Identity {
        Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: kind(stat.st_mode),
        }
    }
    fn timestamp(seconds: i64, nanoseconds: i64) -> io::Result<Timestamp> {
        if !(0..1_000_000_000).contains(&nanoseconds) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Timestamp fuera de rango",
            ));
        }
        Ok(Timestamp {
            seconds,
            nanoseconds: nanoseconds as u32,
        })
    }
    fn properties(stat: &libc::stat) -> io::Result<Properties> {
        // libc nlink_t is u32 on Linux AArch64 and u64 on Linux x86_64.
        #[allow(clippy::useless_conversion)]
        let hardlinks = u64::from(stat.st_nlink);
        Ok(Properties {
            identity: identity(stat),
            apparent_bytes: u64::try_from(stat.st_size)
                .map_err(|_| invalid("Tamaño aparente negativo"))?,
            allocated_bytes: u64::try_from(stat.st_blocks)
                .ok()
                .and_then(|blocks| blocks.checked_mul(512))
                .ok_or_else(|| invalid("Bloques fuera de rango"))?,
            hardlinks,
            uid: stat.st_uid,
            gid: stat.st_gid,
            mode: stat.st_mode & 0o7777,
            accessed: timestamp(stat.st_atime, stat.st_atime_nsec)?,
            modified: timestamp(stat.st_mtime, stat.st_mtime_nsec)?,
            changed: timestamp(stat.st_ctime, stat.st_ctime_nsec)?,
        })
    }
    fn entry_identity(snapshot: &Snapshot) -> io::Result<Identity> {
        let name = c(&snapshot.name)?;
        let mut stat = std::mem::MaybeUninit::uninit();
        if unsafe {
            libc::fstatat(
                snapshot.parent.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(identity(&unsafe { stat.assume_init() }))
    }
    fn revalidate(snapshot: &Snapshot) -> io::Result<Properties> {
        let observed = properties(&stat(&snapshot.object)?)?;
        if observed.identity != snapshot.properties.identity
            || entry_identity(snapshot)? != observed.identity
        {
            return Err(invalid(
                "El objeto de propiedades fue reemplazado o movido; vuelve a abrir sus propiedades",
            ));
        }
        Ok(observed)
    }
    fn link_target(object: &File) -> io::Result<OsString> {
        let mut bytes = [0u8; MAX_LINK_BYTES + 1];
        let length = unsafe {
            libc::readlinkat(
                object.as_raw_fd(),
                c"".as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if length < 0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize > MAX_LINK_BYTES {
            return Err(invalid(
                "Destino del enlace supera4096bytes; no se trunca silenciosamente",
            ));
        }
        Ok(OsString::from_vec(bytes[..length as usize].to_vec()))
    }
    pub(super) fn capture(path: &Path) -> io::Result<Snapshot> {
        let name = path
            .file_name()
            .ok_or_else(|| invalid("Falta nombre para propiedades"))?
            .to_os_string();
        // Resolve only the parent; the last symlink remains the inspected object.
        let parent_path = path
            .parent()
            .ok_or_else(|| invalid("Falta padre de propiedades"))?
            .canonicalize()?;
        if parent_path.as_os_str().len() > MAX_PATH_BYTES {
            return Err(invalid("Padre resuelto fuera del límite"));
        }
        let parent = open_directory(&parent_path)?;
        let name_c = c(&name)?;
        let object = from_fd(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name_c.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let properties = properties(&stat(&object)?)?;
        let symlink_target = if properties.identity.kind == Kind::Symlink {
            Some(link_target(&object)?)
        } else {
            None
        };
        let original = parent_path.join(&name);
        validate_path(&original)?;
        let snapshot = Snapshot {
            properties,
            symlink_target,
            original,
            parent,
            object,
            name,
        };
        revalidate(&snapshot)?;
        Ok(snapshot)
    }
    pub(super) fn current_path(snapshot: &Snapshot) -> io::Result<PathBuf> {
        revalidate(snapshot)?;
        let path = std::fs::read_link(format!("/proc/self/fd/{}", snapshot.object.as_raw_fd()))?;
        validate_path(&path)?;
        let path_c = c(path.as_os_str())?;
        let check = from_fd(unsafe {
            libc::open(
                path_c.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        if identity(&stat(&check)?) != snapshot.properties.identity {
            return Err(invalid("La ruta actual de propiedades fue reemplazada"));
        }
        Ok(path)
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    pub(super) fn apply_mode(object: &File, mode: u32) -> io::Result<()> {
        // Kernel UAPI asm-generic/unistd.h __NR_fchmodat2=452 on aarch64 and
        // x86_64. libc0.2.191 omits the aarch64 constant; do not use fchmodat's
        // older no-flags syscall nor its pathname/procfd emulation fallback.
        #[cfg(target_arch = "aarch64")]
        const CALL: libc::c_long = 452;
        #[cfg(target_arch = "x86_64")]
        const CALL: libc::c_long = libc::SYS_fchmodat2;
        if unsafe {
            libc::syscall(
                CALL,
                object.as_raw_fd(),
                c"".as_ptr(),
                mode as libc::mode_t,
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            let error = io::Error::last_os_error();
            if matches!(
                error.raw_os_error(),
                Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)
            ) {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Cambio seguro de permisos requiere fchmodat2(AT_EMPTY_PATH), Linux6.6+ y soporte del filesystem",
                ));
            }
            return Err(error);
        }
        Ok(())
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    pub(super) fn apply_mode(_: &File, _: u32) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Cambio de permisos por FD pendiente en esta arquitectura",
        ))
    }
    pub(super) fn chmod(
        snapshot: &Snapshot,
        mode: u32,
        cancel: &AtomicBool,
    ) -> io::Result<ChangeReport> {
        chmod_with_observer(snapshot, mode, cancel, revalidate)
    }
    fn chmod_with_observer(
        snapshot: &Snapshot,
        mode: u32,
        cancel: &AtomicBool,
        after: impl FnOnce(&Snapshot) -> io::Result<Properties>,
    ) -> io::Result<ChangeReport> {
        let before = revalidate(snapshot)?;
        if before.mode != snapshot.properties.mode
            || before.uid != snapshot.properties.uid
            || before.gid != snapshot.properties.gid
        {
            return Err(invalid(
                "Los permisos, propietario o grupo cambiaron desde la lectura; vuelve a cargar las propiedades antes de confirmar",
            ));
        }
        if !matches!(before.identity.kind, Kind::File | Kind::Directory) {
            return Err(invalid(
                "Los enlaces y objetos especiales sólo permiten lectura de propiedades",
            ));
        }
        if before.uid != unsafe { libc::geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Sólo se cambian permisos de objetos cuyo propietario es el usuario actual",
            ));
        }
        if before.mode & 0o7000 != 0 {
            return Err(invalid(
                "Los objetos con setuid/setgid/sticky son de sólo lectura en esta versión",
            ));
        }
        check_cancel(cancel)?;
        apply_mode(&snapshot.object, mode)?;
        let mut report = ChangeReport {
            requested_mode: mode,
            after: None,
            warnings: Vec::new(),
        };
        match after(snapshot) {
            Ok(properties) => {
                if properties.mode & 0o777 != mode {
                    report.warnings.push("El modo observado después del cambio es distinto; puede existir una modificación concurrente".into());
                }
                report.after = Some(properties);
            }
            Err(error) => {
                let text = error.to_string();
                let mut end = text.len().min(1024);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                report.warnings.push(format!(
                    "Permisos aplicados; no se pudo verificar el estado posterior: {}",
                    &text[..end]
                ));
            }
        }
        Ok(report)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            fs,
            os::unix::{
                ffi::OsStringExt,
                fs::{PermissionsExt, symlink},
            },
            sync::atomic::{AtomicU64, Ordering},
        };
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let base = crate::theme::home().join(".cache/files-test");
                fs::create_dir_all(&base).unwrap();
                let root = base.join(format!(
                    "properties-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&root).unwrap();
                Self(root)
            }
            fn file(&self, name: &OsStr) -> PathBuf {
                let path = self.0.join(name);
                fs::write(&path, b"property-bytes").unwrap();
                path
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fn change(snapshot: &Snapshot, mode: u32) -> ChangeReport {
            super::super::chmod(snapshot, mode, &AtomicBool::new(false)).unwrap()
        }
        #[test]
        fn captures_unicode_and_raw_names_then_changes_mode_without_changing_bytes() {
            let f = Fixture::new();
            let name = OsString::from_vec(b"Unicode-\xe6\x97\xa5-raw\xff".to_vec());
            let path = f.file(&name);
            let snapshot = super::super::capture(&path).unwrap();
            assert_eq!(snapshot.properties.identity.kind, Kind::File);
            assert_eq!(snapshot.properties.apparent_bytes, 14);
            assert_eq!(snapshot.current_path().unwrap(), path);
            assert!(snapshot.symlink_target.is_none());
            let report = change(&snapshot, 0o640);
            assert_eq!(report.after.unwrap().mode & 0o777, 0o640);
            assert!(report.warnings.is_empty());
            assert_eq!(fs::read(path).unwrap(), b"property-bytes");
        }
        #[test]
        fn replaced_inode_is_rejected_and_foreign_symlink_target_is_untouched() {
            let f = Fixture::new();
            let path = f.file(OsStr::new("before"));
            let snapshot = super::super::capture(&path).unwrap();
            let moved = f.0.join("moved");
            fs::rename(&path, &moved).unwrap();
            let foreign = f.file(OsStr::new("foreign"));
            let mode = fs::metadata(&foreign).unwrap().permissions().mode() & 0o777;
            symlink(&foreign, &path).unwrap();
            assert!(super::super::chmod(&snapshot, 0o777, &AtomicBool::new(false)).is_err());
            assert!(snapshot.current_path().is_err());
            assert_eq!(
                fs::metadata(foreign).unwrap().permissions().mode() & 0o777,
                mode
            );
        }
        #[test]
        fn symlink_target_is_lossless_and_mode_change_never_follows_it() {
            let f = Fixture::new();
            let target = OsString::from_vec(b"nonexistent\xff\n".to_vec());
            let link = f.0.join("link");
            symlink(&target, &link).unwrap();
            let snapshot = super::super::capture(&link).unwrap();
            assert_eq!(snapshot.properties.identity.kind, Kind::Symlink);
            assert_eq!(snapshot.symlink_target.as_deref(), Some(target.as_os_str()));
            assert!(super::super::chmod(&snapshot, 0o777, &AtomicBool::new(false)).is_err());
            assert_eq!(fs::read_link(link).unwrap(), PathBuf::from(target));
        }
        #[test]
        fn directory_parent_rename_is_preserved_and_cancel_is_before_mutation() {
            let f = Fixture::new();
            let directory = f.0.join("directory");
            fs::create_dir(&directory).unwrap();
            let snapshot = super::super::capture(&directory).unwrap();
            let before = fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
            assert!(super::super::chmod(&snapshot, 0o700, &AtomicBool::new(true)).is_err());
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                before
            );
            let moved = f.0.with_extension("relocated");
            fs::rename(&f.0, &moved).unwrap();
            let report = change(&snapshot, 0o700);
            assert_eq!(report.after.unwrap().mode & 0o777, 0o700);
            assert_eq!(snapshot.current_path().unwrap(), moved.join("directory"));
            fs::rename(moved, &f.0).unwrap();
        }
        #[test]
        fn mode_zero_can_be_changed_back_without_read_access_and_special_modes_are_refused() {
            let f = Fixture::new();
            let path = f.file(OsStr::new("zero"));
            let mut snapshot = super::super::capture(&path).unwrap();
            snapshot.properties = change(&snapshot, 0).after.unwrap();
            snapshot.properties = change(&snapshot, 0o600).after.unwrap();
            assert!(super::super::chmod(&snapshot, 0o4755, &AtomicBool::new(false)).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o1700)).unwrap();
            assert!(super::super::chmod(&snapshot, 0o600, &AtomicBool::new(false)).is_err());
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        #[test]
        fn external_mode_change_rejects_stale_dialog_and_own_confirmed_changes_can_continue() {
            let f = Fixture::new();
            let path = f.file(OsStr::new("mode-guard"));
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let mut snapshot = super::super::capture(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let failure =
                super::super::chmod(&snapshot, 0o644, &AtomicBool::new(false)).unwrap_err();
            assert!(failure.to_string().contains("cambiaron desde la lectura"));
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                0o640
            );
            // Reopen only after acknowledging the external change. Own edits
            // can then reuse these FDs by adopting their confirmed observation.
            snapshot = super::super::capture(&path).unwrap();
            snapshot.properties = change(&snapshot, 0o600).after.unwrap();
            snapshot.properties = change(&snapshot, 0o644).after.unwrap();
            assert_eq!(snapshot.properties.mode & 0o7777, 0o644);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                0o644
            );
        }
        #[test]
        fn owner_and_group_observation_guard_rejects_stale_snapshot_without_chown() {
            let f = Fixture::new();
            let path = f.file(OsStr::new("owner-guard"));
            let mut snapshot = super::super::capture(&path).unwrap();
            let original = snapshot.properties;
            snapshot.properties.gid = original.gid.wrapping_add(1);
            assert!(
                super::super::chmod(&snapshot, 0o600, &AtomicBool::new(false))
                    .unwrap_err()
                    .to_string()
                    .contains("cambiaron desde la lectura")
            );
            snapshot.properties = original;
            snapshot.properties.uid = original.uid.wrapping_add(1);
            assert!(
                super::super::chmod(&snapshot, 0o600, &AtomicBool::new(false))
                    .unwrap_err()
                    .to_string()
                    .contains("cambiaron desde la lectura")
            );
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o7777,
                original.mode
            );
        }
        #[test]
        fn post_commit_diagnostic_failure_is_reported_as_applied_warning() {
            let f = Fixture::new();
            let path = f.file(OsStr::new("item"));
            let snapshot = super::super::capture(&path).unwrap();
            let report = chmod_with_observer(&snapshot, 0o640, &AtomicBool::new(false), |_| {
                Err(io::Error::other("Injected diagnostic failure"))
            })
            .unwrap();
            assert_eq!(report.requested_mode, 0o640);
            assert!(report.after.is_none());
            assert_eq!(report.warnings.len(), 1);
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }
}
#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Propiedades/permisos locales pendientes en esta plataforma",
        )
    }
    pub(super) fn capture(_: &Path) -> io::Result<Snapshot> {
        Err(unsupported())
    }
    pub(super) fn current_path(_: &Snapshot) -> io::Result<PathBuf> {
        Err(unsupported())
    }
    pub(super) fn chmod(_: &Snapshot, _: u32, _: &AtomicBool) -> io::Result<ChangeReport> {
        Err(unsupported())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_and_permission_preflight_are_bounded() {
        for path in ["relative", "/", "/a/../b", "/a/./b"] {
            assert!(validate_path(Path::new(path)).is_err());
        }
        assert!(validate_path(Path::new("/home/á/archivo")).is_ok());
        assert!(validate_path(Path::new(&format!("/{}", "a".repeat(4096)))).is_err());
    }
}
