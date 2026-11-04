//! Descriptor-bound local document resources. Linux openat walks normalized
//! components without symlinks; mountpoints/hardlinks are not confined.
use crate::media_source::Revision;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};
pub const MAX_BYTES: usize = 8 * 1024 * 1024 + 1;
const MAX_PATH_BYTES: usize = 4096;
const MAX_COMPONENTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DirectoryIdentity {
    pub device: u64,
    pub inode: u64,
}
impl DirectoryIdentity {
    pub fn from_file(file: &File) -> io::Result<Self> {
        Self::from_metadata(&file.metadata()?)
    }
    #[cfg(unix)]
    pub fn from_metadata(m: &std::fs::Metadata) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        if !m.is_dir() {
            return Err(invalid("La raíz no es un directorio"));
        }
        Ok(Self {
            device: m.dev(),
            inode: m.ino(),
        })
    }
    #[cfg(not(unix))]
    pub fn from_metadata(_: &std::fs::Metadata) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de directorio pendiente",
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proof {
    pub root: DirectoryIdentity,
    pub file: Revision,
}
/// Keeps both descriptors alive through read/decode and verifies current names.
/// The root's parents may be symlinks; only its final component is NOFOLLOW.
pub struct Opened {
    root: File,
    root_path: PathBuf,
    relative: PathBuf,
    pub file: File,
    pub proof: Proof,
}
impl Opened {
    #[cfg(target_os = "linux")]
    pub fn open(root_path: &Path, relative: &Path) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let bytes = root_path.as_os_str().as_encoded_bytes();
        if !root_path.is_absolute() || bytes.len() > 4096 || bytes.contains(&0) {
            return Err(invalid("Raíz inválida"));
        }
        let root = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root_path)?;
        let identity = DirectoryIdentity::from_file(&root)?;
        let file = open_file(&root, relative)?;
        let proof = Proof {
            root: identity,
            file: Revision::from_file(&file)?,
        };
        let opened = Self {
            root,
            root_path: root_path.into(),
            relative: relative.into(),
            file,
            proof,
        };
        opened.revalidate()?;
        Ok(opened)
    }
    #[cfg(not(target_os = "linux"))]
    pub fn open(_: &Path, _: &Path) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Recursos relativos pendientes",
        ))
    }
    pub fn expect(&self, expected: Proof) -> io::Result<()> {
        if self.proof != expected {
            return Err(io::Error::other(
                "El recurso o su raíz fueron reemplazados antes de leer",
            ));
        }
        Ok(())
    }
    pub fn revalidate(&self) -> io::Result<()> {
        let current_root =
            DirectoryIdentity::from_metadata(&std::fs::symlink_metadata(&self.root_path)?)?;
        if current_root != self.proof.root
            || DirectoryIdentity::from_file(&self.root)? != self.proof.root
            || Revision::from_file(&self.file)? != self.proof.file
            || Revision::from_file(&open_file(&self.root, &self.relative)?)? != self.proof.file
        {
            return Err(io::Error::other(
                "El recurso o su raíz cambiaron durante la lectura",
            ));
        }
        Ok(())
    }
}
#[cfg(target_os = "linux")]
pub fn open_file(root: &File, relative: &Path) -> io::Result<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    if relative.as_os_str().as_bytes().len() > MAX_PATH_BYTES {
        return Err(invalid("Recurso fuera del presupuesto"));
    }
    if !root.metadata()?.is_dir() {
        return Err(invalid("La raíz del recurso no es un directorio"));
    }
    // Primera pasada sin I/O ni copia de componentes. Rechazar '..' incluso si
    // otro nivel ya aplicó normalización; no aceptar una raíz o prefijo Windows.
    let mut count = 0;
    for part in relative.components() {
        match part {
            Component::Normal(name) if !name.as_bytes().contains(&0) => {}
            _ => return Err(invalid("El recurso requiere una ruta relativa normalizada")),
        }
        count += 1;
        if count > MAX_COMPONENTS {
            return Err(invalid("Demasiados componentes en el recurso"));
        }
    }
    if count == 0 {
        return Err(invalid("Ruta de recurso vacía"));
    }
    let mut parent: Option<File> = None;
    for (index, part) in relative.components().enumerate() {
        let Component::Normal(name) = part else {
            unreachable!()
        };
        let name = CString::new(name.as_bytes()).map_err(|_| invalid("Ruta con NUL"))?;
        let last = index + 1 == count;
        let flags = libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if last {
                libc::O_RDONLY | libc::O_NONBLOCK
            } else {
                libc::O_PATH | libc::O_DIRECTORY
            };
        let fd = unsafe {
            libc::openat(
                parent.as_ref().unwrap_or(root).as_raw_fd(),
                name.as_ptr(),
                flags,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // openat devuelve un descriptor nuevo y de propiedad exclusiva.
        let file = unsafe { File::from_raw_fd(fd) };
        if last {
            Revision::from_file(&file)?;
            return Ok(file);
        }
        parent = Some(file);
    }
    Err(invalid("Recurso sin archivo final"))
}

#[cfg(not(target_os = "linux"))]
pub fn open_file(_: &File, _: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Recursos relativos pendientes en esta plataforma",
    ))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub fn read_prefix(file: &mut File, limit: usize) -> io::Result<Vec<u8>> {
    if limit > MAX_BYTES {
        return Err(invalid("Recurso fuera del presupuesto"));
    }
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("El recurso debe ser un archivo regular"));
    }
    let size = metadata.len().min(limit as u64) as usize;
    let mut data = Vec::new();
    data.try_reserve_exact(size)
        .map_err(|_| io::Error::other("Sin memoria para el recurso"))?;
    data.resize(size, 0);
    file.read_exact(&mut data)?;
    let mut extra = [0];
    if size < limit && file.read(&mut extra)? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "El recurso creció durante la lectura",
        ));
    }
    Ok(data)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = crate::theme::home().join(".cache/files-test").join(format!(
                "relative-source-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("root/sub")).unwrap();
            std::fs::write(root.join("root/sub/image"), b"red").unwrap();
            Self(root)
        }
        fn open(&self) -> Opened {
            Opened::open(&self.0.join("root"), Path::new("sub/image")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn verified_a_replaced_before_decoder_open_never_authorizes_b() {
        let f = Fixture::new();
        let a = f.open().proof;
        std::fs::write(f.0.join("next"), b"blu").unwrap();
        std::fs::rename(f.0.join("next"), f.0.join("root/sub/image")).unwrap();
        let b = f.open();
        assert_ne!(a.file, b.proof.file);
        assert!(b.expect(a).is_err());
    }
    #[test]
    fn replacing_root_after_document_read_rejects_identical_names() {
        let f = Fixture::new();
        let a = f.open();
        std::fs::rename(f.0.join("root"), f.0.join("old")).unwrap();
        std::fs::create_dir_all(f.0.join("root/sub")).unwrap();
        std::fs::write(f.0.join("root/sub/image"), b"red").unwrap();
        assert!(a.revalidate().is_err());
        assert!(f.open().expect(a.proof).is_err());
    }
    #[test]
    fn changing_intermediate_name_or_file_during_read_is_rejected() {
        let f = Fixture::new();
        let mut a = f.open();
        assert_eq!(read_prefix(&mut a.file, 3).unwrap(), b"red");
        std::fs::rename(f.0.join("root/sub"), f.0.join("root/previous")).unwrap();
        std::fs::create_dir(f.0.join("root/sub")).unwrap();
        std::fs::write(f.0.join("root/sub/image"), b"blu").unwrap();
        assert!(a.revalidate().is_err());
        let b = f.open();
        std::fs::write(f.0.join("root/sub/image"), b"new bytes").unwrap();
        assert!(b.revalidate().is_err());
    }
    #[test]
    fn normalized_components_bounds_and_symlinks_fail_closed() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let root = File::open(f.0.join("root")).unwrap();
        symlink("sub/image", f.0.join("root/link")).unwrap();
        symlink("sub", f.0.join("root/alias")).unwrap();
        for p in ["", "../root/sub/image", "/absolute", "link", "alias/image"] {
            assert!(open_file(&root, Path::new(p)).is_err(), "{p}");
        }
        assert!(open_file(&root, Path::new(&"a/".repeat(65))).is_err());
        assert!(open_file(&root, Path::new(&"x".repeat(4097))).is_err());
        assert!(read_prefix(&mut f.open().file, MAX_BYTES + 1).is_err());
    }
    #[test]
    fn opaque_relative_components_are_lossless_and_never_url_decoded() {
        use std::os::unix::ffi::OsStringExt;
        let f = Fixture::new();
        let name = PathBuf::from(std::ffi::OsString::from_vec(b"%2e%2e-\xff".to_vec()));
        std::fs::write(f.0.join("root").join(&name), b"raw").unwrap();
        let mut a = Opened::open(&f.0.join("root"), &name).unwrap();
        assert_eq!(read_prefix(&mut a.file, 4).unwrap(), b"raw");
        a.revalidate().unwrap();
    }
}
