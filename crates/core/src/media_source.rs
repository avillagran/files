//! Media identity/revision from the descriptor actually read by a decoder.
//! This is change detection, not a content hash or an atomic filesystem snapshot.
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, Metadata},
    io,
    path::Path,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Revision {
    pub device: u64,
    pub inode: u64,
    pub bytes: u64,
    pub modified_seconds: i64,
    pub modified_nanoseconds: i64,
    pub changed_seconds: i64,
    pub changed_nanoseconds: i64,
}
impl Revision {
    #[cfg(unix)]
    pub fn from_metadata(m: &Metadata) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        if !m.is_file() {
            return Err(io::Error::other(
                "El origen de medios debe ser un archivo regular",
            ));
        }
        Ok(Self {
            device: m.dev(),
            inode: m.ino(),
            bytes: m.len(),
            modified_seconds: m.mtime(),
            modified_nanoseconds: m.mtime_nsec(),
            changed_seconds: m.ctime(),
            changed_nanoseconds: m.ctime_nsec(),
        })
    }
    #[cfg(not(unix))]
    pub fn from_metadata(_: &Metadata) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Identidad de medios pendiente en esta plataforma",
        ))
    }
    pub fn from_file(file: &File) -> io::Result<Self> {
        Self::from_metadata(&file.metadata()?)
    }
    /// Check both the pinned file and the current directory entry. A different
    /// inode at the same name or a final symlink is never the original source.
    pub fn revalidate(self, file: &File, path: &Path) -> io::Result<()> {
        if Self::from_file(file)? != self
            || Self::from_metadata(&std::fs::symlink_metadata(path)?)? != self
        {
            return Err(io::Error::other(
                "El original cambió durante la vista previa",
            ));
        }
        Ok(())
    }
}
/// Lossless bounded Unix path transported as JSON bytes, decoded exactly once.
#[cfg(unix)]
pub fn raw_path(bytes: Vec<u8>) -> io::Result<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    if bytes.len() > 4096 || bytes.contains(&0) {
        return Err(io::Error::other("Ruta de medios inválida"));
    }
    let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(bytes));
    if !path.is_absolute() {
        return Err(io::Error::other("La ruta de medios debe ser absoluta"));
    }
    Ok(path)
}
#[cfg(not(unix))]
pub fn raw_path(_: Vec<u8>) -> io::Result<std::path::PathBuf> {
    Err(io::Error::other(
        "Transporte raw de medios pendiente en esta plataforma",
    ))
}
pub fn open(path: &Path) -> io::Result<(File, Revision)> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || bytes.len() > 4096 || bytes.contains(&0) {
        return Err(io::Error::other("Ruta de medios inválida"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    let revision = Revision::from_file(&file)?;
    revision.revalidate(&file, path)?;
    Ok((file, revision))
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let p = crate::theme::home().join(".cache/files-test").join(format!(
                "media-source-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn replacement_same_name_size_and_mtime_is_a_different_identity() {
        let f = Fixture::new();
        let p = f.0.join("image");
        std::fs::write(&p, b"red").unwrap();
        let (file, a) = open(&p).unwrap();
        let modified = file.metadata().unwrap().modified().unwrap();
        let next = f.0.join("next");
        std::fs::write(&next, b"blu").unwrap();
        File::options()
            .write(true)
            .open(&next)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        std::fs::rename(next, &p).unwrap();
        let (_, b) = open(&p).unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(
            (a.modified_seconds, a.modified_nanoseconds),
            (b.modified_seconds, b.modified_nanoseconds)
        );
        assert_ne!((a.device, a.inode), (b.device, b.inode));
        assert!(a.revalidate(&file, &p).is_err());
    }
    #[test]
    fn modification_deletion_rename_and_symlink_do_not_revalidate() {
        let f = Fixture::new();
        let p = f.0.join("image");
        std::fs::write(&p, b"red").unwrap();
        let (file, a) = open(&p).unwrap();
        std::fs::write(&p, b"different").unwrap();
        assert!(a.revalidate(&file, &p).is_err());
        let (file, b) = open(&p).unwrap();
        let moved = f.0.join("moved");
        std::fs::rename(&p, &moved).unwrap();
        assert!(b.revalidate(&file, &p).is_err());
        symlink(&moved, &p).unwrap();
        assert!(open(&p).is_err());
        assert!(b.revalidate(&file, &p).is_err());
        std::fs::remove_file(&moved).unwrap();
        assert!(b.revalidate(&file, &moved).is_err());
    }
    #[test]
    fn unchanged_raw_path_stays_valid_without_losing_identity() {
        let f = Fixture::new();
        let p = f.0.join(OsString::from_vec(b"image\xff".to_vec()));
        std::fs::write(&p, b"red").unwrap();
        let (file, a) = open(&p).unwrap();
        assert!(a.revalidate(&file, &p).is_ok());
        assert_eq!(
            raw_path(p.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
            p
        );
        assert!(raw_path(b"/bad\0path".to_vec()).is_err());
        assert!(raw_path(b"relative".to_vec()).is_err());
        assert!(open(&f.0).is_err());
        assert!(open(Path::new("relative")).is_err());
    }
}
