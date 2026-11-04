//! Bounded prefix under a caller-pinned directory FD. No path/root revalidation:
//! use core::relative_source::Opened when a decoder must bind current names.
use std::{fs::File, io, path::Path};
pub const MAX_BYTES: usize = files_core::relative_source::MAX_BYTES;
pub fn read(root: &File, relative: &Path, limit: usize) -> io::Result<Vec<u8>> {
    if limit > MAX_BYTES {
        return Err(io::Error::other("Recurso fuera del presupuesto"));
    }
    let mut file = files_core::relative_source::open_file(root, relative)?;
    files_core::relative_source::read_prefix(&mut file, limit)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "files-relative-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn root(&self) -> File {
            File::open(&self.0).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn nested_unicode_file_is_read_with_exact_budget() {
        let f = Fixture::new();
        std::fs::create_dir(f.0.join("imágenes")).unwrap();
        std::fs::write(f.0.join("imágenes/a b"), b"abc").unwrap();
        assert_eq!(
            read(&f.root(), Path::new("imágenes/a b"), 3).unwrap(),
            b"abc"
        );
        assert_eq!(
            read(&f.root(), Path::new("imágenes/a b"), 2).unwrap(),
            b"ab"
        );
    }
    #[test]
    fn symlinks_are_rejected_at_every_component() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let outside = Fixture::new();
        std::fs::write(outside.0.join("secret"), b"secret").unwrap();
        symlink(outside.0.join("secret"), f.0.join("link")).unwrap();
        symlink(&outside.0, f.0.join("dir")).unwrap();
        assert!(read(&f.root(), Path::new("link"), 100).is_err());
        assert!(read(&f.root(), Path::new("dir/secret"), 100).is_err());
    }
    #[test]
    fn root_descriptor_stays_pinned_after_directory_replacement() {
        let f = Fixture::new();
        let old = f.0.join("old");
        let base = f.0.join("base");
        std::fs::create_dir(&base).unwrap();
        std::fs::write(base.join("a"), b"original").unwrap();
        let root = File::open(&base).unwrap();
        std::fs::rename(&base, &old).unwrap();
        std::fs::create_dir(&base).unwrap();
        std::fs::write(base.join("a"), b"replacement").unwrap();
        assert_eq!(read(&root, Path::new("a"), 32).unwrap(), b"original");
    }
    #[test]
    fn fifo_and_directory_are_not_read_as_files() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let f = Fixture::new();
        let fifo = CString::new(f.0.join("pipe").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        std::fs::create_dir(f.0.join("dir")).unwrap();
        assert!(read(&f.root(), Path::new("pipe"), 100).is_err());
        assert!(read(&f.root(), Path::new("dir"), 100).is_err());
    }
    #[test]
    fn compressed_budget_sentinel_is_read_not_inferred_from_metadata() {
        let f = Fixture::new();
        let path = f.0.join("large");
        let mut file = File::create(&path).unwrap();
        file.set_len((MAX_BYTES - 1) as u64).unwrap();
        let exact = read(&f.root(), Path::new("large"), MAX_BYTES).unwrap();
        assert_eq!(exact.len(), MAX_BYTES - 1);
        drop(exact);
        use std::io::{Seek, SeekFrom, Write};
        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(&[0xa5]).unwrap();
        let excess = read(&f.root(), Path::new("large"), MAX_BYTES).unwrap();
        assert_eq!(excess.len(), MAX_BYTES);
        assert_eq!(excess[MAX_BYTES - 1], 0xa5);
        drop(excess);
        file.write_all(&[0xb6]).unwrap();
        assert_eq!(
            read(&f.root(), Path::new("large"), MAX_BYTES)
                .unwrap()
                .len(),
            MAX_BYTES
        );
    }
    #[test]
    fn traversal_absolute_empty_and_oversized_requests_fail() {
        let f = Fixture::new();
        for path in ["", "../secret", "a/../../secret", "/etc/passwd", "./a"] {
            assert!(read(&f.root(), Path::new(path), 100).is_err());
        }
        assert!(read(&f.root(), Path::new("a"), MAX_BYTES + 1).is_err());
        assert!(read(&f.root(), Path::new(&"a/".repeat(65)), 100).is_err());
        assert!(read(&f.root(), Path::new(&"a".repeat(4097)), 100).is_err());
    }
}
