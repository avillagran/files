//! Lossless, same-directory batch rename with exclusive staging and undo.
//!
//! Linux uses descriptor-relative `renameat2(RENAME_NOREPLACE)`. Every move is
//! identity-checked before and after the syscall. Unix has no conditional
//! "rename this inode" operation: concurrent changes can still win the interval
//! between check and rename. Such a change is detected after staging and is
//! rolled back without overwriting; failed recovery paths are reported.
//! This is a process-failure transaction, not a crash-durable journal.
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_RENAMES: usize = 100_000;
const MAX_NAME_BYTES: usize = 4096;
const MAX_RENAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESCUE_PATH_BYTES: usize = 256 * 1024;
const MAX_RESCUE_PATHS: usize = 256;
const MAX_DIAGNOSTIC_PARENT_BYTES: usize = MAX_RESCUE_PATH_BYTES / 4;
const MAX_ROLLBACK_ERROR_BYTES: usize = 64 * 1024;
const MAX_ROLLBACK_ERRORS: usize = 64;
const MAX_ONE_ERROR_BYTES: usize = 2048;

// Check the lossless names before allocating duplicate-detection sets or entries.
// The caller owns the input allocation; this bounds additional transaction state.
fn validate_budget(renames: &[(OsString, OsString)]) -> io::Result<()> {
    if renames.len() > MAX_RENAMES {
        return Err(invalid("Too many rename entries"));
    }
    let mut bytes = 0usize;
    for (from, to) in renames {
        for name in [from, to] {
            let len = name.as_encoded_bytes().len();
            if len > MAX_NAME_BYTES {
                return Err(invalid("Rename filename exceeds byte budget"));
            }
            bytes = bytes
                .checked_add(len)
                .filter(|total| *total <= MAX_RENAME_BYTES)
                .ok_or_else(|| invalid("Rename names exceed aggregate byte budget"))?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    kind: u32,
}

#[derive(Clone, Debug)]
struct Entry {
    from: OsString,
    to: OsString,
    identity: Identity,
}

/// A snapshot; capture this when presenting the user's final rename preview.
#[derive(Debug)]
pub struct PreparedRename {
    directory: File,
    path: PathBuf,
    entries: Vec<Entry>,
}

/// Retains expected destination identities. Undo refuses stale/replaced entries.
#[derive(Debug)]
pub struct UndoToken {
    directory: File,
    path: PathBuf,
    entries: Vec<Entry>,
    /// Empty staging directories whose cleanup failed after a successful move.
    pub cleanup_paths: Vec<PathBuf>,
    /// Cleanup failures, including a path-resolution error when procfs is unavailable.
    pub cleanup_errors: Vec<String>,
}

/// Recovery never overwrites. Inspect `preserved_paths` when rollback is partial.
#[derive(Debug)]
pub struct TransactionError {
    pub cause: io::Error,
    pub rollback_errors: Vec<String>,
    pub preserved_paths: Vec<PathBuf>,
    /// Error messages omitted after the bounded diagnostic budget was exhausted.
    pub omitted_rollback_errors: usize,
    pub truncated_rollback_errors: usize,
    /// Rename entries not restored to their sources. External replacement or
    /// relocation may prevent proving that the expected object still exists.
    pub preserved_object_count: usize,
    /// Paths name containing directories rather than individual objects when true.
    pub aggregated_preserved_paths: bool,
    /// Objects whose descriptor could not be resolved to a verified current path.
    pub unresolved_preserved_objects: usize,
    /// Last-known locations ONLY; these may identify replacements and are not
    /// safe recovery/cleanup targets without independent identity verification.
    pub unverified_parent_paths: Vec<PathBuf>,
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} rollback errors, {} omitted, {} truncated; {} unrestored entries, {} verified rescue paths, aggregated={}, {} unresolved objects)",
            self.cause,
            self.rollback_errors.len(),
            self.omitted_rollback_errors,
            self.truncated_rollback_errors,
            self.preserved_object_count,
            self.preserved_paths.len(),
            self.aggregated_preserved_paths,
            self.unresolved_preserved_objects
        )
    }
}
impl std::error::Error for TransactionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
impl From<io::Error> for TransactionError {
    fn from(cause: io::Error) -> Self {
        Self {
            cause,
            rollback_errors: Vec::new(),
            preserved_paths: Vec::new(),
            omitted_rollback_errors: 0,
            truncated_rollback_errors: 0,
            preserved_object_count: 0,
            aggregated_preserved_paths: false,
            unresolved_preserved_objects: 0,
            unverified_parent_paths: Vec::new(),
        }
    }
}

