//! Permanent deletion (Shift+Delete, emptying the trash).
//!
//! Every directory is opened relative to its pinned parent with O_NOFOLLOW,
//! its identity (device, inode) is compared with what `fstatat` reported
//! before opening, and entries are unlinked through that descriptor, so a
//! concurrent rename or symlink swap can never redirect the deletion outside
//! the selected tree. Symlinks are removed, never followed. A directory on
//! another device (a mount point inside the tree) is not entered: it is
//! reported and its parent is kept. The walk is iterative (no recursion
//! depth limit) and checks the cancel flag between entries; a cancelled
//! deletion leaves whatever was not yet removed.
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Live counters for the UI.
#[derive(Debug, Default)]
pub struct Progress {
    pub entries: AtomicU64,
    pub bytes: AtomicU64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Files, links and directories removed.
    pub removed: u64,
    /// Bytes of regular files removed (apparent size).
    pub bytes: u64,
    /// First failures (path, reason); `failed` counts all of them.
    pub failures: Vec<(PathBuf, String)>,
    pub failed: u64,
    /// Directories on another filesystem that were left in place.
    pub mounts_skipped: u64,
    pub cancelled: bool,
}

const MAX_FAILURES: usize = 32;

impl Report {
    fn fail(&mut self, path: &Path, error: impl ToString) {
        self.failed += 1;
        if self.failures.len() < MAX_FAILURES {
            self.failures.push((path.to_path_buf(), error.to_string()));
        }
    }
    pub fn complete(&self) -> bool {
        !self.cancelled && self.failed == 0 && self.mounts_skipped == 0
    }
    pub fn merge(&mut self, other: Report) {
        self.removed += other.removed;
        self.bytes += other.bytes;
        self.failed += other.failed;
        self.mounts_skipped += other.mounts_skipped;
        self.cancelled |= other.cancelled;
        for f in other.failures {
            if self.failures.len() < MAX_FAILURES {
                self.failures.push(f);
            }
        }
    }
    /// One line for the status bar.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{} elementos eliminados ({})",
            self.removed,
            crate::format_size(self.bytes)
        );
        if self.cancelled {
            s.push_str(" · cancelado");
        }
        if self.mounts_skipped > 0 {
            s.push_str(&format!(" · {} montajes conservados", self.mounts_skipped));
        }
        if self.failed > 0 {
            s.push_str(&format!(" · {} errores", self.failed));
            if let Some((path, reason)) = self.failures.first() {
                s.push_str(&format!(
                    ": {}: {reason}",
                    crate::name_label::label(path.as_os_str())
                ));
            }
        }
        s
    }
}

/// Removes each absolute path (file, link or whole directory tree).
pub fn remove_paths(paths: &[PathBuf], cancel: &AtomicBool, progress: &Progress) -> Report {
    let mut report = Report::default();
    for path in paths {
        if cancel.load(Ordering::Acquire) {
            report.cancelled = true;
            break;
        }
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            report.fail(path, "ruta sin carpeta padre");
            continue;
        };
        if !path.is_absolute() || name == OsStr::new("..") {
            report.fail(path, "ruta no absoluta");
            continue;
        }
        match platform::open_dir(parent) {
            Ok(dir) => platform::remove_at(&dir, name, parent, cancel, progress, &mut report),
            Err(e) => report.fail(path, e),
        }
    }
    report
}

#[cfg(target_os = "linux")]
pub(crate) mod platform {
    use super::*;
    use std::{
        ffi::CString,
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };

    fn c(name: &OsStr) -> io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|_| io::Error::other("NUL en el nombre"))
    }
    fn stat_at(parent: &File, name: &OsStr) -> io::Result<libc::stat> {
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
        Ok(unsafe { out.assume_init() })
    }
    fn fstat(file: &File) -> io::Result<libc::stat> {
        let mut out = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(file.as_raw_fd(), out.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { out.assume_init() })
    }
    fn open_child_dir(parent: &File, name: &OsStr) -> io::Result<File> {
        let name = c(name)?;
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
    fn unlink_at(parent: &File, name: &OsStr, dir: bool) -> io::Result<()> {
        let name = c(name)?;
        let flags = if dir { libc::AT_REMOVEDIR } else { 0 };
        if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), flags) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    /// Names in a pinned directory (through its descriptor, not its path).
    fn names(dir: &File) -> io::Result<Vec<OsString>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))? {
            out.push(entry?.file_name());
        }
        Ok(out)
    }
    pub fn open_dir(path: &Path) -> io::Result<File> {
        let p = c(path.as_os_str())?;
        let fd = unsafe {
            libc::open(
                p.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }

    struct Frame {
        dir: File,
        /// Name of `dir` inside the previous frame's directory.
        name: OsString,
        path: PathBuf,
        pending: Vec<OsString>,
        /// A child could not be removed: keep this directory.
        keep: bool,
    }

    /// Removes `name` inside `parent` (whose path is `parent_path`).
    pub fn remove_at(
        parent: &File,
        name: &OsStr,
        parent_path: &Path,
        cancel: &AtomicBool,
        progress: &Progress,
        report: &mut Report,
    ) {
        let path = parent_path.join(name);
        let st = match stat_at(parent, name) {
            Ok(st) => st,
            Err(e) => return report.fail(&path, e),
        };
        if st.st_mode & libc::S_IFMT != libc::S_IFDIR {
            match unlink_at(parent, name, false) {
                Ok(()) => {
                    report.removed += 1;
                    if st.st_mode & libc::S_IFMT == libc::S_IFREG {
                        report.bytes += st.st_size as u64;
                        progress
                            .bytes
                            .fetch_add(st.st_size as u64, Ordering::Relaxed);
                    }
                    progress.entries.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => report.fail(&path, e),
            }
            return;
        }
        let parent_dev = match fstat(parent) {
            Ok(p) => p.st_dev,
            Err(e) => return report.fail(&path, e),
        };
        if st.st_dev != parent_dev {
            report.mounts_skipped += 1;
            return;
        }
        let dir = match open_pinned(parent, name, &st) {
            Ok(d) => d,
            Err(e) => return report.fail(&path, e),
        };
        let pending = match names(&dir) {
            Ok(n) => n,
            Err(e) => return report.fail(&path, e),
        };
        let mut stack = vec![Frame {
            dir,
            name: name.to_os_string(),
            path,
            pending,
            keep: false,
        }];
        let device = st.st_dev;
        while let Some(top) = stack.last_mut() {
            if cancel.load(Ordering::Acquire) {
                report.cancelled = true;
                return;
            }
            let Some(child) = top.pending.pop() else {
                // Directory emptied (or a child was kept): remove it from its parent.
                let frame = stack.pop().expect("frame");
                let parent_dir: &File = match stack.last() {
                    Some(f) => &f.dir,
                    None => parent,
                };
                if frame.keep {
                    if let Some(up) = stack.last_mut() {
                        up.keep = true;
                    }
                    continue;
                }
                match unlink_at(parent_dir, &frame.name, true) {
                    Ok(()) => {
                        report.removed += 1;
                        progress.entries.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        report.fail(&frame.path, e);
                        if let Some(up) = stack.last_mut() {
                            up.keep = true;
                        }
                    }
                }
                continue;
            };
            let child_path = top.path.join(&child);
            let cst = match stat_at(&top.dir, &child) {
                Ok(s) => s,
                Err(e) => {
                    report.fail(&child_path, e);
                    top.keep = true;
                    continue;
                }
            };
            if cst.st_mode & libc::S_IFMT == libc::S_IFDIR {
                if cst.st_dev != device {
                    report.mounts_skipped += 1;
                    top.keep = true;
                    continue;
                }
                let opened = open_pinned(&top.dir, &child, &cst).and_then(|d| {
                    let n = names(&d)?;
                    Ok((d, n))
                });
                match opened {
                    Ok((dir, pending)) => stack.push(Frame {
                        dir,
                        name: child,
                        path: child_path,
                        pending,
                        keep: false,
                    }),
                    Err(e) => {
                        report.fail(&child_path, e);
                        top.keep = true;
                    }
                }
            } else {
                match unlink_at(&top.dir, &child, false) {
                    Ok(()) => {
                        report.removed += 1;
                        if cst.st_mode & libc::S_IFMT == libc::S_IFREG {
                            report.bytes += cst.st_size as u64;
                            progress
                                .bytes
                                .fetch_add(cst.st_size as u64, Ordering::Relaxed);
                        }
                        progress.entries.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        report.fail(&child_path, e);
                        top.keep = true;
                    }
                }
            }
        }
    }

    /// Opens a child directory and checks it is still the one `fstatat` saw.
    fn open_pinned(parent: &File, name: &OsStr, seen: &libc::stat) -> io::Result<File> {
        let dir = open_child_dir(parent, name)?;
        let now = fstat(&dir)?;
        if now.st_dev != seen.st_dev || now.st_ino != seen.st_ino {
            return Err(io::Error::other(
                "la carpeta fue reemplazada durante el borrado",
            ));
        }
        Ok(dir)
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) mod platform {
    use super::*;
    pub struct Dir;
    pub fn open_dir(_: &Path) -> io::Result<Dir> {
        Err(io::Error::other(
            "Borrado permanente pendiente en esta plataforma",
        ))
    }
    pub fn remove_at(_: &Dir, _: &OsStr, p: &Path, _: &AtomicBool, _: &Progress, r: &mut Report) {
        r.fail(p, "Borrado permanente pendiente en esta plataforma");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;

    fn base(name: &str) -> PathBuf {
        let d = crate::persistence::test_dir(name);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn removes_files_links_and_deep_trees_without_following_links() {
        let d = base("remove-tree-basic");
        let outside = d.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        let tree = d.join("tree");
        let mut deep = tree.clone();
        for i in 0..300 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("leaf"), b"12345").unwrap();
        std::os::unix::fs::symlink(&outside, tree.join("link-to-outside")).unwrap();
        fs::write(d.join("single.txt"), b"abc").unwrap();
        let progress = Progress::default();
        let report = remove_paths(
            &[tree.clone(), d.join("single.txt")],
            &AtomicBool::new(false),
            &progress,
        );
        assert!(report.complete(), "{report:?}");
        assert!(!tree.exists() && !d.join("single.txt").exists());
        // The symlink was removed, its target untouched.
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(report.bytes, 8);
        // 300 dirs + tree + leaf + link + single
        assert_eq!(report.removed, 304);
        assert_eq!(progress.entries.load(Ordering::Relaxed), 304);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cancel_and_failures_keep_what_was_not_removed() {
        let d = base("remove-tree-cancel");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("a/b/f"), b"x").unwrap();
        let cancelled = remove_paths(&[d.join("a")], &AtomicBool::new(true), &Progress::default());
        assert!(cancelled.cancelled && d.join("a/b/f").exists());
        let missing = remove_paths(
            &[d.join("nope")],
            &AtomicBool::new(false),
            &Progress::default(),
        );
        assert_eq!(missing.failed, 1);
        assert!(!missing.complete());
        assert!(missing.summary().contains("1 errores"));
        // A read-only directory keeps its child and itself, the rest goes.
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(d.join("ro/locked")).unwrap();
        fs::write(d.join("ro/locked/f"), b"x").unwrap();
        fs::write(d.join("ro/free"), b"y").unwrap();
        fs::set_permissions(d.join("ro/locked"), fs::Permissions::from_mode(0o500)).unwrap();
        let r = remove_paths(
            &[d.join("ro")],
            &AtomicBool::new(false),
            &Progress::default(),
        );
        let root = unsafe { libc::geteuid() } == 0;
        if !root {
            assert!(r.failed >= 1, "{r:?}");
            assert!(d.join("ro/locked/f").exists() && !d.join("ro/free").exists());
            fs::set_permissions(d.join("ro/locked"), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let _ = fs::remove_dir_all(&d);
    }
}
