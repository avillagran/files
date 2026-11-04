//! Bounded copy queue; all filesystem work stays in copy_job's sole worker.
//!
//! At most eight pending jobs, plus one active/unacknowledged job. The 4 MiB
//! controller accounting includes retained inputs until ACK, with 2 MiB reserved
//! for a bounded complete report and 64 KiB for queue/progress/container overhead.
//! Copy engine buffers/registry and OS thread memory have their own limits and
//! still require full-process PSS gates; this is not a 4 MiB process-memory claim.
//! Outcomes retained by the caller after pump (including its last notice/history)
//! are external ownership and require a separate budget; this controller retains
//! no completion history. The ACK reserve covers one internal bounded result.
//! ZIP requests additionally reserve their execution scratch and retained failure
//! capacities before admission. At ACK an archive result must fit the fixed
//! report slot; Model drops it before dispatching the next job. Only retained
//! move batches hold their original input charge for explicit subset retry.
//!
//! pump returns each complete result once. It does not start another job in the
//! same pump that returns an ACK, so the UI can present rescue details first.
//! Closing explicitly discards pending jobs (counted), cancels the active job and
//! must keep pumping until closed. A stuck syscall never causes a replacement.
use super::copy_job::{self, Outcome, Request, Worker};
use files_core::copy_engine::{CopyError, CopyMethod, CopyReport, CopyRequest};
use std::collections::VecDeque;

pub const MAX_PENDING_JOBS: usize = 8;
pub const RETAINED_BUDGET_BYTES: usize = 4 * 1024 * 1024;
// Bound derived from CURRENT core producers, not arbitrary caller-built reports:
// descriptor_path transfers a Vec<u8> allocated at 4097 bytes into each PathBuf;
// bounded_text uses String::with_capacity(1024); every detail set caps at128.
// Conservatively allow THREE full path payload sets (completed+skipped combined, cleanup, rescue) and TWO
// full error sets (report cleanup and failed-copy cleanup), even though today's
// rescue producer emits at most two roots. Backing payloads: 3*128*4097 +
// 2*128*1024 = 1_835_392 bytes. Add all six Vec backing arrays, envelope/boxed
// structs, 64 bytes per separately owned backing allocation and 4096 bytes for
// the cause envelope: the accounting derivation below remains below 2 MiB.
// The 64-byte allowance is accounting slack, not an allocator/PSS guarantee.
// A producer cap/allocation-policy change MUST update this derivation and test.
const REPORT_HEADROOM_BYTES: usize = 2 * 1024 * 1024;
const PRODUCER_DETAILS: usize = 128;
const PRODUCER_PATH_CAPACITY: usize = 4097;
const PRODUCER_ERROR_CAPACITY: usize = 1024;
const REPORT_DERIVED_BYTES: usize = 3 * PRODUCER_DETAILS * PRODUCER_PATH_CAPACITY
    + 2 * PRODUCER_DETAILS * PRODUCER_ERROR_CAPACITY
    + 4 * PRODUCER_DETAILS * std::mem::size_of::<std::path::PathBuf>()
    + 2 * PRODUCER_DETAILS * std::mem::size_of::<String>()
    + std::mem::size_of::<Outcome>()
    + std::mem::size_of::<CopyError>()
    + std::mem::size_of::<CopyReport>()
    + (5 * PRODUCER_DETAILS + 6 + 3) * 64
    + 4096
    + 2 * (PRODUCER_PATH_CAPACITY + 128);
const _: () = assert!(REPORT_DERIVED_BYTES < REPORT_HEADROOM_BYTES);
const STRUCTURE_HEADROOM_BYTES: usize = 64 * 1024;
const FIXED_HEADROOM_BYTES: usize = REPORT_HEADROOM_BYTES + STRUCTURE_HEADROOM_BYTES;
const MAX_REQUESTS: usize = 4096;
const MAX_PATH_BYTES: usize = 4096;
const MAX_REQUEST_PATH_BYTES: usize = 4 * 1024 * 1024;
const MAX_AUXILIARY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    Closing,
    QueueFull,
    InputBounds,
    MemoryBudget,
    /// Accepted IDs must strictly increase during this controller's lifetime.
    IdNotIncreasing,
    WorkerUnavailable,
    FileDescriptors,
    RetainedBatchMismatch,
    ActiveOperation,
}
impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ActiveOperation => "Termina la operación activa antes de recuperar",
            Self::RetainedBatchMismatch => {
                "El resultado retenido ya no corresponde a este reintento"
            }
            Self::Closing => "La cola de copia se está cerrando",
            Self::QueueFull => "La cola admite hasta ocho trabajos pendientes",
            Self::InputBounds => "La selección excede los límites de copia",
            Self::MemoryBudget => "La cola excede su presupuesto de memoria; reduce el lote",
            Self::IdNotIncreasing => "El identificador de copia debe ser mayor al último admitido",
            Self::FileDescriptors => {
                "La cola supera256 descriptores de movimientos; espera o reduce lote"
            }
            Self::WorkerUnavailable => {
                "El trabajador de copia terminó; revisa sus resultados antes de continuar"
            }
        })
    }
}
impl std::error::Error for RejectReason {}
#[derive(Debug)]
pub struct Rejected {
    pub reason: RejectReason,
    /// Original ownership, capacities, paths and options are returned intact.
    pub request: Request,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Queued,
    Held,
    Running,
    Paused,
    Cancelling,
    AwaitingAck,
    Closing,
    Closed,
    WorkerUnavailable,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub id: u64,
    pub bytes_copied: u64,
    pub entries_copied: u64,
    pub method: Option<CopyMethod>,
    pub stats: copy_job::ProgressStats,
}
impl From<&copy_job::Progress> for Progress {
    fn from(value: &copy_job::Progress) -> Self {
        Self {
            id: value.id,
            bytes_copied: value.bytes_copied,
            entries_copied: value.entries_copied,
            method: Some(value.method),
            stats: value.stats,
        }
    }
}