impl TransactionError {
    fn record_rollback_error(&mut self, message: std::fmt::Arguments<'_>) {
        let used: usize = self.rollback_errors.iter().map(String::capacity).sum();
        if self.rollback_errors.len() >= MAX_ROLLBACK_ERRORS || used >= MAX_ROLLBACK_ERROR_BYTES {
            self.omitted_rollback_errors += 1;
            return;
        }
        let limit = (MAX_ROLLBACK_ERROR_BYTES - used).min(MAX_ONE_ERROR_BYTES);
        let (message, truncated) = bounded_diagnostic(message, limit);
        self.truncated_rollback_errors += usize::from(truncated);
        self.rollback_errors.push(message);
    }
}
struct DiagnosticWriter {
    text: String,
    limit: usize,
}
impl std::fmt::Write for DiagnosticWriter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let remaining = self.limit - self.text.len();
        if value.len() <= remaining {
            self.text.push_str(value);
            return Ok(());
        }
        let mut end = remaining;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&value[..end]);
        Err(std::fmt::Error)
    }
}
fn bounded_diagnostic(message: std::fmt::Arguments<'_>, limit: usize) -> (String, bool) {
    let mut writer = DiagnosticWriter {
        text: String::with_capacity(limit),
        limit,
    };
    let truncated = writer.write_fmt(message).is_err();
    (writer.text, truncated)
}

impl PreparedRename {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Canonical pathname captured with the preview; the pinned descriptor remains
    /// authoritative if an external actor later moves this directory.
    pub fn directory(&self) -> &Path {
        &self.path
    }
    pub fn renames(&self) -> impl ExactSizeIterator<Item = (&OsStr, &OsStr)> {
        self.entries
            .iter()
            .map(|entry| (entry.from.as_os_str(), entry.to.as_os_str()))
    }

    pub fn capture(
        directory: impl AsRef<Path>,
        renames: Vec<(OsString, OsString)>,
    ) -> io::Result<Self> {
        let path = directory.as_ref().canonicalize()?;
        let directory = platform::open_directory(&path)?;
        Self::capture_at(directory, path, renames)
    }

    fn capture_at(
        directory: File,
        path: PathBuf,
        renames: Vec<(OsString, OsString)>,
    ) -> io::Result<Self> {
        validate_budget(&renames)?;
        if path.as_os_str().as_encoded_bytes().len() > MAX_DIAGNOSTIC_PARENT_BYTES - 256 {
            return Err(invalid(
                "Rename directory exceeds recovery diagnostic path budget",
            ));
        }
        let mut sources = HashSet::new();
        let mut destinations = HashSet::new();
        for (from, to) in &renames {
            platform::validate_name(from)?;
            platform::validate_name(to)?;
            if !sources.insert(from.clone()) || !destinations.insert(to.clone()) {
                return Err(invalid("Duplicate source or destination"));
            }
        }
        let mut entries = Vec::with_capacity(renames.len());
        for (from, to) in renames {
            let identity = platform::identity(&directory, &from)?;
            if !sources.contains(&to) {
                match platform::identity(&directory, &to) {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "Destination exists",
                        ));
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
            if from != to {
                entries.push(Entry { from, to, identity });
            }
        }
        Ok(Self {
            directory,
            path,
            entries,
        })
    }

    pub fn execute(self) -> Result<UndoToken, TransactionError> {
        self.execute_with_hook(|_, _| Ok(()))
    }

    /// Cancellation is observed between moves and before initial verification.
    /// Once observed, ordinary no-clobber rollback runs to completion; rollback
    /// itself is deliberately not cancelled. No syscall interruption is promised.
    pub fn execute_cancellable(
        self,
        cancelled: &AtomicBool,
    ) -> Result<UndoToken, TransactionError> {
        cancellation_check(cancelled)?;
        self.execute_with_hook_and_cancel(|_, _| cancellation_check(cancelled), Some(cancelled))
    }

    fn execute_with_hook(
        self,
        before_move: impl FnMut(usize, &Path) -> io::Result<()>,
    ) -> Result<UndoToken, TransactionError> {
        self.execute_with_hook_and_cancel(before_move, None)
    }

