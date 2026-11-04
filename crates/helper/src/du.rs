//! Bounded apparent-size traversal. Counts file names, including hard-link aliases;
//! symlinks contribute no bytes and are never traversed. No partial success.
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
pub mod allocation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub bytes: u64,
    pub entries: u64,
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub entries: u64,
    pub depth: usize,
    pub elapsed: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 1_000_000,
            depth: 128,
            elapsed: Duration::from_secs(15),
        }
    }
}

pub fn measure(path: &Path, limits: Limits) -> io::Result<Usage> {
    measure_mode(path, limits, false)
}

/// Directory-only entry point for previews whose cached entry type may be stale.
/// Rejects a non-directory/final symlink both before opening and at O_DIRECTORY
/// |O_NOFOLLOW open. Parent symlinks and mount traversal retain measure's existing
/// semantics; the cooperative timeout cannot interrupt an outstanding FS syscall.
pub fn measure_directory(path: &Path, limits: Limits) -> io::Result<Usage> {
    measure_mode(path, limits, true)
}

#[cfg(target_os = "linux")]
fn measure_mode(path: &Path, limits: Limits, require_directory: bool) -> io::Result<Usage> {
    measure_with(path, limits, require_directory, None, |_| Ok(()))
}
#[cfg(target_os = "linux")]
fn measure_with(
    path: &Path,
    limits: Limits,
    require_directory: bool,
    mut allocation: Option<&mut allocation::Accumulator>,
    mut progress: impl FnMut(Usage) -> io::Result<()>,
) -> io::Result<Usage> {
    use std::fs::{self, File, OpenOptions, ReadDir};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    struct Frame {
        _directory: File,
        entries: ReadDir,
    }
    fn open(path: &Path) -> io::Result<Frame> {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        // The descriptor pins the directory even if its original name is replaced.
        let entries = fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))?;
        Ok(Frame {
            _directory: directory,
            entries,
        })
    }
    let start = Instant::now();
    let mut usage = Usage {
        bytes: 0,
        entries: 0,
    };
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        if require_directory {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Se requiere una carpeta real, sin enlace simbólico final",
            ));
        }
        usage.entries = 1;
        if metadata.is_file() {
            usage.bytes = metadata.len();
            observe_allocation(&metadata, allocation.as_deref_mut());
        }
        if usage.entries > limits.entries || start.elapsed() >= limits.elapsed {
            return Err(io::Error::other("Límite de cálculo de tamaño excedido"));
        }
        return Ok(usage);
    }
    if limits.depth == 0 {
        return Err(io::Error::other("Límite de profundidad excedido"));
    }
    let root = open(path)?;
    let root_proof = root._directory.try_clone()?;
    let mut stack = vec![root];
    progress(usage)?;
    let mut last_progress = Instant::now();
    while !stack.is_empty() {
        if start.elapsed() >= limits.elapsed {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Tiempo de cálculo de tamaño agotado",
            ));
        }
        if last_progress.elapsed() >= Duration::from_millis(100) {
            progress(usage)?;
            last_progress = Instant::now();
        }
        let next = stack.last_mut().unwrap().entries.next();
        let Some(entry) = next else {
            stack.pop();
            continue;
        };
        let entry = entry?;
        usage.entries = usage
            .entries
            .checked_add(1)
            .filter(|n| *n <= limits.entries)
            .ok_or_else(|| io::Error::other("Límite de entradas excedido"))?;
        // DirEntry::path is rooted in /proc/self/fd/<held descriptor>, not a stale name.
        let path = entry.path();
        // read_dir already supplies the entry kind on common local filesystems.
        // Skip lstat for directories and links; O_DIRECTORY|O_NOFOLLOW still
        // validates every actual directory open. This remains a live traversal,
        // not a snapshot, and never follows a replacement symlink.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if stack.len() >= limits.depth {
                return Err(io::Error::other("Límite de profundidad excedido"));
            }
            stack.push(open(&path)?);
        } else if kind.is_file() {
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() {
                continue;
            }
            usage.bytes = usage
                .bytes
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("Tamaño fuera del rango soportado"))?;
            observe_allocation(&metadata, allocation.as_deref_mut());
        }
    }
    use std::os::unix::fs::MetadataExt;
    let actual = open(path)?;
    let before = root_proof.metadata()?;
    let after = actual._directory.metadata()?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(io::Error::other(
            "La carpeta fue reemplazada durante el cálculo",
        ));
    }
    progress(usage)?;
    Ok(usage)
}

