//! SHA-256 de observaciones individuales ligadas a FD/revisión; nunca autoriza borrar.
use crate::relative_source::{DirectoryIdentity, Opened, Proof};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
pub const MAX_FILES: usize = 128;
pub const MAX_NAME_BYTES: usize = 8192;
pub const CHUNK: usize = 64 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Name {
    #[serde(with = "crate::protocol::path_wire")]
    pub path: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub proof: Proof,
    pub sha256: [u8; 32],
}
pub const MAX_PROGRESS: usize = MAX_FILES + 256;
#[derive(Debug, Serialize, Deserialize)]
pub struct Frame {
    pub id: u64,
    #[serde(flatten)]
    pub event: Event,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Progress {
        index: usize,
        bytes: u64,
        total: u64,
    },
    Item {
        index: usize,
        result: Result<Observation, String>,
    },
    Done {
        count: usize,
    },
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn cancelled(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Cálculo SHA-256 cancelado",
        ))
    } else {
        Ok(())
    }
}
fn valid_name(path: &Path) -> bool {
    let mut parts = path.components();
    matches!(parts.next(), Some(Component::Normal(name)) if name == path.as_os_str())
        && parts.next().is_none()
        && !path.as_os_str().as_encoded_bytes().contains(&0)
}
pub fn validate(root: &Path, names: &[Name]) -> io::Result<()> {
    let bytes = root.as_os_str().as_encoded_bytes();
    if !root.is_absolute() || bytes.len() > 4096 || bytes.contains(&0) {
        return Err(invalid(
            "SHA-256 requiere una carpeta absoluta de hasta4096bytes",
        ));
    }
    if names.is_empty() || names.len() > MAX_FILES {
        return Err(invalid("Elige de1 a128 archivos"));
    }
    let mut total = 0usize;
    for (i, name) in names.iter().enumerate() {
        total = total
            .checked_add(name.path.as_os_str().len())
            .ok_or_else(|| invalid("Nombres fuera de presupuesto"))?;
        if total > MAX_NAME_BYTES
            || !valid_name(&name.path)
            || names[..i].iter().any(|n| n.path == name.path)
        {
            return Err(invalid("Nombres inválidos, repetidos o superiores a8KiB"));
        }
    }
    Ok(())
}
/// El descriptor y el nombre se comprueban al abrir y tras leer exactamente el tamaño
/// observado. No representa una instantánea atómica del lote ni vigila cambios posteriores.
pub fn calculate(
    root: &Path,
    name: &Path,
    expected_root: Option<DirectoryIdentity>,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64) -> io::Result<()>,
) -> io::Result<Observation> {
    cancelled(cancel)?;
    if !valid_name(name) {
        return Err(invalid("SHA-256 requiere un nombre directo de archivo"));
    }
    let opened = Opened::open(root, name)?;
    if expected_root.is_some_and(|r| r != opened.proof.root) {
        return Err(io::Error::other("La carpeta capturada fue reemplazada"));
    }
    hash_opened(opened, cancel, progress)
}
/// Variante para candidatos enumerados: comprueba la revisión anterior antes de leer.
pub fn calculate_checked(
    root: &Path,
    relative: &Path,
    expected: Proof,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64) -> io::Result<()>,
) -> io::Result<Observation> {
    cancelled(cancel)?;
    let opened = Opened::open(root, relative)?;
    opened.expect(expected)?;
    hash_opened(opened, cancel, progress)
}
fn hash_opened(
    mut opened: Opened,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64) -> io::Result<()>,
) -> io::Result<Observation> {
    let proof = opened.proof;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; CHUNK];
    let mut read = 0u64;
    progress(0, proof.file.bytes)?;
    while read < proof.file.bytes {
        cancelled(cancel)?;
        let count = opened
            .file
            .read(&mut buffer[..(proof.file.bytes - read).min(CHUNK as u64) as usize])?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "El archivo cambió durante el cálculo",
            ));
        }
        hasher.update(&buffer[..count]);
        read += count as u64;
        progress(read, proof.file.bytes)?;
    }
    cancelled(cancel)?;
    if opened.file.read(&mut buffer[..1])? != 0 {
        return Err(io::Error::other("El archivo creció durante el cálculo"));
    }
    opened.revalidate()?;
    cancelled(cancel)?;
    Ok(Observation {
        proof,
        sha256: hasher.finalize().into(),
    })
}
pub fn hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut value = String::with_capacity(64);
    for byte in digest {
        write!(&mut value, "{byte:02x}").unwrap();
    }
    value
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, sync::atomic::AtomicU64};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = crate::theme::home().join(".cache/files-test").join(format!(
                "checksum-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn known_vectors_and_chunked_progress_are_descriptor_verified() {
        let f = Fixture::new();
        let cancel = AtomicBool::new(false);
        for (data, expected) in [
            (
                b"".as_slice(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".as_slice(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            fs::write(f.0.join("a"), data).unwrap();
            let observed = calculate(&f.0, Path::new("a"), None, &cancel, |_, _| Ok(())).unwrap();
            assert_eq!(hex(&observed.sha256), expected);
            assert_eq!(observed.proof.file.bytes, data.len() as u64);
        }
        fs::write(f.0.join("large"), vec![b'x'; CHUNK + 1]).unwrap();
        let mut counts = Vec::new();
        calculate(&f.0, Path::new("large"), None, &cancel, |n, total| {
            assert_eq!(total, (CHUNK + 1) as u64);
            counts.push(n);
            Ok(())
        })
        .unwrap();
        assert_eq!(counts, vec![0, CHUNK as u64, CHUNK as u64 + 1]);
    }
    #[test]
    fn cancellation_before_open_and_after_final_chunk_never_publishes_digest() {
        let f = Fixture::new();
        let cancel = AtomicBool::new(true);
        assert_eq!(
            calculate(&f.0, Path::new("missing"), None, &cancel, |_, _| Ok(()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        fs::write(f.0.join("a"), b"abc").unwrap();
        cancel.store(false, Ordering::Relaxed);
        assert_eq!(
            calculate(&f.0, Path::new("a"), None, &cancel, |n, _| {
                if n > 0 {
                    cancel.store(true, Ordering::Relaxed);
                }
                Ok(())
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn replacement_during_read_rejects_original_observation() {
        let f = Fixture::new();
        fs::write(f.0.join("a"), b"abc").unwrap();
        assert!(
            calculate(
                &f.0,
                Path::new("a"),
                None,
                &AtomicBool::new(false),
                |n, _| {
                    if n > 0 {
                        fs::rename(f.0.join("a"), f.0.join("old"))?;
                        fs::write(f.0.join("a"), b"abc")?;
                    }
                    Ok(())
                }
            )
            .is_err()
        );
    }
    #[test]
    fn root_replacement_during_read_rejects_even_identical_contents() {
        let f = Fixture::new();
        let root = f.0.join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a"), b"abc").unwrap();
        assert!(
            calculate(
                &root,
                Path::new("a"),
                None,
                &AtomicBool::new(false),
                |n, _| {
                    if n > 0 {
                        fs::rename(&root, f.0.join("old"))?;
                        fs::create_dir(&root)?;
                        fs::write(root.join("a"), b"abc")?;
                    }
                    Ok(())
                }
            )
            .is_err()
        );
    }
    #[test]
    fn symlink_fifo_and_directory_are_rejected_without_reading() {
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        let f = Fixture::new();
        fs::write(f.0.join("a"), b"abc").unwrap();
        symlink("a", f.0.join("link")).unwrap();
        fs::create_dir(f.0.join("dir")).unwrap();
        let fifo = std::ffi::CString::new(f.0.join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        for name in ["link", "dir", "fifo"] {
            assert!(
                calculate(
                    &f.0,
                    Path::new(name),
                    None,
                    &AtomicBool::new(false),
                    |_, _| panic!("No debe empezar lectura")
                )
                .is_err()
            );
        }
    }
    #[test]
    fn batches_reject_duplicates_traversal_and_over_budget_without_io() {
        let n = |s: &str| Name { path: s.into() };
        for names in [
            vec![],
            vec![n("a"), n("a")],
            vec![n("../a")],
            vec![n("a/")],
            vec![n("a/b")],
            vec![n(&"x".repeat(MAX_NAME_BYTES + 1))],
        ] {
            assert!(validate(Path::new("/missing"), &names).is_err());
        }
        assert!(validate(Path::new("/missing"), &[n("a")]).is_ok());
    }
}