    fn execute_with_hook_and_cancel(
        self,
        mut before_move: impl FnMut(usize, &Path) -> io::Result<()>,
        cancelled: Option<&AtomicBool>,
    ) -> Result<UndoToken, TransactionError> {
        for entry in &self.entries {
            if let Some(flag) = cancelled {
                cancellation_check(flag)?;
            }
            verify(&self.directory, &entry.from, &entry.identity)?;
        }
        if self.entries.is_empty() {
            return Ok(UndoToken {
                directory: self.directory,
                path: self.path,
                entries: self.entries,
                cleanup_paths: Vec::new(),
                cleanup_errors: Vec::new(),
            });
        }
        let staging = platform::Staging::create(&self.directory, &self.path, &self.entries)?;
        // State 0 = original source, 1 = staging, 2 = destination.
        let mut states = vec![0u8; self.entries.len()];
        let result = (|| -> io::Result<()> {
            for (i, entry) in self.entries.iter().enumerate() {
                before_move(i, &staging.path)?;
                verify(&self.directory, &entry.from, &entry.identity)?;
                platform::move_exclusive(
                    &self.directory,
                    &entry.from,
                    &staging.directory,
                    &slot(i),
                )?;
                states[i] = 1;
                verify(&staging.directory, &slot(i), &entry.identity)?;
            }
            for (i, entry) in self.entries.iter().enumerate() {
                before_move(self.entries.len() + i, &staging.path)?;
                verify(&staging.directory, &slot(i), &entry.identity)?;
                platform::move_exclusive(&staging.directory, &slot(i), &self.directory, &entry.to)?;
                states[i] = 2;
                verify(&self.directory, &entry.to, &entry.identity)?;
            }
            Ok(())
        })();
        if let Err(cause) = result {
            let mut error = TransactionError::from(cause);
            // First return committed entries to their private slots, freeing all
            // source names for cycles; only then restore originals.
            for (i, entry) in self.entries.iter().enumerate().rev() {
                if states[i] == 2 {
                    let recovery =
                        verify(&self.directory, &entry.to, &entry.identity).and_then(|()| {
                            platform::move_exclusive(
                                &self.directory,
                                &entry.to,
                                &staging.directory,
                                &slot(i),
                            )
                        });
                    match recovery {
                        Ok(()) => states[i] = 1,
                        Err(e) => error.record_rollback_error(format_args!("{:?}: {e}", entry.to)),
                    }
                }
            }
            for (i, entry) in self.entries.iter().enumerate().rev() {
                if states[i] == 1 {
                    // If an external source replacement raced our first move,
                    // return that exact staged object too, without deleting it.
                    match platform::move_exclusive(
                        &staging.directory,
                        &slot(i),
                        &self.directory,
                        &entry.from,
                    ) {
                        Ok(()) => states[i] = 0,
                        Err(e) => {
                            error.record_rollback_error(format_args!("{:?}: {e}", entry.from))
                        }
                    }
                }
            }
            let current_path = match platform::verified_current_path(&self.directory) {
                Ok(path) => Some(path),
                Err(e) => {
                    error.record_rollback_error(format_args!(
                        "Resolve current parent from descriptor: {e}"
                    ));
                    None
                }
            };
            let staging_path = match staging.reported_path() {
                Ok(path) => Some(path),
                Err(e) => {
                    error.record_rollback_error(format_args!(
                        "Resolve staging location from descriptor: {e}"
                    ));
                    None
                }
            };
            collect_rescue_paths(
                &mut error,
                &self.entries,
                &states,
                staging_path.as_deref(),
                current_path.as_deref(),
            );
            if current_path.is_none() && states.contains(&2) {
                error.unverified_parent_paths.push(self.path.clone());
            }
            if staging_path.is_none() && states.contains(&1) {
                error.unverified_parent_paths.push(staging.path.clone());
            }
            if let Err(e) = staging.remove(&self.directory) {
                error.record_rollback_error(format_args!("Remove staging: {e}"));
                if let Some(path) = staging_path
                    && !error.preserved_paths.contains(&path)
                {
                    error.preserved_paths.push(path);
                }
            }
            return Err(error);
        }
        // Empty staging removal failure must remain observable; do not report a
        // failed rename after all files have already moved successfully.
        let mut cleanup_paths = Vec::new();
        let mut cleanup_errors = Vec::new();
        if let Err(e) = staging.remove(&self.directory) {
            cleanup_errors.push(e.to_string());
            match staging.reported_path() {
                Ok(path) => cleanup_paths.push(path),
                Err(e) => {
                    cleanup_errors.push(format!("Resolve staging location from descriptor: {e}"))
                }
            }
        }
        Ok(UndoToken {
            directory: self.directory,
            path: self.path,
            entries: self.entries,
            cleanup_paths,
            cleanup_errors,
        })
    }
}

impl UndoToken {
    pub fn undo(&self) -> Result<UndoToken, TransactionError> {
        self.prepare_undo()?.execute()
    }
    pub fn undo_cancellable(&self, cancelled: &AtomicBool) -> Result<UndoToken, TransactionError> {
        cancellation_check(cancelled)?;
        self.prepare_undo()?.execute_cancellable(cancelled)
    }
    fn prepare_undo(&self) -> io::Result<PreparedRename> {
        let entries = self
            .entries
            .iter()
            .map(|e| Entry {
                from: e.to.clone(),
                to: e.from.clone(),
                identity: e.identity.clone(),
            })
            .collect();
        Ok(PreparedRename {
            directory: self.directory.try_clone()?,
            path: self.path.clone(),
            entries,
        })
    }
}

