//! Read-only tree comparison. Bounded observations never authorize synchronization.
use crate::relative_source::DirectoryIdentity;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_ITEMS: usize = 8192;
pub const MAX_NAMES: usize = 1024 * 1024;
pub const MAX_ENTRIES: u64 = 100_000;
pub const MAX_DEPTH: usize = 32;
pub const MAX_READ: u64 = 1024 * 1024 * 1024 * 1024;
pub const LIMIT: u16 = 1;
pub const UNREADABLE: u16 = 2;
pub const CHANGED: u16 = 4;
pub const READ_LIMIT: u16 = 8;
pub const UNSUPPORTED: u16 = 16;
pub const ALIAS: u16 = 32;
#[derive(Clone, Copy)]
pub struct Limits {
    pub items: usize,
    pub names: usize,
    pub entries: u64,
    pub depth: usize,
    pub read: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            items: MAX_ITEMS,
            names: MAX_NAMES,
            entries: MAX_ENTRIES,
            depth: MAX_DEPTH,
            read: 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    File,
    Directory,
    Symlink,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difference {
    Equal,
    OnlyLeft,
    OnlyRight,
    Type,
    Content,
    Children,
    Link,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    #[serde(with = "crate::protocol::path_wire")]
    pub path: PathBuf,
    pub left: Option<Kind>,
    pub right: Option<Kind>,
    pub difference: Difference,
    pub metadata_differs: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    /// 0/1 enumerate left/right, 2 compare, 3 terminal observation.
    pub phase: u8,
    pub entries: [u64; 2],
    pub compared: u64,
    pub bytes_read: u64,
    pub equal: u64,
    pub different: u64,
    pub unknown: u64,
    pub reasons: u16,
}
impl Progress {
    pub fn complete(self) -> bool {
        self.phase == 3 && self.reasons == 0 && self.unknown == 0
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub roots: [DirectoryIdentity; 2],
    pub progress: Progress,
    pub rows: Vec<Row>,
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Comparación cancelada",
        ))
    } else {
        Ok(())
    }
}
fn validate_inputs(roots: [&Path; 2], limits: Limits) -> io::Result<()> {
    for root in roots {
        crate::navigation_history::validate(root).map_err(invalid)?;
    }
    if limits.items == 0
        || limits.items > MAX_ITEMS
        || limits.names > MAX_NAMES
        || limits.entries == 0
        || limits.entries > MAX_ENTRIES
        || limits.depth > MAX_DEPTH
        || limits.read > MAX_READ
    {
        return Err(invalid("Límites de comparación inválidos"));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::{checksum, media_source::Revision, relative_source::Proof};
    use std::{
        collections::{BTreeMap, HashSet},
        ffi::CString,
        fs::{self, File, Metadata, OpenOptions, ReadDir},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Stamp {
        dev: u64,
        ino: u64,
        mode: u32,
        len: u64,
        mt: i64,
        mn: i64,
        ct: i64,
        cn: i64,
    }
    impl Stamp {
        fn new(m: &Metadata) -> Self {
            Self {
                dev: m.dev(),
                ino: m.ino(),
                mode: m.mode(),
                len: m.len(),
                mt: m.mtime(),
                mn: m.mtime_nsec(),
                ct: m.ctime(),
                cn: m.ctime_nsec(),
            }
        }
        fn identity(self) -> DirectoryIdentity {
            DirectoryIdentity {
                device: self.dev,
                inode: self.ino,
            }
        }
        fn revision(self) -> Revision {
            Revision {
                device: self.dev,
                inode: self.ino,
                bytes: self.len,
                modified_seconds: self.mt,
                modified_nanoseconds: self.mn,
                changed_seconds: self.ct,
                changed_nanoseconds: self.cn,
            }
        }
        fn metadata_differs(self, other: Self, kind: Kind) -> bool {
            self.mode & 0o7777 != other.mode & 0o7777
                || kind != Kind::Directory && (self.mt, self.mn) != (other.mt, other.mn)
        }
    }
    struct Entry {
        kind: Kind,
        stamp: Stamp,
        link: Option<PathBuf>,
    }
    struct Inventory {
        root: File,
        path: PathBuf,
        stamp: Stamp,
        entries: BTreeMap<PathBuf, Entry>,
        reasons: u16,
    }
    fn open_root(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }
    fn open_dir(root: &File, relative: &Path) -> io::Result<File> {
        let mut current = root.try_clone()?;
        for part in relative.components() {
            let std::path::Component::Normal(name) = part else {
                return Err(invalid("Directorio relativo inválido"));
            };
            let name =
                CString::new(name.as_encoded_bytes()).map_err(|_| invalid("Nombre con NUL"))?;
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
            // Newly owned descriptor; every intermediate is a NOFOLLOW directory.
            current = unsafe { File::from_raw_fd(fd) };
        }
        Ok(current)
    }
    fn fd_path(file: &File) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
    }
    fn observe(root: &File, relative: &Path) -> io::Result<Entry> {
        let parent = open_dir(root, relative.parent().unwrap_or(Path::new("")))?;
        let path =
            fd_path(&parent).join(relative.file_name().ok_or_else(|| invalid("Ruta vacía"))?);
        let m = fs::symlink_metadata(&path)?;
        let stamp = Stamp::new(&m);
        let kind = if m.is_file() {
            Kind::File
        } else if m.is_dir() {
            Kind::Directory
        } else if m.file_type().is_symlink() {
            Kind::Symlink
        } else {
            Kind::Other
        };
        let link = if kind == Kind::Symlink {
            Some(fs::read_link(&path)?)
        } else {
            None
        };
        if Stamp::new(&fs::symlink_metadata(path)?) != stamp {
            return Err(io::Error::other("El nombre cambió durante la observación"));
        }
        Ok(Entry { kind, stamp, link })
    }
    struct Directory {
        file: File,
        entries: ReadDir,
        relative: PathBuf,
        stamp: Stamp,
    }
    fn directory(file: File, relative: PathBuf) -> io::Result<Directory> {
        let stamp = Stamp::new(&file.metadata()?);
        let entries = fs::read_dir(fd_path(&file))?;
        Ok(Directory {
            file,
            entries,
            relative,
            stamp,
        })
    }
    fn inventory(
        path: &Path,
        expected: Option<DirectoryIdentity>,
        side: usize,
        limits: Limits,
        cancel: &AtomicBool,
        stats: &mut Progress,
        progress: &mut impl FnMut(Progress) -> io::Result<()>,
    ) -> io::Result<Inventory> {
        check(cancel)?;
        let root = open_root(path)?;
        let stamp = Stamp::new(&root.metadata()?);
        if expected.is_some_and(|id| id != stamp.identity()) {
            return Err(io::Error::other("La raíz capturada fue reemplazada"));
        }
        let mut inventory = Inventory {
            root,
            path: path.into(),
            stamp,
            entries: BTreeMap::new(),
            reasons: 0,
        };
        let mut visited = HashSet::from([stamp.identity()]);
        let mut stack = vec![directory(inventory.root.try_clone()?, PathBuf::new())?];
        let mut names = 0usize;
        stats.phase = side as u8;
        progress(*stats)?;
        while !stack.is_empty() {
            check(cancel)?;
            let item = stack.last_mut().unwrap().entries.next();
            let Some(item) = item else {
                let dir = stack.pop().unwrap();
                if dir.file.metadata().map(|m| Stamp::new(&m)).ok() != Some(dir.stamp) {
                    inventory.reasons |= CHANGED;
                }
                continue;
            };
            if stats.entries[side] >= limits.entries {
                inventory.reasons |= LIMIT;
                break;
            }
            stats.entries[side] += 1;
            let entry = match item {
                Ok(e) => e,
                Err(_) => {
                    inventory.reasons |= UNREADABLE;
                    continue;
                }
            };
            let relative = stack.last().unwrap().relative.join(entry.file_name());
            if relative.as_os_str().len() > 4096 || inventory.entries.len() >= limits.items {
                inventory.reasons |= LIMIT;
                break;
            }
            let observed = match observe(&inventory.root, &relative) {
                Ok(e) => e,
                Err(_) => {
                    inventory.reasons |= UNREADABLE;
                    continue;
                }
            };
            let charge = relative.as_os_str().len()
                + observed.link.as_ref().map_or(0, |p| p.as_os_str().len());
            if charge > limits.names.saturating_sub(names) {
                inventory.reasons |= LIMIT;
                break;
            }
            names += charge;
            if observed.kind == Kind::Directory {
                if stack.len() > limits.depth {
                    inventory.reasons |= LIMIT;
                } else if !visited.insert(observed.stamp.identity()) {
                    inventory.reasons |= ALIAS;
                } else {
                    match open_dir(&inventory.root, &relative)
                        .and_then(|f| directory(f, relative.clone()))
                    {
                        Ok(dir) if dir.stamp == observed.stamp => stack.push(dir),
                        _ => inventory.reasons |= CHANGED,
                    }
                }
            }
            if observed.kind == Kind::Other {
                inventory.reasons |= UNSUPPORTED;
            }
            inventory.entries.insert(relative, observed);
            stats.reasons |= inventory.reasons;
            progress(*stats)?;
        }
        stats.reasons |= inventory.reasons;
        Ok(inventory)
    }
    fn root_valid(i: &Inventory) -> bool {
        fs::symlink_metadata(&i.path)
            .ok()
            .filter(|m| m.is_dir())
            .map(|m| Stamp::new(&m).identity())
            == Some(i.stamp.identity())
            && i.root.metadata().ok().map(|m| Stamp::new(&m).identity()) == Some(i.stamp.identity())
    }
    fn inventory_changed(i: &Inventory, cancel: &AtomicBool) -> io::Result<HashSet<PathBuf>> {
        let mut changed = HashSet::new();
        for (path, entry) in &i.entries {
            check(cancel)?;
            if observe(&i.root, path).ok().is_none_or(|e| {
                e.stamp != entry.stamp || e.kind != entry.kind || e.link != entry.link
            }) {
                changed.insert(path.clone());
            }
        }
        Ok(changed)
    }
    pub(super) fn compare(
        roots: [&Path; 2],
        expected: Option<[DirectoryIdentity; 2]>,
        limits: Limits,
        cancel: &AtomicBool,
        mut progress: impl FnMut(Progress) -> io::Result<()>,
    ) -> io::Result<Report> {
        check(cancel)?;
        validate_inputs(roots, limits)?;
        let mut stats = Progress::default();
        let left = inventory(
            roots[0],
            expected.map(|ids| ids[0]),
            0,
            limits,
            cancel,
            &mut stats,
            &mut progress,
        )?;
        let right = inventory(
            roots[1],
            expected.map(|ids| ids[1]),
            1,
            limits,
            cancel,
            &mut stats,
            &mut progress,
        )?;
        stats.phase = 2;
        progress(stats)?;
        let mut rows = Vec::new();
        let mut paths = left
            .entries
            .keys()
            .chain(right.entries.keys())
            .collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        let absent_known = left.reasons == 0 && right.reasons == 0;
        for path in paths {
            check(cancel)?;
            let a = left.entries.get(path);
            let b = right.entries.get(path);
            let mut row = Row {
                path: path.clone(),
                left: a.map(|e| e.kind),
                right: b.map(|e| e.kind),
                difference: Difference::Unknown,
                metadata_differs: false,
            };
            row.difference = match (a, b) {
                (Some(a), Some(b)) if a.kind != b.kind => Difference::Type,
                (Some(a), Some(b)) => {
                    row.metadata_differs = a.stamp.metadata_differs(b.stamp, a.kind);
                    match a.kind {
                        Kind::Directory => Difference::Equal,
                        Kind::Symlink => {
                            if a.link == b.link {
                                Difference::Equal
                            } else {
                                Difference::Link
                            }
                        }
                        Kind::Other => Difference::Unknown,
                        Kind::File => {
                            if a.stamp.len != b.stamp.len {
                                Difference::Content
                            } else if a.stamp.len > limits.read.saturating_sub(stats.bytes_read) / 2
                            {
                                stats.reasons |= READ_LIMIT;
                                Difference::Unknown
                            } else {
                                let mut hashes = Vec::with_capacity(2);
                                for (inventory, entry) in [(&left, a), (&right, b)] {
                                    let proof = Proof {
                                        root: inventory.stamp.identity(),
                                        file: entry.stamp.revision(),
                                    };
                                    let mut previous = 0;
                                    let mut callback_error = None;
                                    let hash = checksum::calculate_checked(
                                        &inventory.path,
                                        path,
                                        proof,
                                        cancel,
                                        |read, _| {
                                            stats.bytes_read += read - previous;
                                            previous = read;
                                            if let Err(error) = progress(stats) {
                                                callback_error = Some(error);
                                                return Err(io::Error::other(
                                                    "El consumidor dejó de recibir progreso",
                                                ));
                                            }
                                            Ok(())
                                        },
                                    );
                                    if let Some(error) = callback_error {
                                        return Err(error);
                                    }
                                    match hash {
                                        Ok(hash) => hashes.push(hash.sha256),
                                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                                            return Err(e);
                                        }
                                        Err(_) => {
                                            stats.reasons |= CHANGED;
                                            break;
                                        }
                                    }
                                }
                                if hashes.len() != 2 {
                                    Difference::Unknown
                                } else if hashes[0] == hashes[1] {
                                    Difference::Equal
                                } else {
                                    Difference::Content
                                }
                            }
                        }
                    }
                }
                (Some(_), None) if absent_known => Difference::OnlyLeft,
                (None, Some(_)) if absent_known => Difference::OnlyRight,
                _ => Difference::Unknown,
            };
            rows.push(row);
            stats.compared += 1;
            progress(stats)?;
        }
        // Revalidate after all hashing: additions/removals invalidate absence claims.
        let root_ok = root_valid(&left) && root_valid(&right);
        let changed_left = inventory_changed(&left, cancel)?;
        let changed_right = inventory_changed(&right, cancel)?;
        let directory_changed = left.root.metadata().map(|m| Stamp::new(&m)).ok()
            != Some(left.stamp)
            || right.root.metadata().map(|m| Stamp::new(&m)).ok() != Some(right.stamp)
            || changed_left
                .iter()
                .any(|p| left.entries[p].kind == Kind::Directory)
            || changed_right
                .iter()
                .any(|p| right.entries[p].kind == Kind::Directory);
        if !root_ok || directory_changed || !changed_left.is_empty() || !changed_right.is_empty() {
            stats.reasons |= CHANGED;
        }
        for row in &mut rows {
            if !root_ok
                || changed_left.contains(&row.path)
                || changed_right.contains(&row.path)
                || directory_changed
                    && matches!(row.difference, Difference::OnlyLeft | Difference::OnlyRight)
            {
                row.difference = Difference::Unknown;
            }
            if !absent_known
                && row.left == Some(Kind::Directory)
                && row.right == Some(Kind::Directory)
            {
                row.difference = Difference::Unknown;
            }
        }
        // Directory rows summarize retained descendants as well as their own mode.
        // Parents precede children in this normalized relative-path ordering.
        let indices = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.path.clone(), i))
            .collect::<BTreeMap<_, _>>();
        for i in (0..rows.len()).rev() {
            let Some(parent) = rows[i].path.parent().and_then(|p| indices.get(p)).copied() else {
                continue;
            };
            let difference = rows[i].difference;
            let metadata_differs = rows[i].metadata_differs;
            let p = &mut rows[parent];
            if p.left == Some(Kind::Directory) && p.right == Some(Kind::Directory) {
                if difference == Difference::Unknown {
                    p.difference = Difference::Unknown;
                } else if p.difference != Difference::Unknown
                    && (difference != Difference::Equal || metadata_differs)
                {
                    p.difference = Difference::Children;
                }
            }
        }
        for row in &rows {
            match row.difference {
                Difference::Equal if !row.metadata_differs => stats.equal += 1,
                Difference::Unknown => stats.unknown += 1,
                _ => stats.different += 1,
            }
        }
        check(cancel)?;
        stats.phase = 3;
        progress(stats)?;
        check(cancel)?;
        Ok(Report {
            roots: [left.stamp.identity(), right.stamp.identity()],
            progress: stats,
            rows,
        })
    }
}
/// No mutation. The caller must supervise filesystem calls outside the UI thread.
pub fn compare(
    left: &Path,
    right: &Path,
    expected: Option<[DirectoryIdentity; 2]>,
    limits: Limits,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Report> {
    #[cfg(target_os = "linux")]
    {
        linux::compare([left, right], expected, limits, cancel, progress)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (left, right, expected, limits, cancel, progress);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Comparación de carpetas pendiente en esta plataforma",
        ))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::atomic::AtomicU64,
    };
    struct Fixture {
        base: PathBuf,
        left: PathBuf,
        right: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = crate::theme::home().join(".cache/files-test").join(format!(
                "directory-compare-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let left = base.join("left");
            let right = base.join("right");
            fs::create_dir_all(&left).unwrap();
            fs::create_dir(&right).unwrap();
            Self { base, left, right }
        }
        fn compare(&self, limits: Limits) -> Report {
            compare(
                &self.left,
                &self.right,
                None,
                limits,
                &AtomicBool::new(false),
                |_| Ok(()),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn row<'a>(report: &'a Report, path: &Path) -> &'a Row {
        report.rows.iter().find(|r| r.path == path).unwrap()
    }
    #[test]
    fn recursive_namespace_content_types_and_empty_directories() {
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::create_dir(root.join("nested")).unwrap();
            fs::create_dir(root.join("empty")).unwrap();
            fs::write(root.join("nested/same"), b"same").unwrap();
        }
        fs::write(f.left.join("same-size"), b"left").unwrap();
        fs::write(f.right.join("same-size"), b"diff").unwrap();
        fs::write(f.left.join("size"), b"1").unwrap();
        fs::write(f.right.join("size"), b"12").unwrap();
        fs::write(f.left.join("type"), b"data").unwrap();
        fs::create_dir(f.right.join("type")).unwrap();
        fs::write(f.left.join(".only-left"), b"hidden").unwrap();
        fs::write(f.right.join("only-right"), b"right").unwrap();
        let r = f.compare(Limits::default());
        assert!(r.progress.complete());
        assert_eq!(
            row(&r, Path::new("nested/same")).difference,
            Difference::Equal
        );
        assert_eq!(row(&r, Path::new("empty")).difference, Difference::Equal);
        assert_eq!(
            row(&r, Path::new("same-size")).difference,
            Difference::Content
        );
        assert_eq!(row(&r, Path::new("size")).difference, Difference::Content);
        assert_eq!(row(&r, Path::new("type")).difference, Difference::Type);
        assert_eq!(
            row(&r, Path::new(".only-left")).difference,
            Difference::OnlyLeft
        );
        assert_eq!(
            row(&r, Path::new("only-right")).difference,
            Difference::OnlyRight
        );
        assert_eq!(r.progress.bytes_read, 16);
        assert_eq!(
            r.progress.equal + r.progress.different + r.progress.unknown,
            r.rows.len() as u64
        );
        assert_eq!(fs::read(f.left.join("same-size")).unwrap(), b"left");
    }
    #[test]
    fn raw_paths_and_literal_symlinks_never_follow_targets() {
        let f = Fixture::new();
        let name = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        let target = PathBuf::from(std::ffi::OsString::from_vec(b"../external\xff\n".to_vec()));
        for root in [&f.left, &f.right] {
            fs::write(root.join(&name), b"raw").unwrap();
            symlink(&target, root.join("link")).unwrap();
        }
        symlink("/does-not-exist", f.left.join("different-link")).unwrap();
        symlink("/another-missing", f.right.join("different-link")).unwrap();
        fs::write(f.right.join("raw�\n"), b"shadow").unwrap();
        let r = f.compare(Limits::default());
        assert!(r.progress.complete());
        assert_eq!(row(&r, Path::new(&name)).difference, Difference::Equal);
        assert_eq!(row(&r, Path::new("link")).difference, Difference::Equal);
        assert_eq!(
            row(&r, Path::new("different-link")).difference,
            Difference::Link
        );
        assert_eq!(
            row(&r, Path::new("raw�\n")).difference,
            Difference::OnlyRight
        );
        let wire = serde_json::to_vec(&r).unwrap();
        let restored: Report = serde_json::from_slice(&wire).unwrap();
        assert_eq!(
            restored.rows.iter().map(|r| &r.path).collect::<Vec<_>>(),
            r.rows.iter().map(|r| &r.path).collect::<Vec<_>>()
        );
    }
    #[test]
    fn limits_never_turn_unknown_into_absence_or_equality() {
        let f = Fixture::new();
        fs::write(f.left.join("left"), b"1234").unwrap();
        fs::write(f.right.join("right"), b"1234").unwrap();
        let r = f.compare(Limits {
            names: 0,
            ..Limits::default()
        });
        assert!(!r.progress.complete());
        assert_ne!(r.progress.reasons & LIMIT, 0);
        fs::write(f.right.join("left"), b"1234").unwrap();
        let r = f.compare(Limits {
            read: 7,
            ..Limits::default()
        });
        assert_ne!(r.progress.reasons & READ_LIMIT, 0);
        assert_eq!(row(&r, Path::new("left")).difference, Difference::Unknown);
        assert_eq!(r.progress.bytes_read, 0);
        let r = f.compare(Limits {
            items: 1,
            ..Limits::default()
        });
        assert_ne!(r.progress.reasons & LIMIT, 0);
        assert!(
            r.rows
                .iter()
                .all(|r| !matches!(r.difference, Difference::OnlyLeft | Difference::OnlyRight))
        );
    }
    #[test]
    fn depth_bound_and_fifo_are_explicit_partial_without_opening_devices() {
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::create_dir(root.join("dir")).unwrap();
            fs::write(root.join("dir/file"), b"data").unwrap();
        }
        let r = f.compare(Limits {
            depth: 0,
            ..Limits::default()
        });
        assert_ne!(r.progress.reasons & LIMIT, 0);
        assert_eq!(r.rows.len(), 1);
        let fifo =
            std::ffi::CString::new(f.left.join("fifo").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let r = f.compare(Limits::default());
        assert_ne!(r.progress.reasons & UNSUPPORTED, 0);
        assert_eq!(row(&r, Path::new("fifo")).left, Some(Kind::Other));
    }
    #[test]
    fn cancellation_and_captured_root_identity_prevent_publication() {
        let f = Fixture::new();
        let cancel = AtomicBool::new(true);
        assert_eq!(
            compare(&f.left, &f.right, None, Limits::default(), &cancel, |_| Ok(
                ()
            ))
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        cancel.store(false, Ordering::Relaxed);
        let ids = [
            DirectoryIdentity::from_metadata(&fs::metadata(&f.left).unwrap()).unwrap(),
            DirectoryIdentity {
                device: 0,
                inode: 0,
            },
        ];
        assert!(
            compare(
                &f.left,
                &f.right,
                Some(ids),
                Limits::default(),
                &cancel,
                |_| Ok(())
            )
            .is_err()
        );
        for root in [&f.left, &f.right] {
            fs::write(root.join("file"), vec![b'x'; crate::checksum::CHUNK + 1]).unwrap();
        }
        let result = compare(&f.left, &f.right, None, Limits::default(), &cancel, |p| {
            if p.bytes_read > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
    #[test]
    fn changes_after_inventory_revoke_stale_files_and_absence() {
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::write(root.join("same"), b"same").unwrap();
            fs::create_dir(root.join("dir")).unwrap();
        }
        fs::write(f.left.join("only"), b"left").unwrap();
        let mut edited = false;
        let r = compare(
            &f.left,
            &f.right,
            None,
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.phase == 2 && !edited {
                    edited = true;
                    fs::write(f.right.join("same"), b"diff")?;
                    fs::write(f.right.join("dir/new"), b"new")?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_ne!(r.progress.reasons & CHANGED, 0);
        assert!(!r.progress.complete());
        assert_eq!(row(&r, Path::new("same")).difference, Difference::Unknown);
        assert_eq!(row(&r, Path::new("only")).difference, Difference::Unknown);
    }
    #[test]
    fn root_replacement_is_not_the_captured_tree() {
        let f = Fixture::new();
        fs::write(f.left.join("only"), b"left").unwrap();
        let mut replaced = false;
        let r = compare(
            &f.left,
            &f.right,
            None,
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.phase == 2 && !replaced {
                    replaced = true;
                    fs::rename(&f.right, f.base.join("retained"))?;
                    fs::create_dir(&f.right)?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_ne!(r.progress.reasons & CHANGED, 0);
        assert!(r.rows.iter().all(|r| r.difference == Difference::Unknown));
    }
    #[test]
    fn source_mode_differences_are_separate_from_content() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::write(root.join("file"), b"same").unwrap();
        }
        fs::set_permissions(f.left.join("file"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(f.right.join("file"), fs::Permissions::from_mode(0o644)).unwrap();
        let r = f.compare(Limits::default());
        let row = row(&r, Path::new("file"));
        assert_eq!(row.difference, Difference::Equal);
        assert!(row.metadata_differs);
        assert!(r.progress.complete());
        assert_eq!(r.progress.different, 1);
    }
    #[test]
    fn directory_rows_aggregate_children_and_hardlink_names_remain_distinct() {
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::create_dir(root.join("parent")).unwrap();
            fs::write(root.join("parent/child"), b"data").unwrap();
            fs::hard_link(root.join("parent/child"), root.join("alias")).unwrap();
        }
        fs::write(f.right.join("parent/child"), b"DIFF").unwrap();
        let r = f.compare(Limits::default());
        assert!(r.progress.complete());
        assert_eq!(
            row(&r, Path::new("parent")).difference,
            Difference::Children
        );
        assert_eq!(
            row(&r, Path::new("parent/child")).difference,
            Difference::Content
        );
        assert_eq!(row(&r, Path::new("alias")).difference, Difference::Content);
        let r = f.compare(Limits {
            read: 0,
            ..Limits::default()
        });
        assert_eq!(row(&r, Path::new("parent")).difference, Difference::Unknown);
    }
    #[test]
    fn symlink_directory_and_root_symlink_are_never_traversed() {
        let f = Fixture::new();
        let outside = f.base.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("secret"), b"not compared").unwrap();
        for root in [&f.left, &f.right] {
            symlink(&outside, root.join("link")).unwrap();
        }
        let r = f.compare(Limits::default());
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.progress.bytes_read, 0);
        assert_eq!(r.rows[0].difference, Difference::Equal);
        symlink(&f.left, f.base.join("root-link")).unwrap();
        assert!(
            compare(
                &f.base.join("root-link"),
                &f.right,
                None,
                Limits::default(),
                &AtomicBool::new(false),
                |_| Ok(())
            )
            .is_err()
        );
    }
    #[test]
    fn final_revalidation_detects_changes_after_both_hashes() {
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::write(root.join("file"), b"same").unwrap();
        }
        let mut edited = false;
        let r = compare(
            &f.left,
            &f.right,
            None,
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.phase == 2 && p.compared == 1 && !edited {
                    edited = true;
                    fs::write(f.left.join("file"), b"DIFF")?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(r.rows[0].difference, Difference::Unknown);
        assert_ne!(r.progress.reasons & CHANGED, 0);
    }
    #[test]
    fn callback_errors_are_not_swallowed_as_filesystem_changes() {
        let f = Fixture::new();
        for root in [&f.left, &f.right] {
            fs::write(root.join("file"), b"same").unwrap();
        }
        let mut failed = false;
        let result = compare(
            &f.left,
            &f.right,
            None,
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.bytes_read > 0 && !failed {
                    failed = true;
                    Err(io::Error::new(io::ErrorKind::BrokenPipe, "consumer gone"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }
}
