//! Linux same-filesystem home-trash storage, one item per operation. No erase,
//! cross-device copy fallback, permanent deletion, batch journal or crash-atomic
//! claim. UI callers must bound receipt history and run all filesystem work off UI.
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    kind: u32,
}
#[derive(Debug)]
struct Context {
    root: File,
    files: File,
    info: File,
}
/// Share one context across a bounded undo batch: three shared directory FDs.
#[derive(Clone, Debug)]
pub struct TrashContext {
    inner: Arc<Context>,
}
/// Read-only observations, never authority for a later restore or deletion.
#[derive(Debug, Default)]
pub struct Catalog {
    pub items: Vec<CatalogItem>,
    pub limited: bool,
}
#[derive(Debug)]
pub struct CatalogItem {
    pub name: std::ffi::OsString,
    pub original: Option<PathBuf>,
    pub deletion_date: Option<String>,
    pub issue: Option<String>,
}
/// Home-trash catalog, bounded to 4096 names / 4 MiB. Does not create a
/// missing trash, follow stored objects, or grant mutation permissions.
pub fn catalog(root: &Path, cancel: &AtomicBool) -> io::Result<Catalog> {
    platform::catalog(root, cancel)
}
/// Three per-item FDs pin the original parent, object and metadata inode. A
/// receipt is intentionally not Clone. Dropping it never erases a trashed item.
#[derive(Debug)]
pub struct Receipt {
    context: Arc<Context>,
    parent: File,
    object: File,
    info_file: File,
    original: PathBuf,
    name: std::ffi::OsString,
    trash_name: std::ffi::OsString,
    info_name: std::ffi::OsString,
    identity: Identity,
    info_identity: Identity,
}
#[derive(Debug)]
pub struct RestoreReport {
    /// The object was restored and its identity confirmed even when resolving
    /// its current pathname failed. Never substitute the historical pathname.
    pub restored_path: Option<PathBuf>,
    pub unresolved_paths: usize,
    /// Object restored successfully; these metadata paths may require attention.
    pub cleanup_paths: Vec<PathBuf>,
    pub cleanup_errors: Vec<String>,
}
#[derive(Debug)]
pub struct Failure {
    pub cause: io::Error,
    /// A fresh identity check found BOTH expected object and expected metadata
    /// still in their trash slots. False is manual recovery, not a retry promise.
    pub restore_retryable: bool,
    /// Verified current locations, never reconstructed stale parent paths.
    pub preserved_paths: Vec<PathBuf>,
    pub cleanup_paths: Vec<PathBuf>,
    pub rollback_errors: Vec<String>,
    /// A pinned object/location could not be resolved to a verified current path.
    pub unresolved_paths: usize,
}
impl From<io::Error> for Failure {
    fn from(cause: io::Error) -> Self {
        Self {
            cause,
            restore_retryable: false,
            preserved_paths: Vec::new(),
            cleanup_paths: Vec::new(),
            rollback_errors: Vec::new(),
            unresolved_paths: 0,
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} rutas conservadas, {} errores de recuperación, {} rutas sin resolver)",
            self.cause,
            self.preserved_paths.len(),
            self.rollback_errors.len(),
            self.unresolved_paths
        )
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
impl TrashContext {
    /// Root parent must already exist, be owned by the effective user and not be
    /// group/world writable. Ancestors must not be symlinks or writable by others.
    /// Only root/files/info are created; existing private modes are never changed.
    pub fn open(root: &Path) -> Result<Self, Failure> {
        platform::open_context(root)
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(Into::into)
    }
    pub fn trash(&self, source: &Path, cancel: &AtomicBool) -> Result<Receipt, Failure> {
        platform::trash(source, Arc::clone(&self.inner), cancel)
    }
}
pub fn trash(source: &Path, trash_root: &Path, cancel: &AtomicBool) -> Result<Receipt, Failure> {
    check_cancel(cancel)?;
    TrashContext::open(trash_root)?.trash(source, cancel)
}
pub fn restore(receipt: &Receipt, cancel: &AtomicBool) -> Result<RestoreReport, Failure> {
    platform::restore(receipt, cancel)
}
/// Restores the catalog item `name` (its name under `files/`) to the
/// original path recorded in its `.trashinfo`, never replacing an existing
/// entry (renameat2 NOREPLACE) and only within the same filesystem. Returns
/// the restored path and, when the metadata could not be removed afterwards,
/// why (the object is restored either way).
pub fn restore_item(
    root: &Path,
    name: &std::ffi::OsStr,
    cancel: &AtomicBool,
) -> io::Result<(PathBuf, Option<String>)> {
    check_cancel(cancel)?;
    platform::restore_item(root, name, cancel)
}
/// Permanently deletes the catalog item `name` and then its metadata.
pub fn purge_item(
    root: &Path,
    name: &std::ffi::OsStr,
    cancel: &AtomicBool,
    progress: &crate::remove_tree::Progress,
) -> io::Result<crate::remove_tree::Report> {
    check_cancel(cancel)?;
    platform::purge_item(root, name, cancel, progress)
}
/// Permanently deletes everything in the home trash, plus orphan metadata.
pub fn empty(
    root: &Path,
    cancel: &AtomicBool,
    progress: &crate::remove_tree::Progress,
) -> io::Result<crate::remove_tree::Report> {
    check_cancel(cancel)?;
    platform::empty(root, cancel, progress)
}
impl Receipt {
    /// Historical original path recorded in .trashinfo, NOT a current rescue path.
    pub fn original_path(&self) -> &Path {
        &self.original
    }
    pub fn current_original_path(&self) -> io::Result<PathBuf> {
        platform::child_path(&self.parent, &self.name)
    }
    pub fn trashed_path(&self) -> io::Result<PathBuf> {
        platform::verified_child_path(&self.context.files, &self.trash_name, &self.identity)
    }
    pub fn info_path(&self) -> io::Result<PathBuf> {
        platform::verified_child_path(&self.context.info, &self.info_name, &self.info_identity)
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Operación de papelera cancelada",
        ))
    } else {
        Ok(())
    }
}
fn validate_path(path: &Path) -> io::Result<()> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute()
        || bytes.len() > 4096
        || bytes.contains(&0)
        || bytes == b"/"
        || bytes
            .split(|b| *b == b'/')
            .any(|part| matches!(part, b"." | b".."))
    {
        return Err(invalid(
            "Ruta absoluta de papelera no válida (máximo 4096 bytes, sin . ni ..)",
        ));
    }
    Ok(())
}
/// Lossless URI-byte escaping for home-trash absolute Path=. '/' remains a path
/// separator; '%' and all non-unreserved bytes are escaped exactly once.
fn metadata(original: &Path, date: &str) -> io::Result<Vec<u8>> {
    validate_path(original)?;
    if date.len() != 19
        || !date.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            10 => b == b'T',
            13 | 16 => b == b':',
            _ => b.is_ascii_digit(),
        })
    {
        return Err(invalid("Fecha de papelera no válida"));
    }
    let mut text = Vec::with_capacity(original.as_os_str().as_encoded_bytes().len() * 3 + 64);
    text.extend_from_slice(b"[Trash Info]\nPath=");
    const HEX: &[u8] = b"0123456789ABCDEF";
    for b in original.as_os_str().as_encoded_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~' | b'/') {
            text.push(*b);
        } else {
            text.extend_from_slice(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 15) as usize]]);
        }
    }
    text.extend_from_slice(b"\nDeletionDate=");
    text.extend_from_slice(date.as_bytes());
    text.push(b'\n');
    Ok(text)
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        ffi::{CString, OsStr, OsString},
        io::Write,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::{OsStrExt, OsStringExt},
        },
        sync::atomic::AtomicU64,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    const MAX_ATTEMPTS: usize = 128;
    fn c(name: &OsStr) -> io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|_| invalid("NUL en nombre de papelera"))
    }
    fn stat(file: &File) -> io::Result<libc::stat> {
        let mut out = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(file.as_raw_fd(), out.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { out.assume_init() })
    }
    fn id(stat: &libc::stat) -> Identity {
        Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: stat.st_mode & libc::S_IFMT,
        }
    }
    fn fd_identity(file: &File) -> io::Result<Identity> {
        Ok(id(&stat(file)?))
    }
    fn identity(parent: &File, name: &OsStr) -> io::Result<Identity> {
        let name = c(name)?;
        let mut out = std::mem::MaybeUninit::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                name.as_ptr(),
                out.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(id(&unsafe { out.assume_init() }))
    }
    fn open_at(parent: &File, name: &OsStr, flags: i32) -> io::Result<File> {
        let name = c(name)?;
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                0o600,
            )
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn directory(path: &Path, secure_ancestors: bool) -> io::Result<File> {
        if !path.is_absolute() || path.as_os_str().len() > 4096 {
            return Err(invalid("Directorio absoluto fuera de límite"));
        }
        let slash = CString::new("/").unwrap();
        let fd = unsafe {
            libc::open(
                slash.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut current = unsafe { File::from_raw_fd(fd) };
        if secure_ancestors {
            secure_parent(&current, false)?;
        }
        for part in path.components() {
            match part {
                std::path::Component::RootDir => (),
                std::path::Component::Normal(name) => {
                    current = open_at(&current, name, libc::O_RDONLY | libc::O_DIRECTORY)?;
                    if secure_ancestors {
                        secure_parent(&current, false)?;
                    }
                }
                _ => return Err(invalid("Componente de directorio no admitido")),
            }
        }
        Ok(current)
    }
    fn secure_parent(directory: &File, require_owner: bool) -> io::Result<()> {
        let s = stat(directory)?;
        let uid = unsafe { libc::geteuid() };
        if s.st_mode & libc::S_IFMT != libc::S_IFDIR
            || s.st_mode & 0o022 != 0
            || (require_owner && s.st_uid != uid)
            || (!require_owner && s.st_uid != uid && s.st_uid != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Ancestro de papelera ajeno, inseguro o escribible por otros",
            ));
        }
        Ok(())
    }
    fn private(directory: &File) -> io::Result<()> {
        let s = stat(directory)?;
        if s.st_uid != unsafe { libc::geteuid() }
            || s.st_mode & libc::S_IFMT != libc::S_IFDIR
            || s.st_mode & 0o7777 != 0o700
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Papelera/files/info deben pertenecer al usuario y tener modo 0700",
            ));
        }
        Ok(())
    }
    fn ensure_private(parent: &File, name: &OsStr) -> io::Result<File> {
        let name_c = c(name)?;
        if unsafe { libc::mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o700) } != 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::AlreadyExists {
                return Err(e);
            }
        }
        let directory = open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY)?;
        private(&directory)?;
        Ok(directory)
    }
    pub(super) fn open_context(root: &Path) -> io::Result<Context> {
        validate_path(root)?;
        let parent = directory(
            root.parent()
                .ok_or_else(|| invalid("Falta padre de papelera"))?,
            true,
        )?;
        secure_parent(&parent, true)?;
        let root = ensure_private(
            &parent,
            root.file_name()
                .ok_or_else(|| invalid("Falta nombre de papelera"))?,
        )?;
        let files = ensure_private(&root, OsStr::new("files"))?;
        let info = ensure_private(&root, OsStr::new("info"))?;
        Ok(Context { root, files, info })
    }
    pub(super) fn catalog(root: &Path, cancel: &AtomicBool) -> io::Result<Catalog> {
        catalog_limited(root, cancel, 4096, 4 * 1024 * 1024)
    }
    fn catalog_limited(
        root: &Path,
        cancel: &AtomicBool,
        count_limit: usize,
        byte_limit: usize,
    ) -> io::Result<Catalog> {
        use std::io::Read;
        check_cancel(cancel)?;
        validate_path(root)?;
        let root_fd = match directory(root, true) {
            Ok(fd) => fd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Catalog::default()),
            Err(e) => return Err(e),
        };
        private(&root_fd)?;
        let files = open_at(
            &root_fd,
            OsStr::new("files"),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        let info = open_at(
            &root_fd,
            OsStr::new("info"),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        private(&files)?;
        private(&info)?;
        let context = Context {
            root: root_fd,
            files,
            info,
        };
        let mut result = Catalog::default();
        let mut bytes = 0usize;
        // /proc resolves our pinned descriptor, not the historical trash path.
        for entry in std::fs::read_dir(format!("/proc/self/fd/{}", context.files.as_raw_fd()))? {
            check_cancel(cancel)?;
            let name = entry?.file_name();
            if result.items.len() >= count_limit {
                result.limited = true;
                break;
            }
            let mut info_name = name.clone();
            info_name.push(".trashinfo");
            let metadata = (|| -> io::Result<(PathBuf, Option<String>)> {
                let file = open_at(&context.info, &info_name, libc::O_RDONLY | libc::O_NONBLOCK)?;
                let st = stat(&file)?;
                if st.st_mode & libc::S_IFMT != libc::S_IFREG || st.st_size > 65536 {
                    return Err(invalid(
                        "Metadatos de papelera no regulares o demasiado grandes",
                    ));
                }
                let mut data = Vec::new();
                file.take(65537).read_to_end(&mut data)?;
                if data.len() > 65536 {
                    return Err(invalid("Metadatos de papelera demasiado grandes"));
                }
                parse_catalog_info(&data)
            })();
            let (original, deletion_date, issue) = match metadata {
                Ok((path, date)) => (Some(path), date, None),
                Err(_) => (
                    None,
                    None,
                    Some("Metadatos no disponibles o no válidos".into()),
                ),
            };
            let charge = name.len() + original.as_ref().map_or(0, |p| p.as_os_str().len()) + 256;
            if bytes.saturating_add(charge) > byte_limit {
                result.limited = true;
                break;
            }
            bytes += charge;
            result.items.push(CatalogItem {
                name,
                original,
                deletion_date,
                issue,
            });
        }
        validate_context(&context)?;
        result.items.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }
    /// An existing, private home trash (never created here).
    fn existing_context(root: &Path) -> io::Result<Context> {
        validate_path(root)?;
        let root_fd = directory(root, true)?;
        private(&root_fd)?;
        let files = open_at(
            &root_fd,
            OsStr::new("files"),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        let info = open_at(
            &root_fd,
            OsStr::new("info"),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        private(&files)?;
        private(&info)?;
        let context = Context {
            root: root_fd,
            files,
            info,
        };
        validate_context(&context)?;
        Ok(context)
    }
    fn item_name(name: &OsStr) -> io::Result<()> {
        let b = name.as_bytes();
        if b.is_empty() || b.len() > 255 || b == b"." || b == b".." || b.contains(&b'/') {
            return Err(invalid("Nombre de elemento de papelera no válido"));
        }
        Ok(())
    }
    fn info_name(name: &OsStr) -> OsString {
        let mut n = name.to_os_string();
        n.push(".trashinfo");
        n
    }
    fn read_info(context: &Context, info_name: &OsStr) -> io::Result<(PathBuf, Option<String>)> {
        use std::io::Read;
        let file = open_at(&context.info, info_name, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let st = stat(&file)?;
        if st.st_mode & libc::S_IFMT != libc::S_IFREG || st.st_size > 65536 {
            return Err(invalid(
                "Metadatos de papelera no regulares o demasiado grandes",
            ));
        }
        let mut data = Vec::new();
        file.take(65537).read_to_end(&mut data)?;
        if data.len() > 65536 {
            return Err(invalid("Metadatos de papelera demasiado grandes"));
        }
        parse_catalog_info(&data)
    }
    fn unlink_plain(dir: &File, name: &OsStr) -> io::Result<()> {
        let name = c(name)?;
        if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub(super) fn restore_item(
        root: &Path,
        name: &OsStr,
        cancel: &AtomicBool,
    ) -> io::Result<(PathBuf, Option<String>)> {
        item_name(name)?;
        let context = existing_context(root)?;
        let info = info_name(name);
        let (original, _) = read_info(&context, &info)?;
        let (parent_path, target) = match (original.parent(), original.file_name()) {
            (Some(p), Some(n)) => (p, n),
            _ => return Err(invalid("Ruta de origen sin carpeta")),
        };
        identity(&context.files, name)?;
        check_cancel(cancel)?;
        let parent = directory(parent_path, false).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "La carpeta de origen ya no existe; restaura o crea la carpeta primero",
                )
            } else {
                e
            }
        })?;
        move_exclusive(&context.files, name, &parent, target).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Ya existe un elemento con ese nombre en el origen; no se reemplaza",
                )
            } else {
                e
            }
        })?;
        let cleanup = unlink_plain(&context.info, &info)
            .err()
            .map(|e| e.to_string());
        Ok((original, cleanup))
    }
    pub(super) fn purge_item(
        root: &Path,
        name: &OsStr,
        cancel: &AtomicBool,
        progress: &crate::remove_tree::Progress,
    ) -> io::Result<crate::remove_tree::Report> {
        item_name(name)?;
        let context = existing_context(root)?;
        let mut report = crate::remove_tree::Report::default();
        crate::remove_tree::platform::remove_at(
            &context.files,
            name,
            &root.join("files"),
            cancel,
            progress,
            &mut report,
        );
        // Metadata goes only once the object is really gone.
        if matches!(identity(&context.files, name), Err(e) if e.kind() == io::ErrorKind::NotFound)
            && let Err(e) = unlink_plain(&context.info, &info_name(name))
            && e.kind() != io::ErrorKind::NotFound
        {
            report.failed += 1;
            report
                .failures
                .push((root.join("info").join(info_name(name)), e.to_string()));
        }
        Ok(report)
    }
    pub(super) fn empty(
        root: &Path,
        cancel: &AtomicBool,
        progress: &crate::remove_tree::Progress,
    ) -> io::Result<crate::remove_tree::Report> {
        let context = match existing_context(root) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Default::default()),
            Err(e) => return Err(e),
        };
        let mut report = crate::remove_tree::Report::default();
        let names: Vec<OsString> =
            std::fs::read_dir(format!("/proc/self/fd/{}", context.files.as_raw_fd()))?
                .filter_map(|e| e.ok().map(|e| e.file_name()))
                .collect();
        for name in names {
            if cancel.load(Ordering::Acquire) {
                report.cancelled = true;
                return Ok(report);
            }
            let mut one = crate::remove_tree::Report::default();
            crate::remove_tree::platform::remove_at(
                &context.files,
                &name,
                &root.join("files"),
                cancel,
                progress,
                &mut one,
            );
            report.merge(one);
        }
        // Metadata whose object is gone (just deleted, or orphaned before).
        let infos: Vec<OsString> =
            std::fs::read_dir(format!("/proc/self/fd/{}", context.info.as_raw_fd()))?
                .filter_map(|e| e.ok().map(|e| e.file_name()))
                .collect();
        for info in infos {
            let Some(object) = info.as_bytes().strip_suffix(b".trashinfo") else {
                continue;
            };
            let object = OsStr::from_bytes(object);
            if matches!(identity(&context.files, object), Err(e) if e.kind() == io::ErrorKind::NotFound)
                && let Err(e) = unlink_plain(&context.info, &info)
            {
                report.failed += 1;
                if report.failures.len() < 32 {
                    report
                        .failures
                        .push((root.join("info").join(&info), e.to_string()));
                }
            }
        }
        Ok(report)
    }
    fn parse_catalog_info(data: &[u8]) -> io::Result<(PathBuf, Option<String>)> {
        let mut lines = data
            .split(|&b| b == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l));
        if lines.next() != Some(b"[Trash Info]".as_slice()) {
            return Err(invalid("Falta cabecera Trash Info"));
        }
        let mut path = None;
        let mut date = None;
        for line in lines {
            if let Some(encoded) = line.strip_prefix(b"Path=") {
                if path.is_some() || encoded.len() > 3 * 4096 {
                    return Err(invalid("Ruta de papelera duplicada o fuera de límite"));
                }
                let mut decoded = Vec::with_capacity(encoded.len().min(4096));
                let mut i = 0;
                while i < encoded.len() {
                    let b = if encoded[i] == b'%' {
                        let hex = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
                        let pair = encoded
                            .get(i + 1..i + 3)
                            .ok_or_else(|| invalid("Escape incompleto"))?;
                        let hi = hex(pair[0]).ok_or_else(|| invalid("Escape inválido"))?;
                        let lo = hex(pair[1]).ok_or_else(|| invalid("Escape inválido"))?;
                        i += 3;
                        hi * 16 + lo
                    } else {
                        let b = encoded[i];
                        i += 1;
                        b
                    };
                    decoded.push(b);
                    if decoded.len() > 4096 {
                        return Err(invalid("Ruta fuera de límite"));
                    }
                }
                let original = PathBuf::from(OsString::from_vec(decoded));
                validate_path(&original)?;
                path = Some(original);
            } else if let Some(raw) = line.strip_prefix(b"DeletionDate=") {
                let valid = raw.len() == 19
                    && raw.iter().enumerate().all(|(i, b)| match i {
                        4 | 7 => *b == b'-',
                        10 => *b == b'T',
                        13 | 16 => *b == b':',
                        _ => b.is_ascii_digit(),
                    });
                if valid {
                    date = std::str::from_utf8(raw).ok().map(str::to_owned);
                }
            }
        }
        Ok((path.ok_or_else(|| invalid("Falta Path"))?, date))
    }
    fn validate_context(context: &Context) -> io::Result<()> {
        private(&context.root)?;
        private(&context.files)?;
        private(&context.info)?;
        if identity(&context.root, OsStr::new("files"))? != fd_identity(&context.files)?
            || identity(&context.root, OsStr::new("info"))? != fd_identity(&context.info)?
        {
            return Err(invalid("files/info fueron reemplazados o movidos"));
        }
        Ok(())
    }
    fn ensure_identity(parent: &File, name: &OsStr, expected: &Identity) -> io::Result<()> {
        if identity(parent, name)? != *expected {
            return Err(invalid("El objeto fue reemplazado durante la operación"));
        }
        Ok(())
    }
    fn move_exclusive(from: &File, name: &OsStr, to: &File, new: &OsStr) -> io::Result<()> {
        let name = c(name)?;
        let new = c(new)?;
        if unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                from.as_raw_fd(),
                name.as_ptr(),
                to.as_raw_fd(),
                new.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EXDEV) {
                return Err(io::Error::new(
                    io::ErrorKind::CrossesDevices,
                    "Papelera en otro sistema de archivos: origen intacto; papelera por volumen pendiente",
                ));
            }
            return Err(e);
        }
        Ok(())
    }
    fn private_info(file: &File) -> io::Result<()> {
        let s = stat(file)?;
        if s.st_uid != unsafe { libc::geteuid() }
            || s.st_mode & libc::S_IFMT != libc::S_IFREG
            || s.st_mode & 0o7777 != 0o600
            || s.st_nlink != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Metadatos .trashinfo no privados, enlazados o ajenos",
            ));
        }
        Ok(())
    }
    fn remove_info(info: &File, name: &OsStr, expected: &Identity) -> io::Result<()> {
        ensure_identity(info, name, expected)?;
        let name = c(name)?;
        if unsafe { libc::unlinkat(info.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    fn current_directory(directory: &File) -> io::Result<PathBuf> {
        let current = std::fs::read_link(format!("/proc/self/fd/{}", directory.as_raw_fd()))?;
        if current.as_os_str().len() > 4096 || !current.is_absolute() {
            return Err(invalid("Ruta actual de recuperación fuera de límite"));
        }
        let check = self::directory(&current, false)?;
        if fd_identity(&check)? != fd_identity(directory)? {
            return Err(invalid("Ruta del descriptor reemplazada o inaccesible"));
        }
        Ok(current)
    }
    pub(super) fn child_path(parent: &File, name: &OsStr) -> io::Result<PathBuf> {
        Ok(current_directory(parent)?.join(name))
    }
    pub(super) fn verified_child_path(
        parent: &File,
        name: &OsStr,
        expected: &Identity,
    ) -> io::Result<PathBuf> {
        ensure_identity(parent, name, expected)?;
        child_path(parent, name)
    }
    fn ancestor(directory: &File, wanted: &Identity) -> io::Result<bool> {
        let mut current = directory.try_clone()?;
        for _ in 0..1024 {
            let here = fd_identity(&current)?;
            if &here == wanted {
                return Ok(true);
            }
            let parent = open_at(
                &current,
                OsStr::new(".."),
                libc::O_RDONLY | libc::O_DIRECTORY,
            )?;
            if fd_identity(&parent)? == here {
                return Ok(false);
            }
            current = parent;
        }
        Err(invalid("Demasiados ancestros para comprobar la papelera"))
    }
    fn date() -> io::Result<String> {
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        let mut out = std::mem::MaybeUninit::<libc::tm>::uninit();
        if now == -1 || unsafe { libc::localtime_r(&now, out.as_mut_ptr()) }.is_null() {
            return Err(io::Error::other("No se pudo obtener la fecha local"));
        }
        let out = unsafe { out.assume_init() };
        let year = out.tm_year + 1900;
        if !(0..=9999).contains(&year) {
            return Err(invalid("Fecha local fuera de rango"));
        }
        Ok(format!(
            "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            out.tm_mon + 1,
            out.tm_mday,
            out.tm_hour,
            out.tm_min,
            out.tm_sec
        ))
    }
    fn candidate(name: &OsStr) -> OsString {
        // Reserve suffix + .trashinfo below Linux NAME_MAX255. Cutting raw bytes
        // is intentional: storage names need not retain the original encoding.
        let mut raw = name.as_bytes()[..name.len().min(170)].to_vec();
        raw.extend_from_slice(
            format!(
                ".files-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )
            .as_bytes(),
        );
        OsString::from_vec(raw)
    }
    fn record_path(output: &mut Vec<PathBuf>, path: io::Result<PathBuf>, unresolved: &mut usize) {
        match path {
            Ok(path) if output.len() < 4 && path.as_os_str().len() <= 8192 => output.push(path),
            _ => *unresolved += 1,
        }
    }
    fn error_text(error: &io::Error) -> String {
        let text = error.to_string();
        let mut end = text.len().min(1024);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
    }
    fn cleanup(
        failure: &mut Failure,
        context: &Context,
        info_name: &OsStr,
        info_identity: &Identity,
        info_file: &File,
    ) {
        if let Err(e) = remove_info(&context.info, info_name, info_identity) {
            failure.rollback_errors.push(error_text(&e));
            record_path(
                &mut failure.cleanup_paths,
                object_path(info_file),
                &mut failure.unresolved_paths,
            );
        }
    }
    pub(super) fn trash(
        source: &Path,
        context: Arc<Context>,
        cancel: &AtomicBool,
    ) -> Result<Receipt, Failure> {
        trash_inner(source, context, cancel, || {})
    }
    fn trash_inner(
        source: &Path,
        context: Arc<Context>,
        cancel: &AtomicBool,
        before_commit: impl FnOnce(),
    ) -> Result<Receipt, Failure> {
        trash_hooks(source, context, cancel, before_commit, || {})
    }
    fn trash_hooks(
        source: &Path,
        context: Arc<Context>,
        cancel: &AtomicBool,
        before_commit: impl FnOnce(),
        after_move: impl FnOnce(),
    ) -> Result<Receipt, Failure> {
        validate_path(source)?;
        check_cancel(cancel)?;
        validate_context(&context)?;
        let name = source
            .file_name()
            .filter(|name| !name.is_empty() && name.len() <= 4096)
            .ok_or_else(|| invalid("Falta nombre de origen"))?
            .to_os_string();
        // Resolve parent once, preserving the last symlink as the object itself.
        let parent_path = source
            .parent()
            .ok_or_else(|| invalid("Falta padre de origen"))?
            .canonicalize()?;
        let parent = directory(&parent_path, false)?;
        let original = child_path(&parent, &name)?;
        validate_path(&original)?;
        let object = open_at(&parent, &name, libc::O_PATH)?;
        let expected = fd_identity(&object)?;
        if ancestor(&parent, &fd_identity(&context.root)?)?
            || (expected.kind == libc::S_IFDIR && ancestor(&context.root, &expected)?)
        {
            return Err(invalid(
                "No se puede enviar la papelera, su contenido o un ancestro a sí misma",
            )
            .into());
        }
        if expected.device != fd_identity(&context.files)?.device {
            return Err(io::Error::new(io::ErrorKind::CrossesDevices, "Papelera en otro sistema de archivos: origen intacto; papelera por volumen pendiente").into());
        }
        let data = metadata(&original, &date()?)?;
        let mut before_commit = Some(before_commit);
        for _ in 0..MAX_ATTEMPTS {
            check_cancel(cancel)?;
            validate_context(&context)?;
            let trash_name = candidate(&name);
            let mut info_name = trash_name.clone();
            info_name.push(".trashinfo");
            let mut info_file = match open_at(
                &context.info,
                &info_name,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            ) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            };
            let info_identity = fd_identity(&info_file)?;
            let preparation = private_info(&info_file)
                .and_then(|()| info_file.write_all(&data))
                .and_then(|()| info_file.sync_all())
                .and_then(|()| context.info.sync_all());
            if let Err(cause) = preparation {
                let mut failure = Failure::from(cause);
                cleanup(
                    &mut failure,
                    &context,
                    &info_name,
                    &info_identity,
                    &info_file,
                );
                return Err(failure);
            }
            if let Some(hook) = before_commit.take() {
                hook();
            }
            let preparation = check_cancel(cancel)
                .and_then(|()| validate_context(&context))
                .and_then(|()| ensure_identity(&context.info, &info_name, &info_identity))
                .and_then(|()| private_info(&info_file))
                .and_then(|()| ensure_identity(&parent, &name, &expected));
            if let Err(cause) = preparation {
                let mut failure = Failure::from(cause);
                cleanup(
                    &mut failure,
                    &context,
                    &info_name,
                    &info_identity,
                    &info_file,
                );
                return Err(failure);
            }
            match move_exclusive(&parent, &name, &context.files, &trash_name) {
                Err(cause) if cause.kind() == io::ErrorKind::AlreadyExists => {
                    let mut failure = Failure::from(cause);
                    cleanup(
                        &mut failure,
                        &context,
                        &info_name,
                        &info_identity,
                        &info_file,
                    );
                    if failure.rollback_errors.is_empty() {
                        continue;
                    }
                    return Err(failure);
                }
                Err(cause) => {
                    let mut failure = Failure::from(cause);
                    cleanup(
                        &mut failure,
                        &context,
                        &info_name,
                        &info_identity,
                        &info_file,
                    );
                    return Err(failure);
                }
                Ok(()) => (),
            }
            after_move();
            if let Err(cause) = ensure_identity(&context.files, &trash_name, &expected)
                .and_then(|()| validate_context(&context))
            {
                let mut failure = Failure::from(cause);
                // Only the expected pinned object is eligible for rollback.
                // A foreign replacement at the trash slot must remain in place,
                // even when the original pathname is now empty. We cannot infer
                // when the replacement happened from a successful rename alone.
                let moved = identity(&context.files, &trash_name);
                let rollback = ensure_identity(&context.files, &trash_name, &expected)
                    .and_then(|()| move_exclusive(&context.files, &trash_name, &parent, &name));
                match rollback {
                    Ok(()) => cleanup(
                        &mut failure,
                        &context,
                        &info_name,
                        &info_identity,
                        &info_file,
                    ),
                    Err(e) => {
                        failure.rollback_errors.push(error_text(&e));
                        record_path(
                            &mut failure.preserved_paths,
                            moved.and_then(|actual| {
                                verified_child_path(&context.files, &trash_name, &actual)
                            }),
                            &mut failure.unresolved_paths,
                        );
                        record_path(
                            &mut failure.cleanup_paths,
                            verified_child_path(&context.info, &info_name, &info_identity),
                            &mut failure.unresolved_paths,
                        );
                    }
                }
                record_path(
                    &mut failure.preserved_paths,
                    object_path(&object),
                    &mut failure.unresolved_paths,
                );
                return Err(failure);
            }
            return Ok(Receipt {
                context,
                parent,
                object,
                info_file,
                original,
                name,
                trash_name,
                info_name,
                identity: expected,
                info_identity,
            });
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "No se pudo reservar nombre único en la papelera tras 128 intentos",
        )
        .into())
    }
    pub(super) fn restore(
        receipt: &Receipt,
        cancel: &AtomicBool,
    ) -> Result<RestoreReport, Failure> {
        restore_with_hooks(
            receipt,
            cancel,
            || {},
            |receipt| verified_child_path(&receipt.parent, &receipt.name, &receipt.identity),
        )
    }
    fn restore_with_hooks(
        receipt: &Receipt,
        cancel: &AtomicBool,
        after_restore_move: impl FnOnce(),
        resolve_restored_path: impl FnOnce(&Receipt) -> io::Result<PathBuf>,
    ) -> Result<RestoreReport, Failure> {
        match restore_operation(receipt, cancel, after_restore_move, resolve_restored_path) {
            Ok(report) => Ok(report),
            Err(mut failure) => {
                // Error kind does not determine whether this receipt can retry.
                // A conflict/cancel is retryable only while BOTH slots still
                // refer to the pinned expected inodes; any lookup error is false.
                failure.restore_retryable = identity(&receipt.context.files, &receipt.trash_name)
                    .is_ok_and(|actual| actual == receipt.identity)
                    && identity(&receipt.context.info, &receipt.info_name)
                        .is_ok_and(|actual| actual == receipt.info_identity);
                record_path(
                    &mut failure.preserved_paths,
                    object_path(&receipt.object),
                    &mut failure.unresolved_paths,
                );
                record_path(
                    &mut failure.preserved_paths,
                    object_path(&receipt.info_file),
                    &mut failure.unresolved_paths,
                );
                // If a slot was replaced before this operation, show the
                // observed foreign occupant too, without moving/deleting it.
                for (parent, name, expected) in [
                    (
                        &receipt.context.files,
                        receipt.trash_name.as_os_str(),
                        &receipt.identity,
                    ),
                    (
                        &receipt.context.info,
                        receipt.info_name.as_os_str(),
                        &receipt.info_identity,
                    ),
                ] {
                    if let Ok(actual) = identity(parent, name)
                        && actual != *expected
                    {
                        record_path(
                            &mut failure.preserved_paths,
                            verified_child_path(parent, name, &actual),
                            &mut failure.unresolved_paths,
                        );
                    }
                }
                Err(failure)
            }
        }
    }
    fn object_path(object: &File) -> io::Result<PathBuf> {
        let path = std::fs::read_link(format!("/proc/self/fd/{}", object.as_raw_fd()))?;
        validate_path(&path)?;
        let parent = directory(
            path.parent().ok_or_else(|| invalid("Objeto sin padre"))?,
            false,
        )?;
        let name = path
            .file_name()
            .ok_or_else(|| invalid("Objeto sin nombre"))?;
        ensure_identity(&parent, name, &fd_identity(object)?)?;
        Ok(path)
    }
    #[cfg(test)]
    fn restore_with_resolver(
        receipt: &Receipt,
        cancel: &AtomicBool,
        resolve_restored_path: impl FnOnce(&Receipt) -> io::Result<PathBuf>,
    ) -> Result<RestoreReport, Failure> {
        restore_with_hooks(receipt, cancel, || {}, resolve_restored_path)
    }
    fn restore_operation(
        receipt: &Receipt,
        cancel: &AtomicBool,
        after_restore_move: impl FnOnce(),
        resolve_restored_path: impl FnOnce(&Receipt) -> io::Result<PathBuf>,
    ) -> Result<RestoreReport, Failure> {
        check_cancel(cancel)?;
        validate_context(&receipt.context)?;
        if fd_identity(&receipt.object)? != receipt.identity
            || fd_identity(&receipt.info_file)? != receipt.info_identity
        {
            return Err(invalid("Identidad del recibo de papelera no válida").into());
        }
        ensure_identity(
            &receipt.context.files,
            &receipt.trash_name,
            &receipt.identity,
        )?;
        ensure_identity(
            &receipt.context.info,
            &receipt.info_name,
            &receipt.info_identity,
        )?;
        private_info(&receipt.info_file)?;
        // Verify a current path BEFORE mutation; do not restore into an unlinked
        // parent or promise a stale old pathname after external relocation.
        child_path(&receipt.parent, &receipt.name)?;
        check_cancel(cancel)?;
        move_exclusive(
            &receipt.context.files,
            &receipt.trash_name,
            &receipt.parent,
            &receipt.name,
        )?;
        after_restore_move();
        if let Err(cause) = ensure_identity(&receipt.parent, &receipt.name, &receipt.identity) {
            // We cannot tell a pre-syscall source replacement from an actor
            // replacing the restored name afterwards. Moving this observed
            // foreign inode back to Trash would perform an unauthorized second
            // relocation. Preserve it in place; report the pinned original from
            // the wrapper and require manual recovery when slots no longer match.
            let mut failure = Failure::from(cause);
            record_path(
                &mut failure.preserved_paths,
                identity(&receipt.parent, &receipt.name).and_then(|actual| {
                    verified_child_path(&receipt.parent, &receipt.name, &actual)
                }),
                &mut failure.unresolved_paths,
            );
            return Err(failure);
        }
        // Commit is confirmed. A diagnostic path lookup is not another move
        // failure: returning Err here would leave an already-restored receipt
        // in UI undo history and a retry would search the now-empty trash slot.
        let mut report = RestoreReport {
            restored_path: None,
            unresolved_paths: 0,
            cleanup_paths: Vec::new(),
            cleanup_errors: Vec::new(),
        };
        match resolve_restored_path(receipt) {
            Ok(path) => report.restored_path = Some(path),
            Err(error) => {
                report.unresolved_paths += 1;
                report.cleanup_errors.push(format!(
                    "Objeto restaurado e identidad confirmada; no se pudo verificar su ruta actual: {}",
                    error_text(&error)
                ));
            }
        }
        if let Err(e) = remove_info(
            &receipt.context.info,
            &receipt.info_name,
            &receipt.info_identity,
        ) {
            report.cleanup_errors.push(error_text(&e));
            match object_path(&receipt.info_file) {
                Ok(path) => report.cleanup_paths.push(path),
                Err(e) => {
                    report.unresolved_paths += 1;
                    report.cleanup_errors.push(error_text(&e));
                }
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
        };
        struct Fixture {
            base: PathBuf,
            source: PathBuf,
            root: PathBuf,
        }
        impl Fixture {
            fn new() -> Self {
                let cache = crate::theme::home().join(".cache/files-test");
                fs::create_dir_all(&cache).unwrap();
                let base = cache.join(format!(
                    "trash-core-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&base).unwrap();
                fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
                let source = base.join("source");
                fs::create_dir(&source).unwrap();
                let root = base.join("Trash");
                Self { base, source, root }
            }
            fn context(&self) -> TrashContext {
                TrashContext::open(&self.root).unwrap()
            }
            fn file(&self, name: &str) -> PathBuf {
                let path = self.source.join(name);
                fs::write(&path, b"original").unwrap();
                path
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.base);
            }
        }
        fn flag() -> AtomicBool {
            AtomicBool::new(false)
        }
        #[test]
        fn catalog_is_read_only_lossless_and_keeps_orphan_objects_visible() {
            let f = Fixture::new();
            assert!(catalog(&f.root, &flag()).unwrap().items.is_empty());
            assert!(!f.root.exists());
            let original = f.source.join(OsString::from_vec(b"raw\xff%\n".to_vec()));
            fs::write(&original, b"bytes").unwrap();
            let receipt = f.context().trash(&original, &flag()).unwrap();
            let before = fs::read(receipt.info_path().unwrap()).unwrap();
            let entries = catalog(&f.root, &flag()).unwrap();
            assert_eq!(entries.items.len(), 1);
            assert_eq!(entries.items[0].original.as_ref(), Some(&original));
            assert!(entries.items[0].deletion_date.is_some());
            assert_eq!(fs::read(receipt.info_path().unwrap()).unwrap(), before);
            fs::write(f.root.join("files/orphan"), b"preserve").unwrap();
            let entries = catalog(&f.root, &flag()).unwrap();
            assert_eq!(entries.items.len(), 2);
            assert!(
                entries
                    .items
                    .iter()
                    .any(|i| i.name == "orphan" && i.issue.is_some())
            );
            assert_eq!(fs::read(f.root.join("files/orphan")).unwrap(), b"preserve");
        }
        #[test]
        fn restore_purge_and_empty_from_the_catalog() {
            let f = Fixture::new();
            let progress = crate::remove_tree::Progress::default();
            // Restore by catalog name, never over an existing entry.
            let a = f.file("a.txt");
            f.context().trash(&a, &flag()).unwrap();
            let name = catalog(&f.root, &flag()).unwrap().items[0].name.clone();
            fs::write(&a, b"newer").unwrap();
            let occupied = restore_item(&f.root, &name, &flag()).unwrap_err();
            assert_eq!(occupied.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(fs::read(&a).unwrap(), b"newer");
            fs::remove_file(&a).unwrap();
            let (restored, cleanup) = restore_item(&f.root, &name, &flag()).unwrap();
            assert_eq!((restored.as_path(), cleanup), (a.as_path(), None));
            assert_eq!(fs::read(&a).unwrap(), b"original");
            assert!(catalog(&f.root, &flag()).unwrap().items.is_empty());
            assert!(restore_item(&f.root, OsStr::new("../x"), &flag()).is_err());
            // Purge one item: object and metadata gone.
            let dir = f.source.join("dir");
            fs::create_dir_all(dir.join("sub")).unwrap();
            fs::write(dir.join("sub/f"), b"123").unwrap();
            f.context().trash(&dir, &flag()).unwrap();
            let name = catalog(&f.root, &flag()).unwrap().items[0].name.clone();
            let report = purge_item(&f.root, &name, &flag(), &progress).unwrap();
            assert!(report.complete(), "{report:?}");
            assert_eq!(report.removed, 3);
            assert!(catalog(&f.root, &flag()).unwrap().items.is_empty());
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 0);
            // Empty: items, orphan objects and orphan metadata.
            f.context().trash(&f.file("b"), &flag()).unwrap();
            f.context().trash(&f.file("c"), &flag()).unwrap();
            fs::write(f.root.join("files/orphan"), b"x").unwrap();
            fs::write(f.root.join("info/ghost.trashinfo"), b"[Trash Info]\n").unwrap();
            let report = empty(&f.root, &flag(), &progress).unwrap();
            assert!(report.complete(), "{report:?}");
            assert_eq!(fs::read_dir(f.root.join("files")).unwrap().count(), 0);
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 0);
            // Missing trash: nothing to do.
            assert_eq!(
                empty(&f.base.join("none/Trash"), &flag(), &progress)
                    .unwrap()
                    .removed,
                0
            );
        }
        #[test]
        fn catalog_rejects_redirected_directories_and_does_not_follow_info_symlinks() {
            let f = Fixture::new();
            let receipt = f.context().trash(&f.file("a"), &flag()).unwrap();
            let info = receipt.info_path().unwrap();
            fs::remove_file(&info).unwrap();
            symlink(f.file("private"), &info).unwrap();
            let entries = catalog(&f.root, &flag()).unwrap();
            assert!(entries.items[0].original.is_none());
            assert!(entries.items[0].issue.is_some());
            fs::rename(f.root.join("files"), f.base.join("old-files")).unwrap();
            symlink(f.base.join("old-files"), f.root.join("files")).unwrap();
            assert!(catalog(&f.root, &flag()).is_err());
        }
        #[test]
        fn catalog_limits_and_cancellation_are_explicit() {
            let f = Fixture::new();
            let ctx = f.context();
            for name in ["a", "b", "c"] {
                ctx.trash(&f.file(name), &flag()).unwrap();
            }
            let entries = catalog_limited(&f.root, &flag(), 2, 1 << 20).unwrap();
            assert_eq!(entries.items.len(), 2);
            assert!(entries.limited);
            let entries = catalog_limited(&f.root, &flag(), 10, 1).unwrap();
            assert!(entries.items.is_empty() && entries.limited);
            assert_eq!(
                catalog(&f.root, &AtomicBool::new(true)).unwrap_err().kind(),
                io::ErrorKind::Interrupted
            );
        }
        #[test]
        fn catalog_info_rejects_relative_duplicate_and_malformed_paths() {
            for path in [
                "relative",
                "/a%",
                "/a%00b",
                "/a%GG",
                "/a/../b",
                "/a\nPath=/b",
            ] {
                let data = format!("[Trash Info]\nPath={path}\n");
                assert!(parse_catalog_info(data.as_bytes()).is_err(), "{path}");
            }
        }
        #[test]
        fn file_roundtrip_metadata_lossless_and_no_clobber() {
            let f = Fixture::new();
            let name = OsString::from_vec(b"raw\xff%\n".to_vec());
            let path = f.source.join(name);
            fs::write(&path, b"bytes").unwrap();
            let ctx = f.context();
            let receipt = ctx.trash(&path, &flag()).unwrap();
            assert!(!path.exists());
            assert_eq!(fs::read(receipt.trashed_path().unwrap()).unwrap(), b"bytes");
            let info = fs::read_to_string(receipt.info_path().unwrap()).unwrap();
            assert!(info.starts_with("[Trash Info]\nPath=/"));
            assert!(info.contains("raw%FF%25%0A\nDeletionDate="));
            assert_eq!(
                fs::metadata(receipt.info_path().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            fs::write(&path, b"other").unwrap();
            let error = restore(&receipt, &flag()).unwrap_err();
            assert_eq!(error.cause.kind(), io::ErrorKind::AlreadyExists);
            assert!(error.restore_retryable);
            assert!(!error.preserved_paths.is_empty());
            assert_eq!(fs::read(&path).unwrap(), b"other");
            fs::remove_file(&path).unwrap();
            let info_path = receipt.info_path().unwrap();
            let report = restore(&receipt, &flag()).unwrap();
            assert_eq!(report.restored_path.as_deref(), Some(path.as_path()));
            assert_eq!(fs::read(&path).unwrap(), b"bytes");
            assert!(!info_path.exists());
        }
        #[test]
        fn diagnostic_failure_after_confirmed_restore_is_success_not_pending_undo() {
            let f = Fixture::new();
            let source = f.file("item");
            let receipt = f.context().trash(&source, &flag()).unwrap();
            let item = receipt.trashed_path().unwrap();
            let info = receipt.info_path().unwrap();
            let report = restore_with_resolver(&receipt, &flag(), |receipt| {
                // The injected failure occurs only AFTER the exclusive move
                // and identity confirmation; metadata cleanup must still run.
                assert_eq!(
                    identity(&receipt.parent, &receipt.name).unwrap(),
                    receipt.identity
                );
                assert_eq!(fs::read(&source).unwrap(), b"original");
                assert!(!item.exists());
                assert!(info.exists());
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Injected current-path resolver failure",
                ))
            })
            .unwrap();
            assert!(report.restored_path.is_none());
            assert_eq!(report.unresolved_paths, 1);
            assert!(
                report
                    .cleanup_errors
                    .iter()
                    .any(|message| message.contains("Objeto restaurado e identidad confirmada"))
            );
            assert!(report.cleanup_paths.is_empty());
            assert_eq!(fs::read(&source).unwrap(), b"original");
            assert!(!item.exists());
            assert!(!info.exists());
            // Ok(report), not Err, instructs the owning history to drop this
            // receipt. The object is no longer pending in the trash directory.
            assert!(receipt.trashed_path().is_err());
        }
        #[test]
        fn post_restore_replacement_is_not_moved_back_to_trash() {
            let f = Fixture::new();
            let source = f.file("item");
            let receipt = f.context().trash(&source, &flag()).unwrap();
            let trash_slot = receipt.trashed_path().unwrap();
            let info = receipt.info_path().unwrap();
            let rescued = f.base.join("externally-relocated-original");
            let failure = restore_with_hooks(
                &receipt,
                &flag(),
                || {
                    assert_eq!(fs::read(&source).unwrap(), b"original");
                    fs::rename(&source, &rescued).unwrap();
                    fs::write(&source, b"alien-after-restore").unwrap();
                },
                |receipt| verified_child_path(&receipt.parent, &receipt.name, &receipt.identity),
            )
            .unwrap_err();
            assert!(!failure.restore_retryable);
            assert_eq!(fs::read(&source).unwrap(), b"alien-after-restore");
            assert_eq!(fs::read(&rescued).unwrap(), b"original");
            assert!(!trash_slot.exists());
            assert!(info.exists());
            assert!(failure.preserved_paths.contains(&rescued));
            assert!(failure.preserved_paths.contains(&source));
            assert!(failure.preserved_paths.contains(&info));
        }
        #[test]
        fn cancelled_restore_is_retryable_only_with_both_expected_slots() {
            let f = Fixture::new();
            let source = f.file("item");
            let receipt = f.context().trash(&source, &flag()).unwrap();
            let item = receipt.trashed_path().unwrap();
            let info = receipt.info_path().unwrap();
            let failure = restore(&receipt, &AtomicBool::new(true)).unwrap_err();
            assert!(failure.restore_retryable);
            assert!(!source.exists());
            assert_eq!(fs::read(&item).unwrap(), b"original");
            assert!(info.exists());
            let relocated_info = f.base.join("moved-metadata");
            fs::rename(&info, &relocated_info).unwrap();
            fs::write(&info, b"foreign-metadata").unwrap();
            let failure = restore(&receipt, &AtomicBool::new(true)).unwrap_err();
            assert!(!failure.restore_retryable);
            assert!(failure.preserved_paths.contains(&relocated_info));
            assert!(failure.preserved_paths.contains(&info));
            assert_eq!(fs::read(&info).unwrap(), b"foreign-metadata");
        }
        #[test]
        fn symlink_and_nonempty_directory_move_without_following() {
            let f = Fixture::new();
            let target = f.file("target");
            let link = f.source.join("link");
            symlink(&target, &link).unwrap();
            let ctx = f.context();
            let receipt = ctx.trash(&link, &flag()).unwrap();
            assert_eq!(
                fs::read_link(receipt.trashed_path().unwrap()).unwrap(),
                target
            );
            assert_eq!(fs::read(&target).unwrap(), b"original");
            restore(&receipt, &flag()).unwrap();
            assert_eq!(fs::read_link(&link).unwrap(), target);
            let directory = f.source.join("folder");
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("child"), b"child").unwrap();
            let receipt = ctx.trash(&directory, &flag()).unwrap();
            assert_eq!(
                fs::read(receipt.trashed_path().unwrap().join("child")).unwrap(),
                b"child"
            );
            restore(&receipt, &flag()).unwrap();
            assert_eq!(fs::read(directory.join("child")).unwrap(), b"child");
        }
        #[test]
        fn cancel_before_commit_cleans_own_info_and_preserves_source() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let cancel = flag();
            assert!(
                trash_inner(&source, Arc::clone(&ctx.inner), &cancel, || cancel
                    .store(true, Ordering::Release))
                .is_err()
            );
            assert_eq!(fs::read(&source).unwrap(), b"original");
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 0);
            assert_eq!(fs::read_dir(f.root.join("files")).unwrap().count(), 0);
            assert!(ctx.trash(&source, &cancel).is_err());
        }
        #[test]
        fn source_replacement_before_commit_is_not_trashed() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let saved = f.source.join("saved");
            let result = trash_inner(&source, Arc::clone(&ctx.inner), &flag(), || {
                fs::rename(&source, &saved).unwrap();
                fs::write(&source, b"replacement").unwrap();
            });
            assert!(result.is_err());
            assert_eq!(fs::read(&source).unwrap(), b"replacement");
            assert_eq!(fs::read(saved).unwrap(), b"original");
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 0);
        }
        #[test]
        fn receipt_pins_original_parent_after_rename_and_replacement() {
            let f = Fixture::new();
            let source = f.file("item");
            let receipt = f.context().trash(&source, &flag()).unwrap();
            let moved = f.base.join("moved");
            fs::rename(&f.source, &moved).unwrap();
            fs::create_dir(&f.source).unwrap();
            fs::write(f.source.join("item"), b"replacement").unwrap();
            let report = restore(&receipt, &flag()).unwrap();
            assert_eq!(report.restored_path, Some(moved.join("item")));
            assert_eq!(fs::read(moved.join("item")).unwrap(), b"original");
            assert_eq!(fs::read(f.source.join("item")).unwrap(), b"replacement");
        }
        #[test]
        fn relocated_trash_paths_are_current_and_restore_uses_pinned_context() {
            let f = Fixture::new();
            let source = f.file("item");
            let receipt = f.context().trash(&source, &flag()).unwrap();
            let moved = f.base.join("relocated-trash");
            fs::rename(&f.root, &moved).unwrap();
            let _replacement = f.context();
            assert!(receipt.trashed_path().unwrap().starts_with(&moved));
            restore(&receipt, &flag()).unwrap();
            assert_eq!(fs::read(source).unwrap(), b"original");
        }
        #[test]
        fn replaced_object_or_info_is_never_deleted_by_restore() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let receipt = ctx.trash(&source, &flag()).unwrap();
            let item = receipt.trashed_path().unwrap();
            let old = f.base.join("saved-object");
            fs::rename(&item, &old).unwrap();
            fs::write(&item, b"alien").unwrap();
            let failure = restore(&receipt, &flag()).unwrap_err();
            assert_eq!(fs::read(&item).unwrap(), b"alien");
            assert!(failure.preserved_paths.contains(&old));
            assert!(!failure.restore_retryable);
            assert!(failure.preserved_paths.contains(&item));
            fs::remove_file(&item).unwrap();
            fs::rename(&old, &item).unwrap();
            let info = receipt.info_path().unwrap();
            let saved = f.base.join("saved-info");
            fs::rename(&info, &saved).unwrap();
            fs::write(&info, b"alien-info").unwrap();
            assert!(restore(&receipt, &flag()).is_err());
            assert_eq!(fs::read(&info).unwrap(), b"alien-info");
            assert!(receipt.trashed_path().is_ok());
        }
        #[test]
        fn two_same_names_are_distinct_and_shared_root_is_private() {
            let f = Fixture::new();
            let ctx = f.context();
            let source = f.file("item");
            let first = ctx.trash(&source, &flag()).unwrap();
            fs::write(&source, b"second").unwrap();
            let second = ctx.trash(&source, &flag()).unwrap();
            assert_ne!(
                first.trashed_path().unwrap(),
                second.trashed_path().unwrap()
            );
            assert_eq!(
                fs::read(first.trashed_path().unwrap()).unwrap(),
                b"original"
            );
            assert_eq!(fs::read(second.trashed_path().unwrap()).unwrap(), b"second");
            for path in [&f.root, &f.root.join("files"), &f.root.join("info")] {
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o7777,
                    0o700
                );
            }
        }
        #[test]
        fn occupied_files_destination_retries_without_overwrite() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let mut blocked = None;
            let receipt = trash_inner(&source, Arc::clone(&ctx.inner), &flag(), || {
                let info = fs::read_dir(f.root.join("info"))
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .file_name();
                let raw = info.as_bytes();
                let name = OsStr::from_bytes(&raw[..raw.len() - b".trashinfo".len()]);
                let destination = f.root.join("files").join(name);
                fs::write(&destination, b"unrelated").unwrap();
                blocked = Some(destination);
            })
            .unwrap();
            let blocked = blocked.unwrap();
            assert_eq!(fs::read(&blocked).unwrap(), b"unrelated");
            assert_ne!(receipt.trashed_path().unwrap(), blocked);
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 1);
            restore(&receipt, &flag()).unwrap();
            assert_eq!(fs::read(source).unwrap(), b"original");
        }
        #[test]
        fn rejects_trash_ancestry_symlink_root_and_insecure_existing_modes() {
            let f = Fixture::new();
            let ctx = f.context();
            for path in [&f.root, &f.base, &f.root.join("files")] {
                assert!(ctx.trash(path, &flag()).is_err());
            }
            let source = f.file("item");
            let receipt = ctx.trash(&source, &flag()).unwrap();
            assert!(
                ctx.trash(&receipt.trashed_path().unwrap(), &flag())
                    .is_err()
            );
            let alias = f.base.join("alias");
            symlink(&f.root, &alias).unwrap();
            assert!(TrashContext::open(&alias).is_err());
            fs::set_permissions(f.root.join("info"), fs::Permissions::from_mode(0o755)).unwrap();
            assert!(TrashContext::open(&f.root).is_err());
        }
        #[test]
        fn managed_directory_relocation_after_move_rolls_back_without_touching_replacement() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let detached = f.base.join("detached-files");
            let failure = trash_hooks(
                &source,
                Arc::clone(&ctx.inner),
                &flag(),
                || {},
                || {
                    fs::rename(f.root.join("files"), &detached).unwrap();
                    fs::create_dir(f.root.join("files")).unwrap();
                    fs::set_permissions(f.root.join("files"), fs::Permissions::from_mode(0o700))
                        .unwrap();
                },
            )
            .unwrap_err();
            assert_eq!(fs::read(&source).unwrap(), b"original");
            assert!(failure.preserved_paths.contains(&source));
            assert_eq!(fs::read_dir(&detached).unwrap().count(), 0);
            assert_eq!(fs::read_dir(f.root.join("files")).unwrap().count(), 0);
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 0);
        }
        #[test]
        fn partial_rollback_reports_actual_expected_object_and_preserves_new_occupants() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let rescued = f.base.join("externally-relocated-object");
            let failure = trash_hooks(
                &source,
                Arc::clone(&ctx.inner),
                &flag(),
                || {},
                || {
                    let target = fs::read_dir(f.root.join("files"))
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    fs::rename(&target, &rescued).unwrap();
                    fs::write(&target, b"alien-destination").unwrap();
                    fs::write(&source, b"new-original-occupant").unwrap();
                },
            )
            .unwrap_err();
            assert_eq!(fs::read(&source).unwrap(), b"new-original-occupant");
            assert_eq!(fs::read(&rescued).unwrap(), b"original");
            assert!(failure.preserved_paths.contains(&rescued));
            assert!(!failure.rollback_errors.is_empty());
            let target = fs::read_dir(f.root.join("files"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert_eq!(fs::read(&target).unwrap(), b"alien-destination");
            assert!(failure.preserved_paths.contains(&target));
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 1);
        }
        #[test]
        fn trash_post_move_replacement_is_not_rolled_back_into_empty_original() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let rescued = f.base.join("externally-relocated-original");
            let failure = trash_hooks(
                &source,
                Arc::clone(&ctx.inner),
                &flag(),
                || {},
                || {
                    let target = fs::read_dir(f.root.join("files"))
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    fs::rename(&target, &rescued).unwrap();
                    fs::write(&target, b"foreign replacement stays in trash slot").unwrap();
                    assert!(!source.exists());
                },
            )
            .unwrap_err();
            assert!(
                !source.exists(),
                "a foreign object must never be relocated as rollback"
            );
            assert_eq!(fs::read(&rescued).unwrap(), b"original");
            assert!(failure.preserved_paths.contains(&rescued));
            let target = fs::read_dir(f.root.join("files"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert_eq!(
                fs::read(&target).unwrap(),
                b"foreign replacement stays in trash slot"
            );
            assert!(failure.preserved_paths.contains(&target));
            assert!(!failure.rollback_errors.is_empty());
            assert_eq!(fs::read_dir(f.root.join("info")).unwrap().count(), 1);
            assert_eq!(failure.cleanup_paths.len(), 1);
        }
        #[test]
        fn failed_cleanup_does_not_unlink_replaced_metadata() {
            let f = Fixture::new();
            let source = f.file("item");
            let ctx = f.context();
            let result = trash_inner(&source, Arc::clone(&ctx.inner), &flag(), || {
                let entry = fs::read_dir(f.root.join("info"))
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::rename(&entry, f.base.join("relocated-info")).unwrap();
                fs::write(&entry, b"replacement-info").unwrap();
            })
            .unwrap_err();
            assert!(!result.rollback_errors.is_empty());
            assert_eq!(fs::read(&source).unwrap(), b"original");
            let entry = fs::read_dir(f.root.join("info"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert_eq!(fs::read(entry).unwrap(), b"replacement-info");
        }
    }
}
#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Papelera nativa pendiente en esta plataforma",
        )
    }
    pub(super) fn open_context(_: &Path) -> io::Result<Context> {
        Err(unsupported())
    }
    pub(super) fn catalog(_: &Path, _: &AtomicBool) -> io::Result<Catalog> {
        Err(unsupported())
    }
    pub(super) fn restore_item(
        _: &Path,
        _: &std::ffi::OsStr,
        _: &AtomicBool,
    ) -> io::Result<(PathBuf, Option<String>)> {
        Err(unsupported())
    }
    pub(super) fn purge_item(
        _: &Path,
        _: &std::ffi::OsStr,
        _: &AtomicBool,
        _: &crate::remove_tree::Progress,
    ) -> io::Result<crate::remove_tree::Report> {
        Err(unsupported())
    }
    pub(super) fn empty(
        _: &Path,
        _: &AtomicBool,
        _: &crate::remove_tree::Progress,
    ) -> io::Result<crate::remove_tree::Report> {
        Err(unsupported())
    }
    pub(super) fn trash(_: &Path, _: Arc<Context>, _: &AtomicBool) -> Result<Receipt, Failure> {
        Err(unsupported().into())
    }
    pub(super) fn restore(_: &Receipt, _: &AtomicBool) -> Result<RestoreReport, Failure> {
        Err(unsupported().into())
    }
    pub(super) fn child_path(_: &File, _: &std::ffi::OsStr) -> io::Result<PathBuf> {
        Err(unsupported())
    }
    pub(super) fn verified_child_path(
        _: &File,
        _: &std::ffi::OsStr,
        _: &Identity,
    ) -> io::Result<PathBuf> {
        Err(unsupported())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percent_encoding_and_metadata_limits_preserve_bytes() {
        assert_eq!(
            metadata(Path::new("/a b/%#=é"), "2026-10-26T12:34:56").unwrap(),
            b"[Trash Info]\nPath=/a%20b/%25%23%3D%C3%A9\nDeletionDate=2026-10-26T12:34:56\n"
        );
        assert!(metadata(Path::new("/a"), "bad").is_err());
        for path in ["relative", "/", "/a/../b", "/a/./b"] {
            assert!(validate_path(Path::new(path)).is_err());
        }
        assert!(validate_path(Path::new(&format!("/{}", "x".repeat(4096)))).is_err());
    }
}