// Preflight repeated directory prefixes without allocating any per-object path.
// Reserve a possible staging-cleanup path too, before choosing detailed output.
fn rescue_paths_fit(
    entries: &[Entry],
    states: &[u8],
    staging: Option<&Path>,
    current: Option<&Path>,
) -> bool {
    let mut bytes = staging.map_or(0, |p| p.as_os_str().as_encoded_bytes().len());
    let mut count = usize::from(staging.is_some());
    if bytes > MAX_RESCUE_PATH_BYTES {
        return false;
    }
    for (index, entry) in entries.iter().enumerate() {
        let (parent, name_len) = match states[index] {
            1 => (staging, decimal_digits(index)),
            2 => (current, entry.to.as_encoded_bytes().len()),
            _ => continue,
        };
        let Some(parent) = parent else {
            continue;
        };
        count += 1;
        let extra = parent
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_add(name_len));
        let Some(next) = extra.and_then(|n| bytes.checked_add(n)) else {
            return false;
        };
        bytes = next;
        if count > MAX_RESCUE_PATHS || bytes > MAX_RESCUE_PATH_BYTES {
            return false;
        }
    }
    true
}
fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}
fn collect_rescue_paths(
    error: &mut TransactionError,
    entries: &[Entry],
    states: &[u8],
    staging: Option<&Path>,
    current: Option<&Path>,
) {
    let staged = states.iter().filter(|&&state| state == 1).count();
    let committed = states.iter().filter(|&&state| state == 2).count();
    error.preserved_object_count = staged + committed;
    error.unresolved_preserved_objects =
        if staging.is_none() { staged } else { 0 } + if current.is_none() { committed } else { 0 };
    if !rescue_paths_fit(entries, states, staging, current) {
        error.aggregated_preserved_paths = true;
        if staged > 0
            && let Some(parent) = staging
        {
            error.preserved_paths.push(parent.to_path_buf());
        }
        if committed > 0
            && let Some(parent) = current
            && !error.preserved_paths.iter().any(|p| p == parent)
        {
            error.preserved_paths.push(parent.to_path_buf());
        }
        return;
    }
    for (index, entry) in entries.iter().enumerate() {
        match states[index] {
            1 => {
                if let Some(parent) = staging {
                    error.preserved_paths.push(parent.join(slot(index)));
                }
            }
            2 => {
                if let Some(parent) = current {
                    error.preserved_paths.push(parent.join(&entry.to));
                }
            }
            _ => {}
        }
    }
}

