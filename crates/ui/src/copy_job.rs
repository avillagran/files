//! One lazy in-process copy worker, with a single request/result and a latest-only
//! progress cell. There is no pending-job queue; unread completion is still busy.
//!
//! Window shutdown must call begin_close(), then keep polling until closed while
//! presenting every completion (including partial-copy rescue details). Drop only
//! requests cancellation and never blocks UI on a join. An in-flight filesystem
//! syscall can remain blocked; this supervisor never creates a replacement worker
//! around it. No move/delete or crash journal is provided here.
pub use files_core::copy_engine::Control as CopyControl;
use files_core::copy_engine::{
    self, CopyError, CopyMethod, CopyOptions, CopyProgress, CopyReport, CopyRequest,
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_REQUESTS: usize = 4096;
const MAX_PATH_BYTES: usize = 4096;
const MAX_AGGREGATE_PATH_BYTES: usize = 4 * 1024 * 1024;
const MAX_RETAINED_PATH_CAPACITY: usize = 8 * 1024 * 1024;
const MAX_AUXILIARY_BYTES: usize = 32 * 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub struct Request {
    clipboard: Option<Arc<crate::file_clipboard::Lease>>,
    multi: Option<Arc<files_core::multi_origin::Captured>>,
    pub id: u64,
    paths: Vec<CopyRequest>,
    options: CopyOptions,
    start_paused: bool,
    prepared: Option<crate::move_operation::Prepared>,
    archive: Option<crate::archive_operation::Prepared>,
}
impl Request {
    /// Ownership moves into the worker only on successful submit; rejection
    /// returns the entire request so the UI can preserve its selected paths.
    pub fn new(id: u64, paths: Vec<CopyRequest>, options: CopyOptions) -> Self {
        Self {
            clipboard: None,
            multi: None,
            id,
            paths,
            options,
            start_paused: false,
            prepared: None,
            archive: None,
        }
    }
    pub fn start_paused(mut self) -> Self {
        self.start_paused = true;
        self
    }
    #[cfg(test)]
    pub fn native_move(id: u64, prepared: files_core::move_engine::PreparedMove) -> Self {
        Self::reviewed_move(id, crate::move_operation::Prepared::Native(prepared))
    }
    pub fn reviewed_move(id: u64, prepared: crate::move_operation::Prepared) -> Self {
        Self {
            clipboard: None,
            multi: None,
            id,
            paths: Vec::new(),
            options: CopyOptions::default(),
            start_paused: false,
            prepared: Some(prepared),
            archive: None,
        }
    }
    pub fn reviewed_extract(id: u64, archive: crate::archive_operation::Prepared) -> Self {
        let mut request = Self::new(id, Vec::new(), CopyOptions::default());
        request.archive = Some(archive);
        request
    }
    pub fn take_archive(&mut self) -> Option<crate::archive_operation::Prepared> {
        self.archive.take()
    }
    pub fn creating(&self) -> bool {
        self.archive.as_ref().is_some_and(|a| a.creating())
    }
    pub fn archive_action(&self) -> Option<crate::archive_operation::RecoveryAction> {
        self.archive.as_ref().and_then(|a| a.recovery_action())
    }
    pub fn archive_outcome_bytes(&self) -> usize {
        self.archive.as_ref().map_or(0, |a| a.outcome_bytes())
    }
    pub fn is_extract(&self) -> bool {
        self.archive.is_some()
    }
    pub fn take_previous(&mut self) -> Option<Box<crate::move_operation::ExdevFailure>> {
        self.prepared.as_mut().and_then(|p| p.take_previous())
    }
    pub fn is_move(&self) -> bool {
        self.prepared.is_some()
    }
    pub fn is_batch(&self) -> bool {
        matches!(
            self.prepared,
            Some(crate::move_operation::Prepared::Batch { .. })
        )
    }
    pub fn fd_count(&self) -> usize {
        self.archive.as_ref().map_or(0, |p| p.fd_count())
            + self.multi.as_ref().map_or(0, |p| p.fd_reservation())
            + self.prepared.as_ref().map_or(0, |p| p.fd_count())
            + self
                .clipboard
                .as_ref()
                .map_or(0, |c| c.source.fd_count() + 1)
    }
    pub fn metadata_bytes(&self) -> usize {
        self.archive.as_ref().map_or(0, |p| p.metadata_bytes())
            + self.multi.as_ref().map_or(0, |p| p.retained_bytes())
            + self.prepared.as_ref().map_or(0, |p| p.metadata_bytes())
            + self.clipboard.as_ref().map_or(0, |c| c.retained_bytes())
    }
    pub fn with_clipboard(mut self, clipboard: Option<Arc<crate::file_clipboard::Lease>>) -> Self {
        self.clipboard = clipboard;
        self
    }
    pub fn with_multi(mut self, proof: Option<Arc<files_core::multi_origin::Captured>>) -> Self {
        self.multi = proof;
        self
    }
    pub fn take_prepared(&mut self) -> Option<crate::move_operation::Prepared> {
        self.prepared.take()
    }
    pub fn paths(&self) -> &[CopyRequest] {
        if let Some(a) = &self.archive {
            return a.paths();
        }
        self.prepared
            .as_ref()
            .map_or(self.paths.as_slice(), |p| p.paths().as_slice())
    }
    pub fn paths_capacity(&self) -> usize {
        if let Some(a) = &self.archive {
            return a.paths().capacity();
        }
        self.prepared
            .as_ref()
            .map_or(self.paths.capacity(), |p| p.paths().capacity())
    }
    pub fn options(&self) -> &CopyOptions {
        &self.options
    }
}

#[derive(Debug)]
pub enum RejectReason {
    Busy,
    Closing,
    InputBounds,
    WorkerUnavailable,
    WorkerStart(io::Error),
}
impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("Hay una copia o una confirmación pendiente"),
            Self::Closing => f.write_str("El trabajador de copia se está cerrando"),
            Self::InputBounds => f.write_str("La selección excede los límites de copia"),
            Self::WorkerUnavailable => f.write_str(
                "El trabajador de copia terminó; revisa el resultado antes de continuar",
            ),
            Self::WorkerStart(error) => {
                write!(f, "No se pudo iniciar el trabajador de copia: {error}")
            }
        }
    }
}
impl std::error::Error for RejectReason {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WorkerStart(error) => Some(error),
            _ => None,
        }
    }
}
#[derive(Debug)]
pub struct Rejected {
    pub reason: RejectReason,
    pub request: Request,
}

