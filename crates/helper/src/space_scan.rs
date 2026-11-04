//! Bounded apparent-byte scan for a treemap, not a disk-block ownership report.
//! Only immediate child names/results are retained. Directory streams and stat /
//! openat calls are anchored to pinned FDs; final symlinks are never followed.
//! Symlinks/special objects contribute zero bytes and are not traversed. Hardlink
//! aliases are counted per regular-file name. Concurrent in-place changes and
//! mount traversal are not a snapshot; cooperative timeouts cannot stop an FS
//! syscall. The caller must supervise/kill/reap this one-shot helper off the UI.
//! Node.files counts observed regular-file names; Node.directories includes that
//! immediate directory itself. visited counts encountered non-dot entries, not
//! the scan root. omitted_children refers only to unrepresented immediate
//! children: Some(0) requires EOF on the root stream, even if a listed subtree
//! was partial. None is used whenever enumeration stopped before proving EOF.
use files_core::space_map::{
    MAX_ERROR_BYTES, MAX_ERRORS_BYTES, MAX_NAME_BYTES, MAX_NAMES_BYTES, MAX_NODES, NodeKind,
    NodeStatus, SpaceNode, SpaceReport,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
struct Limits {
    visited: u64,
    depth: usize,
    elapsed: Duration,
    nodes: usize,
    names: usize,
    errors: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            visited: 1_000_000,
            depth: 128,
            elapsed: Duration::from_secs(15),
            nodes: MAX_NODES,
            names: MAX_NAMES_BYTES,
            errors: MAX_ERRORS_BYTES,
        }
    }
}
pub fn scan(path: &Path) -> Result<SpaceReport, String> {
    platform::scan(path, Limits::default())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        ffi::{CStr, CString},
        fmt::{self, Write},
        fs::File,
        io,
        os::{
            fd::{AsRawFd, FromRawFd, IntoRawFd},
            unix::ffi::OsStrExt,
        },
        ptr::NonNull,
    };
    struct Stream {
        directory: NonNull<libc::DIR>,
    }
    impl Stream {
        fn from_file(file: File) -> io::Result<Self> {
            let fd = file.into_raw_fd();
            let directory = unsafe { libc::fdopendir(fd) };
            match NonNull::new(directory) {
                Some(directory) => Ok(Self { directory }),
                None => {
                    let error = io::Error::last_os_error();
                    unsafe {
                        libc::close(fd);
                    }
                    Err(error)
                }
            }
        }
        fn fd(&self) -> i32 {
            unsafe { libc::dirfd(self.directory.as_ptr()) }
        }
        fn next(&mut self) -> io::Result<Option<&CStr>> {
            loop {
                unsafe {
                    *libc::__errno_location() = 0;
                }
                let entry = unsafe { libc::readdir(self.directory.as_ptr()) };
                if entry.is_null() {
                    let error = io::Error::last_os_error();
                    return if error.raw_os_error() == Some(0) {
                        Ok(None)
                    } else {
                        Err(error)
                    };
                }
                // Linux readdir returns a NUL-terminated name in its stream-owned
                // buffer. Keep it borrowed until the next call on THIS stream.
                let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
                if !matches!(name.to_bytes(), b"." | b"..") {
                    return Ok(Some(name));
                }
            }
        }
    }
    impl Drop for Stream {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.directory.as_ptr());
            }
        }
    }
    fn file(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn stat_fd(file: &File) -> io::Result<libc::stat> {
        let mut out = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(file.as_raw_fd(), out.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { out.assume_init() })
    }
    fn stat_at(fd: i32, name: &CStr) -> io::Result<libc::stat> {
        let mut out = std::mem::MaybeUninit::uninit();
        if unsafe {
            libc::fstatat(
                fd,
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
    fn child_directory(fd: i32, name: &CStr, expected: &libc::stat) -> io::Result<Stream> {
        let file = file(unsafe {
            libc::openat(
                fd,
                name.as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK,
            )
        })?;
        let actual = stat_fd(&file)?;
        if expected.st_dev != actual.st_dev
            || expected.st_ino != actual.st_ino
            || actual.st_mode & libc::S_IFMT != libc::S_IFDIR
        {
            return Err(io::Error::other(
                "La carpeta cambió antes de abrirse; recorrido parcial",
            ));
        }
        Stream::from_file(file)
    }
    fn kind(stat: &libc::stat) -> NodeKind {
        match stat.st_mode & libc::S_IFMT {
            libc::S_IFREG => NodeKind::File,
            libc::S_IFDIR => NodeKind::Directory,
            libc::S_IFLNK => NodeKind::Symlink,
            _ => NodeKind::Other,
        }
    }
    struct Text {
        text: String,
        cap: usize,
    }
    impl fmt::Write for Text {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            let remaining = self.cap.saturating_sub(self.text.len());
            if value.len() <= remaining {
                self.text.push_str(value);
                return Ok(());
            }
            let mut end = remaining;
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.text.push_str(&value[..end]);
            Err(fmt::Error)
        }
    }
    fn text(arguments: fmt::Arguments<'_>, cap: usize) -> String {
        let mut text = Text {
            text: String::with_capacity(cap),
            cap,
        };
        let _ = text.write_fmt(arguments);
        text.text
    }
    struct Budget {
        limits: Limits,
        started: Instant,
        visited: u64,
        names: usize,
        errors_reserved: usize,
    }
    impl Budget {
        fn new(limits: Limits) -> Self {
            // Reserve one bounded reason for the report BEFORE node diagnostics.
            Self {
                limits,
                started: Instant::now(),
                visited: 0,
                names: 0,
                errors_reserved: MAX_ERROR_BYTES.min(limits.errors),
            }
        }
        fn time(&self) -> Result<(), Issue> {
            if self.started.elapsed() >= self.limits.elapsed {
                Err(Issue::limit("Tiempo de cálculo de espacio agotado"))
            } else {
                Ok(())
            }
        }
        fn visit(&mut self) -> Result<(), Issue> {
            if self.visited >= self.limits.visited {
                Err(Issue::limit("Límite de entradas de espacio alcanzado"))
            } else {
                self.visited += 1;
                Ok(())
            }
        }
        fn name(&mut self, name: &[u8]) -> Result<(), Issue> {
            if name.is_empty() || name.len() > MAX_NAME_BYTES {
                return Err(Issue::limit(
                    "Nombre inmediato fuera del límite de4096bytes",
                ));
            }
            self.names = self
                .names
                .checked_add(name.len())
                .filter(|n| *n <= self.limits.names)
                .ok_or_else(|| {
                    Issue::limit("Presupuesto agregado de nombres inmediatos agotado")
                })?;
            Ok(())
        }
        fn error(&mut self, issue: &Issue) -> Option<String> {
            let cap = self
                .limits
                .errors
                .saturating_sub(self.errors_reserved)
                .min(MAX_ERROR_BYTES);
            if cap == 0 {
                return None;
            }
            self.errors_reserved += cap; // account BEFORE allocating/formatting
            Some(text(format_args!("{}: {}", issue.reason, issue.error), cap))
        }
    }
    struct Issue {
        reason: &'static str,
        error: io::Error,
        stop_all: bool,
    }
    impl Issue {
        fn limit(reason: &'static str) -> Self {
            Self {
                reason,
                error: io::Error::other(reason),
                stop_all: true,
            }
        }
        fn io(reason: &'static str, error: io::Error) -> Self {
            Self {
                reason,
                error,
                stop_all: false,
            }
        }
    }
    #[derive(Default)]
    struct Counts {
        bytes: u64,
        files: u64,
        directories: u64,
    }
    fn file_bytes(stat: &libc::stat, counts: &mut Counts) -> Result<(), Issue> {
        let bytes = u64::try_from(stat.st_size).map_err(|_| {
            Issue::io(
                "Tamaño aparente no válido",
                io::Error::other("Tamaño negativo"),
            )
        })?;
        counts.bytes = counts.bytes.checked_add(bytes).ok_or_else(|| {
            Issue::io(
                "Tamaño aparente fuera de rango",
                io::Error::other("Desbordamiento de bytes"),
            )
        })?;
        counts.files += 1;
        Ok(())
    }
    fn walk(mut stack: Vec<Stream>, budget: &mut Budget, counts: &mut Counts) -> Result<(), Issue> {
        if budget.limits.depth == 0 {
            return Err(Issue::limit("Límite de profundidad de espacio alcanzado"));
        }
        while !stack.is_empty() {
            budget.time()?;
            let depth = stack.len();
            let frame = stack.last_mut().expect("nonempty traversal stack");
            let fd = frame.fd();
            let Some(name) = frame
                .next()
                .map_err(|e| Issue::io("No se pudo enumerar una subcarpeta", e))?
            else {
                stack.pop();
                continue;
            };
            budget.visit()?;
            if name.to_bytes().len() > MAX_NAME_BYTES {
                return Err(Issue::limit("Nombre de subcarpeta fuera del límite"));
            }
            let stat = stat_at(fd, name)
                .map_err(|e| Issue::io("No se pudo leer una entrada de subcarpeta", e))?;
            match kind(&stat) {
                NodeKind::File => file_bytes(&stat, counts)?,
                NodeKind::Directory => {
                    counts.directories += 1;
                    if depth >= budget.limits.depth {
                        return Err(Issue::limit("Límite de profundidad de espacio alcanzado"));
                    }
                    let child = child_directory(fd, name, &stat)
                        .map_err(|e| Issue::io("No se pudo abrir una subcarpeta", e))?;
                    stack.push(child);
                }
                NodeKind::Symlink | NodeKind::Other => (),
            }
        }
        Ok(())
    }
    fn entry(fd: i32, name: &CStr, budget: &mut Budget) -> (SpaceNode, Option<Issue>) {
        // The immediate-name budget was checked by the caller before this clone.
        let mut node = SpaceNode {
            name: name.to_bytes().to_vec(),
            kind: NodeKind::Other,
            bytes: None,
            observed_bytes: 0,
            files: 0,
            directories: 0,
            status: NodeStatus::Error,
            error: None,
        };
        let stat = match stat_at(fd, name) {
            Ok(stat) => stat,
            Err(error) => {
                return (
                    node,
                    Some(Issue::io("No se pudo leer la entrada inmediata", error)),
                );
            }
        };
        node.kind = kind(&stat);
        let mut counts = Counts::default();
        let result = match node.kind {
            NodeKind::File => file_bytes(&stat, &mut counts),
            NodeKind::Directory => {
                counts.directories = 1;
                match child_directory(fd, name, &stat) {
                    Ok(directory) => walk(vec![directory], budget, &mut counts),
                    Err(error) => {
                        node.directories = 1;
                        return (
                            node,
                            Some(Issue::io("No se pudo abrir la carpeta inmediata", error)),
                        );
                    }
                }
            }
            NodeKind::Symlink | NodeKind::Other => Ok(()),
        };
        node.observed_bytes = counts.bytes;
        node.files = counts.files;
        node.directories = counts.directories;
        match result {
            Ok(()) => {
                node.status = NodeStatus::Complete;
                node.bytes = Some(counts.bytes);
                (node, None)
            }
            Err(issue) => {
                node.status = NodeStatus::Partial;
                (node, Some(issue))
            }
        }
    }
    fn reason(report: &mut SpaceReport, issue: &Issue) {
        report.complete = false;
        if report.reason.is_none() {
            report.reason = Some(text(
                format_args!("{}: {}", issue.reason, issue.error),
                MAX_ERROR_BYTES,
            ));
        }
    }
    pub(super) fn scan(path: &Path, mut limits: Limits) -> Result<SpaceReport, String> {
        limits.visited = limits.visited.min(1_000_000);
        limits.depth = limits.depth.min(128);
        limits.elapsed = limits.elapsed.min(Duration::from_secs(15));
        limits.nodes = limits.nodes.min(MAX_NODES);
        limits.names = limits.names.min(MAX_NAMES_BYTES);
        limits.errors = limits.errors.min(MAX_ERRORS_BYTES);
        let bytes = path.as_os_str().as_bytes();
        if !path.is_absolute() || bytes.len() > 4096 || bytes.contains(&0) {
            return Err(
                "La vista Espacio requiere una ruta absoluta de hasta4096bytes, sin NUL".into(),
            );
        }
        // Remove trailing separators so O_NOFOLLOW applies to the named root
        // itself: a path ending in "symlink/" must not bypass the final-link
        // rejection. Ancestor symlinks retain normal path-resolution semantics.
        let mut end = bytes.len();
        while end > 1 && bytes[end - 1] == b'/' {
            end -= 1;
        }
        let path = CString::new(&bytes[..end]).map_err(|_| "NUL en ruta de espacio".to_string())?;
        let mut budget = Budget::new(limits);
        let root = file(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK,
            )
        })
        .map_err(|error| {
            text(
                format_args!("No se pudo abrir una carpeta real para Espacio: {error}"),
                MAX_ERROR_BYTES,
            )
        })?;
        let mut root = Stream::from_file(root).map_err(|error| {
            text(
                format_args!("No se pudo enumerar Espacio: {error}"),
                MAX_ERROR_BYTES,
            )
        })?;
        let mut report = SpaceReport {
            nodes: Vec::new(),
            total_bytes: 0,
            observed_bytes: 0,
            visited: 0,
            complete: true,
            reason: None,
            omitted_children: None,
        };
        loop {
            if let Err(issue) = budget.time() {
                reason(&mut report, &issue);
                break;
            }
            let fd = root.fd();
            let name = match root.next() {
                Ok(Some(name)) => name,
                Ok(None) => {
                    report.omitted_children = Some(0);
                    break;
                }
                Err(error) => {
                    reason(
                        &mut report,
                        &Issue::io("No se pudo enumerar la carpeta raíz", error),
                    );
                    break;
                }
            };
            if report.nodes.len() >= budget.limits.nodes {
                reason(
                    &mut report,
                    &Issue::limit("Límite de hijos inmediatos de Espacio alcanzado"),
                );
                break;
            }
            if let Err(issue) = budget.name(name.to_bytes()).and_then(|()| budget.visit()) {
                reason(&mut report, &issue);
                break;
            }
            let (mut node, issue) = entry(fd, name, &mut budget);
            let mut stop = false;
            if let Some(issue) = issue {
                reason(&mut report, &issue);
                node.error = budget.error(&issue);
                if node.error.is_none() {
                    report.reason = Some("Presupuesto agregado de diagnósticos de Espacio agotado; recorrido detenido".into());
                }
                stop = issue.stop_all || node.error.is_none();
            }
            let Some(observed) = report.observed_bytes.checked_add(node.observed_bytes) else {
                reason(
                    &mut report,
                    &Issue::limit("Suma observada de espacio fuera de rango"),
                );
                break;
            };
            let Some(total) = report.total_bytes.checked_add(node.bytes.unwrap_or(0)) else {
                reason(
                    &mut report,
                    &Issue::limit("Suma completa de espacio fuera de rango"),
                );
                break;
            };
            report.observed_bytes = observed;
            report.total_bytes = total;
            report.nodes.push(node);
            if stop {
                break;
            }
        }
        report.visited = budget.visited;
        Ok(report)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            fs,
            os::unix::{ffi::OsStringExt, fs::symlink},
            sync::atomic::{AtomicU64, Ordering},
        };
        struct Fixture(std::path::PathBuf);
        impl Fixture {
            fn new() -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let base = files_core::theme::home().join(".cache/files-test");
                fs::create_dir_all(&base).unwrap();
                let path = base.join(format!(
                    "space-scan-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fn defaults(path: &Path) -> SpaceReport {
            scan(path, Limits::default()).unwrap()
        }
        #[test]
        fn true_weights_empty_and_lossless_names() {
            let f = Fixture::new();
            fs::write(f.0.join("eight"), [0; 8]).unwrap();
            fs::create_dir(f.0.join("four")).unwrap();
            fs::write(f.0.join("four/data"), [0; 4]).unwrap();
            let raw = std::ffi::OsString::from_vec(b"two\xff".to_vec());
            fs::write(f.0.join(&raw), [0; 2]).unwrap();
            fs::create_dir(f.0.join("empty")).unwrap();
            let report = defaults(&f.0);
            assert!(report.complete);
            assert_eq!((report.total_bytes, report.observed_bytes), (14, 14));
            assert_eq!(report.omitted_children, Some(0));
            assert_eq!(report.visited, 5);
            for (name, bytes) in [
                (b"eight".as_slice(), 8),
                (b"four", 4),
                (b"two\xff", 2),
                (b"empty", 0),
            ] {
                let node = report.nodes.iter().find(|node| node.name == name).unwrap();
                assert_eq!(node.bytes, Some(bytes));
                assert_eq!(node.observed_bytes, bytes);
                assert!(matches!(node.status, NodeStatus::Complete));
            }
        }
        #[test]
        fn no_symlink_cycles_external_follow_or_hardlink_deduplication() {
            let f = Fixture::new();
            let outside = Fixture::new();
            fs::write(outside.0.join("secret"), [0; 100]).unwrap();
            fs::write(f.0.join("file"), [0; 7]).unwrap();
            fs::hard_link(f.0.join("file"), f.0.join("alias")).unwrap();
            symlink(&outside.0, f.0.join("external")).unwrap();
            symlink(&f.0, f.0.join("cycle")).unwrap();
            let report = defaults(&f.0);
            assert!(report.complete);
            assert_eq!(report.total_bytes, 14);
            for node in &report.nodes {
                if matches!(node.kind, NodeKind::Symlink) {
                    assert_eq!(node.bytes, Some(0));
                    assert_eq!(node.files, 0);
                }
            }
            assert!(scan(&f.0.join("external"), Limits::default()).is_err());
            let trailing = f.0.join("external").join("");
            assert!(scan(&trailing, Limits::default()).is_err());
        }
        #[test]
        fn empty_directory_is_complete_but_zero_time_is_not() {
            let f = Fixture::new();
            let report = defaults(&f.0);
            assert!(report.complete && report.nodes.is_empty());
            assert_eq!(report.omitted_children, Some(0));
            let report = scan(
                &f.0,
                Limits {
                    elapsed: Duration::ZERO,
                    ..Limits::default()
                },
            )
            .unwrap();
            assert!(!report.complete);
            assert!(report.reason.is_some());
            assert_eq!(report.omitted_children, None);
        }
        #[test]
        fn depth_and_visit_caps_never_promote_partial_bytes_to_complete() {
            let f = Fixture::new();
            fs::create_dir(f.0.join("child")).unwrap();
            fs::create_dir(f.0.join("child/deep")).unwrap();
            fs::write(f.0.join("child/deep/data"), [0; 20]).unwrap();
            for limits in [
                Limits {
                    depth: 1,
                    ..Limits::default()
                },
                Limits {
                    visited: 1,
                    ..Limits::default()
                },
            ] {
                let report = scan(&f.0, limits).unwrap();
                assert!(!report.complete);
                assert!(report.reason.is_some());
                assert_eq!(report.nodes.len(), 1);
                assert_eq!(report.nodes[0].bytes, None);
                assert!(matches!(report.nodes[0].status, NodeStatus::Partial));
                assert_eq!(report.total_bytes, 0);
                assert_eq!(report.omitted_children, None);
            }
        }
        #[test]
        fn name_and_node_caps_stop_without_claiming_zero_omissions() {
            let f = Fixture::new();
            fs::write(f.0.join("one"), [0; 1]).unwrap();
            fs::write(f.0.join("two"), [0; 2]).unwrap();
            for limits in [
                Limits {
                    nodes: 1,
                    ..Limits::default()
                },
                Limits {
                    names: 2,
                    ..Limits::default()
                },
            ] {
                let report = scan(&f.0, limits).unwrap();
                assert!(!report.complete);
                assert!(report.reason.is_some());
                assert_eq!(report.omitted_children, None);
                assert!(report.nodes.len() <= 1);
            }
        }
        #[test]
        fn stat_failure_and_directory_replacement_are_errors_not_measured_totals() {
            let f = Fixture::new();
            fs::create_dir(f.0.join("child")).unwrap();
            let root = Stream::from_file(fs::File::open(&f.0).unwrap()).unwrap();
            let before = stat_at(root.fd(), c"child").unwrap();
            fs::rename(f.0.join("child"), f.0.join("moved-original")).unwrap();
            fs::create_dir(f.0.join("child")).unwrap();
            assert!(child_directory(root.fd(), c"child", &before).is_err());
            let mut budget = Budget::new(Limits::default());
            let (node, issue) = entry(-1, c"fixture-missing", &mut budget);
            assert!(matches!(node.status, NodeStatus::Error));
            assert!(node.bytes.is_none());
            assert_eq!(node.observed_bytes, 0);
            let issue = issue.unwrap();
            assert!(budget.error(&issue).is_some());
            fs::write(f.0.join("regular"), b"file").unwrap();
            assert!(scan(&f.0.join("regular"), Limits::default()).is_err());
        }
        #[test]
        fn errors_are_bounded_before_formatting() {
            let mut budget = Budget::new(Limits {
                errors: MAX_ERROR_BYTES * 2,
                ..Limits::default()
            });
            let issue = Issue::io("Error", io::Error::other("漢".repeat(5000)));
            let message = budget.error(&issue).unwrap();
            assert!(message.len() <= MAX_ERROR_BYTES);
            assert!(budget.error(&issue).is_none());
        }
    }
}
#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    pub(super) fn scan(_: &Path, _: Limits) -> Result<SpaceReport, String> {
        Err("El recorrido de Espacio por descriptores está pendiente en esta plataforma".into())
    }
}
