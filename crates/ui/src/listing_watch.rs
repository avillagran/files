//! Bounded invalidation hints, never entry paths or operation authority.
//! Used by listing_sync to refresh browsers only when their interaction is idle.
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
// Columns and dual-pane mode are mutually exclusive; the column chain is bounded.
pub const MAX_TARGETS: usize = crate::columns::MAX_COLUMNS;
const RETRY: Duration = Duration::from_secs(2);
const RECONCILE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
}
#[cfg(unix)]
fn identity(path: &std::path::Path) -> notify::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).map_err(notify::Error::io)?;
    if !metadata.is_dir() {
        return Err(notify::Error::generic("Se requiere una carpeta"));
    }
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}
#[cfg(not(unix))]
fn identity(_: &std::path::Path) -> notify::Result<Identity> {
    Err(notify::Error::generic(
        "Identidad de carpeta aún no implementada en esta plataforma",
    ))
}
struct ActiveWatch {
    target: Target,
    epoch: u64,
    watcher: Option<RecommendedWatcher>,
    identity: Option<Identity>,
    due: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub key: u64,
    pub path: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Attaching,
    Ready,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub epoch: u64,
    pub target: Target,
    pub serial: u64,
    pub changes: u64,
    pub attachment: u64,
    pub status: Status,
}
struct Slot {
    target: Target,
    epoch: u64,
    serial: u64,
    changes: u64,
    attachment: u64,
    arm: u64,
    dirty: bool,
    reattach: bool,
    status: Status,
}
struct State {
    epoch: u64,
    next_lease: u64,
    slots: Vec<Slot>,
    stop: bool,
}
struct Shared {
    state: Mutex<State>,
    stopped: AtomicBool,
}
impl Shared {
    fn event(&self, epoch: u64, key: u64, reattach: bool) -> bool {
        self.event_for(epoch, key, None, reattach)
    }
    fn event_for(&self, epoch: u64, key: u64, arm: Option<u64>, reattach: bool) -> bool {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.stop {
            return false;
        }
        if let Some(slot) = s
            .slots
            .iter_mut()
            .find(|s| s.target.key == key && s.epoch == epoch)
        {
            if arm.is_some_and(|arm| arm != slot.arm) {
                return false;
            }
            slot.serial = slot
                .serial
                .checked_add(1)
                .expect("listing event serial exhausted");
            slot.changes = slot
                .changes
                .checked_add(1)
                .expect("listing changes exhausted");
            slot.dirty = true;
            slot.reattach |= reattach;
            return true;
        }
        false
    }
    fn attached(&self, epoch: u64, key: u64, ready: bool) -> bool {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.stop {
            return false;
        }
        let Some(slot) = s
            .slots
            .iter_mut()
            .find(|s| s.target.key == key && s.epoch == epoch)
        else {
            return false;
        };
        let status = if ready { Status::Ready } else { Status::Failed };
        if ready {
            slot.attachment = slot
                .attachment
                .checked_add(1)
                .expect("listing attachment exhausted");
        }
        if ready || slot.status != status {
            slot.serial = slot
                .serial
                .checked_add(1)
                .expect("listing event serial exhausted");
            slot.dirty = true;
        }
        slot.status = status;
        true
    }
}
struct Stopped(Arc<Shared>);
impl Drop for Stopped {
    fn drop(&mut self) {
        self.0.stopped.store(true, Ordering::Release);
    }
}
pub struct ListingWatch {
    shared: Arc<Shared>,
    wake: SyncSender<()>,
}
impl ListingWatch {
    pub fn new() -> Result<Self, String> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                epoch: 0,
                next_lease: 0,
                slots: Vec::new(),
                stop: false,
            }),
            stopped: AtomicBool::new(false),
        });
        let (wake, rx) = mpsc::sync_channel(1);
        let worker = shared.clone();
        let signal = wake.clone();
        std::thread::Builder::new()
            .name("files-listing-watch".into())
            .spawn(move || {
                let _stopped = Stopped(worker.clone());
                let mut epoch = 0;
                let mut watches: Vec<ActiveWatch> = Vec::new();
                loop {
                    let (next, targets, stop) = {
                        let s = worker.state.lock().unwrap_or_else(|e| e.into_inner());
                        (
                            s.epoch,
                            (s.epoch != epoch).then(|| {
                                s.slots
                                    .iter()
                                    .map(|s| (s.target.clone(), s.epoch))
                                    .collect::<Vec<_>>()
                            }),
                            s.stop,
                        )
                    };
                    if stop {
                        break;
                    }
                    if next != epoch {
                        let mut old = std::mem::take(&mut watches);
                        epoch = next;
                        for (target, lease) in targets.unwrap_or_default() {
                            if let Some(i) = old
                                .iter()
                                .position(|w| w.target == target && w.epoch == lease)
                            {
                                watches.push(old.swap_remove(i));
                            } else {
                                watches.push(ActiveWatch {
                                    target,
                                    epoch: lease,
                                    watcher: None,
                                    identity: None,
                                    due: Instant::now(),
                                });
                            }
                        }
                        drop(old); // Only removed/replaced roots lose their native watches.
                    }
                    for active in &mut watches {
                        {
                            let s = worker.state.lock().unwrap_or_else(|e| e.into_inner());
                            if s.stop || s.epoch != epoch {
                                break;
                            }
                        }
                        let ActiveWatch {
                            target,
                            epoch: lease,
                            watcher: watch,
                            identity: observed,
                            due: retry,
                        } = active;
                        // Native notifications follow an inode, while the open path
                        // can resolve to a different inode after alias replacement.
                        if watch.is_some() && Instant::now() >= *retry {
                            if identity(&target.path).ok() != *observed {
                                worker.event(*lease, target.key, true);
                            }
                            *retry = Instant::now() + RECONCILE;
                        }
                        let attach = {
                            let mut s = worker.state.lock().unwrap_or_else(|e| e.into_inner());
                            if s.stop || s.epoch != epoch {
                                None
                            } else if let Some(slot) = s
                                .slots
                                .iter_mut()
                                .find(|s| s.target.key == target.key && s.epoch == *lease)
                            {
                                let attach =
                                    slot.reattach || watch.is_none() && Instant::now() >= *retry;
                                if attach {
                                    slot.reattach = false;
                                    slot.arm = slot
                                        .arm
                                        .checked_add(1)
                                        .expect("listing watch arm exhausted");
                                }
                                attach.then_some(slot.arm)
                            } else {
                                None
                            }
                        };
                        let Some(arm) = attach else {
                            continue;
                        };
                        *watch = None; // Native watcher destruction stays on this worker.
                        *observed = None;
                        let state = worker.clone();
                        let wake = signal.clone();
                        let key = target.key;
                        let lease = *lease;
                        let path = target.path.clone();
                        let attached = identity(&target.path).and_then(|before| {
                            notify::recommended_watcher(
                                move |event: notify::Result<notify::Event>| {
                                    let reattach = match &event {
                                        Ok(event) => {
                                            if !event.need_rescan()
                                                && matches!(event.kind, EventKind::Access(_))
                                            {
                                                return;
                                            }
                                            event.need_rescan()
                                                || event.paths.is_empty()
                                                || event.paths.len() > 64
                                                || event.paths.iter().take(64).any(|p| p == &path)
                                        }
                                        Err(_) => true,
                                    };
                                    if state.event_for(lease, key, Some(arm), reattach) {
                                        let _ = wake.try_send(());
                                    }
                                },
                            )
                            .and_then(|mut w| {
                                w.watch(&target.path, RecursiveMode::NonRecursive)?;
                                if identity(&target.path)? != before {
                                    return Err(notify::Error::generic(
                                        "La carpeta cambió al registrar la vigilancia",
                                    ));
                                }
                                Ok((w, before))
                            })
                        });
                        let ready = attached.is_ok();
                        if worker.attached(lease, target.key, ready)
                            && let Ok((watcher, stamp)) = attached
                        {
                            *watch = Some(watcher);
                            *observed = Some(stamp);
                        }
                        *retry = Instant::now() + if ready { RECONCILE } else { RETRY };
                    }
                    let retry = watches.iter().map(|active| active.due).min();
                    match retry {
                        Some(due) => {
                            if rx.recv_timeout(due.saturating_duration_since(Instant::now()))
                                == Err(mpsc::RecvTimeoutError::Disconnected)
                            {
                                break;
                            }
                        }
                        None => {
                            if rx.recv().is_err() {
                                break;
                            }
                        }
                    }
                }
                // The guard acknowledges after native watchers have been dropped.
                watches.clear();
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { shared, wake })
    }
    /// Complete preflight precedes any epoch or pending-notice change. No FS I/O.
    pub fn configure(&mut self, mut targets: Vec<Target>) -> Result<u64, &'static str> {
        if targets.len() > MAX_TARGETS {
            return Err("Demasiadas carpetas abiertas para vigilar");
        }
        for target in &targets {
            let raw = target.path.as_os_str().as_encoded_bytes();
            if target.key == 0 || !target.path.is_absolute() || raw.len() > 4096 || raw.contains(&0)
            {
                return Err("Destino de vigilancia inválido");
            }
        }
        targets.sort_by_key(|t| t.key);
        if targets.windows(2).any(|p| p[0].key == p[1].key) {
            return Err("Identidad de vigilancia duplicada");
        }
        let mut s = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.stop {
            return Err("Vigilancia cerrada");
        }
        if s.slots.iter().map(|s| &s.target).eq(targets.iter()) {
            return Ok(s.epoch);
        }
        s.epoch = s
            .epoch
            .checked_add(1)
            .expect("listing watch epoch exhausted");
        let mut old = std::mem::take(&mut s.slots);
        for target in targets {
            if let Some(i) = old.iter().position(|slot| slot.target == target) {
                s.slots.push(old.swap_remove(i));
            } else {
                s.next_lease = s
                    .next_lease
                    .checked_add(1)
                    .expect("listing root lease exhausted");
                let lease = s.next_lease;
                s.slots.push(Slot {
                    target,
                    epoch: lease,
                    serial: 0,
                    changes: 0,
                    attachment: 0,
                    arm: 0,
                    dirty: false,
                    reattach: true,
                    status: Status::Attaching,
                });
            }
        }
        let epoch = s.epoch;
        drop(s);
        let _ = self.wake.try_send(());
        Ok(epoch)
    }
    /// Sticky bounded slots let consumers defer during forms without losing events.
    pub fn notices(&self) -> Vec<Notice> {
        let s = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        s.slots
            .iter()
            .filter(|slot| slot.dirty)
            .map(|slot| Notice {
                epoch: slot.epoch,
                target: slot.target.clone(),
                serial: slot.serial,
                changes: slot.changes,
                attachment: slot.attachment,
                status: slot.status,
            })
            .collect()
    }
    pub fn pending(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .slots
            .iter()
            .any(|s| s.dirty)
    }
    pub fn acknowledge(&mut self, notice: &Notice) -> bool {
        let mut s = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.stop {
            return false;
        }
        let Some(slot) = s.slots.iter_mut().find(|s| {
            s.target == notice.target && s.epoch == notice.epoch && s.serial == notice.serial
        }) else {
            return false;
        };
        slot.dirty = false;
        true
    }
    pub fn stop(&mut self) {
        let mut s = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        s.stop = true;
        s.slots.clear();
        drop(s);
        let _ = self.wake.try_send(());
    }
    pub fn stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Acquire)
    }
}
impl Drop for ListingWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(key: u64, path: &str) -> Target {
        Target {
            key,
            path: path.into(),
        }
    }
    fn manual() -> ListingWatch {
        let (wake, _rx) = mpsc::sync_channel(1);
        ListingWatch {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    epoch: 0,
                    next_lease: 0,
                    slots: vec![],
                    stop: false,
                }),
                stopped: AtomicBool::new(false),
            }),
            wake,
        }
    }
    #[test]
    fn unchanged_root_keeps_lease_pending_notice_and_ack_when_another_target_changes() {
        let mut w = manual();
        let first = w.configure(vec![target(1, "/a")]).unwrap();
        assert!(w.shared.attached(first, 1, true));
        let pending = w.notices().remove(0);
        let configuration = w.configure(vec![target(1, "/a"), target(2, "/b")]).unwrap();
        assert_ne!(configuration, first);
        assert_eq!(w.notices(), vec![pending.clone()]);
        assert!(w.acknowledge(&pending));
        assert!(w.shared.event(first, 1, false));
        let next = w.notices().remove(0);
        w.configure(vec![target(1, "/new"), target(2, "/b")])
            .unwrap();
        assert!(!w.acknowledge(&next));
        assert!(!w.shared.event(first, 1, true));
        assert!(!w.shared.attached(first, 1, true));
    }
    #[test]
    fn attachment_and_failure_notices_do_not_invent_filesystem_change_events() {
        let mut w = manual();
        let lease = w.configure(vec![target(1, "/a")]).unwrap();
        w.shared.attached(lease, 1, true);
        let first = w.notices().remove(0);
        assert_eq!(first.changes, 0);
        w.shared.event(lease, 1, false);
        let event = w.notices().remove(0);
        assert_eq!(event.changes, 1);
        assert!(!w.acknowledge(&first));
        w.shared.attached(lease, 1, false);
        let failed = w.notices().remove(0);
        assert_eq!(failed.changes, event.changes);
        assert_eq!(failed.status, Status::Failed);
        w.shared.attached(lease, 1, true);
        let recovered = w.notices().remove(0);
        assert_eq!(recovered.changes, event.changes);
        assert_eq!(recovered.attachment, first.attachment + 1);
    }
    #[test]
    fn coalesced_slots_keep_newer_events_and_reject_stale_epoch_ack() {
        let mut w = manual();
        let epoch = w.configure(vec![target(2, "/b"), target(1, "/a")]).unwrap();
        assert_eq!(
            epoch,
            w.configure(vec![target(1, "/a"), target(2, "/b")]).unwrap()
        );
        w.shared.attached(epoch, 1, true);
        let first = w.notices().remove(0);
        for _ in 0..10_000 {
            w.shared.event(epoch, 1, false);
        }
        assert_eq!(w.notices().len(), 1);
        assert!(!w.acknowledge(&first));
        let latest = w.notices().remove(0);
        assert_eq!(latest.serial, first.serial + 10_000);
        assert!(w.acknowledge(&latest));
        assert!(w.notices().is_empty());
        w.configure(vec![target(1, "/new")]).unwrap();
        w.shared.event(epoch, 1, true);
        assert!(w.notices().is_empty());
        assert!(!w.acknowledge(&latest));
    }
    #[test]
    fn invalid_complete_configuration_preserves_pending_notices_and_identity() {
        let mut w = manual();
        let epoch = w.configure(vec![target(1, "/a")]).unwrap();
        w.shared.attached(epoch, 1, true);
        let before = w.notices();
        for targets in [
            vec![target(0, "/a")],
            vec![target(2, "relative")],
            vec![target(1, "/a"), target(1, "/b")],
            (1..=MAX_TARGETS as u64 + 1)
                .map(|key| target(key, "/a"))
                .collect(),
            vec![target(1, &format!("/{}", "x".repeat(4096)))],
            vec![target(1, "/bad\0path")],
        ] {
            assert!(w.configure(targets).is_err());
            assert_eq!(w.notices(), before);
        }
        w.stop();
        assert!(w.notices().is_empty());
        assert!(w.configure(vec![]).is_err());
        assert!(!w.acknowledge(&before[0]));
    }
    #[test]
    fn failed_retry_does_not_emit_repeated_notifications_and_recovery_is_dirty() {
        let mut w = manual();
        let epoch = w.configure(vec![target(1, "/a")]).unwrap();
        w.shared.attached(epoch, 1, false);
        let notice = w.notices().remove(0);
        assert_eq!(notice.status, Status::Failed);
        assert!(w.acknowledge(&notice));
        w.shared.attached(epoch, 1, false);
        assert!(w.notices().is_empty());
        w.shared.attached(epoch, 1, true);
        assert_eq!(w.notices()[0].status, Status::Ready);
    }
    #[test]
    fn retired_native_callback_cannot_dirty_a_rearmed_target_in_the_same_epoch() {
        let mut w = manual();
        let epoch = w.configure(vec![target(1, "/a")]).unwrap();
        w.shared.state.lock().unwrap().slots[0].arm = 1;
        assert!(w.shared.event_for(epoch, 1, Some(1), false));
        let current = w.notices().remove(0);
        assert!(w.acknowledge(&current));
        w.shared.state.lock().unwrap().slots[0].arm = 2;
        assert!(!w.shared.event_for(epoch, 1, Some(1), true));
        assert!(w.notices().is_empty());
        assert!(w.shared.event_for(epoch, 1, Some(2), false));
        assert!(!w.notices().is_empty());
    }
    #[cfg(target_os = "linux")]
    mod linux {
        use super::*;
        use std::{fs, sync::atomic::AtomicU64};
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let base = files_core::theme::home().join(".cache/files-test");
                fs::create_dir_all(&base).unwrap();
                let path = base.join(format!(
                    "listing-watch-{}-{}",
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
        fn wait(w: &ListingWatch, condition: impl Fn(&[Notice]) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(6);
            while !condition(&w.notices()) {
                assert!(
                    Instant::now() < deadline,
                    "watch notification absent: {:?}",
                    w.notices()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn acknowledge_all(w: &mut ListingWatch) {
            for notice in w.notices() {
                w.acknowledge(&notice);
            }
        }
        fn stop(w: &mut ListingWatch) {
            w.stop();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !w.stopped() {
                assert!(
                    Instant::now() < deadline,
                    "watch worker did not acknowledge stop"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        #[test]
        fn adding_and_removing_another_root_retains_the_native_registration() {
            let f = Fixture::new();
            let a = f.0.join("a");
            let b = f.0.join("b");
            fs::create_dir(&a).unwrap();
            fs::create_dir(&b).unwrap();
            let mut w = ListingWatch::new().unwrap();
            let ta = Target {
                key: 1,
                path: a.clone(),
            };
            let tb = Target { key: 2, path: b };
            w.configure(vec![ta.clone()]).unwrap();
            wait(&w, |n| {
                n.iter()
                    .any(|n| n.target.key == 1 && n.status == Status::Ready)
            });
            let first = w.notices().remove(0);
            acknowledge_all(&mut w);
            w.configure(vec![ta.clone(), tb]).unwrap();
            wait(&w, |n| {
                n.iter()
                    .any(|n| n.target.key == 2 && n.status == Status::Ready)
            });
            assert!(!w.notices().iter().any(|n| n.target.key == 1));
            {
                let s = w.shared.state.lock().unwrap();
                let retained = s.slots.iter().find(|s| s.target.key == 1).unwrap();
                assert_eq!(
                    (retained.epoch, retained.attachment),
                    (first.epoch, first.attachment)
                );
            }
            acknowledge_all(&mut w);
            fs::write(a.join("still-watched"), b"owned").unwrap();
            wait(&w, |n| n.iter().any(|n| n.target.key == 1));
            let current = w.notices().into_iter().find(|n| n.target.key == 1).unwrap();
            assert_eq!(
                (current.epoch, current.attachment),
                (first.epoch, first.attachment)
            );
            w.configure(vec![ta]).unwrap();
            // A later native event may legitimately supersede `current`.
            // The pure lease test checks ACK retention without OS timing.
            let s = w.shared.state.lock().unwrap();
            assert_eq!(s.slots[0].epoch, first.epoch);
            drop(s);
            stop(&mut w);
        }
        #[test]
        fn local_create_rename_remove_raw_names_and_two_roots_are_invalidation_hints() {
            use std::os::unix::ffi::OsStringExt;
            let f = Fixture::new();
            let a = f.0.join("a");
            let b = f.0.join("b");
            fs::create_dir(&a).unwrap();
            fs::create_dir(&b).unwrap();
            let mut w = ListingWatch::new().unwrap();
            w.configure(vec![
                Target {
                    key: 1,
                    path: a.clone(),
                },
                Target {
                    key: 2,
                    path: b.clone(),
                },
            ])
            .unwrap();
            wait(&w, |n| {
                n.len() == 2 && n.iter().all(|n| n.status == Status::Ready)
            });
            acknowledge_all(&mut w);
            let raw = a.join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
            fs::write(&raw, b"owned").unwrap();
            wait(&w, |n| n.iter().any(|n| n.target.key == 1));
            acknowledge_all(&mut w);
            fs::rename(&raw, a.join("renamed")).unwrap();
            wait(&w, |n| n.iter().any(|n| n.target.key == 1));
            acknowledge_all(&mut w);
            fs::remove_file(a.join("renamed")).unwrap();
            fs::write(b.join("other"), b"x").unwrap();
            wait(&w, |n| {
                n.iter().any(|n| n.target.key == 1) && n.iter().any(|n| n.target.key == 2)
            });
            stop(&mut w);
        }
        #[test]
        fn ancestor_alias_replacement_and_directory_to_file_recovery_follow_logical_root() {
            use std::os::unix::{ffi::OsStringExt, fs::symlink};
            let f = Fixture::new();
            let a = f.0.join("a");
            let b = f.0.join("b");
            fs::create_dir(&a).unwrap();
            fs::create_dir(&b).unwrap();
            fs::create_dir(a.join("root")).unwrap();
            fs::create_dir(b.join("root")).unwrap();
            let alias =
                f.0.join(std::ffi::OsString::from_vec(b"ancestor-\xff".to_vec()));
            symlink(&a, &alias).unwrap();
            let root = alias.join("root");
            let mut w = ListingWatch::new().unwrap();
            w.configure(vec![Target {
                key: 1,
                path: root.clone(),
            }])
            .unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Ready));
            let first = w.notices()[0].attachment;
            acknowledge_all(&mut w);
            let replacement = f.0.join("replacement");
            symlink(&b, &replacement).unwrap();
            fs::rename(replacement, &alias).unwrap();
            wait(&w, |n| {
                n.iter()
                    .any(|n| n.status == Status::Ready && n.attachment > first)
            });
            assert_eq!(w.notices()[0].target.path, root);
            acknowledge_all(&mut w);
            fs::write(b.join("root/new"), b"owned").unwrap();
            wait(&w, |n| !n.is_empty());
            acknowledge_all(&mut w);
            fs::rename(b.join("root"), b.join("saved-root")).unwrap();
            fs::write(b.join("root"), b"file").unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Failed));
            acknowledge_all(&mut w);
            fs::remove_file(b.join("root")).unwrap();
            fs::create_dir(b.join("root")).unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Ready));
            stop(&mut w);
        }
        #[test]
        fn root_identity_ignores_child_content_but_rejects_a_regular_file_root() {
            let f = Fixture::new();
            let root = f.0.join("root");
            fs::create_dir(&root).unwrap();
            let before = identity(&root).unwrap();
            fs::write(root.join("child"), b"owned").unwrap();
            assert_eq!(identity(&root).unwrap(), before);
            assert!(identity(&root.join("child")).is_err());
        }
        #[test]
        fn root_symlink_replacement_rearms_the_open_path_and_tracks_the_new_directory() {
            use std::os::unix::fs::symlink;
            let f = Fixture::new();
            let a = f.0.join("a");
            let b = f.0.join("b");
            fs::create_dir(&a).unwrap();
            fs::create_dir(&b).unwrap();
            let root = f.0.join("link");
            symlink(&a, &root).unwrap();
            let mut w = ListingWatch::new().unwrap();
            w.configure(vec![Target {
                key: 1,
                path: root.clone(),
            }])
            .unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Ready));
            let first = w.notices()[0].attachment;
            acknowledge_all(&mut w);
            let replacement = f.0.join("replacement");
            symlink(&b, &replacement).unwrap();
            fs::rename(replacement, &root).unwrap();
            wait(&w, |n| {
                n.iter()
                    .any(|n| n.status == Status::Ready && n.attachment > first)
            });
            acknowledge_all(&mut w);
            fs::write(b.join("new-target"), b"owned").unwrap();
            wait(&w, |n| !n.is_empty());
            stop(&mut w);
        }
        #[test]
        fn watched_root_replacement_and_missing_root_recovery_rearm_without_new_control_workers() {
            let f = Fixture::new();
            let root = f.0.join("root");
            fs::create_dir(&root).unwrap();
            let mut w = ListingWatch::new().unwrap();
            w.configure(vec![Target {
                key: 1,
                path: root.clone(),
            }])
            .unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Ready));
            let attached = w.notices()[0].attachment;
            acknowledge_all(&mut w);
            fs::rename(&root, f.0.join("old")).unwrap();
            fs::create_dir(&root).unwrap();
            wait(&w, |n| {
                n.iter()
                    .any(|n| n.status == Status::Ready && n.attachment > attached)
            });
            acknowledge_all(&mut w);
            fs::write(root.join("new"), b"new").unwrap();
            wait(&w, |n| !n.is_empty());
            let missing = f.0.join("missing");
            w.configure(vec![Target {
                key: 2,
                path: missing.clone(),
            }])
            .unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Failed));
            acknowledge_all(&mut w);
            fs::create_dir(&missing).unwrap();
            wait(&w, |n| n.iter().any(|n| n.status == Status::Ready));
            stop(&mut w);
        }
    }
}