#[derive(Debug)]
pub enum Failure {
    Archive(crate::archive_operation::Failure),
    /// Kept intact: bounded report, verified rescue roots, omitted/truncated
    /// counters and unresolved entries are available to the UI without flattening.
    Engine(Box<CopyError>),
    Exdev(Box<crate::move_operation::ExdevFailure>),
    Batch(Box<crate::move_operation::BatchFailure>),
    CancelledBeforeStart,
    /// A panic can leave staged or already-published copies; state is unconfirmed.
    WorkerPanicked,
    /// The thread exited without ACK. Do not interpret this as a clean rollback.
    WorkerLost,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archive(error) => std::fmt::Display::fmt(error, f),
            Self::Engine(error) => std::fmt::Display::fmt(error, f),
            Self::Exdev(error) => std::fmt::Display::fmt(error, f),
            Self::Batch(error) => match &error.cause {
                files_core::multi_origin::BatchFailure::Review(cause) => {
                    write!(f, "Revisión multipadre: {cause}")
                }
                files_core::multi_origin::BatchFailure::Engine { group, error } => {
                    write!(f, "Grupo {} (informe local): {error}", group + 1)
                }
            },
            Self::CancelledBeforeStart => f.write_str("Copia cancelada antes de comenzar"),
            Self::WorkerPanicked => f.write_str(
                "El trabajador de copia falló; revisa los destinos y las carpetas temporales",
            ),
            Self::WorkerLost => {
                f.write_str("El trabajador terminó sin confirmar el resultado; revisa los destinos")
            }
        }
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Engine(error) => Some(error.as_ref()),
            Self::Exdev(error) => Some(error.as_ref()),
            Self::Batch(_) => None,
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgressStats {
    pub callbacks: u64,
    pub published: u64,
    pub suppressed: u64,
    pub overwritten: u64,
    pub path_details_omitted: u64,
}

#[derive(Debug)]
pub struct Progress {
    pub id: u64,
    /// Lossless paths; omitted explicitly if an engine violates the path bound.
    pub source: Option<PathBuf>,
    pub destination: Option<PathBuf>,
    pub bytes_copied: u64,
    pub entries_copied: u64,
    pub method: CopyMethod,
    pub stats: ProgressStats,
}