#[cfg(target_os = "linux")]
pub fn measure_progress(
    path: &Path,
    limits: Limits,
    progress: impl FnMut(Usage) -> io::Result<()>,
) -> io::Result<Usage> {
    let raw = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || raw.len() > 4096 || raw.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "El cálculo requiere una ruta absoluta de hasta4096B",
        ));
    }
    measure_with(path, limits, true, None, progress)
}

#[cfg(target_os = "linux")]
fn observe_allocation(
    metadata: &std::fs::Metadata,
    allocation: Option<&mut allocation::Accumulator>,
) {
    use std::os::unix::fs::MetadataExt;
    if let Some(allocation) = allocation {
        allocation.observe(metadata.dev(), metadata.ino(), metadata.blocks());
    }
}

#[cfg(all(test, target_os = "linux"))]
pub fn measure_allocated_directory(
    path: &Path,
    limits: Limits,
) -> io::Result<(Usage, allocation::Observation)> {
    measure_allocated_progress(path, limits, |_| Ok(()))
}

/// Allocation is final-only. Progress retains the apparent counters, never a
/// partial allocation presented as a completed observation.
#[cfg(target_os = "linux")]
pub fn measure_allocated_progress(
    path: &Path,
    limits: Limits,
    progress: impl FnMut(Usage) -> io::Result<()>,
) -> io::Result<(Usage, files_core::folder_usage::Observation)> {
    let raw = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || raw.len() > 4096 || raw.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ruta de cálculo inválida",
        ));
    }
    let mut allocation = allocation::Accumulator::default();
    let usage = measure_with(path, limits, true, Some(&mut allocation), progress)?;
    Ok((usage, allocation.finish()))
}
#[cfg(not(target_os = "linux"))]
pub fn measure_allocated_progress(
    _: &Path,
    _: Limits,
    _: impl FnMut(Usage) -> io::Result<()>,
) -> io::Result<(Usage, files_core::folder_usage::Observation)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Asignación segura no implementada en esta plataforma",
    ))
}
#[cfg(not(target_os = "linux"))]
pub fn measure_progress(
    path: &Path,
    limits: Limits,
    _: impl FnMut(Usage) -> io::Result<()>,
) -> io::Result<Usage> {
    measure_directory(path, limits)
}