#[derive(Debug)]
pub enum Completion {
    /// Full engine report/error is moved, never flattened or silently truncated.
    Finished(Box<Outcome>),
    /// Dispatch failed before acceptance. The original Request can be recovered.
    Rejected(Box<copy_job::Rejected>),
}
impl Completion {
    pub fn id(&self) -> u64 {
        match self {
            Self::Finished(value) => value.id,
            Self::Rejected(value) => value.request.id,
        }
    }
    pub fn outcome(&self) -> Option<&Outcome> {
        match self {
            Self::Finished(value) => Some(value.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Pump {
    /// Latest-only owned progress, including lossless source/destination paths.
    pub progress: Option<copy_job::Progress>,
    pub completion: Option<Completion>,
    pub dispatched: Option<u64>,
    /// Cumulative explicit pending cancellations, including close discards.
    pub cancelled_pending_total: u64,
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub limit_bytes: usize,
    pub fixed_headroom_bytes: usize,
    pub queued_input_bytes: usize,
    pub active_input_bytes: usize,
    pub accounted_bytes: usize,
    pub available_bytes: usize,
}

struct Queued {
    request: Request,
    charge: usize,
}
struct Active {
    fds: usize,
    id: u64,
    charge: usize,
}
pub struct Controller {
    external_fds: usize,
    worker: Worker,
    queue: VecDeque<Queued>,
    active: Option<Active>,
    held_batch: Option<Active>,
    retained_bytes: usize,
    limit_bytes: usize,
    last_admitted_id: Option<u64>,
    progress: Option<Progress>,
    cancelled_pending: u64,
    closing: bool,
    dispatch_held: bool,
}
impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
impl Controller {
    pub fn new() -> Self {
        Self {
            external_fds: 0,
            worker: Worker::new(),
            queue: VecDeque::with_capacity(MAX_PENDING_JOBS),
            active: None,
            held_batch: None,
            retained_bytes: FIXED_HEADROOM_BYTES,
            limit_bytes: RETAINED_BUDGET_BYTES,
            last_admitted_id: None,
            progress: None,
            cancelled_pending: 0,
            closing: false,
            dispatch_held: false,
        }
    }
    pub fn set_external_fds(&mut self, count: usize) {
        self.external_fds = count;
    }
    pub fn fd_reservation(&self) -> usize {
        self.external_fds
            + crate::move_operation::EXDEV_FD_RESERVATION
            + self.active.as_ref().map_or(0, |a| a.fds)
            + self.held_batch.as_ref().map_or(0, |a| a.fds)
            + self
                .queue
                .iter()
                .map(|q| q.request.fd_count())
                .sum::<usize>()
    }
    pub fn enqueue(&mut self, request: Request) -> Result<(), Box<Rejected>> {
        self.admit(request, false)
    }
    /// Explicit priority recovery: admitted and submitted atomically, never left
    /// among pending jobs that Clear/Close could silently discard. Other pending
    /// jobs keep their order. Caller reviews this priority before confirmation.
    pub fn start_recovery(&mut self, request: Request) -> Result<(), Box<Rejected>> {
        if request.archive_action().is_none() {
            return Err(Box::new(Rejected {
                reason: RejectReason::InputBounds,
                request,
            }));
        }
        if self.active.is_some() || self.worker.busy() || self.held_batch.is_some() {
            return Err(Box::new(Rejected {
                reason: RejectReason::ActiveOperation,
                request,
            }));
        }
        self.admit(request, true)?;
        let queued = self.queue.pop_back().expect("atomic recovery admission");
        let id = queued.request.id;
        let fds = queued.request.fd_count();
        match self.worker.submit(queued.request) {
            Ok(()) => {
                self.active = Some(Active {
                    fds,
                    id,
                    charge: queued.charge,
                });
                self.progress = None;
                Ok(())
            }
            Err(rejected) => {
                self.release(queued.charge);
                Err(Box::new(Rejected {
                    reason: RejectReason::WorkerUnavailable,
                    request: rejected.request,
                }))
            }
        }
    }
    fn admit(&mut self, request: Request, recovery: bool) -> Result<(), Box<Rejected>> {
        if request.archive_action().is_some() != recovery {
            return Err(Box::new(Rejected {
                reason: RejectReason::InputBounds,
                request,
            }));
        }

        // Reserve one external last-result owner even while queue accepts more.
        // Model releases it before next dispatch; reserve stays conservative.
        let reason = if self.external_fds
            + crate::move_operation::EXDEV_FD_RESERVATION
            + request.fd_count()
            + self
                .queue
                .iter()
                .map(|q| q.request.fd_count())
                .sum::<usize>()
            + self.active.as_ref().map_or(0, |a| a.fds)
            + self.held_batch.as_ref().map_or(0, |a| a.fds)
            > 256
        {
            Some(RejectReason::FileDescriptors)
        } else if self.closing {
            Some(RejectReason::Closing)
        } else if self.worker_unavailable() {
            Some(RejectReason::WorkerUnavailable)
        } else if self.queue.len() == MAX_PENDING_JOBS {
            Some(RejectReason::QueueFull)
        } else if self.last_admitted_id.is_some_and(|last| request.id <= last) {
            Some(RejectReason::IdNotIncreasing)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(Box::new(Rejected { reason, request }));
        }
        let charge = match input_charge(&request) {
            Ok(charge) => charge,
            Err(reason) => return Err(Box::new(Rejected { reason, request })),
        };
        let Some(next) = self
            .retained_bytes
            .checked_add(charge)
            .filter(|n| *n <= self.limit_bytes)
        else {
            return Err(Box::new(Rejected {
                reason: RejectReason::MemoryBudget,
                request,
            }));
        };
        // The queue has fixed eight-slot capacity: moving the checked request
        // into it cannot grow another path/plan allocation.
        self.last_admitted_id = Some(request.id);
        self.retained_bytes = next;
        self.queue.push_back(Queued { request, charge });
        Ok(())
    }
    /// Atomically transfer the retained batch's accounting into a fresh queue
    /// request. Failed admission returns the request AND restores the old owner
    /// reservation. No pump/dispatch occurs between releasing and admitting.
    pub fn requeue_held_batch(
        &mut self,
        previous_id: u64,
        request: Request,
    ) -> Result<(), Box<Rejected>> {
        if !self
            .held_batch
            .as_ref()
            .is_some_and(|held| held.id == previous_id)
            || !request.is_batch()
        {
            return Err(Box::new(Rejected {
                reason: RejectReason::RetainedBatchMismatch,
                request,
            }));
        }
        let held = self.held_batch.take().expect("matching retained owner");
        self.release(held.charge);
        match self.enqueue(request) {
            Ok(()) => Ok(()),
            Err(rejected) => {
                self.retained_bytes += held.charge;
                self.held_batch = Some(held);
                Err(rejected)
            }
        }
    }
    /// Hold only pending dispatch while notices/rescue results are visible.
    /// Active progress, ACK and shutdown draining continue on every pump.
    pub fn hold_queue(&mut self, hold: bool) {
        self.dispatch_held = hold;
    }
    pub fn queue_held(&self) -> bool {
        self.dispatch_held
    }
    pub fn queued_count(&self) -> usize {
        self.queue.len()
    }
    pub fn queued_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.queue.iter().map(|job| job.request.id)
    }
    pub fn creating(&self) -> bool {
        self.worker.creating()
    }
    pub fn archive_action(&self) -> Option<crate::archive_operation::RecoveryAction> {
        self.worker.archive_action()
    }
    pub fn extracting(&self) -> bool {
        self.worker.extracting()
    }
    pub fn active_id(&self) -> Option<u64> {
        self.active.as_ref().map(|job| job.id)
    }
    pub fn progress(&self) -> Option<Progress> {
        self.progress
    }
    pub fn cancelled_pending_count(&self) -> u64 {
        self.cancelled_pending
    }
    pub fn budget(&self) -> Budget {
        Budget {
            limit_bytes: self.limit_bytes,
            fixed_headroom_bytes: FIXED_HEADROOM_BYTES,
            queued_input_bytes: self.queue.iter().map(|job| job.charge).sum(),
            active_input_bytes: self.active.as_ref().map_or(0, |job| job.charge)
                + self.held_batch.as_ref().map_or(0, |job| job.charge),
            accounted_bytes: self.retained_bytes,
            available_bytes: self.limit_bytes.saturating_sub(self.retained_bytes),
        }
    }
    /// Call only after the UI has dropped the corresponding retained batch.
    pub fn release_held_batch(&mut self, id: u64) {
        if self.held_batch.as_ref().is_some_and(|held| held.id == id) {
            let held = self.held_batch.take().expect("matching held batch");
            self.release(held.charge);
        }
    }
    pub fn phase(&self) -> Phase {
        if self.closing {
            return if self.is_closed() {
                Phase::Closed
            } else {
                Phase::Closing
            };
        }
        match self.worker.phase() {
            copy_job::Phase::Dormant | copy_job::Phase::Idle => {
                if self.queue.is_empty() {
                    Phase::Idle
                } else if self.dispatch_held {
                    Phase::Held
                } else {
                    Phase::Queued
                }
            }
            copy_job::Phase::Queued => Phase::Queued,
            copy_job::Phase::Running => Phase::Running,
            copy_job::Phase::Paused => Phase::Paused,
            copy_job::Phase::Cancelling => Phase::Cancelling,
            copy_job::Phase::AwaitingAck => Phase::AwaitingAck,
            copy_job::Phase::Closing | copy_job::Phase::Closed => Phase::WorkerUnavailable,
        }
    }
    pub fn pause(&self, id: u64) -> bool {
        self.active_id() == Some(id) && self.worker.pause(id)
    }
    pub fn resume(&self, id: u64) -> bool {
        self.active_id() == Some(id) && self.worker.resume(id)
    }
    pub fn cancel(&self, id: u64) -> bool {
        self.active_id() == Some(id) && self.worker.cancel(id)
    }
    /// Explicit pending cancellation returns the intact payload for optional retry.
    /// Retried jobs must receive a fresh increasing ID; no history set is retained.
    pub fn cancel_pending(&mut self, id: u64) -> Option<Request> {
        let index = self.queue.iter().position(|job| job.request.id == id)?;
        let queued = self
            .queue
            .remove(index)
            .expect("Index found in bounded queue");
        self.release(queued.charge);
        self.cancelled_pending = self.cancelled_pending.saturating_add(1);
        Some(queued.request)
    }
    /// Drop pending requests explicitly, count them, cancel the active worker and
    /// continue pump until is_closed(). Idempotent; repeated close returns zero.
    pub fn begin_close(&mut self) -> usize {
        self.closing = true;
        let discarded = self.queue.len();
        while let Some(queued) = self.queue.pop_front() {
            self.release(queued.charge);
        }
        self.cancelled_pending = self.cancelled_pending.saturating_add(discarded as u64);
        self.worker.begin_close();
        discarded
    }
    pub fn is_closed(&self) -> bool {
        self.closing && self.queue.is_empty() && self.active.is_none() && self.worker.is_closed()
    }
    pub fn pump(&mut self) -> Pump {
        debug_assert_eq!(self.active_id(), self.worker.active_id());
        let polled = self.worker.poll();
        debug_assert_eq!(polled.closed, self.worker.is_closed());
        let progress = polled.progress;
        if let Some(value) = &progress {
            self.progress = Some(Progress::from(value));
        }
        let mut completion = None;
        let mut dispatched = None;
        if let Some(outcome) = polled.outcome {
            // copy_job's poll is the ACK. Input accounting remains retained until
            // this point, even if the worker freed it before publishing result.
            if let Some(active) = self.active.take() {
                if matches!(&outcome.result, Err(copy_job::Failure::Batch(_))) {
                    debug_assert!(self.held_batch.is_none());
                    self.held_batch = Some(active);
                } else {
                    self.release(active.charge);
                }
            }
            self.progress = None;
            completion = Some(Completion::Finished(Box::new(outcome)));
        } else if !self.closing
            && !self.dispatch_held
            && self.held_batch.is_none()
            && self.active.is_none()
            && !self.worker.busy()
            && !self.worker_unavailable()
            && let Some(queued) = self.queue.pop_front()
        {
            let id = queued.request.id;
            let fds = queued.request.fd_count();
            match self.worker.submit(queued.request) {
                Ok(()) => {
                    self.active = Some(Active {
                        fds,
                        id,
                        charge: queued.charge,
                    });
                    self.progress = None;
                    dispatched = Some(id);
                }
                Err(rejected) => {
                    self.release(queued.charge);
                    completion = Some(Completion::Rejected(rejected));
                }
            }
        }
        Pump {
            progress,
            completion,
            dispatched,
            cancelled_pending_total: self.cancelled_pending,
            closed: self.is_closed(),
        }
    }
    fn worker_unavailable(&self) -> bool {
        matches!(
            self.worker.phase(),
            copy_job::Phase::Closing | copy_job::Phase::Closed
        )
    }
    fn release(&mut self, charge: usize) {
        self.retained_bytes = self
            .retained_bytes
            .checked_sub(charge)
            .expect("Owned input charge retained until release");
        debug_assert!(self.retained_bytes >= FIXED_HEADROOM_BYTES);
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.begin_close();
    }
}

fn input_charge(request: &Request) -> Result<usize, RejectReason> {
    // A ZIP error may retain a registry and two FDs. Prove its bound instead of
    // assuming it has the copy report layout. The existing last-result24FD
    // allowance covers those handles; Model drops the old result before dispatch.
    if request.archive_outcome_bytes() > REPORT_HEADROOM_BYTES {
        return Err(RejectReason::MemoryBudget);
    }
    let options = request.options();
    let paths = request.paths();
    let capacity = request.paths_capacity();
    if paths.len() > MAX_REQUESTS
        || paths.len() > options.max_requests
        || capacity < paths.len()
        || capacity > MAX_REQUESTS * 2
        || !(1..=MAX_REQUESTS).contains(&options.max_requests)
        || !(1..=MAX_REQUEST_PATH_BYTES).contains(&options.max_request_path_bytes)
        || !(1..=MAX_AUXILIARY_BYTES).contains(&options.max_auxiliary_bytes)
    {
        return Err(RejectReason::InputBounds);
    }
    let mut charge = capacity
        .checked_mul(std::mem::size_of::<CopyRequest>())
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(std::mem::size_of::<Request>() * 2 + 128))
        .ok_or(RejectReason::MemoryBudget)?;
    charge = charge
        .checked_add(request.metadata_bytes())
        .ok_or(RejectReason::MemoryBudget)?;
    let mut raw = 0usize;
    for copy in paths {
        for path in [&copy.source, &copy.destination] {
            let length = path.as_os_str().len();
            let retained = path.capacity();
            if length > MAX_PATH_BYTES || retained > MAX_PATH_BYTES * 2 {
                return Err(RejectReason::InputBounds);
            }
            raw = raw.checked_add(length).ok_or(RejectReason::InputBounds)?;
            if raw > options.max_request_path_bytes {
                return Err(RejectReason::InputBounds);
            }
            let bytes = length
                .checked_add(retained)
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(64))
                .ok_or(RejectReason::MemoryBudget)?;
            charge = charge
                .checked_add(bytes)
                .ok_or(RejectReason::MemoryBudget)?;
            if charge > RETAINED_BUDGET_BYTES {
                return Err(RejectReason::MemoryBudget);
            }
        }
    }
    Ok(charge)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::copy_engine::{CopyOptions, CopyStrategy};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{Duration, Instant},
    };
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
    fn empty(id: u64) -> Request {
        Request::new(id, Vec::new(), options())
    }
    #[test]
    fn batch_ack_keeps_one_owner_and_its_charge_until_explicit_release() {
        use files_core::multi_origin::{RootState, capture_provisional, prepare_move_batch};
        use std::sync::{Arc, atomic::AtomicBool};
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        let destination = f.0.join("destination");
        for directory in [&a, &b, &destination] {
            fs::create_dir(directory).unwrap();
        }
        fs::write(a.join("first"), b"first").unwrap();
        fs::write(b.join("second"), b"second").unwrap();
        let cancel = AtomicBool::new(false);
        let proof =
            Arc::new(capture_provisional(&[a.join("first"), b.join("second")], &cancel).unwrap());
        let token = prepare_move_batch(proof, &destination, 256, 2 * 1024 * 1024, &cancel).unwrap();
        let request = Request::reviewed_move(
            1,
            crate::move_operation::Prepared::batch(token, 2 * 1024 * 1024).unwrap(),
        );
        let mut controller = Controller::new();
        controller.enqueue(request).unwrap();
        let before = controller.budget().accounted_bytes;
        let fds = controller.fd_reservation();
        fs::write(destination.join("second"), b"collision").unwrap();
        let ack = completion(&mut controller);
        let outcome = ack.outcome().unwrap();
        let Err(copy_job::Failure::Batch(batch)) = &outcome.result else {
            panic!("expected batch failure")
        };
        assert_eq!(
            batch.ledger.states(),
            &[RootState::Confirmed, RootState::Pending]
        );
        assert_eq!(fs::read(destination.join("first")).unwrap(), b"first");
        assert_eq!(fs::read(b.join("second")).unwrap(), b"second");
        assert_eq!(controller.budget().accounted_bytes, before);
        assert_eq!(controller.fd_reservation(), fds);
        controller.enqueue(empty(2)).unwrap();
        assert!(controller.pump().dispatched.is_none());
        drop(ack);
        controller.release_held_batch(1);
        assert!(controller.budget().accounted_bytes < before + 1024);
        assert_eq!(controller.pump().dispatched, Some(2));
        let _ = completion(&mut controller);
        controller.begin_close();
        while !controller.is_closed() {
            controller.pump();
            thread::yield_now();
        }
    }
    fn lexical(id: u64) -> Request {
        Request::new(
            id,
            vec![CopyRequest {
                source: PathBuf::from("source"),
                destination: PathBuf::from("destination"),
            }],
            options(),
        )
    }
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let root = PathBuf::from(std::env::var_os("HOME").expect("Cache fixture needs HOME"))
                .join(".cache/files-test");
            fs::create_dir_all(&root).unwrap();
            loop {
                let path = root.join(format!(
                    "copy-state-{}-{}",
                    std::process::id(),
                    SERIAL.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
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
    fn completion(controller: &mut Controller) -> Completion {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let pumped = controller.pump();
            if let Some(completion) = pumped.completion {
                assert!(
                    pumped.dispatched.is_none(),
                    "Do not dispatch another job with the same returned ACK"
                );
                return completion;
            }
            assert!(Instant::now() < deadline, "Copy queue failed to complete");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn close(controller: &mut Controller) -> Option<Completion> {
        controller.begin_close();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut result = None;
        loop {
            let pumped = controller.pump();
            if let Some(completion) = pumped.completion {
                assert!(result.is_none());
                result = Some(completion);
            }
            if pumped.closed {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "Copy queue failed to drain close"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn eight_pending_jobs_and_rejected_payload_ownership_remain_bounded() {
        let mut controller = Controller::new();
        for id in 1..=8 {
            controller.enqueue(empty(id)).unwrap();
        }
        assert_eq!(controller.queued_count(), 8);
        assert_eq!(
            controller.queued_ids().collect::<Vec<_>>(),
            (1..=8).collect::<Vec<_>>()
        );
        let request = lexical(9);
        let pointer = request.paths()[0]
            .source
            .as_os_str()
            .as_encoded_bytes()
            .as_ptr();
        let capacity = request.paths_capacity();
        let rejected = controller.enqueue(request).unwrap_err();
        assert_eq!(rejected.reason, RejectReason::QueueFull);
        assert_eq!(rejected.request.id, 9);
        assert_eq!(rejected.request.paths_capacity(), capacity);
        assert_eq!(
            rejected.request.paths()[0]
                .source
                .as_os_str()
                .as_encoded_bytes()
                .as_ptr(),
            pointer
        );
        let budget = controller.budget();
        assert_eq!(
            budget.accounted_bytes,
            budget.fixed_headroom_bytes + budget.queued_input_bytes
        );
        assert!(budget.accounted_bytes <= RETAINED_BUDGET_BYTES);
        assert_eq!(controller.phase(), Phase::Queued);
        assert_eq!(controller.begin_close(), 8);
        assert!(controller.is_closed());
    }

    #[test]
    fn tiny_remaining_budget_rejects_before_any_admission_and_preserves_request() {
        let mut controller = Controller::new();
        let request = lexical(1);
        let charge = input_charge(&request).unwrap();
        controller.limit_bytes = FIXED_HEADROOM_BYTES + charge - 1;
        let pointer = request.paths()[0]
            .destination
            .as_os_str()
            .as_encoded_bytes()
            .as_ptr();
        let rejected = controller.enqueue(request).unwrap_err();
        assert_eq!(rejected.reason, RejectReason::MemoryBudget);
        assert_eq!(
            rejected.request.paths()[0]
                .destination
                .as_os_str()
                .as_encoded_bytes()
                .as_ptr(),
            pointer
        );
        assert_eq!(controller.queued_count(), 0);
        assert_eq!(controller.budget().accounted_bytes, FIXED_HEADROOM_BYTES);
        assert_eq!(controller.phase(), Phase::Idle);
        assert!(controller.active_id().is_none());
    }

    #[test]
    fn accounting_includes_vec_slack_and_path_capacities_not_only_lengths() {
        let small = lexical(1);
        let minimal = input_charge(&small).unwrap();
        let mut paths = Vec::with_capacity(4096);
        paths.push(CopyRequest {
            source: PathBuf::from("source"),
            destination: PathBuf::from("destination"),
        });
        let large_vec = Request::new(2, paths, options());
        let with_vec = input_charge(&large_vec).unwrap();
        assert!(with_vec > minimal + 4095 * std::mem::size_of::<CopyRequest>());
        let mut source = PathBuf::with_capacity(4096);
        source.push("source");
        let mut destination = PathBuf::with_capacity(4096);
        destination.push("destination");
        let large_paths = Request::new(
            3,
            vec![CopyRequest {
                source,
                destination,
            }],
            options(),
        );
        assert!(input_charge(&large_paths).unwrap() > minimal + 8192);
        let mut controller = Controller::new();
        controller.limit_bytes = FIXED_HEADROOM_BYTES + minimal;
        let rejected = controller.enqueue(large_paths).unwrap_err();
        assert_eq!(rejected.reason, RejectReason::MemoryBudget);
        assert_eq!(rejected.request.paths()[0].source.capacity(), 4096);
        assert!(controller.active_id().is_none());
    }

    #[test]
    fn cancellation_returns_pending_input_and_monotonic_ids_prevent_stale_reuse() {
        let mut controller = Controller::new();
        controller.enqueue(lexical(1)).unwrap();
        controller.enqueue(lexical(2)).unwrap();
        let before = controller.budget();
        let cancelled = controller.cancel_pending(2).unwrap();
        assert_eq!(cancelled.id, 2);
        assert_eq!(controller.cancelled_pending_count(), 1);
        assert_eq!(controller.queued_ids().collect::<Vec<_>>(), vec![1]);
        assert!(controller.budget().accounted_bytes < before.accounted_bytes);
        assert!(controller.cancel_pending(99).is_none());
        assert!(!controller.cancel(1));
        assert!(!controller.pause(1));
        let rejected = controller.enqueue(cancelled).unwrap_err();
        assert_eq!(rejected.reason, RejectReason::IdNotIncreasing);
        controller.enqueue(lexical(3)).unwrap();
        assert_eq!(controller.queued_ids().collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(controller.begin_close(), 2);
        assert_eq!(controller.cancelled_pending_count(), 3);
        assert_eq!(controller.budget().accounted_bytes, FIXED_HEADROOM_BYTES);
    }

    #[test]
    fn close_discards_counted_pending_jobs_without_starting_worker_and_is_idempotent() {
        let mut controller = Controller::new();
        for id in 1..=8 {
            controller.enqueue(empty(id)).unwrap();
        }
        assert_eq!(controller.begin_close(), 8);
        assert_eq!(controller.begin_close(), 0);
        let pumped = controller.pump();
        assert!(pumped.closed);
        assert!(pumped.completion.is_none());
        assert_eq!(pumped.cancelled_pending_total, 8);
        assert_eq!(controller.queued_count(), 0);
        assert_eq!(controller.phase(), Phase::Closed);
        assert_eq!(controller.budget().accounted_bytes, FIXED_HEADROOM_BYTES);
        let rejected = controller.enqueue(lexical(9)).unwrap_err();
        assert_eq!(rejected.reason, RejectReason::Closing);
        assert_eq!(rejected.request.id, 9);
    }

    #[test]
    fn fifo_dispatch_retains_active_charge_until_ack_then_waits_one_pump() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"queue bytes").unwrap();
        let mut controller = Controller::new();
        controller
            .enqueue(fixture.request(1, "one").start_paused())
            .unwrap();
        controller.enqueue(fixture.request(2, "two")).unwrap();
        let before = controller.budget().accounted_bytes;
        let first = controller.pump();
        assert_eq!(first.dispatched, Some(1));
        assert!(first.completion.is_none());
        assert_eq!(controller.active_id(), Some(1));
        assert_eq!(controller.queued_count(), 1);
        assert_eq!(controller.budget().accounted_bytes, before);
        assert!(controller.budget().active_input_bytes > 0);
        assert!(!controller.resume(2));
        assert!(controller.resume(1));
        let first = completion(&mut controller);
        assert_eq!(first.id(), 1);
        assert!(first.outcome().unwrap().result.is_ok());
        assert!(controller.active_id().is_none());
        assert_eq!(controller.queued_count(), 1);
        assert_eq!(controller.budget().active_input_bytes, 0);
        assert!(controller.budget().accounted_bytes < before);
        assert_eq!(controller.pump().dispatched, Some(2));
        let second = completion(&mut controller);
        assert_eq!(second.id(), 2);
        assert!(second.outcome().unwrap().result.is_ok());
        assert_eq!(fs::read(fixture.0.join("one")).unwrap(), b"queue bytes");
        assert_eq!(fs::read(fixture.0.join("two")).unwrap(), b"queue bytes");
        assert_eq!(controller.budget().accounted_bytes, FIXED_HEADROOM_BYTES);
        assert!(close(&mut controller).is_none());
    }

    #[test]
    fn active_and_pending_cancellation_are_distinct_and_controls_reject_old_ids() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), [7; 16384]).unwrap();
        let mut controller = Controller::new();
        controller
            .enqueue(fixture.request(1, "one").start_paused())
            .unwrap();
        controller
            .enqueue(fixture.request(2, "two").start_paused())
            .unwrap();
        assert_eq!(controller.pump().dispatched, Some(1));
        assert!(controller.cancel_pending(1).is_none());
        assert!(!controller.cancel(2));
        assert!(controller.cancel(1));
        let cancelled = completion(&mut controller);
        assert!(cancelled.outcome().unwrap().cancelled());
        let pending = controller.cancel_pending(2).unwrap();
        assert_eq!(pending.id, 2);
        assert_eq!(controller.cancelled_pending_count(), 1);
        controller
            .enqueue(fixture.request(3, "three").start_paused())
            .unwrap();
        assert_eq!(controller.pump().dispatched, Some(3));
        assert!(!controller.cancel(1));
        assert!(!controller.pause(1));
        assert!(!controller.resume(1));
        assert!(controller.resume(3));
        assert!(
            completion(&mut controller)
                .outcome()
                .unwrap()
                .result
                .is_ok()
        );
        assert!(!fixture.0.join("one").exists());
        assert!(!fixture.0.join("two").exists());
        assert_eq!(fs::read(fixture.0.join("three")).unwrap(), vec![7; 16384]);
        assert!(close(&mut controller).is_none());
    }

    #[test]
    fn close_counts_pending_discards_but_retains_active_ack_and_budget_until_drained() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"retained").unwrap();
        let mut controller = Controller::new();
        controller
            .enqueue(fixture.request(1, "one").start_paused())
            .unwrap();
        controller.enqueue(fixture.request(2, "two")).unwrap();
        controller.enqueue(fixture.request(3, "three")).unwrap();
        controller.pump();
        assert_eq!(controller.active_id(), Some(1));
        assert_eq!(controller.begin_close(), 2);
        assert_eq!(controller.queued_count(), 0);
        assert_eq!(controller.cancelled_pending_count(), 2);
        assert!(controller.budget().active_input_bytes > 0);
        assert!(!controller.is_closed());
        let result = close(&mut controller).unwrap();
        assert_eq!(result.id(), 1);
        assert!(result.outcome().unwrap().cancelled());
        assert!(controller.is_closed());
        assert_eq!(controller.budget().accounted_bytes, FIXED_HEADROOM_BYTES);
        assert_eq!(controller.cancelled_pending_count(), 2);
        assert!(controller.pump().completion.is_none());
        for name in ["one", "two", "three"] {
            assert!(!fixture.0.join(name).exists());
        }
    }

    #[test]
    fn close_preserves_unread_success_report_and_never_runs_queued_next_job() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"published").unwrap();
        let mut controller = Controller::new();
        controller.enqueue(fixture.request(1, "one")).unwrap();
        controller.enqueue(fixture.request(2, "two")).unwrap();
        controller.pump();
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.phase() != Phase::AwaitingAck {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(controller.begin_close(), 1);
        let finished = close(&mut controller).unwrap();
        let outcome = finished.outcome().unwrap();
        let report = outcome.report().unwrap();
        assert!(outcome.result.is_ok());
        assert_eq!(report.completed_count, 1);
        assert_eq!(report.completed, vec![fixture.0.join("one")]);
        assert!(!fixture.0.join("two").exists());
        assert_eq!(controller.cancelled_pending_count(), 1);
        assert!(controller.is_closed());
    }

