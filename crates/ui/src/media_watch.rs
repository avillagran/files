//! One reusable readonly probe worker, one pending target/result, directory watch.
//! No join on UI: a mount syscall may be uninterruptible. Shutdown requests stop;
//! root exit reclaims this thread, and is NOT reported as a completed probe drain.
use files_core::media_source::Revision;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
struct Target {
    directory: bool,
    token: u64,
    path: PathBuf,
    document: Option<Arc<crate::document_source::Plan>>,
}
impl PartialEq for Target {
    fn eq(&self, other: &Self) -> bool {
        self.directory == other.directory
            && self.token == other.token
            && self.path == other.path
            && match (&self.document, &other.document) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}
impl Eq for Target {}
struct ResultSlot {
    token: u64,
    epoch: u64,
    result: Result<Option<Revision>, String>,
    watch_failed: bool,
}
struct Shared {
    event_lock: Mutex<()>,
    token: AtomicU64,
    epoch: AtomicU64,
    stop: AtomicBool,
    stopped: AtomicBool,
    target: Mutex<Option<Target>>,
    result: Mutex<Option<ResultSlot>>,
}
impl Shared {
    fn event(&self, token: u64) -> bool {
        let _guard = self.event_lock.lock().unwrap();
        if self.token.load(Ordering::Acquire) == token && token != 0 {
            self.epoch.fetch_add(1, Ordering::AcqRel);
            return true;
        }
        false
    }
    #[cfg(test)]
    fn publish(&self, token: u64, epoch: u64, result: Result<Option<Revision>, String>) {
        self.publish_kind(token, epoch, result, false);
    }
    fn publish_kind(
        &self,
        token: u64,
        epoch: u64,
        result: Result<Option<Revision>, String>,
        watch_failed: bool,
    ) {
        if self.token.load(Ordering::Acquire) == token
            && self.epoch.load(Ordering::Acquire) == epoch
            && !self.stop.load(Ordering::Acquire)
        {
            *self.result.lock().unwrap() = Some(ResultSlot {
                token,
                epoch,
                result,
                watch_failed,
            });
        }
    }
}
pub enum Update {
    Dirty,
    Verified(Revision),
    DirectoryReady,
    Unavailable(String),
    WatchUnavailable(String),
}
pub struct Guard {
    shared: Arc<Shared>,
    wake: SyncSender<u64>,
    target: Option<Target>,
    seen: u64,
}
impl Guard {
    pub fn new() -> Self {
        let drop_events = std::env::var_os("FILES_TEST_PREVIEW").is_some()
            && std::env::var_os("FILES_TEST_DROP_MEDIA_EVENTS").is_some();
        Self::with_event_policy(drop_events)
    }
    fn with_event_policy(drop_events: bool) -> Self {
        let shared = Arc::new(Shared {
            event_lock: Mutex::new(()),
            token: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            target: Mutex::new(None),
            result: Mutex::new(None),
        });
        let (wake, rx) = mpsc::sync_channel(1);
        let worker = shared.clone();
        let notify = wake.clone();
        std::thread::Builder::new()
            .name("files-media-probe".into())
            .spawn(move || {
                let mut current: Option<Target> = None;
                let mut watcher: Option<RecommendedWatcher> = None;
                let mut due = Instant::now();
                let mut document_snapshot = None;
                loop {
                    if worker.stop.load(Ordering::Acquire) {
                        break;
                    }
                    let next = worker.target.lock().unwrap().clone();
                    if next != current {
                        watcher = None;
                        current = next;
                        document_snapshot = None;
                        due = Instant::now();
                    }
                    if let Some(target) = &current {
                        let now = Instant::now();
                        if now >= due {
                            // Reattach before each periodic probe: a watched parent may
                            // have moved or been replaced, and its old watch still lives.
                            let token = target.token;
                            let state = worker.clone();
                            let signal = notify.clone();
                            let document_root=target.document.as_ref().map(|p|p.root.clone());
                            let attached = notify::recommended_watcher(
                                move |event: notify::Result<notify::Event>| {
                                    if drop_events { return; }
                                    if event
                                        .as_ref()
                                        .is_ok_and(|e| matches!(e.kind, EventKind::Access(_)))
                                    {
                                        return;
                                    }
                                    if let (Some(root),Ok(event))=(&document_root,&event)
                                        && !event.paths.is_empty() && !event.paths.iter().any(|path|path.starts_with(root)||root.starts_with(path)) {return;}
                                    if state.event(token) {
                                        let _ = signal.try_send(token);
                                    }
                                },
                            )
                            .and_then(|mut w| {
                                if let Some(plan)=&target.document {
                                    for (index,directory) in plan.directories.iter().enumerate() {
                                        // Missing optional resource parents retain
                                        // placeholders, with periodic NOFOLLOW
                                        // metadata reconciliation for recreation.
                                        if let Err(error)=w.watch(directory,RecursiveMode::NonRecursive)
                                            && index<plan.required_directories {return Err(error);}
                                    }
                                } else {
                                    w.watch(if target.directory {target.path.as_path()}else{target.path.parent().unwrap_or(&target.path)},RecursiveMode::NonRecursive)?;
                                }
                                Ok(w)
                            });
                            let epoch = worker.epoch.load(Ordering::Acquire);
                            let watch_failed=attached.is_err();
                            if let Some(plan)=&target.document {
                                // Failure snapshots detect recreation even while a
                                // missing ancestor prevents watch attachment. They
                                // NEVER authorize Ready; verify below still requires
                                // successful watches and every expected proof.
                                let snapshot=plan.observe();
                                if document_snapshot.as_ref().is_some_and(|old|*old!=snapshot) {worker.event(token);}
                                document_snapshot=Some(snapshot);
                            }
                            let result = match attached {
                                Ok(new) => {
                                    watcher = Some(new);
                                    if let Some(plan)=&target.document {
                                        plan.verify().map(Some).map_err(|e|format!("Recurso del documento cambió: {e}"))
                                    } else if target.directory {
                                        std::fs::metadata(&target.path).and_then(|m|if m.is_dir(){Ok(None)}else{Err(std::io::Error::other("La carpeta de miniaturas ya no es un directorio"))}).map_err(|e|format!("Carpeta de miniaturas no disponible: {e}"))
                                    } else { files_core::media_source::open(&target.path)
                                        .map(|(_, revision)| Some(revision))
                                        .map_err(|e| format!("Original no disponible: {e}")) }
                                }
                                Err(e) => {
                                    watcher = None;
                                    Err(format!("No se puede vigilar el original: {e}"))
                                }
                            };
                            worker.publish_kind(token, epoch, result,watch_failed);
                            due = Instant::now() + Duration::from_secs(1);
                        }
                    }
                    let timeout = if current.is_some() {
                        due.saturating_duration_since(Instant::now())
                            .min(Duration::from_secs(1))
                    } else {
                        Duration::from_secs(60)
                    };
                    match rx.recv_timeout(timeout) {
                        Ok(token) => {
                            if current.is_some() && worker.token.load(Ordering::Acquire) == token {
                                due = Instant::now() + Duration::from_millis(100);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    // Keep the backend alive even though it is not otherwise read.
                    let _ = &watcher;
                }
                drop(watcher);
                worker.stopped.store(true, Ordering::Release);
            })
            .expect("media probe worker");
        Self {
            shared,
            wake,
            target: None,
            seen: 0,
        }
    }
    pub fn set_target(&mut self, target: Option<(u64, &std::path::Path)>) -> bool {
        if self
            .target
            .as_ref()
            .filter(|t| !t.directory && t.document.is_none())
            .map(|t| (t.token, t.path.as_path()))
            == target
            && (target.is_some() || self.target.is_none())
        {
            return false;
        }
        self.set(target.map(|(token, path)| Target {
            directory: false,
            token,
            path: path.to_path_buf(),
            document: None,
        }))
    }
    pub fn set_directory(&mut self, token: u64, path: &std::path::Path) -> bool {
        if self.target.as_ref().is_some_and(|t| {
            t.directory && t.document.is_none() && t.token == token && t.path == path
        }) {
            return false;
        }
        self.set(Some(Target {
            directory: true,
            token,
            path: path.to_path_buf(),
            document: None,
        }))
    }
    pub fn set_document(
        &mut self,
        token: u64,
        path: &std::path::Path,
        plan: Arc<crate::document_source::Plan>,
    ) -> bool {
        if self.target.as_ref().is_some_and(|t| {
            t.token == token
                && t.path == path
                && t.document.as_ref().is_some_and(|p| Arc::ptr_eq(p, &plan))
        }) {
            return false;
        }
        self.set(Some(Target {
            directory: false,
            token,
            path: path.into(),
            document: Some(plan),
        }))
    }
    fn set(&mut self, target: Option<Target>) -> bool {
        if self.target == target {
            return false;
        }
        self.target = target;
        let _event_guard = self.shared.event_lock.lock().unwrap();
        self.shared.token.store(
            self.target.as_ref().map_or(0, |t| t.token),
            Ordering::Release,
        );
        self.seen = self.shared.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        *self.shared.result.lock().unwrap() = None;
        *self.shared.target.lock().unwrap() = self.target.clone();
        let _ = self
            .wake
            .try_send(self.target.as_ref().map_or(0, |t| t.token));
        true
    }
    /// Atomic only. Also called immediately before painting, never FS I/O.
    pub fn poll(&mut self) -> Option<Update> {
        self.target.as_ref()?;
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        if self.seen != epoch {
            self.seen = epoch;
            return Some(Update::Dirty);
        }
        let result = self.shared.result.lock().unwrap().take()?;
        if Some(result.token) != self.target.as_ref().map(|t| t.token)
            || result.epoch != epoch
            || self.shared.epoch.load(Ordering::Acquire) != epoch
        {
            return None;
        }
        Some(match result.result {
            Ok(Some(revision)) => Update::Verified(revision),
            Ok(None) => Update::DirectoryReady,
            Err(error) if result.watch_failed => Update::WatchUnavailable(error),
            Err(error) => Update::Unavailable(error),
        })
    }
    /// Own-operation ACK/manual refresh invalidates before the listing reload,
    /// without waiting for a filesystem notification or doing I/O in UI.
    pub fn refresh(&self) {
        let token = self.shared.token.load(Ordering::Acquire);
        if self.shared.event(token) {
            let _ = self.wake.try_send(token);
        }
    }
    pub fn stop(&mut self) {
        self.set_target(None);
        self.shared.stop.store(true, Ordering::Release);
        let _ = self.wake.try_send(0);
    }
    pub fn stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Acquire)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn revision() -> Revision {
        Revision {
            device: 1,
            inode: 2,
            bytes: 3,
            modified_seconds: 4,
            modified_nanoseconds: 5,
            changed_seconds: 6,
            changed_nanoseconds: 7,
        }
    }
    fn shared() -> Shared {
        Shared {
            event_lock: Mutex::new(()),
            token: AtomicU64::new(1),
            epoch: AtomicU64::new(10),
            stop: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            target: Mutex::new(None),
            result: Mutex::new(None),
        }
    }
    #[test]
    fn event_during_probe_never_clears_newer_epoch() {
        let s = shared();
        let before = s.epoch.load(Ordering::Acquire);
        s.event(1);
        s.publish(1, before, Ok(Some(revision())));
        assert!(s.result.lock().unwrap().is_none());
        assert_eq!(s.epoch.load(Ordering::Acquire), before + 1);
    }
    #[test]
    fn callbacks_errors_and_results_from_old_destination_are_ignored() {
        let s = shared();
        s.token.store(2, Ordering::Release);
        assert!(!s.event(1));
        assert_eq!(s.epoch.load(Ordering::Acquire), 10);
        s.publish(1, 10, Err("old watcher".into()));
        assert!(s.result.lock().unwrap().is_none());
        s.publish(2, 10, Ok(Some(revision())));
        assert!(s.result.lock().unwrap().is_some());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn real_directory_watch_reconciles_replace_delete_and_parent_replacement() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "media-watch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let parent = base.join("current");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("image");
        std::fs::write(&path, b"red").unwrap();
        let mut guard = Guard::new();
        guard.set_target(Some((1, &path)));
        fn wait(guard: &mut Guard, matches: impl Fn(&Update) -> bool) -> Update {
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(update) = guard.poll()
                    && matches(&update)
                {
                    return update;
                }
                assert!(Instant::now() < end, "source update timeout");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let Update::Verified(a) = wait(&mut guard, |u| matches!(u, Update::Verified(_))) else {
            unreachable!()
        };
        let next = parent.join("next");
        std::fs::write(&next, b"blu").unwrap();
        std::fs::rename(&next, &path).unwrap();
        let Update::Verified(b) = wait(&mut guard, |u| matches!(u,Update::Verified(b)if *b!=a))
        else {
            unreachable!()
        };
        assert_ne!(a.inode, b.inode);
        std::fs::remove_file(&path).unwrap();
        wait(&mut guard, |u| matches!(u, Update::Unavailable(_)));
        std::fs::rename(&parent, base.join("old")).unwrap();
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(&path, b"new").unwrap();
        wait(&mut guard, |u| matches!(u,Update::Verified(c)if *c!=b));
        guard.stop();
        let end = Instant::now() + Duration::from_secs(3);
        while !guard.stopped() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod document_tests {
    use super::*;
    use files_core::relative_source::Opened;
    fn wait(guard: &mut Guard, check: impl Fn(&Update) -> bool) -> Update {
        let end = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(update) = guard.poll()
                && check(&update)
            {
                return update;
            }
            assert!(Instant::now() < end, "document watch timeout");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn lost_events_reconcile_nested_resource_deletion_replacement_and_root_recovery() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "document-watch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let root = base.join("root");
        std::fs::create_dir_all(root.join("sub/nested")).unwrap();
        std::fs::write(root.join("document.md"), b"doc").unwrap();
        std::fs::write(root.join("sub/nested/image"), b"red").unwrap();
        let proof = Opened::open(&root, std::path::Path::new("document.md"))
            .unwrap()
            .proof;
        let image = Opened::open(&root, std::path::Path::new("sub/nested/image"))
            .unwrap()
            .proof;
        let plan = Arc::new(
            crate::document_source::Plan::new(
                root.clone(),
                "document.md".into(),
                proof,
                Arc::new(vec![("sub/nested/image".into(), image)]),
                true,
            )
            .unwrap(),
        );
        let mut guard = Guard::with_event_policy(true);
        guard.set_document(101, &root.join("document.md"), plan);
        wait(&mut guard, |u| matches!(u, Update::Verified(_)));
        std::fs::remove_file(root.join("sub/nested/image")).unwrap();
        wait(&mut guard, |u| matches!(u, Update::Dirty));
        std::fs::write(root.join("sub/nested/image"), b"blu").unwrap();
        wait(&mut guard, |u| matches!(u, Update::Dirty));
        std::fs::rename(&root, base.join("old")).unwrap();
        wait(&mut guard, |u| matches!(u, Update::Dirty));
        // Missing ancestors prevent watch attachment, but readonly failure
        // snapshots must still notice root recreation without inotify callbacks.
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("document.md"), b"new").unwrap();
        wait(&mut guard, |u| matches!(u, Update::Dirty));
        guard.stop();
        let end = Instant::now() + Duration::from_secs(2);
        while !guard.stopped() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}
