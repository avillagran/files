//! Linux mountpoint provenance. No mounting, unmounting, recursive deletion,
//! chmod of existing paths or cleanup on Drop. A fresh mount-table observation
//! is supplied by the worker; this module does not turn it into atomic authority.
#![cfg(target_os = "linux")]
use crate::mount_table::Mount;
use std::{
    ffi::CString,
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
}
fn stat(file: &File) -> io::Result<libc::stat> {
    let mut s = std::mem::MaybeUninit::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), s.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { s.assume_init() })
}
fn identity(file: &File) -> io::Result<Identity> {
    let s = stat(file)?;
    Ok(Identity {
        device: s.st_dev,
        inode: s.st_ino,
    })
}
fn private_directory(file: &File) -> io::Result<()> {
    let s = stat(file)?;
    if s.st_mode & libc::S_IFMT != libc::S_IFDIR
        || s.st_uid != unsafe { libc::geteuid() }
        || s.st_mode & 0o7777 != 0o700
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Se requiere directorio propio con permisos 0700",
        ));
    }
    Ok(())
}
fn open_at(parent: &File, name: &CString) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
/// Walks every component without following symlinks. Parent renames can still
/// race a later path-based operation; callers must serialize and revalidate.
fn open_absolute(path: &Path) -> io::Result<File> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.as_os_str().as_encoded_bytes().contains(&0)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ruta absoluta fuera de límite",
        ));
    }
    let mut current = File::open("/")?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                current = open_at(&current, &CString::new(name.as_encoded_bytes())?)?
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Ruta con componente ambiguo",
                ));
            }
        }
    }
    Ok(current)
}
pub struct Parent {
    directory: File,
    path: PathBuf,
    identity: Identity,
}
pub struct Point {
    parent: Parent,
    directory: File,
    name: CString,
    path: PathBuf,
    identity: Identity,
}
impl Parent {
    pub(crate) fn private_child_identity(&self, name: &std::ffi::OsStr) -> io::Result<(u64, u64)> {
        let file = open_at(&self.directory, &CString::new(name.as_encoded_bytes())?)?;
        private_directory(&file)?;
        let identity = identity(&file)?;
        Ok((identity.device, identity.inode))
    }
    pub(crate) fn directory(&self) -> &File {
        &self.directory
    }
    pub(crate) fn identity_pair(&self) -> (u64, u64) {
        (self.identity.device, self.identity.inode)
    }
    /// Existing private parent only. Its creation/persistence is an explicit
    /// worker responsibility, never adoption of a user-chosen occupied mountpoint.
    pub fn open(path: &Path) -> io::Result<Self> {
        let directory = open_absolute(path)?;
        private_directory(&directory)?;
        let identity = identity(&directory)?;
        Ok(Self {
            directory,
            path: path.to_owned(),
            identity,
        })
    }
    pub(crate) fn verify(&self) -> io::Result<()> {
        private_directory(&self.directory)?;
        let current = open_absolute(&self.path)?;
        private_directory(&current)?;
        if identity(&current)? != self.identity {
            return Err(io::Error::other("Directorio padre sustituido"));
        }
        Ok(())
    }
    pub fn create(&self) -> io::Result<Point> {
        self.verify()?;
        for _ in 0..8 {
            let mut random = [0u8; 16];
            let mut used = 0;
            while used < random.len() {
                let n = unsafe {
                    libc::getrandom(random[used..].as_mut_ptr().cast(), random.len() - used, 0)
                };
                if n < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                }
                if n == 0 {
                    return Err(io::Error::other("getrandom sin progreso"));
                }
                used += n as usize;
            }
            use std::fmt::Write;
            let mut label = String::from("files-sftp-");
            for byte in random {
                write!(label, "{byte:02x}").expect("String formatting");
            }
            if self.path.as_os_str().len() + 1 + label.len() > 4096 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Punto de montaje fuera del límite",
                ));
            }
            let name = CString::new(label.as_bytes())?;
            if unsafe { libc::mkdirat(self.directory.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::AlreadyExists {
                    continue;
                }
                return Err(e);
            }
            let path = self.path.join(label);
            // After mkdir acknowledgement, failures retain the created path.
            // Never unlink a name whose first acquired identity is unknown.
            let finish = || -> io::Result<Point> {
                let directory = open_at(&self.directory, &name)?;
                private_directory(&directory)?;
                let point_identity = identity(&directory)?;
                self.verify()?;
                Ok(Point {
                    parent: Parent {
                        directory: self.directory.try_clone()?,
                        path: self.path.clone(),
                        identity: self.identity,
                    },
                    directory,
                    name: name.clone(),
                    path: path.clone(),
                    identity: point_identity,
                })
            };
            return finish().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("Punto creado y conservado en {path:?}; no se borró: {e}"),
                )
            });
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "No se obtuvo nombre exclusivo de montaje",
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub parent_device: u64,
    pub parent_inode: u64,
    pub point_device: u64,
    pub point_inode: u64,
}
impl Point {
    pub fn provenance(&self) -> Provenance {
        Provenance {
            parent_device: self.parent.identity.device,
            parent_inode: self.parent.identity.inode,
            point_device: self.identity.device,
            point_inode: self.identity.inode,
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Before spawn, or after confirmed absence. Does not touch a mounted FUSE
    /// root; reject mount-table coverage before any child path/open operation.
    pub fn verify_unmounted_empty(&mut self, mounts: &[Mount]) -> io::Result<()> {
        if mounts.iter().any(|m| m.point.starts_with(&self.path)) {
            return Err(io::Error::other(
                "Montaje presente en el punto o debajo; no inspeccionar contenido",
            ));
        }
        self.parent.verify()?;
        let named = open_at(&self.parent.directory, &self.name)?;
        private_directory(&named)?;
        private_directory(&self.directory)?;
        if identity(&named)? != self.identity || identity(&self.directory)? != self.identity {
            return Err(io::Error::other("Punto de montaje sustituido"));
        }
        if unsafe { libc::lseek(self.directory.as_raw_fd(), 0, libc::SEEK_SET) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = [0u8; 4096];
        for _ in 0..4 {
            let n = unsafe {
                libc::syscall(
                    libc::SYS_getdents64,
                    self.directory.as_raw_fd(),
                    buffer.as_mut_ptr(),
                    buffer.len(),
                )
            };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            if n == 0 {
                return Ok(());
            }
            if n as usize > buffer.len() {
                return Err(io::Error::other("Lectura de directorio inválida"));
            }
            let mut offset = 0;
            while offset < n as usize {
                let entry = &buffer[offset..n as usize];
                if entry.len() < 20 {
                    return Err(io::Error::other("Entrada truncada"));
                }
                let length = u16::from_ne_bytes([entry[16], entry[17]]) as usize;
                if length < 20 || length > entry.len() {
                    return Err(io::Error::other("Entrada inválida"));
                }
                let end = entry[19..length]
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or_else(|| io::Error::other("Nombre truncado"))?;
                if !matches!(&entry[19..19 + end], b"." | b"..") {
                    return Err(io::Error::other("Punto no vacío; contenido conservado"));
                }
                offset += length;
            }
        }
        Err(io::Error::other(
            "No se pudo confirmar directorio vacío dentro del límite",
        ))
    }
    /// Validates only local parent mapping while FUSE may be offline. The
    /// worker separately requires exact fresh mount identity before unmount.
    pub fn verify_parent(&self) -> io::Result<()> {
        self.parent.verify()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/files-test");
            fs::create_dir_all(&base).unwrap();
            loop {
                let path = base.join(format!(
                    "sftp-point-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => panic!("{e}"),
                }
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn table() -> Vec<Mount> {
        crate::mount_table::parse(&fs::read("/proc/self/mountinfo").unwrap()).unwrap()
    }
    #[test]
    fn exclusive_points_private_empty_and_drop_retains_them() {
        let f = Fixture::new();
        let parent = Parent::open(&f.0).unwrap();
        let mut a = parent.create().unwrap();
        let mut b = parent.create().unwrap();
        assert_ne!(a.path(), b.path());
        a.verify_unmounted_empty(&table()).unwrap();
        b.verify_unmounted_empty(&table()).unwrap();
        let path = a.path().to_owned();
        drop(a);
        assert!(path.is_dir());
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o7777,
            0o700
        );
    }
    #[test]
    fn parent_permissions_and_symlink_components_are_rejected_without_changes() {
        let f = Fixture::new();
        let parent = f.0.join("private");
        fs::DirBuilder::new().mode(0o700).create(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Parent::open(&parent).is_err());
        assert_eq!(
            fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        std::os::unix::fs::symlink(&parent, f.0.join("alias")).unwrap();
        assert!(Parent::open(&f.0.join("alias")).is_err());
        fs::create_dir(parent.join("child")).unwrap();
        assert!(Parent::open(&f.0.join("alias/child")).is_err());
    }
    #[test]
    fn replacement_of_point_preserves_both_original_and_actor() {
        let f = Fixture::new();
        let parent = Parent::open(&f.0).unwrap();
        let mut point = parent.create().unwrap();
        let moved = f.0.join("retained");
        fs::rename(point.path(), &moved).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(point.path())
            .unwrap();
        fs::write(point.path().join("actor"), b"keep").unwrap();
        assert!(point.verify_unmounted_empty(&table()).is_err());
        assert_eq!(fs::read(point.path().join("actor")).unwrap(), b"keep");
        assert!(moved.is_dir());
    }
    #[test]
    fn renamed_parent_blocks_spawn_and_preserves_replacement_parent() {
        let f = Fixture::new();
        let path = f.0.join("parent");
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let parent = Parent::open(&path).unwrap();
        let mut point = parent.create().unwrap();
        fs::rename(&path, f.0.join("old-parent")).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        assert!(point.verify_parent().is_err());
        assert!(point.verify_unmounted_empty(&table()).is_err());
        assert!(parent.create().is_err());
        assert!(path.is_dir());
    }
    #[test]
    fn nonempty_and_synthetic_mount_coverage_prevent_inspection_or_cleanup() {
        let f = Fixture::new();
        let mut point = Parent::open(&f.0).unwrap().create().unwrap();
        fs::write(point.path().join("keep"), b"actor").unwrap();
        assert!(point.verify_unmounted_empty(&table()).is_err());
        let mounted = Mount {
            id: 9,
            parent: 1,
            device: (0, 2),
            root: "/".into(),
            point: point.path().join("nested"),
            filesystem: b"fuse.sshfs".to_vec(),
            source: "fixture".into(),
        };
        assert!(
            point
                .verify_unmounted_empty(&[mounted])
                .unwrap_err()
                .to_string()
                .contains("no inspeccionar")
        );
        assert_eq!(fs::read(point.path().join("keep")).unwrap(), b"actor");
    }
}