    #[test]
    fn recoverable_engine_error_is_returned_complete_then_next_job_can_run() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"source").unwrap();
        fs::write(fixture.0.join("occupied"), b"keep").unwrap();
        let mut controller = Controller::new();
        controller.enqueue(fixture.request(1, "occupied")).unwrap();
        controller.enqueue(fixture.request(2, "copied")).unwrap();
        let failed = completion(&mut controller);
        let error = failed.outcome().unwrap().engine_error().unwrap();
        assert_eq!(error.cause.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(error.report.completed_count, 0);
        assert!(error.rescue_paths.is_empty());
        assert_eq!(controller.queued_count(), 1);
        assert!(controller.active_id().is_none());
        assert_eq!(controller.pump().dispatched, Some(2));
        assert!(
            completion(&mut controller)
                .outcome()
                .unwrap()
                .result
                .is_ok()
        );
        assert_eq!(fs::read(fixture.0.join("occupied")).unwrap(), b"keep");
        assert_eq!(fs::read(fixture.0.join("copied")).unwrap(), b"source");
        assert!(close(&mut controller).is_none());
    }
    #[test]
    fn held_pending_queue_stays_dormant_and_close_still_discards_and_drains() {
        let mut controller = Controller::new();
        controller.hold_queue(true);
        controller.enqueue(empty(1)).unwrap();
        let before = controller.budget();
        for _ in 0..3 {
            let pumped = controller.pump();
            assert!(pumped.dispatched.is_none());
            assert!(pumped.completion.is_none());
        }
        assert!(controller.queue_held());
        assert_eq!(controller.phase(), Phase::Held);
        assert!(controller.active_id().is_none());
        assert_eq!(controller.worker.phase(), copy_job::Phase::Dormant);
        assert_eq!(controller.budget(), before);
        assert_eq!(controller.begin_close(), 1);
        assert!(controller.pump().closed);
        assert_eq!(controller.cancelled_pending_count(), 1);
    }

    #[test]
    fn hold_blocks_next_dispatch_but_continues_active_ack_progress_and_shutdown() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"hold bytes").unwrap();
        let mut controller = Controller::new();
        controller
            .enqueue(fixture.request(1, "one").start_paused())
            .unwrap();
        controller.enqueue(fixture.request(2, "two")).unwrap();
        assert_eq!(controller.pump().dispatched, Some(1));
        controller.hold_queue(true);
        assert!(controller.resume(1));
        let first = completion(&mut controller);
        assert_eq!(first.id(), 1);
        assert!(first.outcome().unwrap().result.is_ok());
        assert_eq!(controller.phase(), Phase::Held);
        assert_eq!(controller.queued_count(), 1);
        for _ in 0..3 {
            assert!(controller.pump().dispatched.is_none());
        }
        assert!(!fixture.0.join("two").exists());
        assert_eq!(controller.budget().active_input_bytes, 0);
        controller.hold_queue(false);
        assert_eq!(controller.pump().dispatched, Some(2));
        assert!(
            completion(&mut controller)
                .outcome()
                .unwrap()
                .result
                .is_ok()
        );
        controller.hold_queue(true);
        assert!(close(&mut controller).is_none());
        assert!(controller.is_closed());
    }
    #[test]
    fn maximum_producer_capacity_outcome_fits_documented_ack_reserve() {
        use std::os::unix::ffi::OsStringExt;
        // Synthetic capacity fixture only: these strings are not verified rescue
        // locations and the test makes no filesystem or process-PSS claim.
        fn paths() -> Vec<PathBuf> {
            let mut output = Vec::with_capacity(PRODUCER_DETAILS);
            for _ in 0..PRODUCER_DETAILS {
                let mut bytes = Vec::with_capacity(PRODUCER_PATH_CAPACITY);
                bytes.resize(PRODUCER_PATH_CAPACITY, b'x');
                bytes.truncate(PRODUCER_PATH_CAPACITY - 1);
                output.push(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
            }
            output
        }
        fn errors() -> Vec<String> {
            let mut output = Vec::with_capacity(PRODUCER_DETAILS);
            for _ in 0..PRODUCER_DETAILS {
                let mut text = String::with_capacity(PRODUCER_ERROR_CAPACITY);
                text.push_str("Synthetic bounded error");
                output.push(text);
            }
            output
        }
        let mut completed = paths();
        let mut skipped = Vec::with_capacity(PRODUCER_DETAILS);
        skipped.extend(completed.drain(PRODUCER_DETAILS / 2..));
        let report = CopyReport {
            completed,
            skipped,
            completed_count: PRODUCER_DETAILS as u64,
            cleanup_paths: paths(),
            cleanup_errors: errors(),
            ..Default::default()
        };
        assert_eq!(
            report.completed.len() + report.skipped.len(),
            PRODUCER_DETAILS
        );
        let mut cause = String::with_capacity(PRODUCER_ERROR_CAPACITY);
        cause.push_str("Synthetic cause envelope");
        let cause_capacity = cause.capacity();
        let error = CopyError {
            cause: std::io::Error::other(cause),
            report: Box::new(report),
            rescue_paths: paths(),
            cleanup_errors: errors(),
            rescue_paths_omitted: 0,
            cleanup_errors_omitted: 0,
            cleanup_unresolved_entries: 0,
            details_truncated: 0,
        };
        let outcome = Outcome {
            moving: false,
            extracting: false,
            creating: false,
            archive_action: None,
            exdev: None,
            id: 1,
            result: Err(copy_job::Failure::Engine(Box::new(error))),
            progress: copy_job::ProgressStats::default(),
        };
        let error = outcome.engine_error().unwrap();
        let report = outcome.report().unwrap();
        let mut observed = std::mem::size_of::<Outcome>()
            + std::mem::size_of::<CopyError>()
            + std::mem::size_of::<CopyReport>();
        for set in [
            &report.completed,
            &report.skipped,
            &report.cleanup_paths,
            &error.rescue_paths,
        ] {
            assert!(set.len() <= PRODUCER_DETAILS);
            assert_eq!(set.capacity(), PRODUCER_DETAILS);
            observed += set.capacity() * std::mem::size_of::<PathBuf>();
            for path in set {
                assert_eq!(path.capacity(), PRODUCER_PATH_CAPACITY);
                observed += path.capacity();
            }
        }
        for set in [&report.cleanup_errors, &error.cleanup_errors] {
            assert_eq!(set.len(), PRODUCER_DETAILS);
            assert_eq!(set.capacity(), PRODUCER_DETAILS);
            observed += set.capacity() * std::mem::size_of::<String>();
            for text in set {
                assert_eq!(text.capacity(), PRODUCER_ERROR_CAPACITY);
                observed += text.capacity();
            }
        }
        observed += cause_capacity + std::mem::size_of::<String>() + 3 * 64;
        let allocation_slack = (5 * PRODUCER_DETAILS + 6 + 3) * 64;
        assert!(observed + allocation_slack <= REPORT_DERIVED_BYTES);
        assert!(observed + allocation_slack < REPORT_HEADROOM_BYTES);
        // The constant assertion above covers the declared worst-case producer
        // derivation. Neither result bounds external UI history or measured PSS.
        assert_eq!(
            Controller::new().budget().fixed_headroom_bytes,
            REPORT_HEADROOM_BYTES + STRUCTURE_HEADROOM_BYTES
        );
    }
    #[test]
    fn move_descriptor_budget_includes_active_unread_and_pending_tokens() {
        use files_core::move_engine::PreparedMove;
        use std::sync::atomic::AtomicBool;
        static MOVE_SERIAL: AtomicU64 = AtomicU64::new(0);
        let root = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap()
            .join(format!(
                ".cache/files-test/move-fd-{}-{}",
                std::process::id(),
                MOVE_SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir(root.join("dst")).unwrap();
        for i in 0..126 {
            fs::write(root.join("src").join(i.to_string()), b"x").unwrap();
        }
        let capture = |count| {
            PreparedMove::capture(
                (0..count)
                    .map(|i| CopyRequest {
                        source: root.join("src").join(i.to_string()),
                        destination: root.join("dst").join(i.to_string()),
                    })
                    .collect(),
                &AtomicBool::new(false),
            )
            .unwrap()
        };
        let mut c = Controller::new();
        c.enqueue(Request::native_move(1, capture(126)).start_paused())
            .unwrap();
        c.pump();
        assert_eq!(c.active_id(), Some(1));
        let remaining_roots = 256 - crate::move_operation::EXDEV_FD_RESERVATION - 128 - 2;
        assert_eq!(remaining_roots, 102);
        assert_eq!(24 + 128 + (remaining_roots + 2), 256);
        c.enqueue(Request::native_move(2, capture(remaining_roots)))
            .unwrap();
        let third = c.enqueue(Request::native_move(3, capture(1))).unwrap_err();
        assert_eq!(third.reason, RejectReason::FileDescriptors);
        c.cancel(1);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while c.worker.phase() != copy_job::Phase::AwaitingAck && std::time::Instant::now() < until
        {
            std::thread::yield_now();
        }
        assert_eq!(c.worker.phase(), copy_job::Phase::AwaitingAck);
        let third = c.enqueue(third.request).unwrap_err();
        assert_eq!(third.reason, RejectReason::FileDescriptors);
        let ack = c.pump();
        assert!(ack.completion.unwrap().outcome().unwrap().moving);
        assert!(c.enqueue(third.request).is_ok());
        c.begin_close();
        while !c.is_closed() {
            c.pump();
            std::thread::yield_now();
        }
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}