#[derive(Debug)]
pub struct Outcome {
    pub exdev: Option<crate::move_operation::Context>,
    pub moving: bool,
    pub extracting: bool,
    pub creating: bool,
    pub archive_action: Option<crate::archive_operation::RecoveryAction>,
    pub id: u64,
    pub result: Result<CopyReport, Failure>,
    pub progress: ProgressStats,
}
impl Outcome {
    pub fn report(&self) -> Option<&CopyReport> {
        match &self.result {
            Ok(report) => Some(report),
            Err(Failure::Engine(error)) => Some(error.report.as_ref()),
            Err(Failure::Exdev(error)) => Some(&error.report),
            Err(Failure::Batch(error)) => Some(&error.report),
            _ => None,
        }
    }
    pub fn exdev_error(&self) -> Option<&crate::move_operation::ExdevFailure> {
        match &self.result {
            Err(Failure::Exdev(e)) => Some(e),
            _ => None,
        }
    }
    pub fn engine_error(&self) -> Option<&CopyError> {
        match &self.result {
            Err(Failure::Engine(error)) => Some(error.as_ref()),
            Err(Failure::Batch(error)) => match &error.cause {
                files_core::multi_origin::BatchFailure::Engine { error, .. } => {
                    Some(error.as_ref())
                }
                _ => None,
            },
            _ => None,
        }
    }
    pub fn cancelled(&self) -> bool {
        matches!(&self.result, Err(Failure::CancelledBeforeStart))
            || matches!(&self.result, Err(Failure::Archive(e)) if e.cancelled())
            || matches!(&self.result, Err(Failure::Batch(error)) if matches!(&error.cause, files_core::multi_origin::BatchFailure::Review(cause) if cause.kind() == io::ErrorKind::Interrupted))
            || self
                .exdev_error()
                .is_some_and(|e| e.cause.kind() == io::ErrorKind::Interrupted)
            || self
                .engine_error()
                .is_some_and(|error| error.cause.kind() == io::ErrorKind::Interrupted)
    }
    pub fn supervisor_state_unknown(&self) -> bool {
        matches!(
            &self.result,
            Err(Failure::WorkerPanicked | Failure::WorkerLost)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Dormant,
    Idle,
    Queued,
    Running,
    Paused,
    Cancelling,
    AwaitingAck,
    Closing,
    Closed,
}

#[derive(Debug)]
pub struct Poll {
    pub progress: Option<Progress>,
    /// Taking this value is the ACK that releases busy. Consume/display its full
    /// report/error before accepting another job in the UI's outer queue.
    pub outcome: Option<Outcome>,
    pub closed: bool,
}

#[derive(Default)]
struct State {
    request: Option<Request>,
    active_id: Option<u64>,
    active_moving: bool,
    active_extracting: bool,
    active_creating: bool,
    active_archive_action: Option<crate::archive_operation::RecoveryAction>,
    active_exdev: Option<crate::move_operation::Context>,
    control: Option<CopyControl>,
    paused: bool,
    running: bool,
    latest: Option<Progress>,
    outcome: Option<Outcome>,
    closing: bool,
    exited: bool,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub struct Worker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}
impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}
impl Worker {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            thread: None,
        }
    }
    pub fn archive_action(&self) -> Option<crate::archive_operation::RecoveryAction> {
        lock(&self.shared).active_archive_action
    }
    pub fn creating(&self) -> bool {
        let s = lock(&self.shared);
        s.active_id.is_some() && s.active_creating
    }
    pub fn extracting(&self) -> bool {
        let s = lock(&self.shared);
        s.active_id.is_some() && s.active_extracting
    }
    pub fn busy(&self) -> bool {
        lock(&self.shared).active_id.is_some()
    }
    pub fn active_id(&self) -> Option<u64> {
        lock(&self.shared).active_id
    }
    pub fn phase(&self) -> Phase {
        let state = lock(&self.shared);
        if state.closing {
            if state.exited
                && self.thread.is_none()
                && state.active_id.is_none()
                && state.outcome.is_none()
            {
                Phase::Closed
            } else {
                Phase::Closing
            }
        } else if state.outcome.is_some() {
            Phase::AwaitingAck
        } else if state
            .control
            .as_ref()
            .is_some_and(CopyControl::is_cancelled)
        {
            Phase::Cancelling
        } else if state.request.is_some() {
            Phase::Queued
        } else if state.running && state.paused {
            Phase::Paused
        } else if state.running {
            Phase::Running
        } else if self.thread.is_some() {
            Phase::Idle
        } else {
            Phase::Dormant
        }
    }
    pub fn is_closed(&self) -> bool {
        self.phase() == Phase::Closed
    }
    pub fn submit(&mut self, request: Request) -> Result<(), Box<Rejected>> {
        self.reap_finished();
        {
            let state = lock(&self.shared);
            let reason = if state.closing {
                Some(RejectReason::Closing)
            } else if state.active_id.is_some() {
                Some(RejectReason::Busy)
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(Box::new(Rejected { reason, request }));
            }
        }
        if !valid_input(&request) {
            return Err(Box::new(Rejected {
                reason: RejectReason::InputBounds,
                request,
            }));
        }
        if self.thread.is_none() {
            let shared = Arc::clone(&self.shared);
            match thread::Builder::new()
                .name("files-copy".into())
                .stack_size(2 * 1024 * 1024)
                .spawn(move || run(shared))
            {
                Ok(handle) => self.thread = Some(handle),
                Err(error) => {
                    return Err(Box::new(Rejected {
                        reason: RejectReason::WorkerStart(error),
                        request,
                    }));
                }
            }
        }
        let mut state = lock(&self.shared);
        if state.exited {
            return Err(Box::new(Rejected {
                reason: RejectReason::WorkerUnavailable,
                request,
            }));
        }
        let control = CopyControl::default();
        if request.start_paused {
            control.pause();
        }
        state.active_moving = request.is_move();
        state.active_extracting = request.is_extract();
        state.active_creating = request.creating();
        state.active_archive_action = request.archive_action();
        state.active_exdev = request.prepared.as_ref().and_then(|p| p.context()).cloned();
        state.active_id = Some(request.id);
        state.paused = request.start_paused;
        state.control = Some(control);
        state.latest = None;
        state.request = Some(request);
        self.shared.changed.notify_all();
        Ok(())
    }
    /// IDs reject delayed clicks/shortcuts that belong to an older job.
    pub fn pause(&self, id: u64) -> bool {
        let mut state = lock(&self.shared);
        if state.active_id != Some(id) || state.outcome.is_some() || state.closing {
            return false;
        }
        if let Some(control) = &state.control {
            control.pause();
            state.paused = true;
            true
        } else {
            false
        }
    }
    pub fn resume(&self, id: u64) -> bool {
        let mut state = lock(&self.shared);
        if state.active_id != Some(id) || state.outcome.is_some() || state.closing {
            return false;
        }
        if let Some(control) = &state.control {
            control.resume();
            state.paused = false;
            true
        } else {
            false
        }
    }
    pub fn cancel(&self, id: u64) -> bool {
        let state = lock(&self.shared);
        if state.active_id != Some(id) || state.outcome.is_some() {
            return false;
        }
        if let Some(control) = &state.control {
            control.cancel();
            true
        } else {
            false
        }
    }
    /// Nonblocking shutdown initiation. A paused worker is woken by cancel.
    pub fn begin_close(&mut self) {
        let mut state = lock(&self.shared);
        state.closing = true;
        if let Some(control) = &state.control {
            control.cancel();
        }
        if self.thread.is_none() {
            state.exited = true;
        }
        self.shared.changed.notify_all();
    }
    pub fn poll(&mut self) -> Poll {
        self.reap_finished();
        let (progress, outcome) = {
            let mut state = lock(&self.shared);
            let progress = state.latest.take();
            let outcome = state.outcome.take();
            if outcome.is_some() {
                state.active_id = None;
                state.control = None;
                state.paused = false;
                self.shared.changed.notify_all();
            }
            (progress, outcome)
        };
        self.reap_finished();
        Poll {
            progress,
            outcome,
            closed: self.is_closed(),
        }
    }
    fn reap_finished(&mut self) {
        if !self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        let joined = self.thread.take().expect("Finished handle checked").join();
        let mut state = lock(&self.shared);
        if !state.exited || joined.is_err() {
            state.closing = true;
            if state.outcome.is_none()
                && let Some(id) = state.active_id
            {
                state.outcome = Some(Outcome {
                    moving: state.active_moving,
                    extracting: state.active_extracting,
                    creating: state.active_creating,
                    archive_action: state.active_archive_action,
                    exdev: state.active_exdev.take(),
                    id,
                    result: Err(Failure::WorkerLost),
                    progress: ProgressStats::default(),
                });
            }
            state.request = None;
            state.running = false;
        }
        state.exited = true;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.begin_close();
        self.reap_finished();
        // Dropping an unfinished JoinHandle detaches that single existing worker.
        // The window's normal close path must drain poll instead of relying on it.
    }
}

