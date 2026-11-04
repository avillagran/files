//! Bounded, generation-independent spatial saves. The queue owns immutable
//! snapshots until an exact path/revision acknowledgement arrives. Cloning an
//! Arc for a request does not duplicate the layout, even if the sender is full.
use crate::aux_io::Request;
use files_core::spatial::{MAX_LAYOUT_BYTES, SpatialLayout};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{SyncSender, TrySendError},
    },
};

pub const SNAPSHOT_RESERVATION_BYTES: usize = MAX_LAYOUT_BYTES;
const MAX_DIRECTORY_BYTES: usize = 4096;
const MAX_ERROR_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub struct QueueLimits {
    pub max_directories: usize,
    pub max_snapshots: usize,
    pub max_reserved_bytes: usize,
}
impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            max_directories: 2,
            max_snapshots: 2,
            max_reserved_bytes: 2 * SNAPSHOT_RESERVATION_BYTES,
        }
    }
}

#[derive(Debug)]
pub struct RejectedSnapshot {
    pub directory: PathBuf,
    pub revision: u64,
    pub layout: Arc<SpatialLayout>,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveFailure {
    pub directory: PathBuf,
    pub revision: u64,
    pub message: String,
}

struct Snapshot {
    directory: PathBuf,
    revision: u64,
    layout: Arc<SpatialLayout>,
}
struct Pending {
    snapshot: Snapshot,
    error: Option<SaveFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchStatus {
    Sent,
    Empty,
    Busy,
    Full,
    Blocked,
    Disconnected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrainState {
    Idle,
    Pending,
    Failed,
}

pub struct SaveQueue {
    limits: QueueLimits,
    pending: BTreeMap<PathBuf, Pending>,
    in_flight: Option<Snapshot>,
    last_error: Option<SaveFailure>,
}
impl Default for SaveQueue {
    fn default() -> Self {
        Self::new(QueueLimits::default())
    }
}
impl SaveQueue {
    pub fn new(limits: QueueLimits) -> Self {
        Self {
            limits,
            pending: BTreeMap::new(),
            in_flight: None,
            last_error: None,
        }
    }

    /// The caller retains its active Arc on rejection. A newer revision replaces
    /// the pending snapshot for this exact lossless path; an in-flight older
    /// revision remains retained until ACK. Revision numbers are caller-owned.
    pub fn submit_shared(
        &mut self,
        directory: PathBuf,
        revision: u64,
        layout: Arc<SpatialLayout>,
    ) -> Result<(), RejectedSnapshot> {
        if let Some(reason) = self.rejection_reason(&directory, revision) {
            return Err(RejectedSnapshot {
                directory,
                revision,
                layout,
                reason,
            });
        }
        let error = self.pending.get(&directory).and_then(|p| p.error.clone());
        self.pending.insert(
            directory.clone(),
            Pending {
                snapshot: Snapshot {
                    directory,
                    revision,
                    layout,
                },
                error,
            },
        );
        Ok(())
    }

    fn rejection_reason(&self, directory: &Path, revision: u64) -> Option<&'static str> {
        if directory.as_os_str().as_encoded_bytes().len() > MAX_DIRECTORY_BYTES {
            return Some("spatial save directory path exceeds limit");
        }
        if self
            .latest_revision(directory)
            .is_some_and(|latest| revision <= latest)
        {
            return Some("spatial save revision is already buffered or stale");
        }
        let replacing = self.pending.contains_key(directory);
        let snapshots = self.retained_count().checked_add(usize::from(!replacing));
        let Some(snapshots) = snapshots else {
            return Some("spatial save snapshot limit exceeded");
        };
        if snapshots > self.limits.max_snapshots
            || snapshots
                .checked_mul(SNAPSHOT_RESERVATION_BYTES)
                .is_none_or(|bytes| bytes > self.limits.max_reserved_bytes)
        {
            return Some("spatial save snapshot budget is full");
        }
        let known = replacing
            || self
                .in_flight
                .as_ref()
                .is_some_and(|s| s.directory == directory);
        let directories = self.pending.len()
            + usize::from(
                self.in_flight
                    .as_ref()
                    .is_some_and(|s| !self.pending.contains_key(&s.directory)),
            );
        if !known && directories >= self.limits.max_directories {
            return Some("spatial save directory limit exceeded");
        }
        None
    }

    pub fn latest_snapshot(&self, directory: &Path) -> Option<(u64, Arc<SpatialLayout>)> {
        self.pending
            .get(directory)
            .map(|p| &p.snapshot)
            .into_iter()
            .chain(self.in_flight.as_ref().filter(|s| s.directory == directory))
            .max_by_key(|s| s.revision)
            .map(|s| (s.revision, Arc::clone(&s.layout)))
    }
    pub fn failed_directories(&self) -> Vec<PathBuf> {
        self.pending
            .iter()
            .filter(|(_, p)| p.error.is_some())
            .map(|(p, _)| p.clone())
            .collect()
    }

    pub fn latest_revision(&self, directory: &Path) -> Option<u64> {
        self.pending
            .get(directory)
            .map(|p| p.snapshot.revision)
            .into_iter()
            .chain(
                self.in_flight
                    .as_ref()
                    .filter(|s| s.directory == directory)
                    .map(|s| s.revision),
            )
            .max()
    }

    /// At most one outstanding save regardless of AuxIo's channel capacity.
    /// Full/disconnected channels retain the original pending snapshot.
    pub fn dispatch(&mut self, sender: &SyncSender<Request>) -> DispatchStatus {
        if self.in_flight.is_some() {
            return DispatchStatus::Busy;
        }
        let directory = self
            .pending
            .iter()
            .find(|(_, p)| p.error.is_none())
            .map(|(p, _)| p.clone());
        let Some(directory) = directory else {
            return if self.pending.is_empty() {
                DispatchStatus::Empty
            } else {
                DispatchStatus::Blocked
            };
        };
        let mut pending = self
            .pending
            .remove(&directory)
            .expect("selected pending snapshot");
        let request = Request::Save(
            pending.snapshot.revision,
            directory.clone(),
            Arc::clone(&pending.snapshot.layout),
        );
        match sender.try_send(request) {
            Ok(()) => {
                self.in_flight = Some(pending.snapshot);
                DispatchStatus::Sent
            }
            Err(TrySendError::Full(_)) => {
                self.pending.insert(directory, pending);
                DispatchStatus::Full
            }
            Err(TrySendError::Disconnected(_)) => {
                let failure = SaveFailure {
                    directory: directory.clone(),
                    revision: pending.snapshot.revision,
                    message: "spatial persistence worker disconnected; retry is required".into(),
                };
                pending.error = Some(failure.clone());
                self.last_error = Some(failure);
                self.pending.insert(directory, pending);
                DispatchStatus::Disconnected
            }
        }
    }

    /// An unrelated or stale ACK cannot clear current work. Failed older saves
    /// block the latest pending revision for explicit retry rather than losing
    /// the error during navigation. Returns whether this ACK was recognized.
    pub fn acknowledge(
        &mut self,
        directory: &Path,
        revision: u64,
        result: Result<(), String>,
    ) -> bool {
        if !self
            .in_flight
            .as_ref()
            .is_some_and(|s| s.directory == directory && s.revision == revision)
        {
            return false;
        }
        let snapshot = self.in_flight.take().expect("matched in-flight snapshot");
        match result {
            Ok(()) => {
                if self
                    .last_error
                    .as_ref()
                    .is_some_and(|e| e.directory == directory && e.revision <= revision)
                {
                    self.last_error = None;
                }
            }
            Err(mut message) => {
                truncate_message(&mut message);
                let failure = SaveFailure {
                    directory: snapshot.directory.clone(),
                    revision,
                    message,
                };
                self.last_error = Some(failure.clone());
                if let Some(latest) = self.pending.get_mut(directory) {
                    latest.error = Some(failure);
                } else {
                    self.pending.insert(
                        snapshot.directory.clone(),
                        Pending {
                            snapshot,
                            error: Some(failure),
                        },
                    );
                }
            }
        }
        true
    }

    pub fn retry(&mut self, directory: &Path) -> bool {
        match self.pending.get_mut(directory) {
            Some(pending) if pending.error.is_some() => {
                pending.error = None;
                true
            }
            _ => false,
        }
    }
    #[cfg(test)]
    pub fn failure(&self, directory: &Path) -> Option<&SaveFailure> {
        self.pending
            .get(directory)
            .and_then(|pending| pending.error.as_ref())
    }
    pub fn last_error(&self) -> Option<&SaveFailure> {
        self.last_error.as_ref()
    }
    pub fn retained_count(&self) -> usize {
        self.pending.len() + usize::from(self.in_flight.is_some())
    }
    pub fn drain_state(&self) -> DrainState {
        if self.pending.values().any(|p| p.error.is_some()) {
            DrainState::Failed
        } else if self.retained_count() != 0 {
            DrainState::Pending
        } else {
            DrainState::Idle
        }
    }
}
fn truncate_message(message: &mut String) {
    if message.len() > MAX_ERROR_BYTES {
        let mut end = MAX_ERROR_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    fn shared() -> Arc<SpatialLayout> {
        Arc::new(SpatialLayout::default())
    }

    #[test]
    fn saturated_sender_and_rejected_navigation_preserve_owned_snapshots() {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(Request::Load(0, "occupied".into())).unwrap();
        let mut queue = SaveQueue::default();
        let original = shared();
        queue
            .submit_shared("first".into(), 1, Arc::clone(&original))
            .unwrap();
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Full);
        assert_eq!(Arc::strong_count(&original), 2);
        assert_eq!(queue.latest_revision(Path::new("first")), Some(1));
        queue.submit_shared("second".into(), 1, shared()).unwrap();
        let rejected = shared();
        let error = queue
            .submit_shared("third".into(), 1, Arc::clone(&rejected))
            .unwrap_err();
        assert!(Arc::ptr_eq(&error.layout, &rejected));
        assert_eq!(queue.retained_count(), 2);
        drop(rx.recv().unwrap());
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Sent);
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Busy);
    }

