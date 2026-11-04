//! Descubrimiento local por tamaños y SHA-256; informes limitados nunca significan ausencia.
use crate::{
    checksum::{self, Observation},
    relative_source::{DirectoryIdentity, Proof},
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
pub const MAX_FILES: usize = 16_384;
pub const MAX_PATH_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTRIES: u64 = 100_000;
pub const MAX_DIRS: usize = 4096;
pub const MAX_DEPTH: usize = 32;
pub const MAX_ROWS: usize = 128;
pub const MAX_RESULT_BYTES: usize = 8192;
pub const LIMIT_ENTRIES: u16 = 1;
pub const LIMIT_FILES: u16 = 2;
pub const LIMIT_PATHS: u16 = 4;
pub const LIMIT_DEPTH: u16 = 8;
pub const LIMIT_DIRS: u16 = 16;
pub const LIMIT_RESULTS: u16 = 32;
pub const CHANGED: u16 = 64;
pub const BYTE_OVERFLOW: u16 = 128;
#[derive(Clone, Copy)]
pub struct Limits {
    pub entries: u64,
    pub files: usize,
    pub paths: usize,
    pub depth: usize,
    pub dirs: usize,
    pub rows: usize,
    pub result_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: MAX_ENTRIES,
            files: MAX_FILES,
            paths: MAX_PATH_BYTES,
            depth: MAX_DEPTH,
            dirs: MAX_DIRS,
            rows: MAX_ROWS,
            result_bytes: MAX_RESULT_BYTES,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub phase: u8,
    pub entries: u64,
    pub files: u64,
    pub aliases: u64,
    pub skipped: u64,
    pub errors: u64,
    pub candidates: u64,
    pub hashed: u64,
    pub bytes_read: u64,
    pub bytes_total: u64,
    pub groups: u64,
    pub matches: u64,
    pub reasons: u16,
}
impl Progress {
    pub fn complete(self) -> bool {
        self.phase == 2 && self.errors == 0 && self.reasons == 0
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    #[serde(with = "crate::protocol::path_wire")]
    pub path: PathBuf,
    pub observation: Observation,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub root: DirectoryIdentity,
    pub progress: Progress,
    pub rows: Vec<Row>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Frame {
    pub id: u64,
    #[serde(flatten)]
    pub event: Event,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Error { message: String },
    Progress { progress: Progress },
    Page { offset: usize, rows: Vec<Row> },
    Done { report: Report },
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Búsqueda de duplicados cancelada",
        ))
    } else {
        Ok(())
    }
}
pub fn validate_root(root: &Path) -> io::Result<()> {
    let b = root.as_os_str().as_encoded_bytes();
    if !root.is_absolute() || b.len() > 4096 || b.contains(&0) {
        Err(invalid("Carpeta de duplicados inválida"))
    } else {
        Ok(())
    }
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.as_os_str().len() <= 4096
        && !path.as_os_str().as_encoded_bytes().contains(&0)
        && path.components().all(|c| matches!(c, Component::Normal(_)))
}
pub fn validate_progress(p: Progress) -> io::Result<()> {
    if p.phase > 2
        || p.entries > MAX_ENTRIES
        || p.files > MAX_FILES as u64
        || p.aliases > p.entries
        || p.skipped > p.entries
        || p.errors > 3 * MAX_ENTRIES + 1
        || p.candidates > p.files
        || p.hashed > p.candidates
        || p.bytes_read > p.bytes_total
        || p.groups > p.hashed / 2
        || p.matches > p.hashed
        || p.groups.saturating_mul(2) > p.matches
        || (p.groups == 0 && p.matches != 0)
        || p.reasons & !255 != 0
    {
        return Err(invalid("Contadores de duplicados inválidos"));
    }
    Ok(())
}
pub fn validate(report: &Report) -> io::Result<()> {
    validate_progress(report.progress)?;
    if report.progress.phase != 2
        || report.rows.len() > MAX_ROWS
        || report.rows.len() as u64 > report.progress.matches
    {
        return Err(invalid("Informe de duplicados inválido"));
    }
    let mut bytes = 0usize;
    for (i, row) in report.rows.iter().enumerate() {
        bytes = bytes
            .checked_add(row.path.as_os_str().len())
            .ok_or_else(|| invalid("Rutas fuera de presupuesto"))?;
        if bytes > MAX_RESULT_BYTES
            || !relative(&row.path)
            || row.observation.proof.root != report.root
            || report.rows[..i].iter().any(|r| {
                r.path == row.path
                    || r.observation.proof.file.device == row.observation.proof.file.device
                        && r.observation.proof.file.inode == row.observation.proof.file.inode
            })
        {
            return Err(invalid("Rutas o identidades de duplicados inválidas"));
        }
        if !report.rows.iter().enumerate().any(|(j, r)| {
            i != j
                && r.observation.sha256 == row.observation.sha256
                && r.observation.proof.file.bytes == row.observation.proof.file.bytes
        }) {
            return Err(invalid("Grupo de duplicados incompleto"));
        }
    }
    let groups = report
        .rows
        .iter()
        .enumerate()
        .filter(|(i, row)| {
            !report.rows[..*i].iter().any(|r| {
                r.observation.sha256 == row.observation.sha256
                    && r.observation.proof.file.bytes == row.observation.proof.file.bytes
            })
        })
        .count() as u64;
    if report.progress.groups < groups
        || report.progress.reasons & LIMIT_RESULTS == 0
            && (groups != report.progress.groups
                || report.rows.len() as u64 != report.progress.matches)
    {
        return Err(invalid("Contadores y grupos no coinciden"));
    }
    Ok(())
}
/// Complete retained observations, bounded by the same traversal metadata limits.
/// Transport and UI presentation paginate this collection without re-reading files.
pub fn validate_all(report: &Report) -> io::Result<()> {
    use std::collections::{HashMap, HashSet};
    validate_progress(report.progress)?;
    if report.progress.phase != 2
        || report.rows.len() > MAX_FILES
        || report.rows.len() as u64 != report.progress.matches
        || report.progress.reasons & LIMIT_RESULTS != 0
    {
        return Err(invalid("Informe completo de observaciones inválido"));
    }
    let mut paths = HashSet::new();
    let mut identities = HashSet::new();
    let mut groups = HashMap::new();
    let mut bytes = 0usize;
    for row in &report.rows {
        bytes = bytes
            .checked_add(row.path.as_os_str().len())
            .ok_or_else(|| invalid("Rutas excedidas"))?;
        let f = row.observation.proof.file;
        if bytes > MAX_PATH_BYTES
            || !relative(&row.path)
            || row.observation.proof.root != report.root
            || !paths.insert(&row.path)
            || !identities.insert((f.device, f.inode))
        {
            return Err(invalid("Rutas o identidades repetidas/fuera del límite"));
        }
        *groups
            .entry((row.observation.sha256, f.bytes))
            .or_insert(0usize) += 1;
    }
    if groups.len() as u64 != report.progress.groups || groups.values().any(|n| *n < 2) {
        return Err(invalid("Grupos incompletos o contadores incoherentes"));
    }
    Ok(())
}
/// End index of a bounded presentation page; a group may span several pages.
pub fn page_end(rows: &[Row], start: usize) -> usize {
    let mut end = start.min(rows.len());
    let mut bytes = 0;
    while end < rows.len() && end - start < MAX_ROWS {
        let next = rows[end].path.as_os_str().len();
        if bytes + next > MAX_RESULT_BYTES {
            break;
        }
        bytes += next;
        end += 1;
    }
    end
}
pub fn scan_all(
    root: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Report> {
    scan_internal(root, Limits::default(), cancel, progress, true)
}
pub fn scan(
    root: &Path,
    limits: Limits,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Report> {
    scan_internal(root, limits, cancel, progress, false)
}
#[cfg(target_os = "linux")]
fn scan_internal(
    root: &Path,
    limits: Limits,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress) -> io::Result<()>,
    all: bool,
) -> io::Result<Report> {
    use std::{
        collections::HashSet,
        fs::{self, File, OpenOptions, ReadDir},
        os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
    };
    check(cancel)?;
    validate_root(root)?;
    if limits.entries == 0
        || limits.entries > MAX_ENTRIES
        || limits.files == 0
        || limits.files > MAX_FILES
        || limits.paths > MAX_PATH_BYTES
        || limits.depth > MAX_DEPTH
        || limits.dirs == 0
        || limits.dirs > MAX_DIRS
        || limits.rows > MAX_ROWS
        || limits.result_bytes > MAX_RESULT_BYTES
    {
        return Err(invalid("Límites de duplicados inválidos"));
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Stamp {
        dev: u64,
        ino: u64,
        mtime: i64,
        mns: i64,
        ctime: i64,
        cns: i64,
    }
    fn stamp(f: &File) -> io::Result<Stamp> {
        let m = f.metadata()?;
        Ok(Stamp {
            dev: m.dev(),
            ino: m.ino(),
            mtime: m.mtime(),
            mns: m.mtime_nsec(),
            ctime: m.ctime(),
            cns: m.ctime_nsec(),
        })
    }
    struct Dir {
        file: File,
        entries: ReadDir,
        path: PathBuf,
        stamp: Stamp,
    }
    fn open(path: &Path, relative: PathBuf) -> io::Result<Dir> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let before = stamp(&file)?;
        let entries = fs::read_dir(format!("/proc/self/fd/{}", file.as_raw_fd()))?;
        Ok(Dir {
            file,
            entries,
            path: relative,
            stamp: before,
        })
    }
    fn reopen_directory(root: &File, path: &Path) -> io::Result<File> {
        use std::{ffi::CString, os::fd::FromRawFd};
        let mut current = root.try_clone()?;
        for part in path.components() {
            let Component::Normal(name) = part else {
                return Err(invalid("Directorio relativo inválido"));
            };
            let name =
                CString::new(name.as_encoded_bytes()).map_err(|_| invalid("Ruta con NUL"))?;
            let fd = unsafe {
                libc::openat(
                    current.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // openat devuelve un descriptor nuevo de propiedad exclusiva.
            current = unsafe { File::from_raw_fd(fd) };
        }
        Ok(current)
    }
    struct Candidate {
        path: PathBuf,
        proof: Proof,
        digest: Option<[u8; 32]>,
    }
    let first = open(root, PathBuf::new())?;
    let identity = DirectoryIdentity::from_file(&first.file)?;
    let root_stamp = first.stamp;
    let root_fd = first.file.try_clone()?;
    let mut visited = HashSet::new();
    visited.insert(identity);
    let mut inodes = HashSet::new();
    let mut stack = vec![first];
    let mut files: Vec<Candidate> = Vec::new();
    let mut used = 0usize;
    let mut stats = Progress::default();
    progress(stats)?;
    while !stack.is_empty() {
        check(cancel)?;
        if stats.entries >= limits.entries {
            stats.reasons |= LIMIT_ENTRIES;
            break;
        }
        let item = stack.last_mut().unwrap().entries.next();
        let Some(item) = item else {
            let dir = stack.pop().unwrap();
            let current = if dir.path.as_os_str().is_empty() {
                stamp(&root_fd)
            } else {
                reopen_directory(&root_fd, &dir.path).and_then(|file| stamp(&file))
            };
            if stamp(&dir.file).ok() != Some(dir.stamp) || current.ok() != Some(dir.stamp) {
                stats.reasons |= CHANGED;
                stats.errors += 1;
            }
            continue;
        };
        stats.entries += 1;
        let entry = match item {
            Ok(e) => e,
            Err(_) => {
                stats.errors += 1;
                progress(stats)?;
                continue;
            }
        };
        let path = stack.last().unwrap().path.join(entry.file_name());
        if !relative(&path) {
            stats.reasons |= LIMIT_PATHS;
            progress(stats)?;
            continue;
        }
        let kind = match entry.file_type() {
            Ok(k) => k,
            Err(_) => {
                stats.errors += 1;
                progress(stats)?;
                continue;
            }
        };
        if kind.is_dir() {
            if stack.len() > limits.depth {
                stats.reasons |= LIMIT_DEPTH;
            } else if visited.len() >= limits.dirs {
                stats.reasons |= LIMIT_DIRS;
            } else {
                match open(&entry.path(), path) {
                    Ok(dir) => {
                        let id = DirectoryIdentity::from_file(&dir.file)?;
                        if visited.insert(id) {
                            stack.push(dir);
                        } else {
                            stats.skipped += 1;
                        }
                    }
                    Err(_) => stats.errors += 1,
                }
            }
        } else if kind.is_file() {
            let metadata = match fs::symlink_metadata(entry.path())
                .and_then(|m| crate::media_source::Revision::from_metadata(&m))
            {
                Ok(m) => m,
                Err(_) => {
                    stats.errors += 1;
                    progress(stats)?;
                    continue;
                }
            };
            if !inodes.insert((metadata.device, metadata.inode)) {
                stats.aliases += 1;
            } else if files.len() >= limits.files {
                stats.reasons |= LIMIT_FILES;
                break;
            } else if used.saturating_add(path.as_os_str().len()) > limits.paths {
                stats.reasons |= LIMIT_PATHS;
                break;
            } else {
                used += path.as_os_str().len();
                files.push(Candidate {
                    path,
                    proof: Proof {
                        root: identity,
                        file: metadata,
                    },
                    digest: None,
                });
                stats.files += 1;
            }
        } else {
            stats.skipped += 1;
        }
        progress(stats)?;
    }
    drop(stack);
    files.sort_unstable_by(|a, b| {
        a.proof
            .file
            .bytes
            .cmp(&b.proof.file.bytes)
            .then_with(|| a.path.cmp(&b.path))
    });
    let mut buckets = Vec::new();
    let mut first = 0;
    while first < files.len() {
        let mut end = first + 1;
        while end < files.len() && files[end].proof.file.bytes == files[first].proof.file.bytes {
            end += 1;
        }
        if end - first > 1 {
            stats.candidates += (end - first) as u64;
            buckets.push(first..end);
        }
        first = end;
    }
    for range in &buckets {
        for f in &files[range.clone()] {
            match stats.bytes_total.checked_add(f.proof.file.bytes) {
                Some(total) => stats.bytes_total = total,
                None => {
                    stats.bytes_total = u64::MAX;
                    stats.reasons |= BYTE_OVERFLOW;
                }
            }
        }
    }
    stats.phase = 1;
    progress(stats)?;
    for range in buckets {
        for candidate in &mut files[range] {
            check(cancel)?;
            let mut previous = 0;
            let result = checksum::calculate_checked(
                root,
                &candidate.path,
                candidate.proof,
                cancel,
                |bytes, _| {
                    let delta = bytes - previous;
                    previous = bytes;
                    stats.bytes_read = stats
                        .bytes_read
                        .checked_add(delta)
                        .ok_or_else(|| invalid("Conteo de bytes excedido"))?;
                    progress(stats)
                },
            );
            match result {
                Ok(o) => {
                    candidate.digest = Some(o.sha256);
                    stats.hashed += 1;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
                Err(_) => stats.errors += 1,
            }
            progress(stats)?;
        }
    }
    if open(root, PathBuf::new()).as_ref().map(|d| d.stamp).ok() != Some(root_stamp) {
        stats.reasons |= CHANGED;
        stats.errors += 1;
    }
    check(cancel)?;
    files.retain(|f| f.digest.is_some());
    files.sort_unstable_by(|a, b| {
        a.proof
            .file
            .bytes
            .cmp(&b.proof.file.bytes)
            .then_with(|| a.digest.cmp(&b.digest))
            .then_with(|| a.path.cmp(&b.path))
    });
    let mut rows = Vec::new();
    let mut result_bytes = 0;
    let mut first = 0;
    while first < files.len() {
        let mut end = first + 1;
        while end < files.len()
            && files[end].proof.file.bytes == files[first].proof.file.bytes
            && files[end].digest == files[first].digest
        {
            end += 1;
        }
        if end - first > 1 {
            stats.groups += 1;
            stats.matches += (end - first) as u64;
            let bytes: usize = files[first..end]
                .iter()
                .map(|f| f.path.as_os_str().len())
                .sum();
            if !all
                && (rows.len() + end - first > limits.rows
                    || result_bytes + bytes > limits.result_bytes)
            {
                stats.reasons |= LIMIT_RESULTS;
            } else {
                result_bytes += bytes;
                for f in &files[first..end] {
                    rows.push(Row {
                        path: f.path.clone(),
                        observation: Observation {
                            proof: f.proof,
                            sha256: f.digest.unwrap(),
                        },
                    });
                }
            }
        }
        first = end;
    }
    stats.phase = 2;
    let report = Report {
        root: identity,
        progress: stats,
        rows,
    };
    if all {
        validate_all(&report)?;
    } else {
        validate(&report)?;
    }
    Ok(report)
}
#[cfg(not(target_os = "linux"))]
fn scan_internal(
    _: &Path,
    _: Limits,
    _: &AtomicBool,
    _: impl FnMut(Progress) -> io::Result<()>,
    _: bool,
) -> io::Result<Report> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Duplicados por carpeta pendientes en esta plataforma",
    ))
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, sync::atomic::AtomicU64};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let p = crate::theme::home().join(".cache/files-test").join(format!(
                "duplicates-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn complete_observations_paginate_a_large_group_without_rehashing_or_omission() {
        let f = Fixture::new();
        for i in 0..129 {
            fs::write(f.0.join(format!("copy-{i:03}")), b"abc").unwrap();
        }
        let mut r = scan_all(&f.0, &AtomicBool::new(false), |_| Ok(())).unwrap();
        assert!(r.progress.complete());
        assert_eq!(
            (r.progress.groups, r.progress.matches, r.progress.bytes_read),
            (1, 129, 387)
        );
        assert_eq!(page_end(&r.rows, 0), 128);
        assert_eq!(page_end(&r.rows, 128), 129);
        validate_all(&r).unwrap();
        r.rows.pop();
        assert!(validate_all(&r).is_err());
    }
    #[test]
    fn nested_duplicates_skip_unique_sizes_symlinks_and_hardlink_aliases() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("sub")).unwrap();
        fs::write(f.0.join("a"), b"abc").unwrap();
        fs::write(f.0.join("sub/b"), b"abc").unwrap();
        fs::write(f.0.join("unique"), b"longer").unwrap();
        fs::hard_link(f.0.join("a"), f.0.join("alias")).unwrap();
        std::os::unix::fs::symlink("sub", f.0.join("link")).unwrap();
        let r = scan(&f.0, Limits::default(), &AtomicBool::new(false), |_| Ok(())).unwrap();
        assert!(r.progress.complete());
        assert_eq!(
            (
                r.progress.groups,
                r.progress.matches,
                r.progress.hashed,
                r.progress.aliases
            ),
            (1, 2, 2, 1)
        );
        assert_eq!(r.rows.len(), 2);
        assert_eq!(
            checksum::hex(&r.rows[0].observation.sha256),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn result_and_scan_limits_never_report_complete_absence() {
        let f = Fixture::new();
        for n in ["a", "b", "c"] {
            fs::write(f.0.join(n), b"abc").unwrap();
        }
        for limits in [
            Limits {
                entries: 1,
                ..Limits::default()
            },
            Limits {
                files: 1,
                ..Limits::default()
            },
            Limits {
                rows: 2,
                ..Limits::default()
            },
        ] {
            let r = scan(&f.0, limits, &AtomicBool::new(false), |_| Ok(())).unwrap();
            assert!(!r.progress.complete());
            assert_ne!(r.progress.reasons, 0);
            if limits.rows == 2 {
                assert_eq!(r.progress.matches, 3);
                assert!(r.rows.is_empty());
            }
        }
    }
    #[test]
    fn cancellation_and_root_replacement_cannot_become_clean_no_matches() {
        let f = Fixture::new();
        for n in ["a", "b"] {
            fs::write(f.0.join(n), b"abc").unwrap();
        }
        let c = AtomicBool::new(true);
        assert_eq!(
            scan(&f.0, Limits::default(), &c, |_| Ok(()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        c.store(false, Ordering::Relaxed);
        assert_eq!(
            scan(&f.0, Limits::default(), &c, |p| {
                if p.entries > 0 {
                    c.store(true, Ordering::Relaxed);
                }
                Ok(())
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn stale_candidate_revision_is_rejected_before_content_read() {
        let f = Fixture::new();
        for n in ["a", "b"] {
            fs::write(f.0.join(n), b"abc").unwrap();
        }
        let mut changed = false;
        let r = scan(&f.0, Limits::default(), &AtomicBool::new(false), |p| {
            if p.phase == 1 && !changed {
                changed = true;
                fs::write(f.0.join("a"), b"xyz")?;
            }
            Ok(())
        })
        .unwrap();
        assert!(!r.progress.complete());
        assert_eq!(r.progress.groups, 0);
        assert!(r.progress.errors > 0);
    }
    #[test]
    fn root_rebinding_is_partial_and_never_reads_replacement_contents() {
        let f = Fixture::new();
        let root = f.0.join("root");
        fs::create_dir(&root).unwrap();
        for n in ["a", "b"] {
            fs::write(root.join(n), b"abc").unwrap();
        }
        let mut changed = false;
        let r = scan(&root, Limits::default(), &AtomicBool::new(false), |p| {
            if p.phase == 0 && p.entries > 0 && !changed {
                changed = true;
                fs::rename(&root, f.0.join("old"))?;
                fs::create_dir(&root)?;
                fs::write(root.join("a"), b"replacement")?;
            }
            Ok(())
        })
        .unwrap();
        assert!(!r.progress.complete());
        assert_ne!(r.progress.reasons & CHANGED, 0);
        assert_eq!(r.progress.bytes_read, 0);
    }
    #[test]
    fn report_cannot_claim_zero_groups_when_rows_match() {
        let f = Fixture::new();
        for n in ["a", "b"] {
            fs::write(f.0.join(n), b"abc").unwrap();
        }
        let mut r = scan(&f.0, Limits::default(), &AtomicBool::new(false), |_| Ok(())).unwrap();
        r.progress.groups = 0;
        assert!(validate(&r).is_err());
    }
}