fn valid_input(request: &Request) -> bool {
    if request.paths().len() > MAX_REQUESTS
        || request.paths().len() > request.options.max_requests
        || request.paths_capacity() > MAX_REQUESTS * 2
        || !(1..=MAX_REQUESTS).contains(&request.options.max_requests)
        || !(1..=MAX_AGGREGATE_PATH_BYTES).contains(&request.options.max_request_path_bytes)
        || !(1..=MAX_AUXILIARY_BYTES).contains(&request.options.max_auxiliary_bytes)
    {
        return false;
    }
    let mut bytes = 0usize;
    let mut capacity = 0usize;
    for copy in request.paths() {
        for path in [&copy.source, &copy.destination] {
            let length = path.as_os_str().len();
            if length > MAX_PATH_BYTES || path.capacity() > MAX_PATH_BYTES * 2 {
                return false;
            }
            bytes = match bytes.checked_add(length) {
                Some(n) => n,
                None => return false,
            };
            capacity = match capacity.checked_add(path.capacity()) {
                Some(n) => n,
                None => return false,
            };
            if bytes > request.options.max_request_path_bytes
                || capacity > MAX_RETAINED_PATH_CAPACITY
            {
                return false;
            }
        }
    }
    true
}

fn owned_progress_path(path: &Path, stats: &mut ProgressStats) -> Option<PathBuf> {
    if path.as_os_str().len() > MAX_PATH_BYTES {
        stats.path_details_omitted = stats.path_details_omitted.saturating_add(1);
        None
    } else {
        Some(PathBuf::from(path.as_os_str()))
    }
}
fn publish(
    shared: &Shared,
    id: u64,
    value: CopyProgress<'_>,
    stats: &mut ProgressStats,
    last: &mut Option<Instant>,
) {
    stats.callbacks = stats.callbacks.saturating_add(1);
    let now = Instant::now();
    if last.is_some_and(|at| now.duration_since(at) < PROGRESS_INTERVAL) {
        stats.suppressed = stats.suppressed.saturating_add(1);
        return;
    }
    let source = owned_progress_path(value.source, stats);
    let destination = owned_progress_path(value.destination, stats);
    let mut state = lock(shared);
    if state.active_id != Some(id) || state.outcome.is_some() {
        return;
    }
    stats.published = stats.published.saturating_add(1);
    if state.latest.is_some() {
        stats.overwritten = stats.overwritten.saturating_add(1);
    }
    state.latest = Some(Progress {
        id,
        source,
        destination,
        bytes_copied: value.bytes_copied,
        entries_copied: value.entries_copied,
        method: value.method,
        stats: *stats,
    });
    *last = Some(now);
}