#[cfg(not(target_os = "linux"))]
fn measure_mode(_: &Path, _: Limits, _: bool) -> io::Result<Usage> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Recorrido seguro aún no implementado en esta plataforma",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "files-du-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn optional_allocation_observes_sparse_aliases_without_changing_apparent_total() {
        use allocation::Value;
        use std::os::unix::fs::MetadataExt;
        let f = Fixture::new();
        fs::create_dir(f.0.join("sub")).unwrap();
        let sparse = fs::File::create(f.0.join("sparse")).unwrap();
        sparse.set_len(16 * 1024 * 1024).unwrap();
        fs::hard_link(f.0.join("sparse"), f.0.join("sub/alias")).unwrap();
        fs::write(f.0.join("data"), b"allocated data").unwrap();
        fs::hard_link(f.0.join("data"), f.0.join("sub/data-alias")).unwrap();
        symlink(f.0.join("data"), f.0.join("link")).unwrap();
        let sparse_meta = fs::symlink_metadata(f.0.join("sparse")).unwrap();
        let data_meta = fs::symlink_metadata(f.0.join("data")).unwrap();
        let before = measure_directory(&f.0, Limits::default()).unwrap();
        let (usage, observation) = measure_allocated_directory(&f.0, Limits::default()).unwrap();
        assert_eq!(usage, before);
        assert_eq!(
            usage,
            Usage {
                bytes: 32 * 1024 * 1024 + 28,
                entries: 6
            }
        );
        assert_eq!(
            observation.by_name,
            Value::Bytes((2 * sparse_meta.blocks() + 2 * data_meta.blocks()) * 512)
        );
        assert_eq!(
            observation.by_identity,
            Value::Bytes((sparse_meta.blocks() + data_meta.blocks()) * 512)
        );
        assert_eq!(fs::read(f.0.join("data")).unwrap(), b"allocated data");
        assert!(measure_allocated_directory(&f.0.join("link"), Limits::default()).is_err());
        assert!(
            measure_allocated_directory(
                &f.0,
                Limits {
                    entries: 1,
                    ..Limits::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn allocation_empty_is_measured_zero_and_relative_root_is_rejected() {
        use allocation::Value;
        let f = Fixture::new();
        let (usage, observation) = measure_allocated_directory(&f.0, Limits::default()).unwrap();
        assert_eq!(usage, Usage::default());
        assert_eq!(
            observation,
            allocation::Observation {
                by_name: Value::Bytes(0),
                by_identity: Value::Bytes(0)
            }
        );
        assert!(measure_allocated_directory(Path::new("."), Limits::default()).is_err());
    }
    #[test]
    fn totals_nested_files_without_following_links_or_cycles() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("sub")).unwrap();
        fs::write(f.0.join("a"), b"123").unwrap();
        fs::write(f.0.join("sub/b"), b"12345").unwrap();
        symlink(&f.0, f.0.join("sub/cycle")).unwrap();
        symlink("missing", f.0.join("broken")).unwrap();
        let u = measure(&f.0, Limits::default()).unwrap();
        assert_eq!((u.bytes, u.entries), (8, 5));
        let u = measure(&f.0.join("sub/cycle"), Limits::default()).unwrap();
        assert_eq!((u.bytes, u.entries), (0, 1));
    }
    #[test]
    fn bounds_return_errors_not_partial_totals() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("sub")).unwrap();
        fs::write(f.0.join("sub/a"), b"123").unwrap();
        assert!(
            measure(
                &f.0,
                Limits {
                    entries: 1,
                    ..Limits::default()
                }
            )
            .is_err()
        );
        assert!(
            measure(
                &f.0,
                Limits {
                    depth: 1,
                    ..Limits::default()
                }
            )
            .is_err()
        );
        assert!(
            measure(
                &f.0,
                Limits {
                    elapsed: Duration::ZERO,
                    ..Limits::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn apparent_bytes_count_sparse_files_and_hardlink_names() {
        let f = Fixture::new();
        fs::File::create(f.0.join("sparse"))
            .unwrap()
            .set_len(1 << 30)
            .unwrap();
        fs::hard_link(f.0.join("sparse"), f.0.join("alias")).unwrap();
        assert_eq!(measure(&f.0, Limits::default()).unwrap().bytes, 2 << 30);
    }
    #[test]
    fn directory_preview_counts_only_real_directory_roots() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("sub")).unwrap();
        fs::write(f.0.join("sub/data"), b"12345").unwrap();
        let result = measure_directory(&f.0, Limits::default()).unwrap();
        assert_eq!((result.bytes, result.entries), (5, 2));
        let error = measure_directory(&f.0.join("sub/data"), Limits::default())
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        // The general-purpose API still accepts a file unchanged.
        let result = measure(&f.0.join("sub/data"), Limits::default()).unwrap();
        assert_eq!((result.bytes, result.entries), (5, 1));
    }
    #[test]
    fn directory_preview_rejects_final_symlinks_and_replaced_file_roots() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("directory")).unwrap();
        fs::write(f.0.join("file"), b"data").unwrap();
        symlink("directory", f.0.join("directory-link")).unwrap();
        symlink("file", f.0.join("file-link")).unwrap();
        symlink("missing", f.0.join("broken-link")).unwrap();
        for name in ["directory-link", "file-link", "broken-link"] {
            let error = measure_directory(&f.0.join(name), Limits::default())
                .err()
                .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
        // A stale UI directory hint is insufficient after replacement by a file.
        fs::rename(f.0.join("directory"), f.0.join("original-directory")).unwrap();
        fs::write(f.0.join("directory"), b"replacement").unwrap();
        assert!(measure_directory(&f.0.join("directory"), Limits::default()).is_err());
    }
    #[test]
    fn progress_has_initial_and_final_samples_and_a_replaced_root_is_never_confirmed() {
        let f = Fixture::new();
        let root = f.0.join("folder");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("data"), b"12345").unwrap();
        let mut samples = Vec::new();
        let final_usage = measure_progress(&root, Limits::default(), |u| {
            samples.push(u);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            samples.first().copied(),
            Some(Usage {
                bytes: 0,
                entries: 0
            })
        );
        assert_eq!(samples.last().copied(), Some(final_usage));
        assert_eq!(final_usage.bytes, 5);
        let mut replaced = false;
        let result = measure_progress(&root, Limits::default(), |_| {
            if !replaced {
                fs::rename(&root, f.0.join("original")).unwrap();
                fs::create_dir(&root).unwrap();
                fs::write(root.join("actor"), b"actor").unwrap();
                replaced = true;
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(fs::read(root.join("actor")).unwrap(), b"actor");
        assert_eq!(fs::read(f.0.join("original/data")).unwrap(), b"12345");
    }
    #[test]
    fn progress_sink_failure_stops_without_a_final_total() {
        let f = Fixture::new();
        fs::write(f.0.join("a"), b"data").unwrap();
        let e = measure_progress(&f.0, Limits::default(), |_| {
            Err(io::Error::new(io::ErrorKind::Interrupted, "stop"))
        })
        .err()
        .unwrap();
        assert_eq!(e.kind(), io::ErrorKind::Interrupted);
    }
}
