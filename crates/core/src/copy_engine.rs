//! Bounded local copy worker; publication is no-clobber and per selected root.
//!
//! Linux only until another platform supplies safe descriptor-relative publish
//! and cleanup primitives. A copied directory tree is staged privately in its
//! destination parent before a single `renameat2(RENAME_NOREPLACE)` publishes it
//! by default. Explicit VisibleExclusive creates final names exclusively and
//! retains partial destinations on failure, without automatic rollback.
//! Sources are never removed. Symlinks are copied literally, never traversed.
//!
//! No crash journal/fsync durability, ownership/ACL/xattr preservation, sparse
//! stream-copy guarantee, or hostile-writer isolation is claimed. Linux has no
//! inode-conditional rename/unlink: identities are checked immediately before
//! operations, but another writer can still race the following syscall. In-flight
//! filesystem syscalls cannot be interrupted by Control (notably offline mounts).
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CopyStrategy {
    /// Reflink, then copy_file_range, then a bounded userspace buffer.
    #[default]
    Auto,
    /// Useful for predictable compatibility behavior and verification.
    Stream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyMethod {
    /// Native same-filesystem rename; no bytes copied.
    Rename,
    Reflink,
    Kernel,
    Stream,
    Symlink,
    Directory,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExistingDestination {
    #[default]
    Stop,
    Skip,
    KeepBoth,
}
/// Publication contract is explicit; visible destinations are never rolled back automatically.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Publication {
    #[default]
    StagedRename,
    VisibleExclusive,
}
#[derive(Clone, Debug)]
pub struct CopyOptions {
    pub strategy: CopyStrategy,
    pub publication: Publication,
    pub existing_destination: ExistingDestination,
    /// Conservative auxiliary-allocation accounting cap, not measured PSS.
    /// Accepted up to 32 MiB; includes buffer, registry capacity, recursion and reports.
    pub max_auxiliary_bytes: usize,
    /// Preflight limits before any worker-owned request/path duplication.
    /// Hard maxima: 4096 requests, 4 MiB aggregate path bytes; each path <=4096 bytes.
    pub max_requests: usize,
    pub max_request_path_bytes: usize,
    /// Per report category, 1..=128; error text <=1024 UTF-8 bytes and paths <=4096 bytes.
    /// Completed+skipped paths share that cap; other categories are independent.
    /// Overflow/truncation is counted explicitly, never silent.
    pub max_report_details: usize,
    /// Buffer and kernel-copy chunk: accepted range 4 KiB through 4 MiB.
    pub chunk_bytes: usize,
    /// Root is depth zero; accepted range 0..=128; symlinks never recurse.
    pub max_depth: usize,
    /// Includes selected roots and every visited directory, file and symlink.
    pub max_entries: u64,
    /// Sum of regular-file snapshot lengths, across all selected roots.
    pub max_total_bytes: u64,
    pub max_symlink_bytes: usize,
    /// Preserves rwx bits only; setuid/setgid/sticky, ownership and ACLs omitted.
    pub preserve_permissions: bool,
    /// Requests source snapshot atime and mtime; symlink behavior also requires
    /// preserve_symlink_times and a backend with genuine no-follow semantics.
    pub preserve_times: bool,
    /// Explicit omission is required where the backend lacks no-follow symlink metadata.
    pub preserve_symlink_times: bool,
}
impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            strategy: CopyStrategy::Auto,
            publication: Publication::StagedRename,
            existing_destination: ExistingDestination::Stop,
            max_auxiliary_bytes: 16 * 1024 * 1024,
            max_requests: 1024,
            max_request_path_bytes: 1024 * 1024,
            max_report_details: 64,
            chunk_bytes: 256 * 1024,
            max_depth: 128,
            max_entries: 100_000,
            max_total_bytes: u64::MAX,
            max_symlink_bytes: 64 * 1024,
            preserve_permissions: true,
            preserve_times: true,
            preserve_symlink_times: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CopyRequest {
    pub source: PathBuf,
    /// Exact destination name, whose parent must already exist.
    pub destination: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct CopyReport {
    pub publication: Publication,
    /// Root creation acknowledged; incomplete visible roots are retained, never deletion authority.
    pub visible_started_count: u64,
    pub visible_completed_count: u64,
    /// Failed creation without a definitive collision or held object handle; effect uncertain.
    pub visible_uncertain_count: u64,
    pub symlink_times_omitted: u64,
    /// Verified current parent paths for native moves; never deletion authority.
    pub affected_directories: Vec<PathBuf>,
    /// Published and identity-verified roots; previous roots survive later errors.
    pub completed: Vec<PathBuf>,
    /// Requested paths occupied in the opened destination parent when checked;
    /// not current-location snapshots or deletion authority.
    /// Shares max_report_details with completed paths; omitted counters separate.
    pub skipped: Vec<PathBuf>,
    pub skipped_count: u64,
    pub skipped_omitted: u64,
    pub completed_count: u64,
    /// Verified published roots assigned a numbered destination; no deletion authority.
    pub renamed_count: u64,
    pub completed_omitted: u64,
    /// Logical bytes transferred, including staged bytes discarded after failure.
    pub bytes_copied: u64,
    pub files_copied: u64,
    pub directories_copied: u64,
    pub symlinks_copied: u64,
    pub reflink_files: u64,
    pub kernel_files: u64,
    pub stream_files: u64,
    /// Verified locations of staging directories left after successful publication.
    pub cleanup_paths: Vec<PathBuf>,
    pub cleanup_errors: Vec<String>,
    pub cleanup_paths_omitted: u64,
    pub cleanup_errors_omitted: u64,
    pub details_truncated: u64,
    /// Accounting high-water mark; allocator/OS memory must still be measured.
    pub auxiliary_accounted_peak_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct CopyProgress<'a> {
    pub source: &'a Path,
    pub destination: &'a Path,
    pub bytes_copied: u64,
    pub entries_copied: u64,
    pub method: CopyMethod,
}

#[derive(Debug)]
pub struct CopyError {
    pub cause: io::Error,
    pub report: Box<CopyReport>,
    /// Verified locations of surviving partial copies; never delete these blindly.
    pub rescue_paths: Vec<PathBuf>,
    pub cleanup_errors: Vec<String>,
    pub rescue_paths_omitted: u64,
    pub cleanup_errors_omitted: u64,
    /// Registry entries not confirmed removed, including missing/replaced copies.
    /// Their original locations can be unknown; source originals remain intact.
    pub cleanup_unresolved_entries: u64,
    pub details_truncated: u64,
}
impl CopyError {
    fn new(cause: io::Error, report: CopyReport) -> Self {
        Self {
            cause,
            report: Box::new(report),
            rescue_paths: Vec::new(),
            cleanup_errors: Vec::new(),
            rescue_paths_omitted: 0,
            cleanup_errors_omitted: 0,
            cleanup_unresolved_entries: 0,
            details_truncated: 0,
        }
    }
}
impl std::fmt::Display for CopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} completed roots; {} completed paths omitted; {} rescue locations, {} omitted; {} cleanup errors, {} omitted; {} unresolved entries; {} truncated details)",
            self.cause,
            self.report.completed_count,
            self.report.completed_omitted,
            self.rescue_paths.len(),
            self.rescue_paths_omitted,
            self.cleanup_errors.len(),
            self.cleanup_errors_omitted,
            self.cleanup_unresolved_entries,
            self.details_truncated + self.report.details_truncated
        )
    }
}
impl std::error::Error for CopyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Debug, Default)]
struct ControlState {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    changed: Condvar,
}
/// Clone for the UI; all waiting happens in the worker calling `copy_paths`.
#[derive(Clone, Debug, Default)]
pub struct Control {
    state: Arc<ControlState>,
}
impl Control {
    pub fn pause(&self) {
        *self.state.paused.lock().unwrap_or_else(|e| e.into_inner()) = true;
    }
    pub fn resume(&self) {
        *self.state.paused.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.state.changed.notify_all();
    }
    pub fn cancel(&self) {
        // The waiter's mutex prevents a lost wakeup between check and wait.
        let _guard = self.state.paused.lock().unwrap_or_else(|e| e.into_inner());
        self.state.cancelled.store(true, Ordering::Release);
        self.state.changed.notify_all();
    }
    pub(crate) fn cancellation_flag(&self) -> &AtomicBool {
        &self.state.cancelled
    }
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn checkpoint(&self) -> io::Result<()> {
        let mut paused = self.state.paused.lock().unwrap_or_else(|e| e.into_inner());
        while *paused && !self.is_cancelled() {
            paused = self
                .state
                .changed
                .wait(paused)
                .unwrap_or_else(|e| e.into_inner());
        }
        if self.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Copy cancelled"));
        }
        Ok(())
    }
}