fn run(shared: Arc<Shared>) {
    loop {
        let (mut request, control) = {
            let mut state = lock(&shared);
            loop {
                if state.closing && state.request.is_none() {
                    state.exited = true;
                    shared.changed.notify_all();
                    return;
                }
                if state.outcome.is_none()
                    && let Some(request) = state.request.take()
                {
                    state.running = true;
                    break (
                        request,
                        state
                            .control
                            .as_ref()
                            .expect("Queued control exists")
                            .clone(),
                    );
                }
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
        };
        let id = request.id;
        let moving = request.is_move();
        let extracting = request.is_extract();
        let creating = request.creating();
        let archive_action = request.archive_action();
        let exdev = request.prepared.as_ref().and_then(|p| p.context()).cloned();
        let mut stats = ProgressStats::default();
        let mut last = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if control.is_cancelled() && !request.is_batch() && request.archive_action().is_none() {
                return Err(Failure::CancelledBeforeStart);
            }
            if let Some(c) = &request.clipboard {
                let checked = c
                    .source
                    .verify_subset_controlled(c.names.iter().map(|n| n.as_os_str()), &control)
                    .and_then(|_| {
                        if let Some(p) = &request.prepared {
                            p.verify_selection_controlled(&c.source, &control)
                        } else {
                            Ok(())
                        }
                    });
                if let Err(cause) = checked {
                    return Err(Failure::Engine(Box::new(CopyError {
                        cause,
                        report: Box::default(),
                        rescue_paths: Vec::new(),
                        cleanup_errors: Vec::new(),
                        rescue_paths_omitted: 0,
                        cleanup_errors_omitted: 0,
                        cleanup_unresolved_entries: 0,
                        details_truncated: 0,
                    })));
                }
            }
            let progress =
                |progress: CopyProgress<'_>| publish(&shared, id, progress, &mut stats, &mut last);
            if let Some(archive) = request.archive.take() {
                archive
                    .execute(&control, progress)
                    .map_err(Failure::Archive)
            } else if let Some(prepared) = request.prepared.take() {
                prepared.execute(&control, progress)
            } else if let Some(proof) = &request.multi {
                copy_engine::copy_paths_captured(
                    &request.paths,
                    &request.options,
                    &control,
                    proof,
                    progress,
                )
                .map_err(|e| Failure::Engine(Box::new(e)))
            } else if let Some(clipboard) = &request.clipboard {
                copy_engine::copy_paths_selected(
                    &request.paths,
                    &request.options,
                    &control,
                    &clipboard.source,
                    progress,
                )
                .map_err(|error| Failure::Engine(Box::new(error)))
            } else {
                copy_engine::copy_paths(&request.paths, &request.options, &control, progress)
                    .map_err(|error| Failure::Engine(Box::new(error)))
            }
        }));
        let panicked = result.is_err();
        let result = result.unwrap_or(Err(Failure::WorkerPanicked));
        // Input storage is released before ACK becomes readable.
        drop(request);
        let mut state = lock(&shared);
        state.running = false;
        state.active_exdev = None;
        state.outcome = Some(Outcome {
            exdev,
            moving,
            extracting,
            creating,
            archive_action,
            id,
            result,
            progress: stats,
        });
        if panicked {
            state.closing = true;
        }
        shared.changed.notify_all();
        if state.closing {
            state.exited = true;
            return;
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::copy_engine::CopyStrategy;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let root = PathBuf::from(std::env::var_os("HOME").expect("Cache fixture needs HOME"))
                .join(".cache/files-test");
            fs::create_dir_all(&root).unwrap();
            loop {
                let path = root.join(format!(
                    "copy-job-{}-{}",
                    std::process::id(),
                    SERIAL.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Create fixture: {error}"),
                }
            }
        }
        fn request(&self, id: u64, to: &str) -> Request {
            Request::new(
                id,
                vec![CopyRequest {
                    source: self.0.join("source"),
                    destination: self.0.join(to),
                }],
                options(),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn options() -> CopyOptions {
        CopyOptions {
            strategy: CopyStrategy::Stream,
            chunk_bytes: 4096,
            max_depth: 4,
            max_symlink_bytes: 128,
            max_report_details: 4,
            max_auxiliary_bytes: 2 * 1024 * 1024,
            ..Default::default()
        }
    }
    fn wait_ready(worker: &Worker) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while lock(&worker.shared).outcome.is_none() {
            assert!(Instant::now() < deadline, "Worker never produced ACK");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn receive(worker: &mut Worker) -> Outcome {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(outcome) = worker.poll().outcome {
                return outcome;
            }
            assert!(Instant::now() < deadline, "Worker did not complete");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn drain_close(worker: &mut Worker) -> Option<Outcome> {
        worker.begin_close();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut received = None;
        loop {
            let polled = worker.poll();
            if let Some(outcome) = polled.outcome {
                assert!(received.is_none());
                received = Some(outcome);
            }
            if polled.closed {
                return received;
            }
            assert!(Instant::now() < deadline, "Worker failed to drain shutdown");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn lazy_single_worker_reused_after_ack_and_never_queues_a_second_job() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"safe copy").unwrap();
        let mut worker = Worker::new();
        assert_eq!(worker.phase(), Phase::Dormant);
        assert!(worker.thread.is_none());
        worker.submit(fixture.request(1, "copy-one")).unwrap();
        let thread_id = worker.thread.as_ref().unwrap().thread().id();
        let rejected = worker.submit(fixture.request(2, "copy-two")).unwrap_err();
        assert!(matches!(rejected.reason, RejectReason::Busy));
        assert_eq!(rejected.request.id, 2);
        assert_eq!(
            rejected.request.paths()[0].destination,
            fixture.0.join("copy-two")
        );
        let first = receive(&mut worker);
        assert_eq!(first.id, 1);
        assert!(first.result.is_ok());
        assert!(!worker.busy());
        worker.submit(rejected.request).unwrap();
        let second = receive(&mut worker);
        assert_eq!(second.id, 2);
        assert!(second.result.is_ok());
        assert_eq!(worker.thread.as_ref().unwrap().thread().id(), thread_id);
        assert_eq!(fs::read(fixture.0.join("copy-one")).unwrap(), b"safe copy");
        assert_eq!(fs::read(fixture.0.join("copy-two")).unwrap(), b"safe copy");
        assert!(drain_close(&mut worker).is_none());
        assert!(worker.thread.is_none());
    }

    #[test]
    fn unread_completion_remains_busy_until_poll_acknowledges_it() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"data").unwrap();
        let mut worker = Worker::new();
        worker.submit(fixture.request(10, "copy")).unwrap();
        wait_ready(&worker);
        assert!(worker.busy());
        assert_eq!(worker.phase(), Phase::AwaitingAck);
        assert!(matches!(
            worker
                .submit(fixture.request(11, "other"))
                .unwrap_err()
                .reason,
            RejectReason::Busy
        ));
        let polled = worker.poll();
        assert_eq!(polled.outcome.unwrap().id, 10);
        assert!(!worker.busy());
        assert!(worker.poll().outcome.is_none());
        assert!(drain_close(&mut worker).is_none());
    }

    #[test]
    fn pause_cancel_resume_and_stale_controls_use_job_identity() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), [7; 16_384]).unwrap();
        let mut worker = Worker::new();
        worker
            .submit(fixture.request(20, "cancelled").start_paused())
            .unwrap();
        assert!(!worker.resume(999));
        assert!(worker.cancel(20));
        let cancelled = receive(&mut worker);
        assert!(cancelled.cancelled());
        assert!(!fixture.0.join("cancelled").exists());
        worker
            .submit(fixture.request(21, "copied").start_paused())
            .unwrap();
        assert!(!worker.cancel(20));
        assert!(!worker.pause(20));
        assert!(!worker.resume(20));
        assert!(worker.pause(21));
        assert!(worker.resume(21));
        let completed = receive(&mut worker);
        assert!(completed.result.is_ok());
        assert_eq!(fs::read(fixture.0.join("copied")).unwrap(), vec![7; 16_384]);
        assert!(drain_close(&mut worker).is_none());
    }

    #[test]
    fn close_cancels_paused_work_and_drains_exactly_one_outcome_before_join() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"data").unwrap();
        let mut worker = Worker::new();
        worker
            .submit(fixture.request(30, "cancelled").start_paused())
            .unwrap();
        worker.begin_close();
        assert!(matches!(
            worker
                .submit(fixture.request(31, "rejected"))
                .unwrap_err()
                .reason,
            RejectReason::Closing
        ));
        let outcome = drain_close(&mut worker).unwrap();
        assert_eq!(outcome.id, 30);
        assert!(outcome.cancelled());
        assert!(worker.is_closed());
        assert!(!worker.busy());
        assert!(worker.thread.is_none());
        assert!(worker.poll().outcome.is_none());
        assert!(!fixture.0.join("cancelled").exists());
    }

    #[test]
    fn close_keeps_unread_success_report_available() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"published").unwrap();
        let mut worker = Worker::new();
        worker.submit(fixture.request(40, "copy")).unwrap();
        wait_ready(&worker);
        let outcome = drain_close(&mut worker).unwrap();
        assert_eq!(outcome.id, 40);
        let report = outcome.result.unwrap();
        assert_eq!(report.completed_count, 1);
        assert_eq!(report.completed, vec![fixture.0.join("copy")]);
        assert!(worker.is_closed());
        assert_eq!(fs::read(fixture.0.join("source")).unwrap(), b"published");
    }

    #[test]
    fn engine_error_reports_and_all_omission_counters_are_preserved_without_flattening() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("rescue")).unwrap();
        let report = CopyReport {
            completed: vec![fixture.0.join("copied")],
            completed_count: 9,
            completed_omitted: 8,
            cleanup_paths: vec![fixture.0.join("cleanup")],
            cleanup_paths_omitted: 4,
            cleanup_errors_omitted: 3,
            details_truncated: 2,
            auxiliary_accounted_peak_bytes: 1234,
            ..Default::default()
        };
        let error = CopyError {
            cause: io::Error::other("Injected report transport fixture"),
            report: Box::new(report),
            rescue_paths: vec![fixture.0.join("rescue")],
            cleanup_errors: vec!["retained replacement".into()],
            rescue_paths_omitted: 7,
            cleanup_errors_omitted: 6,
            cleanup_unresolved_entries: 5,
            details_truncated: 4,
        };
        let mut worker = Worker::new();
        {
            let mut state = lock(&worker.shared);
            state.active_id = Some(50);
            state.control = Some(CopyControl::default());
            state.outcome = Some(Outcome {
                moving: false,
                extracting: false,
                creating: false,
                archive_action: None,
                exdev: None,
                id: 50,
                result: Err(Failure::Engine(Box::new(error))),
                progress: ProgressStats::default(),
            });
        }
        assert!(worker.busy());
        let outcome = worker.poll().outcome.unwrap();
        assert!(!worker.busy());
        let error = outcome.engine_error().unwrap();
        assert_eq!(error.rescue_paths, vec![fixture.0.join("rescue")]);
        assert_eq!(error.rescue_paths_omitted, 7);
        assert_eq!(error.cleanup_errors_omitted, 6);
        assert_eq!(error.cleanup_unresolved_entries, 5);
        assert_eq!(error.details_truncated, 4);
        let report = outcome.report().unwrap();
        assert_eq!(report.completed_count, 9);
        assert_eq!(report.completed_omitted, 8);
        assert_eq!(report.cleanup_paths_omitted, 4);
        assert_eq!(report.cleanup_errors_omitted, 3);
        assert_eq!(report.details_truncated, 2);
        assert_eq!(report.auxiliary_accounted_peak_bytes, 1234);
        assert!(drain_close(&mut worker).is_none());
    }

    #[test]
    fn progress_overwrites_one_slot_and_counts_suppressed_callbacks() {
        let fixture = Fixture::new();
        let shared = Shared::default();
        lock(&shared).active_id = Some(60);
        let mut stats = ProgressStats::default();
        let mut last = None;
        let from = fixture.0.join("source");
        let to = fixture.0.join("copy");
        for bytes in 0..10_000 {
            publish(
                &shared,
                60,
                CopyProgress {
                    source: &from,
                    destination: &to,
                    bytes_copied: bytes,
                    entries_copied: 0,
                    method: CopyMethod::Stream,
                },
                &mut stats,
                &mut last,
            );
        }
        assert_eq!(stats.callbacks, 10_000);
        assert_eq!(stats.published + stats.suppressed, stats.callbacks);
        assert!(stats.published >= 1);
        assert_eq!(stats.overwritten, stats.published - 1);
        let latest = lock(&shared).latest.take().unwrap();
        assert_eq!(latest.id, 60);
        assert!(latest.source.unwrap().as_os_str().len() <= MAX_PATH_BYTES);
        assert!(lock(&shared).latest.is_none());
    }

    #[test]
    fn oversized_input_and_retained_path_capacity_reject_without_starting_thread() {
        let fixture = Fixture::new();
        let mut worker = Worker::new();
        let mut large_capacity = fixture.0.join("short");
        large_capacity.reserve(1024 * 1024);
        let request = Request::new(
            70,
            vec![CopyRequest {
                source: large_capacity,
                destination: fixture.0.join("copy"),
            }],
            options(),
        );
        let rejected = worker.submit(request).unwrap_err();
        assert!(matches!(rejected.reason, RejectReason::InputBounds));
        assert!(rejected.request.paths()[0].source.capacity() > MAX_PATH_BYTES * 2);
        assert!(worker.thread.is_none());
        let request = Request::new(
            71,
            vec![CopyRequest {
                source: PathBuf::from("x".repeat(MAX_PATH_BYTES + 1)),
                destination: fixture.0.join("copy"),
            }],
            options(),
        );
        assert!(matches!(
            worker.submit(request).unwrap_err().reason,
            RejectReason::InputBounds
        ));
        assert!(worker.thread.is_none());
        assert!(!worker.busy());
        assert!(drain_close(&mut worker).is_none());
        assert!(worker.is_closed());
    }

    #[test]
    fn unexpected_thread_exit_generates_unknown_state_ack_and_prevents_replacement_worker() {
        let mut worker = Worker::new();
        {
            let mut state = lock(&worker.shared);
            state.active_id = Some(80);
            state.control = Some(CopyControl::default());
            state.running = true;
        }
        worker.thread = Some(thread::spawn(|| {}));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !worker.thread.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        let outcome = worker.poll().outcome.unwrap();
        assert_eq!(outcome.id, 80);
        assert!(outcome.supervisor_state_unknown());
        assert!(worker.is_closed());
        assert!(worker.thread.is_none());
        assert!(!worker.busy());
        let request = Request::new(81, Vec::new(), options());
        assert!(matches!(
            worker.submit(request).unwrap_err().reason,
            RejectReason::Closing
        ));
    }
}