fn cancellation_check(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Rename cancelled",
        ))
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
fn validate_report_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().as_encoded_bytes().len() > MAX_DIAGNOSTIC_PARENT_BYTES {
        return Err(io::Error::other(
            "Resolved recovery directory exceeds diagnostic path budget",
        ));
    }
    Ok(())
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn slot(index: usize) -> OsString {
    index.to_string().into()
}
fn verify(directory: &File, name: &OsString, expected: &Identity) -> io::Result<()> {
    if &platform::identity(directory, name)? != expected {
        return Err(io::Error::other("File identity changed since preview"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    fn c_name(name: &OsString) -> io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|_| invalid("NUL in filename"))
    }
    pub fn validate_name(name: &OsString) -> io::Result<()> {
        let bytes = name.as_bytes();
        if bytes.is_empty()
            || bytes == b"."
            || bytes == b".."
            || bytes.contains(&b'/')
            || bytes.contains(&0)
        {
            return Err(invalid("Expected one filename component"));
        }
        Ok(())
    }
    pub fn verified_current_path(directory: &File) -> io::Result<PathBuf> {
        let path = std::fs::read_link(format!("/proc/self/fd/{}", directory.as_raw_fd()))?;
        validate_report_path(&path)?;
        let candidate = open_directory(&path)?;
        if descriptor_identity(&candidate)? != descriptor_identity(directory)? {
            return Err(io::Error::other(
                "Directory path changed while resolving descriptor",
            ));
        }
        Ok(path)
    }
    pub fn open_directory(path: &Path) -> io::Result<File> {
        let name =
            CString::new(path.as_os_str().as_bytes()).map_err(|_| invalid("NUL in directory"))?;
        let fd = unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_DIRECTORY | libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    pub fn identity(directory: &File, name: &OsString) -> io::Result<Identity> {
        let name = c_name(name)?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let result = unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        Ok(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: stat.st_mode & libc::S_IFMT,
        })
    }
    fn descriptor_identity(directory: &File) -> io::Result<Identity> {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(directory.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        Ok(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: stat.st_mode & libc::S_IFMT,
        })
    }
    pub fn move_exclusive(
        from_dir: &File,
        from: &OsString,
        to_dir: &File,
        to: &OsString,
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
    pub struct Staging {
        pub directory: File,
        pub path: PathBuf,
        name: OsString,
    }
    impl Staging {
        pub fn create(parent: &File, path: &Path, entries: &[Entry]) -> io::Result<Self> {
            for _ in 0..128 {
                let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
                let name: OsString =
                    format!(".files-rename-{}-{sequence}", std::process::id()).into();
                if entries.iter().any(|entry| entry.to == name) {
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
                let fd = unsafe {
                    libc::openat(
                        parent.as_raw_fd(),
                        c.as_ptr(),
                        libc::O_DIRECTORY | libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    )
                };
                if fd < 0 {
                    let cause = io::Error::last_os_error();
                    // No user entry has moved yet. Without a descriptor we
                    // cannot verify ownership for safe cleanup after a race.
                    return Err(io::Error::other(format!(
                        "{cause}; unverified empty staging candidate left at {:?}",
                        path.join(&name)
                    )));
                }
                return Ok(Self {
                    directory: unsafe { File::from_raw_fd(fd) },
                    path: path.join(&name),
                    name,
                });
            }
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Cannot allocate exclusive staging",
            ))
        }
        pub fn reported_path(&self) -> io::Result<PathBuf> {
            // Resolve the held directory, which may have been renamed. Never
            // substitute parent/name: that can now identify another object.
            let path = std::fs::read_link(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))?;
            validate_report_path(&path)?;
            let candidate = open_directory(&path)?;
            if descriptor_identity(&candidate)? != descriptor_identity(&self.directory)? {
                return Err(io::Error::other(
                    "Staging path changed while resolving descriptor",
                ));
            }
            Ok(path)
        }
        pub fn remove(&self, parent: &File) -> io::Result<()> {
            let expected = descriptor_identity(&self.directory)?;
            if identity(parent, &self.name)? != expected {
                return Err(io::Error::other(
                    "Staging name no longer identifies owned directory; refusing cleanup",
                ));
            }
            // There is no inode-conditional unlink on Linux. This check rejects
            // observed replacements; a hostile writer could still race unlink.
            let name = c_name(&self.name)?;
            if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Safe no-replace rename currently requires Linux",
        ))
    }
    pub fn verified_current_path(_: &File) -> io::Result<PathBuf> {
        unsupported()
    }
    pub fn validate_name(_: &OsString) -> io::Result<()> {
        unsupported()
    }
    pub fn open_directory(_: &Path) -> io::Result<File> {
        unsupported()
    }
    pub fn identity(_: &File, _: &OsString) -> io::Result<Identity> {
        unsupported()
    }
    pub fn move_exclusive(_: &File, _: &OsString, _: &File, _: &OsString) -> io::Result<()> {
        unsupported()
    }
    pub struct Staging {
        pub directory: File,
        pub path: PathBuf,
    }
    impl Staging {
        pub fn create(_: &File, _: &Path, _: &[Entry]) -> io::Result<Self> {
            unsupported()
        }
        pub fn reported_path(&self) -> io::Result<PathBuf> {
            unsupported()
        }
        pub fn remove(&self, _: &File) -> io::Result<()> {
            unsupported()
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn transaction_budget_bounds_count_before_copying_names() {
        let mut names = vec![(OsString::new(), OsString::new()); MAX_RENAMES];
        assert!(validate_budget(&names).is_ok());
        names.push((OsString::new(), OsString::new()));
        assert_eq!(
            validate_budget(&names).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn transaction_budget_counts_both_lossless_names_and_exact_boundary() {
        let name = OsString::from_vec(vec![0xff; MAX_NAME_BYTES]);
        let mut names = vec![(name.clone(), name); MAX_RENAME_BYTES / (2 * MAX_NAME_BYTES)];
        assert!(validate_budget(&names).is_ok());
        names.push(("a".into(), OsString::new()));
        assert!(validate_budget(&names).is_err());
        assert!(validate_budget(&[("a".repeat(MAX_NAME_BYTES + 1).into(), "b".into())]).is_err());
    }

    #[test]
    fn rescue_path_budget_aggregates_before_repeated_parent_allocation() {
        let entries = (0..1000)
            .map(|index| Entry {
                from: format!("from{index}").into(),
                to: format!("to{index}").into(),
                identity: Identity {
                    device: 1,
                    inode: index,
                    kind: 0,
                },
            })
            .collect::<Vec<_>>();
        let states = vec![1; entries.len()];
        let parent = PathBuf::from(format!("/{}", "p".repeat(4095)));
        assert!(!rescue_paths_fit(&entries, &states, Some(&parent), None));
        let mut error = TransactionError::from(io::Error::other("partial rollback"));
        collect_rescue_paths(&mut error, &entries, &states, Some(&parent), None);
        assert!(error.aggregated_preserved_paths);
        assert_eq!(error.preserved_object_count, 1000);
        assert_eq!(error.preserved_paths, vec![parent]);
        assert_eq!(error.unresolved_preserved_objects, 0);
        assert!(
            error
                .preserved_paths
                .iter()
                .map(|path| path.as_os_str().as_encoded_bytes().len())
                .sum::<usize>()
                <= MAX_RESCUE_PATH_BYTES
        );
        let mut unresolved = TransactionError::from(io::Error::other("unresolved"));
        collect_rescue_paths(&mut unresolved, &entries, &states, None, None);
        assert_eq!(unresolved.unresolved_preserved_objects, 1000);
    }

    #[test]
    fn rollback_diagnostics_bound_strings_and_report_omitted_errors() {
        let mut error = TransactionError::from(io::Error::other("rollback"));
        let oversized = "á".repeat(MAX_ONE_ERROR_BYTES * 2);
        for _ in 0..1000 {
            error.record_rollback_error(format_args!("{oversized}"));
        }
        assert_eq!(
            error.rollback_errors.len() + error.omitted_rollback_errors,
            1000
        );
        assert!(error.omitted_rollback_errors > 0);
        assert!(error.truncated_rollback_errors > 0);
        assert!(error.rollback_errors.len() <= MAX_ROLLBACK_ERRORS);
        assert!(
            error
                .rollback_errors
                .iter()
                .map(String::capacity)
                .sum::<usize>()
                <= MAX_ROLLBACK_ERROR_BYTES
        );
        assert!(
            error.rollback_errors.iter().map(String::len).sum::<usize>()
                <= MAX_ROLLBACK_ERROR_BYTES
        );
        assert!(
            error
                .rollback_errors
                .iter()
                .all(|message| message.len() <= MAX_ONE_ERROR_BYTES)
        );
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let root = PathBuf::from(
                std::env::var_os("HOME").expect("HOME needed for safe cache fixtures"),
            )
            .join(".cache/files-test");
            fs::create_dir_all(&root).unwrap();
            loop {
                let path = root.join(format!(
                    "rename-{}-{}",
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
        fn write(&self, name: impl AsRef<Path>, value: &[u8]) {
            fs::write(self.0.join(name), value).unwrap();
        }
        fn read(&self, name: impl AsRef<Path>) -> Vec<u8> {
            fs::read(self.0.join(name)).unwrap()
        }
        fn capture(&self, names: &[(&str, &str)]) -> PreparedRename {
            PreparedRename::capture(
                &self.0,
                names
                    .iter()
                    .map(|(a, b)| ((*a).into(), (*b).into()))
                    .collect(),
            )
            .unwrap()
        }
        fn clean_staging(&self) -> bool {
            !fs::read_dir(&self.0).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".files-rename-")
            })
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn prepared_preview_borrows_names_and_cancel_before_start_is_clean() {
        let f = Fixture::new();
        f.write("a", b"A");
        let prepared = f.capture(&[("a", "b")]);
        assert_eq!(prepared.len(), 1);
        assert!(!prepared.is_empty());
        assert_eq!(prepared.directory(), f.0.canonicalize().unwrap());
        {
            let mut names = prepared.renames();
            assert_eq!(names.len(), 1);
            assert_eq!(names.next(), Some((OsStr::new("a"), OsStr::new("b"))));
        }
        let error = prepared
            .execute_cancellable(&AtomicBool::new(true))
            .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
        assert_eq!(f.read("a"), b"A");
        assert!(!f.0.join("b").exists());
        assert!(f.clean_staging());
    }

    #[test]
    fn cancellation_between_each_move_uses_complete_cycle_rollback() {
        for cancel_at in 0..6 {
            let f = Fixture::new();
            f.write("a", b"A");
            f.write("b", b"B");
            f.write("c", b"C");
            let cancelled = AtomicBool::new(false);
            let error = f
                .capture(&[("a", "b"), ("b", "c"), ("c", "a")])
                .execute_with_hook(|step, _| {
                    if step == cancel_at {
                        cancelled.store(true, Ordering::Release);
                    }
                    cancellation_check(&cancelled)
                })
                .unwrap_err();
            assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
            assert!(error.rollback_errors.is_empty());
            assert_eq!(error.preserved_object_count, 0);
            assert_eq!(f.read("a"), b"A");
            assert_eq!(f.read("b"), b"B");
            assert_eq!(f.read("c"), b"C");
            assert!(f.clean_staging());
        }
    }

    #[test]
    fn cycles_swap_and_undo_redo_without_clobber() {
        let f = Fixture::new();
        f.write("a", b"A");
        f.write("b", b"B");
        f.write("c", b"C");
        let undo = f
            .capture(&[("a", "b"), ("b", "c"), ("c", "a")])
            .execute()
            .unwrap();
        assert_eq!(f.read("a"), b"C");
        assert_eq!(f.read("b"), b"A");
        assert_eq!(f.read("c"), b"B");
        let redo = undo.undo().unwrap();
        assert_eq!(f.read("a"), b"A");
        assert_eq!(f.read("b"), b"B");
        assert_eq!(f.read("c"), b"C");
        redo.undo().unwrap();
        assert_eq!(f.read("a"), b"C");
        assert!(f.clean_staging());
        f.capture(&[("a", "b"), ("b", "a")]).execute().unwrap();
        assert_eq!(f.read("a"), b"A");
        assert_eq!(f.read("b"), b"C");
    }

    #[test]
    fn refuses_preexisting_destination_and_invalid_names() {
        let f = Fixture::new();
        f.write("a", b"A");
        f.write("b", b"B");
        assert!(PreparedRename::capture(&f.0, vec![("a".into(), "b".into())]).is_err());
        for bad in ["", ".", "..", "../escape", "a/b", "x\0y"] {
            assert!(PreparedRename::capture(&f.0, vec![("a".into(), bad.into())]).is_err());
        }
        assert_eq!(f.read("a"), b"A");
        assert_eq!(f.read("b"), b"B");
    }

    #[test]
    fn changed_identity_after_preview_is_rejected() {
        let f = Fixture::new();
        f.write("a", b"A");
        let prepared = f.capture(&[("a", "b")]);
        fs::rename(f.0.join("a"), f.0.join("kept")).unwrap();
        f.write("a", b"replacement");
        assert!(prepared.execute().is_err());
        assert_eq!(f.read("a"), b"replacement");
        assert_eq!(f.read("kept"), b"A");
        assert!(!f.0.join("b").exists());
        assert!(f.clean_staging());
    }

    #[test]
    fn unicode_invalid_utf8_and_symlink_are_lossless() {
        let f = Fixture::new();
        let old = OsString::from_vec(vec![b'a', 255]);
        let new = OsString::from_vec(vec![b'b', 254]);
        f.write(&old, b"bytes");
        f.write("área", b"unicode");
        std::os::unix::fs::symlink("nonexistent", f.0.join("link")).unwrap();
        let undo = PreparedRename::capture(
            &f.0,
            vec![
                (old.clone(), new.clone()),
                ("área".into(), "niño".into()),
                ("link".into(), "moved-link".into()),
            ],
        )
        .unwrap()
        .execute()
        .unwrap();
        assert_eq!(f.read(&new), b"bytes");
        assert_eq!(f.read("niño"), b"unicode");
        assert_eq!(
            fs::read_link(f.0.join("moved-link")).unwrap(),
            PathBuf::from("nonexistent")
        );
        undo.undo().unwrap();
        assert_eq!(f.read(old), b"bytes");
    }

    #[test]
    fn each_injected_failure_restores_cycle() {
        for fail_at in 0..6 {
            let f = Fixture::new();
            f.write("a", b"A");
            f.write("b", b"B");
            f.write("c", b"C");
            let error = f
                .capture(&[("a", "b"), ("b", "c"), ("c", "a")])
                .execute_with_hook(|step, _| {
                    if step == fail_at {
                        Err(io::Error::other("Injected failure"))
                    } else {
                        Ok(())
                    }
                })
                .unwrap_err();
            assert!(error.rollback_errors.is_empty(), "{error:?}");
            assert!(error.preserved_paths.is_empty());
            assert_eq!(f.read("a"), b"A");
            assert_eq!(f.read("b"), b"B");
            assert_eq!(f.read("c"), b"C");
            assert!(f.clean_staging());
        }
    }

    #[test]
    fn destination_race_never_overwrites_and_rolls_back() {
        let f = Fixture::new();
        f.write("a", b"A");
        let error = f
            .capture(&[("a", "b")])
            .execute_with_hook(|step, _| {
                if step == 1 {
                    f.write("b", b"new occupant");
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::AlreadyExists);
        assert!(error.rollback_errors.is_empty());
        assert_eq!(f.read("a"), b"A");
        assert_eq!(f.read("b"), b"new occupant");
        assert!(f.clean_staging());
    }

    #[test]
    fn rollback_conflict_reports_preserved_data() {
        let f = Fixture::new();
        f.write("a", b"A");
        f.write("b", b"B");
        let error = f
            .capture(&[("a", "x"), ("b", "y")])
            .execute_with_hook(|step, _| {
                if step == 3 {
                    f.write("a", b"new a");
                    return Err(io::Error::other("Injected after first commit"));
                }
                Ok(())
            })
            .unwrap_err();
        assert!(!error.rollback_errors.is_empty());
        assert_eq!(f.read("a"), b"new a");
        assert_eq!(f.read("b"), b"B");
        assert!(
            error
                .preserved_paths
                .iter()
                .any(|path| fs::read(path).is_ok_and(|bytes| bytes == b"A"))
        );
        assert!(!f.0.join("x").exists());
        assert!(!f.0.join("y").exists());
    }

    #[test]
    fn undo_refuses_replacement_and_unselected_original_name() {
        let f = Fixture::new();
        f.write("a", b"A");
        let undo = f.capture(&[("a", "b")]).execute().unwrap();
        f.write("a", b"new a");
        assert!(undo.undo().is_err());
        assert_eq!(f.read("a"), b"new a");
        assert_eq!(f.read("b"), b"A");
        fs::remove_file(f.0.join("a")).unwrap();
        fs::rename(f.0.join("b"), f.0.join("kept")).unwrap();
        f.write("b", b"replacement");
        assert!(undo.undo().is_err());
        assert_eq!(f.read("b"), b"replacement");
        assert_eq!(f.read("kept"), b"A");
        assert!(f.clean_staging());
    }

    #[test]
    fn pinned_directory_handles_survive_path_rename() {
        let f = Fixture::new();
        let sub = f.0.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("a"), b"A").unwrap();
        let prepared = PreparedRename::capture(&sub, vec![("a".into(), "b".into())]).unwrap();
        fs::rename(&sub, f.0.join("renamed")).unwrap();
        let undo = prepared.execute().unwrap();
        assert_eq!(fs::read(f.0.join("renamed/b")).unwrap(), b"A");
        undo.undo().unwrap();
        assert_eq!(fs::read(f.0.join("renamed/a")).unwrap(), b"A");
    }
    #[test]
    fn relocated_staging_cleanup_keeps_replacement_and_reports_real_path() {
        let f = Fixture::new();
        f.write("a", b"A");
        let relocated = f.0.join("relocated-staging");
        let mut replacement = None;
        let undo = f
            .capture(&[("a", "b")])
            .execute_with_hook(|step, staging| {
                if step == 0 {
                    fs::rename(staging, &relocated)?;
                    fs::create_dir(staging)?;
                    replacement = Some(staging.to_path_buf());
                }
                Ok(())
            })
            .unwrap();
        let replacement = replacement.unwrap();
        assert!(
            replacement.is_dir(),
            "Must not unlink an actor's replacement"
        );
        assert_eq!(fs::read_dir(&replacement).unwrap().count(), 0);
        assert_eq!(undo.cleanup_paths, vec![relocated.clone()]);
        assert!(!undo.cleanup_errors.is_empty());
        assert_eq!(fs::read_dir(&relocated).unwrap().count(), 0);
        assert_eq!(f.read("b"), b"A");
        undo.undo().unwrap();
        assert_eq!(f.read("a"), b"A");
        assert!(replacement.is_dir());
    }

    #[test]
    fn rollback_rescue_paths_follow_relocated_staging_descriptor() {
        let f = Fixture::new();
        f.write("a", b"A");
        let relocated = f.0.join("relocated-staging");
        let mut replacement = None;
        let error = f
            .capture(&[("a", "b")])
            .execute_with_hook(|step, staging| {
                if step == 1 {
                    fs::rename(staging, &relocated)?;
                    fs::create_dir(staging)?;
                    replacement = Some(staging.to_path_buf());
                    f.write("a", b"new occupant");
                    return Err(io::Error::other("Injected rollback conflict"));
                }
                Ok(())
            })
            .unwrap_err();
        let replacement = replacement.unwrap();
        assert!(replacement.is_dir());
        assert_eq!(f.read("a"), b"new occupant");
        assert_eq!(
            error.preserved_paths,
            vec![relocated.join("0"), relocated.clone()]
        );
        assert_eq!(fs::read(relocated.join("0")).unwrap(), b"A");
        assert!(
            !error
                .preserved_paths
                .iter()
                .any(|path| path.starts_with(&replacement))
        );
    }

    #[test]
    fn nonempty_directory_swap_and_undo_preserve_nested_contents() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("a/sub")).unwrap();
        fs::create_dir(f.0.join("b")).unwrap();
        f.write("a/sub/file", b"nested A");
        f.write("b/file", b"B");
        let undo = f.capture(&[("a", "b"), ("b", "a")]).execute().unwrap();
        assert_eq!(f.read("b/sub/file"), b"nested A");
        assert_eq!(f.read("a/file"), b"B");
        undo.undo().unwrap();
        assert_eq!(f.read("a/sub/file"), b"nested A");
        assert_eq!(f.read("b/file"), b"B");
        assert!(f.clean_staging());
    }

    #[test]
    fn case_only_rename_and_undo() {
        let f = Fixture::new();
        f.write("report.txt", b"case preserved");
        let undo = f
            .capture(&[("report.txt", "Report.txt")])
            .execute()
            .unwrap();
        assert_eq!(f.read("Report.txt"), b"case preserved");
        assert!(!f.0.join("report.txt").exists());
        undo.undo().unwrap();
        assert_eq!(f.read("report.txt"), b"case preserved");
        assert!(!f.0.join("Report.txt").exists());
    }
    #[test]
    fn cancelled_undo_preserves_applied_names_and_token_remains_usable() {
        let f = Fixture::new();
        f.write("a", b"A");
        let undo = f.capture(&[("a", "b")]).execute().unwrap();
        let error = undo.undo_cancellable(&AtomicBool::new(true)).unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
        assert_eq!(f.read("b"), b"A");
        assert!(!f.0.join("a").exists());
        undo.undo_cancellable(&AtomicBool::new(false)).unwrap();
        assert_eq!(f.read("a"), b"A");
        assert!(!f.0.join("b").exists());
        assert!(f.clean_staging());
    }
}