    #[test]
    fn latest_revision_coalesces_and_stale_ack_cannot_clear_it() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut queue = SaveQueue::default();
        queue.submit_shared("folder".into(), 1, shared()).unwrap();
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Sent);
        drop(rx.recv().unwrap());
        queue.submit_shared("folder".into(), 2, shared()).unwrap();
        let latest = shared();
        queue
            .submit_shared("folder".into(), 3, Arc::clone(&latest))
            .unwrap();
        assert_eq!(queue.retained_count(), 2);
        assert!(!queue.acknowledge(Path::new("folder"), 0, Ok(())));
        assert!(queue.acknowledge(Path::new("folder"), 1, Ok(())));
        assert_eq!(queue.latest_revision(Path::new("folder")), Some(3));
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Sent);
        match rx.recv().unwrap() {
            Request::Save(3, _, layout) => assert!(Arc::ptr_eq(&layout, &latest)),
            _ => panic!("expected latest save"),
        }
    }

    #[test]
    fn save_error_survives_navigation_and_requires_explicit_retry() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut queue = SaveQueue::default();
        queue.submit_shared("old".into(), 1, shared()).unwrap();
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        queue.submit_shared("new".into(), 1, shared()).unwrap();
        assert!(queue.acknowledge(Path::new("old"), 1, Err("disk full".into())));
        assert_eq!(queue.last_error().unwrap().directory, Path::new("old"));
        assert_eq!(queue.drain_state(), DrainState::Failed);
        assert!(queue.retry(Path::new("old")));
        assert_eq!(queue.drain_state(), DrainState::Pending);
        // BTree order dispatches 'new' first. Its success must retain 'old' error.
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        queue.acknowledge(Path::new("new"), 1, Ok(()));
        assert_eq!(queue.last_error().unwrap().message, "disk full");
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        queue.acknowledge(Path::new("old"), 1, Ok(()));
        assert!(queue.last_error().is_none());
        assert_eq!(queue.drain_state(), DrainState::Idle);
    }

    #[test]
    fn closing_is_pending_until_ack_and_failure_keeps_latest_snapshot() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut queue = SaveQueue::default();
        assert_eq!(queue.drain_state(), DrainState::Idle);
        queue.submit_shared("folder".into(), 1, shared()).unwrap();
        assert_eq!(queue.drain_state(), DrainState::Pending);
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        queue.submit_shared("folder".into(), 2, shared()).unwrap();
        queue.acknowledge(
            Path::new("folder"),
            1,
            Err("failed earlier revision".into()),
        );
        assert_eq!(queue.latest_revision(Path::new("folder")), Some(2));
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Blocked);
        queue.retry(Path::new("folder"));
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        assert_eq!(queue.drain_state(), DrainState::Pending);
        queue.acknowledge(Path::new("folder"), 2, Ok(()));
        assert_eq!(queue.drain_state(), DrainState::Idle);
    }

    #[test]
    fn separate_directory_failures_are_both_retained() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut queue = SaveQueue::default();
        queue.submit_shared("a".into(), 1, shared()).unwrap();
        queue.submit_shared("b".into(), 1, shared()).unwrap();
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        queue.acknowledge(Path::new("a"), 1, Err("a error".into()));
        queue.dispatch(&tx);
        drop(rx.recv().unwrap());
        queue.acknowledge(Path::new("b"), 1, Err("b error".into()));
        assert_eq!(queue.failure(Path::new("a")).unwrap().message, "a error");
        assert_eq!(queue.failure(Path::new("b")).unwrap().message, "b error");
        assert_eq!(queue.drain_state(), DrainState::Failed);
    }

    #[test]
    fn disconnected_worker_retains_snapshot_and_bounded_error() {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(rx);
        let mut queue = SaveQueue::default();
        queue.submit_shared("folder".into(), 1, shared()).unwrap();
        assert_eq!(queue.dispatch(&tx), DispatchStatus::Disconnected);
        assert_eq!(queue.retained_count(), 1);
        assert_eq!(queue.drain_state(), DrainState::Failed);
        let mut error = "á".repeat(1025);
        truncate_message(&mut error);
        assert!(error.len() <= MAX_ERROR_BYTES);
        assert!(queue.retry(Path::new("folder")));
    }
}