/// Call only on a worker. This function never overwrites existing destinations.
/// A multi-root job is not atomic: inspect CopyError.report.completed on failure.
/// Progress runs on this worker per chunk/entry; UI adapters should coalesce it.
/// The auxiliary cap is conservative allocation accounting, not measured process
/// PSS. A large tree may hit it before max_entries; the unpublished root is cleaned
/// safely and the error is explicit. Report totals and omitted/truncated counters
/// must be displayed by UI adapters, alongside verified rescue roots.
pub fn copy_paths(
    requests: &[CopyRequest],
    options: &CopyOptions,
    control: &Control,
    progress: impl FnMut(CopyProgress<'_>),
) -> Result<CopyReport, CopyError> {
    #[cfg(target_os = "linux")]
    {
        linux::run(requests, options, control, progress, linux::SystemBackend)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (requests, options, control, progress);
        Err(CopyError::new(
            io::Error::new(
                io::ErrorKind::Unsupported,
                "Safe local copy currently requires Linux no-replace publication",
            ),
            CopyReport::default(),
        ))
    }
}

/// Clipboard/selection copy with effective root descriptor verification. The
/// check is at the file actually opened by this engine, not only before dispatch.
pub fn copy_paths_selected(
    requests: &[CopyRequest],
    options: &CopyOptions,
    control: &Control,
    selection: &crate::selection_source::Source,
    progress: impl FnMut(CopyProgress<'_>),
) -> Result<CopyReport, CopyError> {
    #[cfg(target_os = "linux")]
    {
        linux::run_selected(
            requests,
            options,
            control,
            progress,
            linux::SystemBackend,
            Some(linux::Selection::Single(selection)),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = selection;
        copy_paths(requests, options, control, progress)
    }
}

/// Multi-parent copy. Capture/metadata verification is followed by effective FD proof
/// at each root; a later failure preserves the ordinary completed/rescue report.
pub fn copy_paths_captured(
    requests: &[CopyRequest],
    options: &CopyOptions,
    control: &Control,
    proof: &crate::multi_origin::Captured,
    progress: impl FnMut(CopyProgress<'_>),
) -> Result<CopyReport, CopyError> {
    #[cfg(target_os = "linux")]
    control
        .checkpoint()
        .map_err(|e| CopyError::new(e, CopyReport::default()))?;
    proof
        .verify(control.cancellation_flag())
        .map_err(|e| CopyError::new(e, CopyReport::default()))?;
    #[cfg(target_os = "linux")]
    {
        linux::run_selected(
            requests,
            options,
            control,
            progress,
            linux::SystemBackend,
            Some(linux::Selection::Multi(proof)),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (requests, options, control, progress);
        Err(CopyError::new(
            io::Error::new(
                io::ErrorKind::Unsupported,
                "Captured multiparent copy requires Linux",
            ),
            CopyReport::default(),
        ))
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::ffi::{CString, OsStr, OsString};
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::sync::atomic::AtomicU64;

    const HARD_AUXILIARY_BYTES: usize = 32 * 1024 * 1024;
    const HARD_REQUESTS: usize = 4096;
    const HARD_REQUEST_PATH_BYTES: usize = 4 * 1024 * 1024;
    const PATH_BYTES: usize = 4096;
    const DETAIL_BYTES: usize = 1024;

    struct AuxiliaryBudget {
        limit: usize,
        used: usize,
        peak: usize,
    }
    impl AuxiliaryBudget {
        fn reserve(&mut self, bytes: usize) -> io::Result<()> {
            let next = self
                .used
                .checked_add(bytes)
                .ok_or_else(|| invalid("Auxiliary accounting overflow"))?;
            if next > self.limit {
                return Err(invalid("Copy auxiliary accounting limit exceeded"));
            }
            self.used = next;
            self.peak = self.peak.max(next);
            Ok(())
        }
        fn release(&mut self, bytes: usize) {
            self.used = self.used.saturating_sub(bytes);
        }
    }
    struct BoundedText {
        text: String,
        truncated: bool,
    }
    impl std::fmt::Write for BoundedText {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            let mut keep = text.len().min(DETAIL_BYTES.saturating_sub(self.text.len()));
            while !text.is_char_boundary(keep) {
                keep -= 1;
            }
            self.text.push_str(&text[..keep]);
            self.truncated |= keep != text.len();
            Ok(())
        }
    }
    fn bounded_text(arguments: std::fmt::Arguments<'_>) -> (String, bool) {
        let mut writer = BoundedText {
            text: String::with_capacity(DETAIL_BYTES),
            truncated: false,
        };
        let _ = std::fmt::write(&mut writer, arguments);
        if writer.truncated {
            const MARKER: &str = " [truncated]";
            let mut end = writer.text.len().min(DETAIL_BYTES - MARKER.len());
            while !writer.text.is_char_boundary(end) {
                end -= 1;
            }
            writer.text.truncate(end);
            writer.text.push_str(MARKER);
        }
        (writer.text, writer.truncated)
    }
    #[derive(Default)]
    struct Details {
        paths: Vec<PathBuf>,
        errors: Vec<String>,
        paths_omitted: u64,
        errors_omitted: u64,
        truncated: u64,
        unresolved_entries: u64,
    }
    impl Details {
        fn error(&mut self, limit: usize, arguments: std::fmt::Arguments<'_>) {
            if self.errors.len() == limit {
                self.errors_omitted += 1;
                return;
            }
            let (text, truncated) = bounded_text(arguments);
            self.truncated += u64::from(truncated);
            self.errors.push(text);
        }
        fn path(&mut self, limit: usize, path: PathBuf) {
            if self.paths.len() == limit || path.as_os_str().len() > PATH_BYTES {
                self.paths_omitted += 1;
            } else {
                self.paths.push(path);
            }
        }
    }
    fn child_path(parent: &Path, name: &OsStr) -> io::Result<PathBuf> {
        if parent
            .as_os_str()
            .len()
            .checked_add(name.len() + 1)
            .is_none_or(|n| n > PATH_BYTES)
        {
            return Err(invalid("Copy path exceeds bounded path length"));
        }
        Ok(parent.join(name))
    }
    fn invalid(message: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, message)
    }
    fn c_name(name: &OsStr) -> io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|_| invalid("NUL in path"))
    }
    fn validate_name(name: &OsStr) -> io::Result<()> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.as_bytes().contains(&b'/')
            || name.as_bytes().contains(&0)
        {
            return Err(invalid("Expected one filename component"));
        }
        Ok(())
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Identity {
        device: u64,
        inode: u64,
        kind: u32,
    }
    fn identity(stat: &libc::stat) -> Identity {
        Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: stat.st_mode & libc::S_IFMT,
        }
    }
    fn stat_at(directory: &File, name: &OsStr) -> io::Result<libc::stat> {
        let name = c_name(name)?;
        let mut stat = std::mem::MaybeUninit::uninit();
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { stat.assume_init() })
    }
    fn stat_fd(file: &File) -> io::Result<libc::stat> {
        let mut stat = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { stat.assume_init() })
    }
    fn same_snapshot(expected: &libc::stat, actual: &libc::stat) -> bool {
        identity(expected) == identity(actual)
            && expected.st_mode == actual.st_mode
            && expected.st_size == actual.st_size
            && expected.st_mtime == actual.st_mtime
            && expected.st_mtime_nsec == actual.st_mtime_nsec
            && expected.st_ctime == actual.st_ctime
            && expected.st_ctime_nsec == actual.st_ctime_nsec
    }
    fn verify_snapshot(expected: &libc::stat, actual: &libc::stat) -> io::Result<()> {
        if !same_snapshot(expected, actual) {
            return Err(io::Error::other("Source changed during copy"));
        }
        Ok(())
    }
    fn open_at(parent: &File, name: &OsStr, flags: i32, mode: libc::mode_t) -> io::Result<File> {
        let name = c_name(name)?;
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                mode,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn open_directory(path: &Path) -> io::Result<File> {
        let path = c_name(path.as_os_str())?;
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn descriptor_path(file: &File) -> io::Result<PathBuf> {
        let link =
            CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).expect("Numeric fd path");
        let mut bytes = vec![0u8; PATH_BYTES + 1];
        let length =
            unsafe { libc::readlink(link.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len()) };
        if length < 0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize > PATH_BYTES {
            return Err(invalid(
                "Descriptor location exceeds bounded report path length",
            ));
        }
        bytes.truncate(length as usize);
        let path = PathBuf::from(OsString::from_vec(bytes));
        let candidate = c_name(path.as_os_str())?;
        let mut stat = std::mem::MaybeUninit::uninit();
        if unsafe {
            libc::fstatat(
                libc::AT_FDCWD,
                candidate.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        if identity(&unsafe { stat.assume_init() }) != identity(&stat_fd(file)?) {
            return Err(io::Error::other(
                "Descriptor location changed while resolving",
            ));
        }
        Ok(path)
    }
    fn rename_exclusive(
        from_dir: &File,
        from: &OsStr,
        to_dir: &File,
        to: &OsStr,
    ) -> io::Result<()> {
        let from = c_name(from)?;
        let to = c_name(to)?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                from_dir.as_raw_fd(),
                from.as_ptr(),
                to_dir.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    fn split(path: &Path) -> io::Result<(PathBuf, OsString)> {
        if path.as_os_str().len() > PATH_BYTES {
            return Err(invalid("Copy path exceeds bounded path length"));
        }
        let name = path
            .file_name()
            .ok_or_else(|| invalid("Copy root needs a filename"))?
            .to_os_string();
        validate_name(&name)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let input = c_name(parent.as_os_str())?;
        let mut output = vec![0u8; PATH_BYTES + 1];
        if unsafe { libc::realpath(input.as_ptr(), output.as_mut_ptr().cast()) }.is_null() {
            return Err(io::Error::last_os_error());
        }
        let length = output
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| invalid("Canonical path exceeds fixed buffer"))?;
        output.truncate(length);
        Ok((PathBuf::from(OsString::from_vec(output)), name))
    }
    fn source_open(
        parent: &File,
        name: &OsStr,
        snapshot: &libc::stat,
        directory: bool,
    ) -> io::Result<File> {
        let flags = libc::O_RDONLY
            | if directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        let file = open_at(parent, name, flags, 0)?;
        verify_snapshot(snapshot, &stat_fd(&file)?)?;
        Ok(file)
    }
    fn no_destination(parent: &File, name: &OsStr) -> io::Result<()> {
        match stat_at(parent, name) {
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destination already exists",
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
    fn available_name(
        parent: &File,
        names: &mut crate::copy_unique_name::Names<'_>,
        control: &Control,
    ) -> io::Result<()> {
        loop {
            control.checkpoint()?;
            match no_destination(parent, names.name()) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => names.advance()?,
                Err(error) => return Err(error),
            }
        }
    }
    fn chmod_owned(file: &File, mode: libc::mode_t) -> io::Result<()> {
        // Also works with O_PATH, and acts on our held inode rather than a name.
        let path =
            CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).expect("Numeric fd path");
        if unsafe { libc::chmod(path.as_ptr(), mode) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    fn metadata_fd(file: &File, source: &libc::stat, options: &CopyOptions) -> io::Result<()> {
        if options.preserve_times {
            let times = timestamps(source);
            if unsafe { libc::futimens(file.as_raw_fd(), times.as_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if options.preserve_permissions
            && unsafe { libc::fchmod(file.as_raw_fd(), source.st_mode & 0o777) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    fn timestamps(source: &libc::stat) -> [libc::timespec; 2] {
        [
            libc::timespec {
                tv_sec: source.st_atime,
                tv_nsec: source.st_atime_nsec,
            },
            libc::timespec {
                tv_sec: source.st_mtime,
                tv_nsec: source.st_mtime_nsec,
            },
        ]
    }

    pub(super) trait Backend {
        fn reflink(&mut self, source: &File, destination: &File) -> io::Result<()>;
        fn copy_range(
            &mut self,
            source: &File,
            destination: &File,
            length: usize,
        ) -> io::Result<usize>;
    }
    pub(super) struct SystemBackend;
    impl Backend for SystemBackend {
        fn reflink(&mut self, source: &File, destination: &File) -> io::Result<()> {
            if unsafe { libc::ioctl(destination.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) }
                != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        fn copy_range(
            &mut self,
            source: &File,
            destination: &File,
            length: usize,
        ) -> io::Result<usize> {
            let result = unsafe {
                libc::syscall(
                    libc::SYS_copy_file_range,
                    source.as_raw_fd(),
                    std::ptr::null_mut::<libc::loff_t>(),
                    destination.as_raw_fd(),
                    std::ptr::null_mut::<libc::loff_t>(),
                    length,
                    0u32,
                )
            };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(result as usize)
        }
    }
    fn unsupported_transfer(error: &io::Error) -> bool {
        matches!(
            error.raw_os_error(),
            Some(
                libc::EXDEV
                    | libc::EINVAL
                    | libc::ENOSYS
                    | libc::EOPNOTSUPP
                    | libc::ENOTTY
                    | libc::EPERM
            )
        ) || error.kind() == io::ErrorKind::Unsupported
    }

    /// Explicitly bounded getdents storage instead of libc DIR buffers whose
    /// filesystem-dependent allocation can grow once per recursive level.
    struct DirectoryCursor<'a> {
        directory: &'a File,
        buffer: Vec<u8>,
        position: usize,
        length: usize,
    }
    impl<'a> DirectoryCursor<'a> {
        fn new(directory: &'a File) -> io::Result<Self> {
            let mut buffer = Vec::new();
            buffer
                .try_reserve_exact(8192)
                .map_err(|_| io::Error::other("Cannot allocate bounded directory cursor"))?;
            buffer.resize(8192, 0);
            Ok(Self {
                directory,
                buffer,
                position: 0,
                length: 0,
            })
        }
        fn next(&mut self, control: &Control) -> io::Result<Option<OsString>> {
            loop {
                control.checkpoint()?;
                if self.position == self.length {
                    let count = unsafe {
                        libc::syscall(
                            libc::SYS_getdents64,
                            self.directory.as_raw_fd(),
                            self.buffer.as_mut_ptr(),
                            self.buffer.len(),
                        )
                    };
                    if count < 0 {
                        let e = io::Error::last_os_error();
                        if e.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(e);
                    }
                    if count == 0 {
                        return Ok(None);
                    }
                    if count as usize > self.buffer.len() {
                        return Err(io::Error::other("Invalid directory syscall length"));
                    }
                    self.position = 0;
                    self.length = count as usize;
                }
                let remaining = &self.buffer[self.position..self.length];
                if remaining.len() < 20 {
                    return Err(io::Error::other("Truncated directory record"));
                }
                let size = u16::from_ne_bytes([remaining[16], remaining[17]]) as usize;
                if size < 20 || size > remaining.len() {
                    return Err(io::Error::other("Invalid directory record size"));
                }
                let names = &remaining[19..size];
                let length = names
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or_else(|| io::Error::other("Unterminated directory name"))?;
                let name = &names[..length];
                self.position += size;
                if name == b"." || name == b".." {
                    continue;
                }
                if name.is_empty() || name.len() > PATH_BYTES || name.contains(&b'/') {
                    return Err(io::Error::other("Invalid directory name"));
                }
                return Ok(Some(OsString::from_vec(name.to_vec())));
            }
        }
    }

    struct OwnedNode {
        parent: Option<usize>,
        name: OsString,
        identity: Identity,
    }
    struct Stage {
        directory: File,
        name: OsString,
        identity: Identity,
        nodes: Vec<OwnedNode>,
        published: bool,
        root_handle: Option<File>,
        accounted_bytes: usize,
        visible: bool,
        attempted: bool,
        created: bool,
        root_collision: bool,
    }
    impl Stage {
        fn create(parent: &File, destination_name: &OsStr) -> io::Result<Self> {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            for _ in 0..128 {
                let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
                let name: OsString = format!(".files-copy-{}-{serial}", std::process::id()).into();
                if name == destination_name {
                    continue;
                }
                let c = c_name(&name)?;
                if unsafe { libc::mkdirat(parent.as_raw_fd(), c.as_ptr(), 0o700) } != 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() == io::ErrorKind::AlreadyExists {
                        continue;
                    }
                    return Err(e);
                }
                let directory = open_at(parent, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
                    .map_err(|e| {
                        io::Error::other(
                            bounded_text(format_args!(
                                "{e}; unverified empty staging candidate left at {:?}",
                                descriptor_path(parent).and_then(|p| child_path(&p, &name))
                            ))
                            .0,
                        )
                    })?;
                let expected = identity(&stat_fd(&directory)?);
                if identity(&stat_at(parent, &name)?) != expected {
                    return Err(io::Error::other("Staging replaced while opening"));
                }
                return Ok(Self {
                    directory,
                    name,
                    identity: expected,
                    nodes: Vec::new(),
                    published: false,
                    root_handle: None,
                    accounted_bytes: 0,
                    visible: false,
                    attempted: false,
                    created: false,
                    root_collision: false,
                });
            }
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Cannot allocate private copy staging",
            ))
        }
        fn visible(parent: &File, name: &OsStr) -> io::Result<Self> {
            Ok(Self {
                directory: parent.try_clone()?,
                name: name.to_owned(),
                identity: identity(&stat_fd(parent)?),
                nodes: Vec::new(),
                published: false,
                root_handle: None,
                accounted_bytes: 0,
                visible: true,
                attempted: false,
                created: false,
                root_collision: false,
            })
        }
        fn create_exclusive<T>(
            &mut self,
            parent: Option<usize>,
            create: impl FnOnce() -> io::Result<T>,
        ) -> io::Result<T> {
            let root = self.visible && parent.is_none();
            if root {
                self.attempted = true;
                self.root_collision = false;
            }
            let result = create();
            if root {
                match &result {
                    Ok(_) => self.created = true,
                    Err(e) => self.root_collision = e.kind() == io::ErrorKind::AlreadyExists,
                }
            }
            result
        }
        fn pin_visible_root(&mut self, parent: Option<usize>, handle: &File) -> io::Result<()> {
            if self.visible && parent.is_none() {
                self.root_handle = Some(handle.try_clone()?);
            }
            Ok(())
        }
        fn reserve_record(&mut self, name: &OsStr, budget: &mut AuxiliaryBudget) -> io::Result<()> {
            // Two times capacity covers old+new backing storage during growth;
            // names include alignment/allocator slack before any OsString clone.
            let old = self.nodes.capacity();
            let target = if self.nodes.len() == old {
                old.saturating_mul(2).max(16)
            } else {
                old
            };
            let storage = target
                .checked_sub(old)
                .and_then(|n| n.checked_mul(std::mem::size_of::<OwnedNode>() * 2))
                .ok_or_else(|| invalid("Copy registry accounting overflow"))?;
            let names = name
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(64))
                .ok_or_else(|| invalid("Copy name accounting overflow"))?;
            let charged = storage
                .checked_add(names)
                .ok_or_else(|| invalid("Copy registry accounting overflow"))?;
            budget.reserve(charged)?;
            self.accounted_bytes += charged;
            if target != old {
                self.nodes
                    .try_reserve_exact(target - self.nodes.len())
                    .map_err(|_| io::Error::other("Cannot allocate bounded copy registry"))?;
                if self.nodes.capacity() > target {
                    return Err(io::Error::other(
                        "Allocator exceeded reserved registry capacity",
                    ));
                }
            }
            Ok(())
        }
        fn record(&mut self, parent: Option<usize>, name: &OsStr, stat: &libc::stat) -> usize {
            let index = self.nodes.len();
            self.nodes.push(OwnedNode {
                parent,
                name: name.to_owned(),
                identity: identity(stat),
            });
            index
        }
        fn parent_for(&self, index: usize, cleanup: bool) -> io::Result<File> {
            let mut ancestors = Vec::new();
            let mut parent = self.nodes[index].parent;
            while let Some(i) = parent {
                ancestors.push(i);
                parent = self.nodes[i].parent;
            }
            let mut directory = self.directory.try_clone()?;
            if cleanup {
                chmod_owned(&directory, 0o700)?;
            }
            for i in ancestors.into_iter().rev() {
                let expected = &self.nodes[i];
                let next = open_at(
                    &directory,
                    &expected.name,
                    libc::O_PATH | libc::O_DIRECTORY,
                    0,
                )?;
                if identity(&stat_fd(&next)?) != expected.identity {
                    return Err(io::Error::other("Owned cleanup ancestor was replaced"));
                }
                if cleanup {
                    chmod_owned(&next, 0o700)?;
                }
                directory = next;
            }
            Ok(directory)
        }
        fn verify_payload(&self, control: &Control) -> io::Result<()> {
            for (i, node) in self.nodes.iter().enumerate() {
                control.checkpoint()?;
                let parent = self.parent_for(i, false)?;
                if identity(&stat_at(&parent, &node.name)?) != node.identity {
                    return Err(io::Error::other(
                        "Staged copy entry identity changed before publication",
                    ));
                }
            }
            Ok(())
        }
        fn remove_empty(&self, parent: &File) -> io::Result<()> {
            if identity(&stat_at(parent, &self.name)?) != self.identity {
                return Err(io::Error::other(
                    "Staging name was replaced; refusing cleanup",
                ));
            }
            let name = c_name(&self.name)?;
            if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        fn cleanup(&self, parent: &File, limit: usize) -> Details {
            let mut details = Details::default();
            if self.visible {
                // Parent is not a staging directory. Never chmod/unlink it or any visible entry.
                if !self.published && self.attempted && !self.root_collision {
                    details.unresolved_entries = self.nodes.len().max(1) as u64;
                    if let Some(handle) = &self.root_handle {
                        match descriptor_path(handle) {
                            Ok(path) => details.path(limit, path),
                            Err(e) => {
                                details.paths_omitted += 1;
                                details.error(
                                    limit,
                                    format_args!("Resolve retained visible root: {e}"),
                                );
                            }
                        }
                    } else {
                        details.paths_omitted += 1;
                        details.error(limit, format_args!("Visible destination creation may have taken effect; no verified rescue handle. Requested candidate: {:?}; do not delete by inference", descriptor_path(&self.directory).and_then(|p|child_path(&p,&self.name))));
                    }
                }
                return details;
            }
            if !self.published {
                for i in (0..self.nodes.len()).rev() {
                    let node = &self.nodes[i];
                    let result = (|| -> io::Result<()> {
                        let directory = self.parent_for(i, true)?;
                        let actual = stat_at(&directory, &node.name)?;
                        if identity(&actual) != node.identity {
                            return Err(io::Error::other(
                                "Owned cleanup entry was replaced; retained",
                            ));
                        }
                        let name = c_name(&node.name)?;
                        let flags = if node.identity.kind == libc::S_IFDIR {
                            libc::AT_REMOVEDIR
                        } else {
                            0
                        };
                        if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), flags) }
                            != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        details.unresolved_entries += 1;
                        details.error(limit, format_args!("Cleanup {:?}: {e}", node.name));
                    }
                }
            }
            if let Err(e) = self.remove_empty(parent) {
                details.error(limit, format_args!("Cleanup staging: {e}"));
                // A single verified rescuable root represents all entries that
                // remain. Do not duplicate an arbitrarily long parent per item.
                match descriptor_path(&self.directory) {
                    Ok(path) => details.path(limit, path),
                    Err(e) => {
                        details.paths_omitted += 1;
                        details.error(limit, format_args!("Resolve surviving staging: {e}"));
                    }
                }
            } else if details.unresolved_entries != 0 {
                details.paths_omitted += 1;
                details.error(limit,format_args!("Unresolved copied entries have no verified rescue root; source originals retained"));
            }
            details
        }
    }

    #[derive(Clone, Copy)]
    pub(super) enum Selection<'a> {
        Single(&'a crate::selection_source::Source),
        Multi(&'a crate::multi_origin::Captured),
    }
    struct Engine<'a, F, B> {
        selection: Option<Selection<'a>>,
        options: &'a CopyOptions,
        control: &'a Control,
        progress: F,
        backend: B,
        report: CopyReport,
        visited: u64,
        budget_bytes: u64,
        buffer: Vec<u8>,
        budget: AuxiliaryBudget,
    }
    pub(super) fn run<F: FnMut(CopyProgress<'_>), B: Backend>(
        requests: &[CopyRequest],
        options: &CopyOptions,
        control: &Control,
        progress: F,
        backend: B,
    ) -> Result<CopyReport, CopyError> {
        run_selected(requests, options, control, progress, backend, None)
    }
    pub(super) fn run_selected<F: FnMut(CopyProgress<'_>), B: Backend>(
        requests: &[CopyRequest],
        options: &CopyOptions,
        control: &Control,
        progress: F,
        backend: B,
        selection: Option<Selection<'_>>,
    ) -> Result<CopyReport, CopyError> {
        let valid = (4096..=4 * 1024 * 1024).contains(&options.chunk_bytes)
            && options.max_depth <= 128
            && (1..=1024 * 1024).contains(&options.max_symlink_bytes)
            && (1..=HARD_AUXILIARY_BYTES).contains(&options.max_auxiliary_bytes)
            && (1..=HARD_REQUESTS).contains(&options.max_requests)
            && (1..=HARD_REQUEST_PATH_BYTES).contains(&options.max_request_path_bytes)
            && (1..=128).contains(&options.max_report_details);
        if !valid {
            return Err(CopyError::new(
                invalid("Invalid copy bounds"),
                CopyReport::default(),
            ));
        }
        // Borrowed requests are inspected before split/canonicalize/clone or any
        // buffer/registry allocation. Their input footprint is also charged.
        if requests.len() > options.max_requests {
            return Err(CopyError::new(
                invalid("Copy request count limit exceeded"),
                CopyReport::default(),
            ));
        }
        let mut path_bytes = 0usize;
        for request in requests {
            for path in [&request.source, &request.destination] {
                let length = path.as_os_str().len();
                path_bytes = path_bytes.checked_add(length).ok_or_else(|| {
                    CopyError::new(
                        invalid("Copy request path accounting overflow"),
                        CopyReport::default(),
                    )
                })?;
                if length > PATH_BYTES || path_bytes > options.max_request_path_bytes {
                    return Err(CopyError::new(
                        invalid("Copy request path byte limit exceeded"),
                        CopyReport::default(),
                    ));
                }
            }
        }
        let mut budget = AuxiliaryBudget {
            limit: options.max_auxiliary_bytes,
            used: 0,
            peak: 0,
        };
        // Conservative accounting, not a PSS claim: two backing stores for
        // allocation growth, fixed root workspace, bounded recursion and eight
        // report-detail sets (growth/clone/failure paths can coexist).
        let fixed = path_bytes * 2
            + std::mem::size_of_val(requests) * 2
            + options.chunk_bytes * 2
            + (options.max_symlink_bytes + 1) * 2
            + 512 * 1024
            + (options.max_depth + 1) * (4 * PATH_BYTES + 32 * 1024)
            + options.max_report_details * (2 * PATH_BYTES + DETAIL_BYTES + 256) * 8;
        budget
            .reserve(fixed)
            .map_err(|e| CopyError::new(e, CopyReport::default()))?;
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(options.chunk_bytes).map_err(|_| {
            CopyError::new(
                io::Error::other("Cannot allocate bounded copy buffer"),
                CopyReport::default(),
            )
        })?;
        buffer.resize(options.chunk_bytes, 0);
        let mut engine = Engine {
            selection,
            options,
            control,
            progress,
            backend,
            report: CopyReport {
                publication: options.publication,
                ..Default::default()
            },
            visited: 0,
            budget_bytes: 0,
            buffer,
            budget,
        };
        for request in requests {
            engine.one(request)?;
        }
        engine.report.auxiliary_accounted_peak_bytes = engine.budget.peak;
        Ok(engine.report)
    }

    impl<F: FnMut(CopyProgress<'_>), B: Backend> Engine<'_, F, B> {
        fn error(&self, cause: io::Error) -> CopyError {
            let mut report = self.report.clone();
            report.auxiliary_accounted_peak_bytes = self.budget.peak;
            CopyError::new(cause, report)
        }
        fn notify(&mut self, source: &Path, destination: &Path, method: CopyMethod) {
            (self.progress)(CopyProgress {
                source,
                destination,
                bytes_copied: self.report.bytes_copied,
                entries_copied: self.report.files_copied
                    + self.report.directories_copied
                    + self.report.symlinks_copied,
                method,
            });
        }
        fn reserve(&mut self, snapshot: &libc::stat, depth: usize) -> io::Result<()> {
            self.control.checkpoint()?;
            if depth > self.options.max_depth {
                return Err(invalid("Copy depth limit exceeded"));
            }
            self.visited = self
                .visited
                .checked_add(1)
                .ok_or_else(|| invalid("Copy entry count overflow"))?;
            if self.visited > self.options.max_entries {
                return Err(invalid("Copy entry limit exceeded"));
            }
            if snapshot.st_mode & libc::S_IFMT == libc::S_IFREG {
                let size =
                    u64::try_from(snapshot.st_size).map_err(|_| invalid("Negative source size"))?;
                self.budget_bytes = self
                    .budget_bytes
                    .checked_add(size)
                    .ok_or_else(|| invalid("Copy byte count overflow"))?;
                if self.budget_bytes > self.options.max_total_bytes {
                    return Err(invalid("Copy byte limit exceeded"));
                }
            }
            Ok(())
        }
        fn skip(&mut self, destination: &Path) {
            self.report.skipped_count += 1;
            if self.report.completed.len() + self.report.skipped.len()
                >= self.options.max_report_details
            {
                self.report.skipped_omitted += 1;
            } else {
                let mut bytes = Vec::with_capacity(PATH_BYTES + 1);
                bytes.extend_from_slice(destination.as_os_str().as_bytes());
                self.report
                    .skipped
                    .push(PathBuf::from(OsString::from_vec(bytes)));
            }
        }
        fn one(&mut self, request: &CopyRequest) -> Result<(), CopyError> {
            self.control.checkpoint().map_err(|e| self.error(e))?;
            let (source_path, source_name) = split(&request.source).map_err(|e| self.error(e))?;
            let (destination_path, destination_name) =
                split(&request.destination).map_err(|e| self.error(e))?;
            let source_parent = open_directory(&source_path).map_err(|e| self.error(e))?;
            let destination_parent =
                open_directory(&destination_path).map_err(|e| self.error(e))?;
            let source = child_path(&source_path, &source_name).map_err(|e| self.error(e))?;
            let destination =
                child_path(&destination_path, &destination_name).map_err(|e| self.error(e))?;
            let source_pin = open_at(&source_parent, &source_name, libc::O_PATH, 0)
                .map_err(|e| self.error(e))?;
            let snapshot = stat_fd(&source_pin).map_err(|e| self.error(e))?;
            if let Some(selection) = self.selection {
                match selection {
                    Selection::Single(s) => {
                        s.verify_captured(&source_parent, &source_name, &source_pin)
                    }
                    Selection::Multi(proof) => {
                        let group = proof
                            .groups()
                            .iter()
                            .position(|g| g.source.parent() == source_path)
                            .ok_or_else(|| invalid("Source parent absent from captured selection"));
                        group.and_then(|i| {
                            proof.verify_effective(i, &source_parent, &source_name, &source_pin)
                        })
                    }
                }
                .map_err(|e| self.error(e))?;
            }
            verify_snapshot(
                &snapshot,
                &stat_at(&source_parent, &source_name).map_err(|e| self.error(e))?,
            )
            .map_err(|e| self.error(e))?;
            let keep_both = self.options.existing_destination == ExistingDestination::KeepBoth;
            let same = source == destination;
            let mut names = crate::copy_unique_name::Names::new(
                &destination_name,
                snapshot.st_mode & libc::S_IFMT == libc::S_IFDIR,
                crate::copy_unique_name::MAX_NAME
                    .min(PATH_BYTES.saturating_sub(destination_path.as_os_str().len() + 1)),
            )
            .map_err(|e| self.error(e))?;
            if (same && !keep_both)
                || (snapshot.st_mode & libc::S_IFMT == libc::S_IFDIR
                    && destination.starts_with(&source)
                    && !(same && keep_both))
            {
                return Err(self.error(invalid(
                    "Cannot copy a directory into itself or its descendants",
                )));
            }
            if let Err(cause) = no_destination(&destination_parent, &destination_name) {
                if cause.kind() == io::ErrorKind::AlreadyExists && keep_both {
                    names.advance().map_err(|e| self.error(e))?;
                    available_name(&destination_parent, &mut names, self.control)
                        .map_err(|e| self.error(e))?;
                } else if cause.kind() == io::ErrorKind::AlreadyExists
                    && self.options.existing_destination == ExistingDestination::Skip
                {
                    self.reserve(&snapshot, 0).map_err(|e| self.error(e))?;
                    self.skip(&destination);
                    return Ok(());
                } else {
                    return Err(self.error(cause));
                }
            }
            let visible = self.options.publication == Publication::VisibleExclusive;
            let mut stage = if visible {
                Stage::visible(&destination_parent, names.name())
            } else {
                Stage::create(&destination_parent, names.name())
            }
            .map_err(|e| self.error(e))?;
            let mut publication_conflict = false;
            let result = (|| -> io::Result<()> {
                let staging_parent = stage.directory.try_clone()?;
                loop {
                    if visible {
                        stage.name = names.name().to_owned();
                    }
                    let actual = child_path(&destination_path, names.name())?;
                    let target_name = if visible {
                        names.name()
                    } else {
                        OsStr::new("payload")
                    };
                    let copied = self.node(
                        &source_parent,
                        &source_name,
                        &staging_parent,
                        target_name,
                        None,
                        &source,
                        if visible { &actual } else { &destination },
                        0,
                        Some(&source_pin),
                        &mut stage,
                    );
                    match copied {
                        Err(e)
                            if visible
                                && stage.root_collision
                                && !stage.created
                                && stage.nodes.is_empty() =>
                        {
                            if keep_both {
                                names.advance()?;
                                available_name(&destination_parent, &mut names, self.control)?;
                                verify_snapshot(
                                    &snapshot,
                                    &stat_at(&source_parent, &source_name)?,
                                )?;
                                continue;
                            }
                            publication_conflict = true;
                            return Err(e);
                        }
                        result => {
                            result?;
                            break;
                        }
                    }
                }
                self.control.checkpoint()?;
                stage.verify_payload(self.control)?;
                verify_snapshot(&snapshot, &stat_at(&source_parent, &source_name)?)?;
                let handle = if visible {
                    stage
                        .root_handle
                        .as_ref()
                        .ok_or_else(|| io::Error::other("Visible root handle missing"))?
                        .try_clone()?
                } else {
                    open_at(&stage.directory, OsStr::new("payload"), libc::O_PATH, 0)?
                };
                let root_identity = identity(&stat_fd(&handle)?);
                if stage.nodes.first().map(|n| n.identity) != Some(root_identity) {
                    return Err(io::Error::other("Staged root identity changed"));
                }
                stage.root_handle = Some(handle);
                if !visible {
                    loop {
                        match rename_exclusive(
                            &stage.directory,
                            OsStr::new("payload"),
                            &destination_parent,
                            names.name(),
                        ) {
                            Ok(()) => break,
                            Err(cause)
                                if cause.kind() == io::ErrorKind::AlreadyExists && keep_both =>
                            {
                                names.advance()?;
                                available_name(&destination_parent, &mut names, self.control)?;
                                // Keep the same payload; every retry revalidates source and staging.
                                self.control.checkpoint()?;
                                stage.verify_payload(self.control)?;
                                verify_snapshot(
                                    &snapshot,
                                    &stat_at(&source_parent, &source_name)?,
                                )?;
                                if identity(&stat_at(&stage.directory, OsStr::new("payload"))?)
                                    != root_identity
                                {
                                    return Err(io::Error::other(
                                        "Staged root identity changed before retry",
                                    ));
                                }
                            }
                            Err(cause) => {
                                publication_conflict = cause.kind() == io::ErrorKind::AlreadyExists;
                                return Err(cause);
                            }
                        }
                    }
                    stage.published = true;
                }
                if identity(&stat_at(&destination_parent, names.name())?) != root_identity {
                    return Err(io::Error::other("Published destination identity changed"));
                }
                stage.published = true;
                self.report.completed_count += 1;
                self.report.visible_completed_count += u64::from(visible);
                self.report.renamed_count += u64::from(names.changed());
                if self.report.completed.len() + self.report.skipped.len()
                    >= self.options.max_report_details
                {
                    self.report.completed_omitted += 1;
                } else {
                    let actual_path = match descriptor_path(
                        stage.root_handle.as_ref().expect("Root handle held"),
                    ) {
                        Ok(path) => path,
                        Err(e) => {
                            self.report.completed_omitted += 1;
                            return Err(e);
                        }
                    };
                    if actual_path.as_os_str().len() > PATH_BYTES {
                        self.report.completed_omitted += 1;
                    } else {
                        self.report.completed.push(actual_path);
                    }
                }
                Ok(())
            })();
            self.report.visible_started_count += u64::from(visible && stage.created);
            self.report.visible_uncertain_count +=
                u64::from(visible && stage.attempted && !stage.created && !stage.root_collision);
            let mut details = stage.cleanup(&destination_parent, self.options.max_report_details);
            self.report.auxiliary_accounted_peak_bytes = self.budget.peak;
            // Registry allocations are dropped at this function's return before
            // another root can reserve this accounting space.
            self.budget.release(stage.accounted_bytes);
            if let Err(cause) = result {
                if publication_conflict
                    && self.options.existing_destination == ExistingDestination::Skip
                    && !stage.published
                    && details.paths.is_empty()
                    && details.errors.is_empty()
                    && details.paths_omitted == 0
                    && details.errors_omitted == 0
                    && details.unresolved_entries == 0
                    && details.truncated == 0
                {
                    self.skip(&destination);
                    return Ok(());
                }
                if stage.published
                    && let Some(handle) = &stage.root_handle
                {
                    match descriptor_path(handle) {
                        Ok(path) => details.path(self.options.max_report_details, path),
                        Err(e) => {
                            details.paths_omitted += 1;
                            details.error(
                                self.options.max_report_details,
                                format_args!("Resolve published rescue root: {e}"),
                            );
                        }
                    }
                }
                return Err(CopyError {
                    cause,
                    report: Box::new(self.report.clone()),
                    rescue_paths: details.paths,
                    cleanup_errors: details.errors,
                    rescue_paths_omitted: details.paths_omitted,
                    cleanup_errors_omitted: details.errors_omitted,
                    cleanup_unresolved_entries: details.unresolved_entries,
                    details_truncated: details.truncated,
                });
            }
            self.report.cleanup_paths_omitted += details.paths_omitted;
            self.report.cleanup_errors_omitted += details.errors_omitted;
            self.report.details_truncated += details.truncated;
            for path in details.paths {
                if self.report.cleanup_paths.len() == self.options.max_report_details {
                    self.report.cleanup_paths_omitted += 1;
                } else {
                    self.report.cleanup_paths.push(path);
                }
            }
            for error in details.errors {
                if self.report.cleanup_errors.len() == self.options.max_report_details {
                    self.report.cleanup_errors_omitted += 1;
                } else {
                    self.report.cleanup_errors.push(error);
                }
            }
            Ok(())
        }
        #[allow(clippy::too_many_arguments)]
        fn node(
            &mut self,
            source_parent: &File,
            source_name: &OsStr,
            destination_parent: &File,
            destination_name: &OsStr,
            parent_index: Option<usize>,
            source_path: &Path,
            destination_path: &Path,
            depth: usize,
            root_pin: Option<&File>,
            stage: &mut Stage,
        ) -> io::Result<()> {
            let snapshot = stat_at(source_parent, source_name)?;
            if let Some(pin) = root_pin {
                verify_snapshot(&stat_fd(pin)?, &snapshot)?;
            }
            self.reserve(&snapshot, depth)?;
            stage.reserve_record(destination_name, &mut self.budget)?;
            if identity(&snapshot) == stage.identity {
                return Err(invalid(
                    "Copy encountered its own staging directory (destination alias inside source)",
                ));
            }
            match snapshot.st_mode & libc::S_IFMT {
                libc::S_IFREG => {
                    let mut source = source_open(source_parent, source_name, &snapshot, false)?;
                    let mut destination = stage.create_exclusive(parent_index, || {
                        open_at(
                            destination_parent,
                            destination_name,
                            libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY,
                            0o600,
                        )
                    })?;
                    stage.pin_visible_root(parent_index, &destination)?;
                    stage.record(parent_index, destination_name, &stat_fd(&destination)?);
                    let method = self.data(
                        &mut source,
                        &mut destination,
                        snapshot.st_size as u64,
                        source_path,
                        destination_path,
                    )?;
                    verify_snapshot(&snapshot, &stat_fd(&source)?)?;
                    verify_snapshot(&snapshot, &stat_at(source_parent, source_name)?)?;
                    metadata_fd(&destination, &snapshot, self.options)?;
                    self.report.files_copied += 1;
                    match method {
                        CopyMethod::Reflink => self.report.reflink_files += 1,
                        CopyMethod::Kernel => self.report.kernel_files += 1,
                        _ => self.report.stream_files += 1,
                    }
                    self.notify(source_path, destination_path, method);
                }
                libc::S_IFDIR => {
                    let source = source_open(source_parent, source_name, &snapshot, true)?;
                    let name = c_name(destination_name)?;
                    stage.create_exclusive(parent_index, || {
                        if unsafe {
                            libc::mkdirat(destination_parent.as_raw_fd(), name.as_ptr(), 0o700)
                        } != 0
                        {
                            Err(io::Error::last_os_error())
                        } else {
                            Ok(())
                        }
                    })?;
                    let destination = open_at(
                        destination_parent,
                        destination_name,
                        libc::O_RDONLY | libc::O_DIRECTORY,
                        0,
                    )?;
                    stage.pin_visible_root(parent_index, &destination)?;
                    let index =
                        stage.record(parent_index, destination_name, &stat_fd(&destination)?);
                    let mut children = DirectoryCursor::new(&source)?;
                    while let Some(child) = children.next(self.control)? {
                        self.node(
                            &source,
                            &child,
                            &destination,
                            &child,
                            Some(index),
                            &child_path(source_path, &child)?,
                            &child_path(destination_path, &child)?,
                            depth + 1,
                            None,
                            stage,
                        )?;
                    }
                    verify_snapshot(&snapshot, &stat_fd(&source)?)?;
                    verify_snapshot(&snapshot, &stat_at(source_parent, source_name)?)?;
                    metadata_fd(&destination, &snapshot, self.options)?;
                    self.report.directories_copied += 1;
                    self.notify(source_path, destination_path, CopyMethod::Directory);
                }
                libc::S_IFLNK => {
                    if stage.visible
                        && self.options.preserve_times
                        && self.options.preserve_symlink_times
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "Visible copy cannot preserve symlink timestamps without a backend-specific no-follow setter; choose explicit omission",
                        ));
                    }
                    if self.options.preserve_times && self.options.preserve_symlink_times {
                        // Some FUSE backends (including SSHFS) follow the target
                        // despite AT_SYMLINK_NOFOLLOW. Never call that setter on
                        // an unvalidated FUSE backend: omission must be explicit.
                        let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
                        if unsafe {
                            libc::fstatfs(destination_parent.as_raw_fd(), filesystem.as_mut_ptr())
                        } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        // Linux include/uapi/linux/magic.h: FUSE_SUPER_MAGIC.
                        if unsafe { filesystem.assume_init() }.f_type as u64 == 0x65735546 {
                            return Err(io::Error::new(
                                io::ErrorKind::Unsupported,
                                "FUSE symlink timestamp no-follow is not validated; choose explicit omission before copying links",
                            ));
                        }
                    }
                    let name = c_name(source_name)?;
                    let mut bytes = vec![0u8; self.options.max_symlink_bytes + 1];
                    let (fd, name) = root_pin
                        .map_or((source_parent.as_raw_fd(), name.as_c_str()), |pin| {
                            (pin.as_raw_fd(), c"")
                        });
                    let length = unsafe {
                        libc::readlinkat(fd, name.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len())
                    };
                    if length < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    let length = length as usize;
                    if length > self.options.max_symlink_bytes {
                        return Err(invalid("Symlink target limit exceeded"));
                    }
                    bytes.truncate(length);
                    verify_snapshot(&snapshot, &stat_at(source_parent, source_name)?)?;
                    let target =
                        CString::new(bytes).map_err(|_| invalid("NUL in symlink target"))?;
                    let destination = c_name(destination_name)?;
                    stage.create_exclusive(parent_index, || {
                        if unsafe {
                            libc::symlinkat(
                                target.as_ptr(),
                                destination_parent.as_raw_fd(),
                                destination.as_ptr(),
                            )
                        } != 0
                        {
                            Err(io::Error::last_os_error())
                        } else {
                            Ok(())
                        }
                    })?;
                    let visible_link = if stage.visible {
                        let file = open_at(destination_parent, destination_name, libc::O_PATH, 0)?;
                        if stat_fd(&file)?.st_mode & libc::S_IFMT != libc::S_IFLNK {
                            return Err(io::Error::other(
                                "Visible symlink replaced before metadata",
                            ));
                        }
                        stage.pin_visible_root(parent_index, &file)?;
                        Some(file)
                    } else {
                        None
                    };
                    let link_stat = match &visible_link {
                        Some(fd) => stat_fd(fd)?,
                        None => stat_at(destination_parent, destination_name)?,
                    };
                    stage.record(parent_index, destination_name, &link_stat);
                    if self.options.preserve_times && self.options.preserve_symlink_times {
                        let times = timestamps(&snapshot);
                        if unsafe {
                            libc::utimensat(
                                destination_parent.as_raw_fd(),
                                destination.as_ptr(),
                                times.as_ptr(),
                                libc::AT_SYMLINK_NOFOLLOW,
                            )
                        } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    self.report.symlink_times_omitted += u64::from(
                        self.options.preserve_times && !self.options.preserve_symlink_times,
                    );
                    self.report.symlinks_copied += 1;
                    self.notify(source_path, destination_path, CopyMethod::Symlink);
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "Only regular files, directories and symlinks can be copied",
                    ));
                }
            }
            self.control.checkpoint()
        }
        fn data(
            &mut self,
            source: &mut File,
            destination: &mut File,
            size: u64,
            source_path: &Path,
            destination_path: &Path,
        ) -> io::Result<CopyMethod> {
            self.control.checkpoint()?;
            if self.options.strategy == CopyStrategy::Auto {
                match self.backend.reflink(source, destination) {
                    Ok(()) => {
                        if stat_fd(destination)?.st_size != size as i64 {
                            return Err(io::Error::other(
                                "Reflink length differs from source snapshot",
                            ));
                        }
                        self.report.bytes_copied += size;
                        self.notify(source_path, destination_path, CopyMethod::Reflink);
                        self.control.checkpoint()?;
                        return Ok(CopyMethod::Reflink);
                    }
                    Err(e) if unsupported_transfer(&e) => {
                        destination.set_len(0)?;
                        source.seek(SeekFrom::Start(0))?;
                        destination.seek(SeekFrom::Start(0))?;
                    }
                    Err(e) => return Err(e),
                }
            }
            let mut copied = 0u64;
            let mut method = if self.options.strategy == CopyStrategy::Auto {
                CopyMethod::Kernel
            } else {
                CopyMethod::Stream
            };
            while copied < size {
                self.control.checkpoint()?;
                let length = (size - copied).min(self.options.chunk_bytes as u64) as usize;
                let count = if method == CopyMethod::Kernel {
                    match self.backend.copy_range(source, destination, length) {
                        Ok(0) => {
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "Source shortened during kernel copy",
                            ));
                        }
                        Ok(n) if n <= length => n,
                        Ok(_) => {
                            return Err(io::Error::other("Kernel copy exceeded requested chunk"));
                        }
                        Err(e) if unsupported_transfer(&e) => {
                            method = CopyMethod::Stream;
                            continue;
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e),
                    }
                } else {
                    let count = match source.read(&mut self.buffer[..length]) {
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        result => result?,
                    };
                    if count == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Source shortened during stream copy",
                        ));
                    }
                    destination.write_all(&self.buffer[..count])?;
                    count
                };
                copied += count as u64;
                self.report.bytes_copied += count as u64;
                self.notify(source_path, destination_path, method);
            }
            Ok(method)
        }
    }

    #[cfg(test)]
    mod accounting_tests {
        use super::*;
        #[test]
        fn registry_budget_fails_before_capacity_or_name_allocation() {
            static ID: AtomicU64 = AtomicU64::new(0);
            let root = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/files-test");
            std::fs::create_dir_all(&root).unwrap();
            let fixture = loop {
                let path = root.join(format!(
                    "copy-budget-{}-{}",
                    std::process::id(),
                    ID.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("Fixture: {e}"),
                }
            };
            let parent = open_directory(&fixture).unwrap();
            let mut stage = Stage::create(&parent, OsStr::new("destination")).unwrap();
            let mut budget = AuxiliaryBudget {
                limit: 1,
                used: 0,
                peak: 0,
            };
            assert!(
                stage
                    .reserve_record(OsStr::new("payload"), &mut budget)
                    .is_err()
            );
            assert_eq!(stage.nodes.capacity(), 0);
            assert_eq!(stage.accounted_bytes, 0);
            assert_eq!(budget.used, 0);
            assert_eq!(budget.peak, 0);
            let cleanup = stage.cleanup(&parent, 1);
            assert!(cleanup.paths.is_empty());
            assert!(cleanup.errors.is_empty());
            std::fs::remove_dir(&fixture).unwrap();
        }
        #[test]
        fn details_bound_unicode_text_paths_and_count_every_omission() {
            let mut details = Details::default();
            let long = "é".repeat(2048);
            for _ in 0..10 {
                details.error(1, format_args!("Error: {long}"));
            }
            assert_eq!(details.errors.len(), 1);
            assert_eq!(details.errors_omitted, 9);
            assert_eq!(details.truncated, 1);
            assert!(details.errors[0].len() <= DETAIL_BYTES);
            assert!(details.errors[0].ends_with(" [truncated]"));
            details.path(1, PathBuf::from("first"));
            details.path(1, PathBuf::from("second"));
            details.path(1, PathBuf::from("x".repeat(PATH_BYTES + 1)));
            assert_eq!(details.paths.len(), 1);
            assert_eq!(details.paths_omitted, 2);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::{Backend, run};
    use super::*;
    use std::ffi::OsString;
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::sync::atomic::AtomicU64;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let root =
                PathBuf::from(std::env::var_os("HOME").expect("Cache fixture requires HOME"))
                    .join(".cache/files-test");
            fs::create_dir_all(&root).unwrap();
            loop {
                let path = root.join(format!(
                    "copy-{}-{}",
                    std::process::id(),
                    ID.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("Create fixture: {e}"),
                }
            }
        }
        fn request(&self, from: &str, to: &str) -> CopyRequest {
            CopyRequest {
                source: self.0.join(from),
                destination: self.0.join(to),
            }
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            fs::write(self.0.join(name), bytes).unwrap();
        }
        fn no_staging(&self) -> bool {
            !fs::read_dir(&self.0).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".files-copy-")
            })
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn stream() -> CopyOptions {
        CopyOptions {
            strategy: CopyStrategy::Stream,
            chunk_bytes: 4096,
            ..Default::default()
        }
    }

    #[test]
    fn selected_copy_rejects_replacement_after_capture_before_any_staging() {
        let f = Fixture::new();
        f.write("a", b"old");
        let names: std::sync::Arc<[OsString]> = vec!["a".into()].into();
        let selected =
            crate::selection_source::Source::capture(f.0.clone(), names, &AtomicBool::new(false))
                .unwrap();
        fs::rename(f.0.join("a"), f.0.join("old-a")).unwrap();
        f.write("a", b"new");
        let error = copy_paths_selected(
            &[f.request("a", "renamed")],
            &stream(),
            &Control::default(),
            &selected,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.report.completed_count, 0);
        assert!(!f.0.join("renamed").exists());
        assert!(f.no_staging());
        assert_eq!(fs::read(f.0.join("a")).unwrap(), b"new");
        assert_eq!(fs::read(f.0.join("old-a")).unwrap(), b"old");
    }
    #[test]
    fn selected_copy_new_destination_name_and_symlink_use_original_root_identity() {
        let f = Fixture::new();
        f.write("a", b"bytes");
        std::os::unix::fs::symlink("a", f.0.join("link")).unwrap();
        let names: std::sync::Arc<[OsString]> = vec!["a".into(), "link".into()].into();
        let selected =
            crate::selection_source::Source::capture(f.0.clone(), names, &AtomicBool::new(false))
                .unwrap();
        let r = copy_paths_selected(
            &[
                f.request("a", "other-name"),
                f.request("link", "other-link"),
            ],
            &stream(),
            &Control::default(),
            &selected,
            |_| {},
        )
        .unwrap();
        assert_eq!(r.completed_count, 2);
        assert_eq!(fs::read(f.0.join("other-name")).unwrap(), b"bytes");
        assert_eq!(
            fs::read_link(f.0.join("other-link")).unwrap(),
            PathBuf::from("a")
        );
        assert!(f.no_staging());
    }

    #[test]
    fn selected_copy_checks_later_root_after_earlier_progress_not_only_at_dispatch() {
        let f = Fixture::new();
        f.write("a", &[1; 8192]);
        f.write("b", b"old-b");
        let selected = crate::selection_source::Source::capture(
            f.0.clone(),
            vec!["a".into(), "b".into()].into(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut swapped = false;
        let error = copy_paths_selected(
            &[f.request("a", "copy-a"), f.request("b", "copy-b")],
            &stream(),
            &Control::default(),
            &selected,
            |_| {
                if !swapped {
                    fs::rename(f.0.join("b"), f.0.join("old-b")).unwrap();
                    f.write("b", b"new-b");
                    swapped = true;
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.report.completed_count, 1);
        assert_eq!(fs::read(f.0.join("copy-a")).unwrap(), vec![1; 8192]);
        assert!(!f.0.join("copy-b").exists());
        assert_eq!(fs::read(f.0.join("b")).unwrap(), b"new-b");
        assert!(f.no_staging());
    }
    #[test]
    fn stream_bytes_metadata_and_empty_file() {
        let f = Fixture::new();
        let bytes: Vec<u8> = (0..50_123).map(|n| (n % 251) as u8).collect();
        f.write("source", &bytes);
        f.write("empty", b"");
        fs::set_permissions(f.0.join("source"), fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::metadata(f.0.join("source")).unwrap();
        let report = copy_paths(
            &[
                f.request("source", "copy"),
                f.request("empty", "empty-copy"),
            ],
            &stream(),
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), bytes);
        assert_eq!(fs::read(f.0.join("source")).unwrap(), bytes);
        let after = fs::metadata(f.0.join("copy")).unwrap();
        assert_eq!(before.mode() & 0o777, after.mode() & 0o777);
        assert_eq!(
            (before.mtime(), before.mtime_nsec()),
            (after.mtime(), after.mtime_nsec())
        );
        assert_eq!(report.bytes_copied, bytes.len() as u64);
        assert_eq!(report.completed.len(), 2);
        assert_eq!(report.files_copied, 2);
        assert!(report.cleanup_paths.is_empty());
        assert!(f.no_staging());
    }

    #[test]
    fn directory_symlinks_invalid_utf8_never_followed() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("source/sub")).unwrap();
        f.write("outside", b"untouched");
        f.write("source/sub/file", b"nested");
        let non_utf8 = OsString::from_vec(vec![b'x', 255]);
        fs::write(f.0.join("source").join(&non_utf8), b"lossless").unwrap();
        std::os::unix::fs::symlink("../../outside", f.0.join("source/sub/link")).unwrap();
        std::os::unix::fs::symlink("missing", f.0.join("source/dangling")).unwrap();
        let report = copy_paths(
            &[f.request("source", "copy")],
            &stream(),
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(f.0.join("copy/sub/file")).unwrap(), b"nested");
        assert_eq!(
            fs::read(f.0.join("copy").join(non_utf8)).unwrap(),
            b"lossless"
        );
        assert_eq!(
            fs::read_link(f.0.join("copy/sub/link")).unwrap(),
            PathBuf::from("../../outside")
        );
        assert!(
            fs::symlink_metadata(f.0.join("copy/dangling"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(f.0.join("outside")).unwrap(), b"untouched");
        assert_eq!(report.symlinks_copied, 2);
        assert_eq!(report.directories_copied, 2);
        assert!(f.no_staging());
    }

    #[test]
    fn cancellation_discards_only_staged_copy() {
        let f = Fixture::new();
        f.write("source", &[42; 32 * 1024]);
        f.write("unrelated", b"keep");
        let control = Control::default();
        let observer = control.clone();
        let error = copy_paths(
            &[f.request("source", "copy")],
            &stream(),
            &control,
            move |p| {
                if p.bytes_copied >= 4096 {
                    observer.cancel();
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
        assert!(error.rescue_paths.is_empty());
        assert!(error.cleanup_errors.is_empty());
        assert!(!f.0.join("copy").exists());
        assert_eq!(fs::read(f.0.join("unrelated")).unwrap(), b"keep");
        assert_eq!(fs::metadata(f.0.join("source")).unwrap().len(), 32 * 1024);
        assert!(f.no_staging());
    }

    #[test]
    fn destination_race_no_clobber_source_unchanged() {
        let f = Fixture::new();
        f.write("source", &[17; 8192]);
        let mut inserted = false;
        let error = copy_paths(
            &[f.request("source", "copy")],
            &stream(),
            &Control::default(),
            |_| {
                if !inserted {
                    f.write("copy", b"new occupant");
                    inserted = true;
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), b"new occupant");
        assert_eq!(fs::read(f.0.join("source")).unwrap(), vec![17; 8192]);
        assert!(f.no_staging());
    }

    #[test]
    fn skip_existing_root_continues_without_merging_or_following_destination_link() {
        let f = Fixture::new();
        f.write("a", b"source-a");
        f.write("b", b"source-b");
        f.write("actor", b"occupant");
        std::os::unix::fs::symlink("actor", f.0.join("copy-a")).unwrap();
        let opts = CopyOptions {
            existing_destination: ExistingDestination::Skip,
            ..stream()
        };
        let r = copy_paths(
            &[f.request("a", "copy-a"), f.request("b", "copy-b")],
            &opts,
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!((r.skipped_count, r.completed_count), (1, 1));
        assert_eq!(r.skipped, [f.0.join("copy-a")]);
        assert_eq!(r.skipped[0].capacity(), 4097);
        assert_eq!(fs::read(f.0.join("actor")).unwrap(), b"occupant");
        assert!(
            fs::symlink_metadata(f.0.join("copy-a"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(f.0.join("copy-b")).unwrap(), b"source-b");
        assert!(f.no_staging());
        let error = copy_paths(
            &[f.request("missing", "copy-a")],
            &opts,
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::NotFound);
        assert_eq!(error.report.skipped_count, 0);
        let error =
            copy_paths(&[f.request("a", "a")], &opts, &Control::default(), |_| {}).unwrap_err();
        assert_eq!(error.report.skipped_count, 0);
    }
    #[test]
    fn skip_publication_race_only_after_confirmed_cleanup_then_continues() {
        let f = Fixture::new();
        f.write("a", &[17; 8192]);
        f.write("b", b"source-b");
        let mut inserted = false;
        let opts = CopyOptions {
            existing_destination: ExistingDestination::Skip,
            ..stream()
        };
        let r = copy_paths(
            &[f.request("a", "copy-a"), f.request("b", "copy-b")],
            &opts,
            &Control::default(),
            |_| {
                if !inserted {
                    f.write("copy-a", b"actor");
                    inserted = true;
                }
            },
        )
        .unwrap();
        assert_eq!((r.skipped_count, r.completed_count), (1, 1));
        assert_eq!(fs::read(f.0.join("copy-a")).unwrap(), b"actor");
        assert_eq!(fs::read(f.0.join("a")).unwrap(), vec![17; 8192]);
        assert_eq!(fs::read(f.0.join("copy-b")).unwrap(), b"source-b");
        assert!(r.bytes_copied >= 8192);
        assert!(f.no_staging());
    }
    #[test]
    fn skip_publication_race_with_unclean_stage_is_error_and_preserves_actor_data() {
        let f = Fixture::new();
        f.write("a", &[17; 8192]);
        f.write("b", b"source-b");
        let mut actor = None;
        let opts = CopyOptions {
            existing_destination: ExistingDestination::Skip,
            ..stream()
        };
        let e = copy_paths(
            &[f.request("a", "copy-a"), f.request("b", "copy-b")],
            &opts,
            &Control::default(),
            |_| {
                if actor.is_none() {
                    f.write("copy-a", b"occupant");
                    let stage = fs::read_dir(&f.0)
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .find(|p| {
                            p.file_name()
                                .unwrap()
                                .as_encoded_bytes()
                                .starts_with(b".files-copy-")
                        })
                        .unwrap();
                    let p = stage.join("actor-data");
                    fs::write(&p, b"preserve me").unwrap();
                    actor = Some(p);
                }
            },
        )
        .unwrap_err();
        assert_eq!(e.cause.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(e.report.skipped_count, 0);
        assert!(!e.cleanup_errors.is_empty() || e.cleanup_unresolved_entries > 0);
        assert!(!f.0.join("copy-b").exists());
        assert_eq!(fs::read(actor.unwrap()).unwrap(), b"preserve me");
        assert_eq!(fs::read(f.0.join("copy-a")).unwrap(), b"occupant");
    }

    #[test]
    fn skip_directory_never_merges_and_skipped_roots_still_obey_entry_budget() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("source-dir")).unwrap();
        fs::create_dir(f.0.join("occupied-dir")).unwrap();
        f.write("source-dir/new", b"new source");
        f.write("occupied-dir/actor", b"keep");
        f.write("b", b"b");
        f.write("occupied-b", b"actor");
        let opts = CopyOptions {
            existing_destination: ExistingDestination::Skip,
            ..stream()
        };
        let r = copy_paths(
            &[
                f.request("source-dir", "occupied-dir"),
                f.request("b", "copy-b"),
            ],
            &opts,
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            (r.skipped_count, r.completed_count, r.directories_copied),
            (1, 1, 0)
        );
        assert!(!f.0.join("occupied-dir/new").exists());
        assert_eq!(fs::read(f.0.join("occupied-dir/actor")).unwrap(), b"keep");
        let opts = CopyOptions {
            max_entries: 1,
            ..opts
        };
        let e = copy_paths(
            &[
                f.request("source-dir", "occupied-dir"),
                f.request("b", "occupied-b"),
            ],
            &opts,
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert!(e.cause.to_string().contains("entry limit"));
        assert_eq!(e.report.skipped_count, 1);
        assert!(f.no_staging());
    }
    #[test]
    fn skipped_and_completed_detail_payloads_share_cap_with_explicit_omissions() {
        let f = Fixture::new();
        for n in ["a", "b", "c"] {
            f.write(n, b"data");
        }
        f.write("copy-a", b"actor");
        f.write("copy-c", b"actor");
        let opts = CopyOptions {
            existing_destination: ExistingDestination::Skip,
            max_report_details: 1,
            ..stream()
        };
        let r = copy_paths(
            &[
                f.request("a", "copy-a"),
                f.request("b", "copy-b"),
                f.request("c", "copy-c"),
            ],
            &opts,
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!((r.skipped_count, r.completed_count), (2, 1));
        assert_eq!(r.skipped.len() + r.completed.len(), 1);
        assert_eq!((r.skipped_omitted, r.completed_omitted), (1, 1));
        assert!(f.no_staging());
    }
    #[test]
    fn source_mutation_rejected_before_publication() {
        let f = Fixture::new();
        f.write("source", &[1; 16 * 1024]);
        let mut changed = false;
        let error = copy_paths(
            &[f.request("source", "copy")],
            &stream(),
            &Control::default(),
            |p| {
                if !changed && p.bytes_copied > 0 {
                    f.write("source", &[2; 16 * 1024]);
                    changed = true;
                }
            },
        )
        .unwrap_err();
        assert!(error.cause.to_string().contains("Source changed"));
        assert!(!f.0.join("copy").exists());
        assert_eq!(fs::read(f.0.join("source")).unwrap(), vec![2; 16 * 1024]);
        assert!(f.no_staging());
    }

    struct UnsupportedBackend;
    impl Backend for UnsupportedBackend {
        fn reflink(&mut self, _: &File, _: &File) -> io::Result<()> {
            Err(io::Error::from_raw_os_error(libc::EOPNOTSUPP))
        }
        fn copy_range(&mut self, _: &File, _: &File, _: usize) -> io::Result<usize> {
            Err(io::Error::from_raw_os_error(libc::EXDEV))
        }
    }
    #[test]
    fn unsupported_reflink_and_kernel_fall_back_to_bounded_stream() {
        let f = Fixture::new();
        f.write("source", &[19; 20_000]);
        let report = run(
            &[f.request("source", "copy")],
            &CopyOptions::default(),
            &Control::default(),
            |_| {},
            UnsupportedBackend,
        )
        .unwrap();
        assert_eq!(report.stream_files, 1);
        assert_eq!(report.reflink_files, 0);
        assert_eq!(report.kernel_files, 0);
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), vec![19; 20_000]);
        assert!(f.no_staging());
    }
    struct PartialKernel {
        calls: usize,
    }
    impl Backend for PartialKernel {
        fn reflink(&mut self, _: &File, _: &File) -> io::Result<()> {
            Err(io::Error::from_raw_os_error(libc::ENOTTY))
        }
        fn copy_range(
            &mut self,
            source: &File,
            destination: &File,
            length: usize,
        ) -> io::Result<usize> {
            self.calls += 1;
            if self.calls > 1 {
                return Err(io::Error::from_raw_os_error(libc::EXDEV));
            }
            let mut source = source;
            let mut destination = destination;
            let mut bytes = vec![0; length.min(2000)];
            let count = source.read(&mut bytes)?;
            destination.write_all(&bytes[..count])?;
            Ok(count)
        }
    }
    #[test]
    fn partial_kernel_copy_then_stream_uses_current_offsets() {
        let f = Fixture::new();
        let bytes: Vec<u8> = (0..12_345).map(|n| (n % 239) as u8).collect();
        f.write("source", &bytes);
        let report = run(
            &[f.request("source", "copy")],
            &CopyOptions::default(),
            &Control::default(),
            |_| {},
            PartialKernel { calls: 0 },
        )
        .unwrap();
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), bytes);
        assert_eq!(report.bytes_copied, 12_345);
        assert_eq!(report.stream_files, 1);
    }

    #[test]
    fn byte_depth_entry_bounds_and_self_copy_leave_no_destination() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("source/sub/deep")).unwrap();
        f.write("source/file", b"12345");
        for options in [
            CopyOptions {
                max_total_bytes: 4,
                ..stream()
            },
            CopyOptions {
                max_depth: 1,
                ..stream()
            },
            CopyOptions {
                max_entries: 1,
                ..stream()
            },
        ] {
            assert!(
                copy_paths(
                    &[f.request("source", "copy")],
                    &options,
                    &Control::default(),
                    |_| {}
                )
                .is_err()
            );
            assert!(!f.0.join("copy").exists());
            assert!(f.no_staging());
        }
        assert!(
            copy_paths(
                &[f.request("source", "source/copy")],
                &stream(),
                &Control::default(),
                |_| {}
            )
            .is_err()
        );
        assert!(
            copy_paths(
                &[f.request("source", "source")],
                &stream(),
                &Control::default(),
                |_| {}
            )
            .is_err()
        );
        assert!(f.no_staging());
    }

    #[test]
    fn copied_readonly_directories_are_cleaned_on_cancel() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("source/sub")).unwrap();
        f.write("source/sub/file", b"one");
        fs::set_permissions(f.0.join("source/sub"), fs::Permissions::from_mode(0o500)).unwrap();
        let control = Control::default();
        let observer = control.clone();
        let error = copy_paths(
            &[f.request("source", "copy")],
            &stream(),
            &control,
            move |p| {
                if p.method == CopyMethod::Directory {
                    observer.cancel();
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
        assert!(error.cleanup_errors.is_empty());
        assert!(f.no_staging());
        // Restore only our fixture so Drop can remove it as an unprivileged user.
        fs::set_permissions(f.0.join("source/sub"), fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn moved_staging_replacement_is_retained_and_actual_rescue_reported() {
        let f = Fixture::new();
        f.write("source", &[5; 8192]);
        let control = Control::default();
        let observer = control.clone();
        let mut replacement = None;
        let relocated = f.0.join("relocated-stage");
        let error = copy_paths(&[f.request("source", "copy")], &stream(), &control, |p| {
            if p.bytes_copied > 0 && replacement.is_none() {
                let stage = fs::read_dir(&f.0)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .find(|p| {
                        p.file_name()
                            .unwrap()
                            .as_encoded_bytes()
                            .starts_with(b".files-copy-")
                    })
                    .unwrap();
                fs::rename(&stage, &relocated).unwrap();
                fs::create_dir(&stage).unwrap();
                replacement = Some(stage);
                observer.cancel();
            }
        })
        .unwrap_err();
        assert!(replacement.unwrap().is_dir());
        assert!(error.rescue_paths.contains(&relocated));
        assert!(!error.cleanup_errors.is_empty());
        assert_eq!(fs::read_dir(&relocated).unwrap().count(), 0);
        assert_eq!(fs::read(f.0.join("source")).unwrap(), vec![5; 8192]);
    }

    #[test]
    fn partial_job_reports_previous_commits() {
        let f = Fixture::new();
        f.write("one", b"one");
        f.write("two", b"two");
        f.write("occupied", b"keep");
        let error = copy_paths(
            &[f.request("one", "copied"), f.request("two", "occupied")],
            &stream(),
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.report.completed, vec![f.0.join("copied")]);
        assert_eq!(fs::read(f.0.join("occupied")).unwrap(), b"keep");
        assert!(f.no_staging());
    }

    #[test]
    fn paused_worker_cancels_without_waiting_for_resume() {
        let f = Fixture::new();
        f.write("source", b"one");
        let request = f.request("source", "copy");
        let control = Control::default();
        control.pause();
        let worker_control = control.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            copy_paths(&[request], &stream(), &worker_control, |_| {})
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        control.cancel();
        assert_eq!(
            worker.join().unwrap().unwrap_err().cause.kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!f.0.join("copy").exists());
        assert!(f.no_staging());
    }
    #[test]
    fn automatic_system_transfer_preserves_bytes() {
        let f = Fixture::new();
        let bytes: Vec<u8> = (0..30_017).map(|n| (n % 233) as u8).collect();
        f.write("source", &bytes);
        let report = copy_paths(
            &[f.request("source", "copy")],
            &CopyOptions::default(),
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), bytes);
        assert_eq!(
            report.reflink_files + report.kernel_files + report.stream_files,
            1
        );
        assert_eq!(report.bytes_copied, 30_017);
        assert!(f.no_staging());
    }

    #[test]
    fn unrelated_staged_replacement_is_not_deleted() {
        let f = Fixture::new();
        f.write("source", &[9; 8192]);
        let control = Control::default();
        let observer = control.clone();
        let mut replacement = None;
        let error = copy_paths(&[f.request("source", "copy")], &stream(), &control, |p| {
            if p.bytes_copied > 0 && replacement.is_none() {
                let stage = fs::read_dir(&f.0)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .find(|p| {
                        p.file_name()
                            .unwrap()
                            .as_encoded_bytes()
                            .starts_with(b".files-copy-")
                    })
                    .unwrap();
                let payload = stage.join("payload");
                fs::rename(&payload, f.0.join("relocated-partial")).unwrap();
                fs::write(&payload, b"actor replacement").unwrap();
                replacement = Some(payload);
                observer.cancel();
            }
        })
        .unwrap_err();
        assert_eq!(
            fs::read(replacement.unwrap()).unwrap(),
            b"actor replacement"
        );
        assert!(!error.cleanup_errors.is_empty());
        assert!(!error.rescue_paths.is_empty());
        assert_eq!(fs::read(f.0.join("source")).unwrap(), vec![9; 8192]);
        assert!(!f.0.join("copy").exists());
    }

    #[test]
    fn paused_worker_resumes_and_completes() {
        let f = Fixture::new();
        f.write("source", &[3; 16384]);
        let request = f.request("source", "copy");
        let control = Control::default();
        let worker_control = control.clone();
        let observer = control.clone();
        let (paused_tx, paused_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut paused_once = false;
            let result = copy_paths(&[request], &stream(), &worker_control, move |_| {
                if !paused_once {
                    observer.pause();
                    paused_tx.send(()).unwrap();
                    paused_once = true;
                }
            });
            done_tx.send(result).unwrap();
        });
        paused_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(done_rx.try_recv().is_err());
        control.resume();
        let report = done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(report.files_copied, 1);
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), vec![3; 16384]);
    }

    #[test]
    fn symlink_limit_and_special_files_are_explicit_errors() {
        let f = Fixture::new();
        std::os::unix::fs::symlink("a-target-that-is-too-long", f.0.join("link")).unwrap();
        let options = CopyOptions {
            max_symlink_bytes: 4,
            ..stream()
        };
        assert!(
            copy_paths(
                &[f.request("link", "copy")],
                &options,
                &Control::default(),
                |_| {}
            )
            .is_err()
        );
        let fifo = std::ffi::CString::new(f.0.join("fifo").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let error = copy_paths(
            &[f.request("fifo", "copy")],
            &stream(),
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::Unsupported);
        assert!(f.no_staging());
        assert!(!f.0.join("copy").exists());
    }
    #[test]
    fn nested_staged_replacement_prevents_publication_and_survives_cleanup() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("source/sub")).unwrap();
        f.write("source/sub/file", &[8; 8192]);
        let mut replacement = None;
        let error = copy_paths(
            &[f.request("source", "copy")],
            &stream(),
            &Control::default(),
            |p| {
                if p.bytes_copied > 0 && replacement.is_none() {
                    let stage = fs::read_dir(&f.0)
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .find(|p| {
                            p.file_name()
                                .unwrap()
                                .as_encoded_bytes()
                                .starts_with(b".files-copy-")
                        })
                        .unwrap();
                    let payload = stage.join("payload/sub/file");
                    fs::rename(&payload, f.0.join("relocated-partial")).unwrap();
                    fs::write(&payload, b"actor replacement").unwrap();
                    replacement = Some(payload);
                }
            },
        )
        .unwrap_err();
        assert!(
            error
                .cause
                .to_string()
                .contains("Staged copy entry identity changed")
        );
        assert_eq!(
            fs::read(replacement.unwrap()).unwrap(),
            b"actor replacement"
        );
        assert!(!f.0.join("copy").exists());
        assert!(!error.cleanup_errors.is_empty());
        assert_eq!(
            fs::read(f.0.join("source/sub/file")).unwrap(),
            vec![8; 8192]
        );
    }
    #[test]
    fn tiny_auxiliary_budget_rejects_before_staging() {
        let f = Fixture::new();
        f.write("source", b"data");
        let options = CopyOptions {
            max_auxiliary_bytes: 1,
            max_depth: 0,
            max_symlink_bytes: 1,
            max_report_details: 1,
            ..stream()
        };
        let error = copy_paths(
            &[f.request("source", "copy")],
            &options,
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert!(
            error
                .cause
                .to_string()
                .contains("auxiliary accounting limit")
        );
        assert_eq!(error.report.auxiliary_accounted_peak_bytes, 0);
        assert!(!f.0.join("copy").exists());
        assert!(f.no_staging());
    }

    #[test]
    fn request_and_hard_auxiliary_bounds_precede_any_filesystem_copy() {
        let f = Fixture::new();
        let cases = [
            CopyOptions {
                max_requests: 1,
                ..stream()
            },
            CopyOptions {
                max_request_path_bytes: 1,
                ..stream()
            },
            CopyOptions {
                max_auxiliary_bytes: 32 * 1024 * 1024 + 1,
                ..stream()
            },
            CopyOptions {
                max_report_details: 129,
                ..stream()
            },
        ];
        for options in cases {
            let error = copy_paths(
                &[
                    f.request("missing-one", "copy-one"),
                    f.request("missing-two", "copy-two"),
                ],
                &options,
                &Control::default(),
                |_| {},
            )
            .unwrap_err();
            assert_ne!(error.cause.kind(), io::ErrorKind::NotFound);
            assert!(f.no_staging());
        }
        let request = CopyRequest {
            source: PathBuf::from("x".repeat(4097)),
            destination: f.0.join("copy"),
        };
        let error = copy_paths(&[request], &stream(), &Control::default(), |_| {}).unwrap_err();
        assert!(error.cause.to_string().contains("path byte limit"));
        assert!(f.no_staging());
    }

    #[test]
    fn completed_report_is_capped_with_explicit_total_and_omitted_count() {
        let f = Fixture::new();
        for name in ["one", "two", "three"] {
            f.write(name, b"data");
        }
        let options = CopyOptions {
            max_auxiliary_bytes: 1024 * 1024,
            max_depth: 0,
            max_symlink_bytes: 1,
            max_report_details: 1,
            ..stream()
        };
        let report = copy_paths(
            &[
                f.request("one", "one-copy"),
                f.request("two", "two-copy"),
                f.request("three", "three-copy"),
            ],
            &options,
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.completed_count, 3);
        assert_eq!(report.completed.len(), 1);
        assert_eq!(report.completed_omitted, 2);
        assert!(report.auxiliary_accounted_peak_bytes <= options.max_auxiliary_bytes);
        assert!(report.auxiliary_accounted_peak_bytes > 0);
        assert!(f.no_staging());
        for name in ["one-copy", "two-copy", "three-copy"] {
            assert_eq!(fs::read(f.0.join(name)).unwrap(), b"data");
        }
    }

    #[test]
    fn many_cleanup_conflicts_keep_replacements_with_bounded_root_report() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("source")).unwrap();
        fs::create_dir(f.0.join("kept")).unwrap();
        for n in 0..6 {
            f.write(&format!("source/f{n}"), b"original");
        }
        let options = CopyOptions {
            max_auxiliary_bytes: 2 * 1024 * 1024,
            max_depth: 2,
            max_report_details: 1,
            ..stream()
        };
        let mut replaced = std::collections::HashSet::new();
        let mut stage_root = None;
        let error = copy_paths(
            &[f.request("source", "copy")],
            &options,
            &Control::default(),
            |progress| {
                if progress.method != CopyMethod::Stream {
                    return;
                }
                let name = progress.source.file_name().unwrap().to_os_string();
                if !replaced.insert(name.clone()) {
                    return;
                }
                let stage = fs::read_dir(&f.0)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .find(|p| {
                        p.file_name()
                            .unwrap()
                            .as_encoded_bytes()
                            .starts_with(b".files-copy-")
                    })
                    .unwrap();
                let payload = stage.join("payload").join(&name);
                fs::rename(&payload, f.0.join("kept").join(&name)).unwrap();
                fs::write(&payload, b"actor replacement").unwrap();
                stage_root = Some(stage);
            },
        )
        .unwrap_err();
        let stage = stage_root.unwrap();
        assert_eq!(error.cleanup_errors.len(), 1);
        assert!(error.cleanup_errors_omitted >= 6);
        assert!(error.cleanup_unresolved_entries >= 6);
        assert_eq!(error.rescue_paths, vec![stage.clone()]);
        assert_eq!(error.rescue_paths_omitted, 0);
        assert!(!f.0.join("copy").exists());
        for n in 0..6 {
            assert_eq!(
                fs::read(stage.join(format!("payload/f{n}"))).unwrap(),
                b"actor replacement"
            );
            assert_eq!(
                fs::read(f.0.join(format!("source/f{n}"))).unwrap(),
                b"original"
            );
        }
        assert!(error.report.auxiliary_accounted_peak_bytes <= options.max_auxiliary_bytes);
    }
    #[test]
    fn keep_both_duplicates_same_folder_file_directory_raw_and_literal_link() {
        let f = Fixture::new();
        f.write("a.txt", b"original");
        fs::create_dir(f.0.join("dir.txt")).unwrap();
        f.write("dir.txt/child", b"nested");
        std::os::unix::fs::symlink("a.txt", f.0.join("link.txt")).unwrap();
        let raw = OsString::from_vec(b"raw-\xff.txt".to_vec());
        fs::write(f.0.join(&raw), b"raw").unwrap();
        let mut requests = vec![
            f.request("a.txt", "a.txt"),
            f.request("dir.txt", "dir.txt"),
            f.request("link.txt", "link.txt"),
        ];
        requests.push(CopyRequest {
            source: f.0.join(&raw),
            destination: f.0.join(&raw),
        });
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..stream()
        };
        let r = copy_paths(&requests, &options, &Control::default(), |_| {}).unwrap();
        assert_eq!(
            (r.completed_count, r.renamed_count, r.skipped_count),
            (4, 4, 0)
        );
        assert_eq!(fs::read(f.0.join("a.txt")).unwrap(), b"original");
        assert_eq!(fs::read(f.0.join("a (2).txt")).unwrap(), b"original");
        assert_eq!(fs::read(f.0.join("dir.txt/child")).unwrap(), b"nested");
        assert_eq!(fs::read(f.0.join("dir.txt (2)/child")).unwrap(), b"nested");
        assert_eq!(
            fs::read_link(f.0.join("link (2).txt")).unwrap(),
            PathBuf::from("a.txt")
        );
        assert_eq!(
            fs::read(f.0.join(OsString::from_vec(b"raw-\xff (2).txt".to_vec()))).unwrap(),
            b"raw"
        );
        assert_ne!(
            fs::metadata(f.0.join("a.txt")).unwrap().ino(),
            fs::metadata(f.0.join("a (2).txt")).unwrap().ino()
        );
        assert!(f.no_staging());
    }
    #[test]
    fn keep_both_late_collision_reuses_payload_and_reports_actual_published_name() {
        let f = Fixture::new();
        f.write("a", &[17; 8192]);
        let mut inserted = false;
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..stream()
        };
        let r = copy_paths(
            &[f.request("a", "copy")],
            &options,
            &Control::default(),
            |_| {
                if !inserted {
                    f.write("copy", b"actor");
                    f.write("copy (2)", b"actor2");
                    inserted = true;
                }
            },
        )
        .unwrap();
        assert_eq!(
            (
                r.completed_count,
                r.renamed_count,
                r.files_copied,
                r.bytes_copied
            ),
            (1, 1, 1, 8192)
        );
        assert_eq!(r.completed, [f.0.join("copy (3)")]);
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), b"actor");
        assert_eq!(fs::read(f.0.join("copy (2)")).unwrap(), b"actor2");
        assert_eq!(fs::read(f.0.join("copy (3)")).unwrap(), [17; 8192]);
        assert!(f.no_staging());
    }
    #[test]
    fn keep_both_exhaustion_before_staging_never_mutates_occupants() {
        let f = Fixture::new();
        f.write("a", b"source");
        let mut names =
            crate::copy_unique_name::Names::new(std::ffi::OsStr::new("copy"), false, 255).unwrap();
        for index in 1..=crate::copy_unique_name::MAX_CANDIDATES {
            fs::write(f.0.join(names.name()), b"occupant").unwrap();
            if index < crate::copy_unique_name::MAX_CANDIDATES {
                names.advance().unwrap();
            }
        }
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..stream()
        };
        let e = copy_paths(
            &[f.request("a", "copy")],
            &options,
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert!(e.cause.to_string().contains("1000"));
        assert_eq!(
            (
                e.report.completed_count,
                e.report.renamed_count,
                e.report.bytes_copied
            ),
            (0, 0, 0)
        );
        assert_eq!(fs::read(f.0.join("copy (1000)")).unwrap(), b"occupant");
        assert_eq!(fs::read(f.0.join("a")).unwrap(), b"source");
        assert!(f.no_staging());
    }
    #[test]
    fn keep_both_source_change_stops_before_publication() {
        let f = Fixture::new();
        f.write("a", &[17; 8192]);
        f.write("copy", b"occupant");
        let mut changed = false;
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..stream()
        };
        let e = copy_paths(
            &[f.request("a", "copy")],
            &options,
            &Control::default(),
            |_| {
                if !changed {
                    f.write("a", b"actor-source");
                    changed = true;
                }
            },
        )
        .unwrap_err();
        assert_eq!((e.report.completed_count, e.report.renamed_count), (0, 0));
        assert!(!f.0.join("copy (2)").exists());
        assert_eq!(fs::read(f.0.join("a")).unwrap(), b"actor-source");
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), b"occupant");
        assert!(f.no_staging());
    }
    #[test]
    fn keep_both_cancel_discards_private_payload_and_preserves_source() {
        let f = Fixture::new();
        f.write("a", &[17; 8192]);
        f.write("copy", b"occupant");
        let control = Control::default();
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..stream()
        };
        let e = copy_paths(&[f.request("a", "copy")], &options, &control, |_| {
            control.cancel()
        })
        .unwrap_err();
        assert_eq!(e.cause.kind(), io::ErrorKind::Interrupted);
        assert_eq!(e.report.renamed_count, 0);
        assert_eq!(fs::read(f.0.join("a")).unwrap(), [17; 8192]);
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), b"occupant");
        assert!(!f.0.join("copy (2)").exists());
        assert!(f.no_staging());
    }
    #[test]
    fn keep_both_still_rejects_directory_descendants() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("dir/nested")).unwrap();
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..stream()
        };
        assert!(
            copy_paths(
                &[f.request("dir", "dir/nested")],
                &options,
                &Control::default(),
                |_| {}
            )
            .is_err()
        );
        assert!(f.no_staging());
    }
    fn visible_options() -> CopyOptions {
        CopyOptions {
            publication: Publication::VisibleExclusive,
            preserve_symlink_times: false,
            ..stream()
        }
    }
    #[test]
    fn visible_tree_preserves_sources_and_omits_link_times_explicitly() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("tree/nested")).unwrap();
        f.write("tree/nested/data", b"content");
        std::os::unix::fs::symlink("nested/data", f.0.join("tree/link")).unwrap();
        let r = copy_paths(
            &[f.request("tree", "copy")],
            &visible_options(),
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            (
                r.visible_started_count,
                r.visible_completed_count,
                r.visible_uncertain_count
            ),
            (1, 1, 0)
        );
        assert_eq!(r.symlink_times_omitted, 1);
        assert_eq!(fs::read(f.0.join("copy/nested/data")).unwrap(), b"content");
        assert_eq!(
            fs::read_link(f.0.join("copy/link")).unwrap(),
            PathBuf::from("nested/data")
        );
        assert_eq!(fs::read(f.0.join("tree/nested/data")).unwrap(), b"content");
        assert!(f.no_staging());
    }
    #[test]
    fn visible_link_times_require_explicit_omission_before_creation() {
        let f = Fixture::new();
        f.write("target", b"unchanged");
        std::os::unix::fs::symlink("target", f.0.join("link")).unwrap();
        let options = CopyOptions {
            preserve_symlink_times: true,
            ..visible_options()
        };
        let e = copy_paths(
            &[f.request("link", "copy")],
            &options,
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(e.cause.kind(), io::ErrorKind::Unsupported);
        assert_eq!(e.report.visible_started_count, 0);
        assert!(!f.0.join("copy").symlink_metadata().is_ok());
        assert_eq!(fs::read(f.0.join("target")).unwrap(), b"unchanged");
    }
    #[test]
    fn visible_cancel_retains_partial_and_verified_rescue() {
        let f = Fixture::new();
        f.write("source", &[23; 16384]);
        let control = Control::default();
        let e = copy_paths(
            &[f.request("source", "copy")],
            &visible_options(),
            &control,
            |p| {
                if p.bytes_copied > 0 {
                    control.cancel();
                }
            },
        )
        .unwrap_err();
        assert_eq!(e.cause.kind(), io::ErrorKind::Interrupted);
        assert_eq!(
            (
                e.report.visible_started_count,
                e.report.visible_completed_count
            ),
            (1, 0)
        );
        assert!(e.rescue_paths.contains(&f.0.join("copy")));
        assert!(f.0.join("copy").exists());
        assert_eq!(fs::read(f.0.join("source")).unwrap(), [23; 16384]);
        assert!(f.no_staging());
    }
    #[test]
    fn visible_replacement_preserves_actor_and_reports_relocated_owned_root() {
        let f = Fixture::new();
        f.write("source", &[23; 16384]);
        let mut replaced = false;
        let e = copy_paths(
            &[f.request("source", "copy")],
            &visible_options(),
            &Control::default(),
            |p| {
                if p.bytes_copied > 0 && !replaced {
                    fs::rename(f.0.join("copy"), f.0.join("relocated")).unwrap();
                    f.write("copy", b"actor");
                    replaced = true;
                }
            },
        )
        .unwrap_err();
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), b"actor");
        assert!(e.rescue_paths.contains(&f.0.join("relocated")));
        assert_eq!(e.report.visible_completed_count, 0);
        assert_eq!(fs::read(f.0.join("source")).unwrap(), [23; 16384]);
    }
    #[test]
    fn visible_keep_both_reports_actual_name_without_touching_occupant() {
        let f = Fixture::new();
        f.write("source", b"new");
        f.write("copy", b"occupant");
        let options = CopyOptions {
            existing_destination: ExistingDestination::KeepBoth,
            ..visible_options()
        };
        let r = copy_paths(
            &[f.request("source", "copy")],
            &options,
            &Control::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(r.completed, vec![f.0.join("copy (2)")]);
        assert_eq!(r.renamed_count, 1);
        assert_eq!(fs::read(f.0.join("copy")).unwrap(), b"occupant");
        assert_eq!(fs::read(f.0.join("copy (2)")).unwrap(), b"new");
    }
    #[test]
    fn visible_child_collision_is_not_a_skipped_root_or_rollback() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("tree")).unwrap();
        f.write("tree/a", &[1; 8192]);
        f.write("tree/b", &[2; 8192]);
        f.write("later", b"later");
        let options = CopyOptions {
            existing_destination: ExistingDestination::Skip,
            ..visible_options()
        };
        let mut injected = false;
        let e = copy_paths(
            &[f.request("tree", "copy"), f.request("later", "later-copy")],
            &options,
            &Control::default(),
            |p| {
                if p.bytes_copied > 0 && !injected {
                    let current = p.source.file_name().unwrap();
                    let other = if current == "a" { "b" } else { "a" };
                    f.write(&format!("copy/{other}"), b"actor");
                    injected = true;
                }
            },
        )
        .unwrap_err();
        assert_eq!(e.report.skipped_count, 0);
        assert_eq!(e.report.visible_completed_count, 0);
        assert_eq!(e.report.visible_started_count, 1);
        assert!(!f.0.join("later-copy").exists());
        assert!(
            fs::read_dir(f.0.join("copy"))
                .unwrap()
                .any(|x| fs::read(x.unwrap().path()).unwrap() == b"actor")
        );
        assert_eq!(fs::read(f.0.join("tree/a")).unwrap(), [1; 8192]);
        assert_eq!(fs::read(f.0.join("tree/b")).unwrap(), [2; 8192]);
        assert!(e.rescue_paths.contains(&f.0.join("copy")));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod captured_multi_tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "files-multi-copy-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            for d in ["a", "b", "out"] {
                fs::create_dir_all(p.join(d)).unwrap();
            }
            fs::write(p.join("a/first"), vec![7; 65536]).unwrap();
            fs::write(p.join("b/second"), b"old").unwrap();
            Self(p)
        }
        fn paths(&self) -> Vec<CopyRequest> {
            ["a/first", "b/second"]
                .iter()
                .map(|p| CopyRequest {
                    source: self.0.join(p),
                    destination: self
                        .0
                        .join("out")
                        .join(std::path::Path::new(p).file_name().unwrap()),
                })
                .collect()
        }
        fn proof(&self) -> crate::multi_origin::Captured {
            crate::multi_origin::capture_provisional(
                &self
                    .paths()
                    .iter()
                    .map(|r| r.source.clone())
                    .collect::<Vec<_>>(),
                &AtomicBool::new(false),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn all_groups_are_verified_before_first_copy_and_preexisting_replacement_preserved() {
        let f = Fixture::new();
        let proof = f.proof();
        fs::rename(f.0.join("b/second"), f.0.join("b/old-second")).unwrap();
        fs::write(f.0.join("b/second"), b"new").unwrap();
        let error = copy_paths_captured(
            &f.paths(),
            &CopyOptions::default(),
            &Control::default(),
            &proof,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.report.completed_count, 0);
        assert!(fs::read_dir(f.0.join("out")).unwrap().next().is_none());
        assert_eq!(fs::read(f.0.join("b/second")).unwrap(), b"new");
    }
    #[test]
    fn later_root_replacement_after_initial_proof_is_rejected_by_effective_engine_fd() {
        let f = Fixture::new();
        let proof = f.proof();
        let mut replaced = false;
        let error = copy_paths_captured(
            &f.paths(),
            &CopyOptions {
                chunk_bytes: 4096,
                strategy: CopyStrategy::Stream,
                ..Default::default()
            },
            &Control::default(),
            &proof,
            |_| {
                if !replaced {
                    replaced = true;
                    fs::rename(f.0.join("b/second"), f.0.join("b/old-second")).unwrap();
                    fs::write(f.0.join("b/second"), b"new").unwrap();
                }
            },
        )
        .unwrap_err();
        assert!(replaced);
        assert_eq!(error.report.completed_count, 1);
        assert_eq!(fs::read(f.0.join("out/first")).unwrap(), vec![7; 65536]);
        assert!(!f.0.join("out/second").exists());
        assert_eq!(fs::read(f.0.join("b/second")).unwrap(), b"new");
        assert_eq!(fs::read_dir(f.0.join("out")).unwrap().count(), 1);
    }
    #[test]
    fn captured_multiparent_copy_preserves_all_sources_and_exact_bytes() {
        let f = Fixture::new();
        let proof = f.proof();
        let report = copy_paths_captured(
            &f.paths(),
            &CopyOptions::default(),
            &Control::default(),
            &proof,
            |_| {},
        )
        .unwrap();
        assert_eq!(report.completed_count, 2);
        for r in f.paths() {
            assert_eq!(
                fs::read(r.source).unwrap(),
                fs::read(r.destination).unwrap()
            );
        }
    }
}
