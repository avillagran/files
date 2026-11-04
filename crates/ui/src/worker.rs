//! One reusable listing thread per Model; at most four filesystem operations
//! run concurrently process-wide. Idle threads hold no filesystem permit.
//! Restart only replaces the pending path: a stuck syscall is never bypassed by
//! spawning another thread. Jobs and responses carry an internal generation.
use files_core::listing::{Entry, LocalVfs, Metadata, Vfs};
use std::{
    collections::VecDeque,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::Duration,
};

pub const CAPACITY_MESSAGE: &str =
    "Cuatro consultas FS activas; esperando capacidad sin bloquear la interfaz";
const JOB_CAPACITY: usize = 128;
const RETRY_DELAY: Duration = Duration::from_millis(20);
static LIMITER: OnceLock<Arc<Limiter>> = OnceLock::new();
struct Limiter {
    active: AtomicUsize,
    limit: usize,
}
impl Limiter {
    fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            active: AtomicUsize::new(0),
            limit,
        })
    }
    fn acquire(self: &Arc<Self>) -> Option<Permit> {
        self.active
            .try_update(Ordering::AcqRel, Ordering::Relaxed, |n| {
                (n < self.limit).then_some(n + 1)
            })
            .ok()
            .map(|_| Permit(self.clone()))
    }
}
struct Permit(Arc<Limiter>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Release);
    }
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
pub enum Job {
    Stat(Vec<(usize, OsString)>),
    Summary(usize, OsString),
}
pub enum Update {
    Rounding(i32),
    FreeSpace(Option<u64>),
    Entries(Vec<Entry>),
    Done,
    Error(String),
    Waiting(bool),
    Stats(Vec<(usize, Option<Metadata>)>),
    Summary(usize, crate::folder_summary::Summary),
}
struct Requests {
    pending: Option<(u64, PathBuf)>,
    jobs: VecDeque<(u64, Job)>,
}
struct Shared {
    generation: AtomicU64,
    stopped: AtomicBool,
    requests: Mutex<Requests>,
    wake: Condvar,
}
impl Shared {
    fn cancelled(&self, generation: u64) -> bool {
        self.stopped.load(Ordering::Acquire)
            || self.generation.load(Ordering::Acquire) != generation
    }
    fn wait(&self, duration: Duration) {
        let requests = lock(&self.requests);
        // A timed wait also handles a permit released by a different Model.
        let _ = self.wake.wait_timeout(requests, duration);
    }
}
struct Envelope {
    generation: u64,
    update: Update,
}
pub struct Updates {
    receiver: Receiver<Envelope>,
    producer: Option<thread::Thread>,
    shared: Arc<Shared>,
}
impl Updates {
    pub fn try_recv(&self) -> Result<Update, mpsc::TryRecvError> {
        loop {
            let envelope = self.receiver.try_recv()?;
            // Releasing a bounded queue slot must wake the producer immediately.
            // unpark retains a token even if it precedes the producer's park.
            if let Some(producer) = &self.producer {
                producer.unpark();
            }
            if !self.shared.cancelled(envelope.generation) {
                return Ok(envelope.update);
            }
            // The channel holds at most eight responses, so discarding old
            // generations does not turn a UI poll into unbounded work.
        }
    }
}
pub struct Jobs {
    shared: Arc<Shared>,
}
impl Jobs {
    pub fn try_send(&self, job: Job) -> Result<(), mpsc::TrySendError<Job>> {
        let mut requests = lock(&self.shared.requests);
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(mpsc::TrySendError::Disconnected(job));
        }
        if requests.jobs.len() == JOB_CAPACITY {
            return Err(mpsc::TrySendError::Full(job));
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        requests.jobs.push_back((generation, job));
        self.shared.wake.notify_one();
        Ok(())
    }
}
pub struct Worker {
    pub rx: Updates,
    pub tx: Jobs,
    shared: Arc<Shared>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        let mut requests = lock(&self.shared.requests);
        requests.pending = None;
        requests.jobs.clear();
        self.shared.wake.notify_one();
        // Do not join a thread that may be in a mount syscall on the UI thread.
    }
}
type Entries = Box<dyn Iterator<Item = io::Result<Entry>> + Send>;
type Children = Box<dyn Iterator<Item = io::Result<()>> + Send>;
trait Backend: Send + Sync {
    fn list(&self, path: &Path) -> io::Result<Entries>;
    fn children(&self, path: &Path) -> io::Result<Children>;
    fn stat(&self, path: &Path) -> io::Result<Metadata>;
    fn free_space(&self, path: &Path) -> Option<u64>;
    fn rounding(&self) -> i32;
}
struct Local;
impl Backend for Local {
    fn children(&self, path: &Path) -> io::Result<Children> {
        Ok(Box::new(
            std::fs::read_dir(path)?.map(|row| row.map(|_| ())),
        ))
    }
    fn list(&self, path: &Path) -> io::Result<Entries> {
        LocalVfs.list(path)
    }
    fn stat(&self, path: &Path) -> io::Result<Metadata> {
        LocalVfs.stat(path)
    }
    fn free_space(&self, path: &Path) -> Option<u64> {
        files_core::system::free_space(path)
    }
    fn rounding(&self) -> i32 {
        files_core::theme::rounding(files_core::theme::Corners::Omarchy)
    }
}
impl Worker {
    pub fn start(path: PathBuf) -> Self {
        Self::with_backend(
            path,
            Arc::new(Local),
            LIMITER.get_or_init(|| Limiter::new(4)).clone(),
        )
    }
    fn with_backend(path: PathBuf, backend: Arc<dyn Backend>, limiter: Arc<Limiter>) -> Self {
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
            requests: Mutex::new(Requests {
                pending: Some((1, path)),
                jobs: VecDeque::new(),
            }),
            wake: Condvar::new(),
        });
        let (updates, receiver) = mpsc::sync_channel(8);
        let thread_shared = shared.clone();
        let error_updates = updates.clone();
        let producer = match thread::Builder::new()
            .name("files-listing".into())
            .spawn(move || run(thread_shared, updates, backend, limiter))
        {
            Ok(handle) => Some(handle.thread().clone()),
            Err(error) => {
                let _ = error_updates.try_send(Envelope {
                    generation: 1,
                    update: Update::Error(format!("No se pudo iniciar el listado: {error}")),
                });
                None
            }
        };
        Self {
            rx: Updates {
                receiver,
                producer,
                shared: shared.clone(),
            },
            tx: Jobs {
                shared: shared.clone(),
            },
            shared,
        }
    }
    /// Cancel pending work without starting another thread or joining a syscall.
    pub fn suspend(&mut self) {
        let mut requests = lock(&self.shared.requests);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        requests.pending = None;
        requests.jobs.clear();
        self.shared.wake.notify_one();
    }
    pub fn restart(&mut self, path: PathBuf) {
        let mut requests = lock(&self.shared.requests);
        let generation = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        requests.pending = Some((generation, path));
        requests.jobs.clear();
        self.shared.wake.notify_one();
    }
}
struct Sender<'a> {
    shared: &'a Shared,
    updates: &'a SyncSender<Envelope>,
    generation: u64,
}
impl Sender<'_> {
    fn cancelled(&self) -> bool {
        self.shared.cancelled(self.generation)
    }
    fn send(&self, update: Update) -> bool {
        let mut envelope = Envelope {
            generation: self.generation,
            update,
        };
        loop {
            if self.cancelled() {
                return false;
            }
            match self.updates.try_send(envelope) {
                Ok(()) => {
                    // Wake the event loop for new listing work instead of waiting
                    // for its periodic fallback poll. No callback or extra queue.
                    fltk::app::awake();
                    return true;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => return false,
                Err(mpsc::TrySendError::Full(value)) => {
                    envelope = value;
                    thread::park_timeout(Duration::from_millis(2));
                }
            }
        }
    }
    fn permit(&self, limiter: &Arc<Limiter>) -> Option<Permit> {
        let mut waiting = false;
        loop {
            if self.cancelled() {
                return None;
            }
            if let Some(permit) = limiter.acquire() {
                if waiting && !self.send(Update::Waiting(false)) {
                    return None;
                }
                return Some(permit);
            }
            if !waiting {
                if !self.send(Update::Waiting(true)) {
                    return None;
                }
                waiting = true;
            }
            self.shared.wait(RETRY_DELAY);
        }
    }
}
fn run(
    shared: Arc<Shared>,
    updates: SyncSender<Envelope>,
    backend: Arc<dyn Backend>,
    limiter: Arc<Limiter>,
) {
    let mut current = None;
    let mut rounding = None;
    loop {
        let (generation, path, job) = {
            let mut requests = lock(&shared.requests);
            loop {
                if shared.stopped.load(Ordering::Acquire) {
                    return;
                }
                if let Some((generation, path)) = requests.pending.take() {
                    current = Some((generation, path.clone()));
                    break (generation, path, None);
                }
                if let Some((generation, job)) = requests.jobs.pop_front() {
                    if let Some((current_generation, path)) = &current
                        && *current_generation == generation
                    {
                        break (generation, path.clone(), Some(job));
                    }
                    continue;
                }
                requests = shared
                    .wake
                    .wait(requests)
                    .unwrap_or_else(|error| error.into_inner());
            }
        };
        let sender = Sender {
            shared: &shared,
            updates: &updates,
            generation,
        };
        let Some(_permit) = sender.permit(&limiter) else {
            continue;
        };
        if sender.cancelled() {
            continue;
        }
        match job {
            None => {
                let value = *rounding.get_or_insert_with(|| backend.rounding());
                // Cache the process-level probe, but deliver it in each current
                // generation even when an earlier response was discarded.
                if sender.send(Update::Rounding(value)) {
                    list(&sender, backend.as_ref(), &path);
                }
            }
            Some(Job::Stat(names)) => stats(&sender, backend.as_ref(), &path, names),
            Some(Job::Summary(index, name)) => {
                summary(&sender, backend.as_ref(), &path, index, name)
            }
        }
        // The FS permit drops here, before waiting for another UI request.
    }
}
fn list(sender: &Sender<'_>, backend: &dyn Backend, path: &Path) {
    // The first listing batch and Done precede free-space probes.
    match backend.list(path) {
        Ok(entries) => {
            let mut batch = Vec::with_capacity(256);
            for entry in entries {
                if sender.cancelled() {
                    return;
                }
                match entry {
                    Ok(entry) => batch.push(entry),
                    Err(error) => {
                        if !sender.send(Update::Error(error.to_string())) {
                            return;
                        }
                    }
                }
                if batch.len() == 256
                    && !sender.send(Update::Entries(std::mem::replace(
                        &mut batch,
                        Vec::with_capacity(256),
                    )))
                {
                    return;
                }
            }
            if !batch.is_empty() && !sender.send(Update::Entries(batch)) {
                return;
            }
        }
        Err(error) => {
            if !sender.send(Update::Error(error.to_string())) {
                return;
            }
        }
    }
    if sender.send(Update::Done) && !sender.cancelled() {
        sender.send(Update::FreeSpace(backend.free_space(path)));
    }
}
fn stats(sender: &Sender<'_>, backend: &dyn Backend, path: &Path, names: Vec<(usize, OsString)>) {
    let mut out = Vec::with_capacity(128);
    for (index, name) in names {
        if sender.cancelled() {
            return;
        }
        out.push((index, backend.stat(&path.join(name)).ok()));
        if out.len() == 128
            && !sender.send(Update::Stats(std::mem::replace(
                &mut out,
                Vec::with_capacity(128),
            )))
        {
            return;
        }
    }
    if !out.is_empty() {
        sender.send(Update::Stats(out));
    }
}
fn summary(sender: &Sender<'_>, backend: &dyn Backend, path: &Path, index: usize, name: OsString) {
    if sender.cancelled() {
        return;
    }
    let path = path.join(name);
    if let Some(value) = crate::folder_summary::scan(backend.children(&path), || sender.cancelled())
    {
        sender.send(Update::Summary(index, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::listing::Kind;
    use std::{
        collections::HashMap,
        time::{Instant, SystemTime},
    };

    #[derive(Default)]
    struct Gate {
        entered: AtomicBool,
        released: Mutex<bool>,
        wake: Condvar,
    }
    impl Gate {
        fn enter(&self) {
            self.entered.store(true, Ordering::Release);
            let mut released = lock(&self.released);
            while !*released {
                released = self.wake.wait(released).unwrap();
            }
        }
        fn release(&self) {
            *lock(&self.released) = true;
            self.wake.notify_all();
        }
    }
    type Call = (&'static str, PathBuf);
    #[derive(Default)]
    struct Fake {
        gates: Mutex<HashMap<Call, Arc<Gate>>>,
        calls: Mutex<Vec<(Call, thread::ThreadId)>>,
    }
    impl Fake {
        fn block(&self, kind: &'static str, path: &str) -> Arc<Gate> {
            let gate = Arc::new(Gate::default());
            lock(&self.gates).insert((kind, path.into()), gate.clone());
            gate
        }
        fn visit(&self, kind: &'static str, path: &Path) {
            let call = (kind, path.to_owned());
            lock(&self.calls).push((call.clone(), thread::current().id()));
            let gate = lock(&self.gates).get(&call).cloned();
            if let Some(gate) = gate {
                gate.enter();
            }
        }
        fn saw(&self, kind: &'static str, path: &str) -> bool {
            lock(&self.calls)
                .iter()
                .any(|(call, _)| call == &(kind, path.into()))
        }
    }
    impl Backend for Fake {
        fn children(&self, path: &Path) -> io::Result<Children> {
            self.visit("children", path);
            Ok(Box::new(std::iter::once(Ok(()))))
        }
        fn list(&self, path: &Path) -> io::Result<Entries> {
            self.visit("list", path);
            Ok(Box::new(std::iter::once(Ok(Entry {
                name: path.file_name().unwrap().into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            }))))
        }
        fn stat(&self, path: &Path) -> io::Result<Metadata> {
            self.visit("stat", path);
            Ok(Metadata {
                size: 17,
                modified: SystemTime::UNIX_EPOCH,
            })
        }
        fn free_space(&self, path: &Path) -> Option<u64> {
            self.visit("free", path);
            Some(42)
        }
        fn rounding(&self) -> i32 {
            0
        }
    }
    fn wait(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for worker");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn listing(worker: &Worker) -> Vec<OsString> {
        let mut names = Vec::new();
        wait(|| {
            while let Ok(update) = worker.rx.try_recv() {
                match update {
                    Update::Entries(entries) => {
                        names.extend(entries.into_iter().map(|entry| entry.name));
                    }
                    Update::Done => return true,
                    Update::Stats(_) => panic!("stale stat reached new directory"),
                    Update::Error(error) => panic!("unexpected error: {error}"),
                    _ => {}
                }
            }
            false
        });
        names
    }

    #[test]
    fn direct_summary_does_not_load_child_metadata_or_descend() {
        let backend = Arc::new(Fake::default());
        let worker = Worker::with_backend("/root".into(), backend.clone(), Limiter::new(1));
        assert_eq!(listing(&worker), [OsString::from("root")]);
        worker
            .tx
            .try_send(Job::Summary(77, "folder".into()))
            .unwrap();
        let mut observed = None;
        wait(|| {
            while let Ok(update) = worker.rx.try_recv() {
                if let Update::Summary(77, summary) = update {
                    observed = Some(summary);
                }
            }
            observed.is_some()
        });
        let summary = observed.unwrap();
        assert_eq!(summary.entries, 1);
        assert_eq!(summary.status, crate::folder_summary::Status::Complete);
        assert!(backend.saw("children", "/root/folder"));
        assert!(!backend.saw("list", "/root/folder"));
        assert!(
            lock(&backend.calls)
                .iter()
                .all(|((kind, _), _)| *kind != "stat")
        );
    }

    #[test]
    fn full_listing_queue_preserves_every_entry_and_rejects_old_generation() {
        struct Many(Arc<AtomicUsize>);
        impl Backend for Many {
            fn list(&self, path: &Path) -> io::Result<Entries> {
                let prefix = path.file_name().unwrap().to_os_string();
                let produced = self.0.clone();
                Ok(Box::new((0..4097).map(move |i| {
                    produced.fetch_add(1, Ordering::Release);
                    let mut name = prefix.clone();
                    name.push(format!("-{i}"));
                    Ok(Entry {
                        name,
                        kind: Kind::File,
                        metadata: None,
                        stat_attempted: false,
                    })
                })))
            }
            fn children(&self, _path: &Path) -> io::Result<Children> {
                unreachable!()
            }
            fn stat(&self, _path: &Path) -> io::Result<Metadata> {
                unreachable!()
            }
            fn free_space(&self, _path: &Path) -> Option<u64> {
                None
            }
            fn rounding(&self) -> i32 {
                0
            }
        }
        for restart in [false, true] {
            let produced = Arc::new(AtomicUsize::new(0));
            let mut worker = Worker::with_backend(
                "/old".into(),
                Arc::new(Many(produced.clone())),
                Limiter::new(1),
            );
            // Rounding plus seven entry batches occupy all eight response slots.
            // The next batch is produced but cannot pass the bounded queue.
            wait(|| produced.load(Ordering::Acquire) >= 8 * 256);
            assert!(produced.load(Ordering::Acquire) < 4097);
            let prefix = if restart {
                worker.restart("/new".into());
                "new"
            } else {
                "old"
            };
            let names = listing(&worker);
            assert_eq!(names.len(), 4097);
            for (i, name) in names.iter().enumerate() {
                assert_eq!(name, &OsString::from(format!("{prefix}-{i}")));
            }
        }
    }
    #[test]
    fn restarts_replace_pending_path_without_replacing_blocked_thread() {
        let backend = Arc::new(Fake::default());
        let gate = backend.block("list", "/old");
        let limiter = Limiter::new(1);
        let mut worker = Worker::with_backend("/old".into(), backend.clone(), limiter.clone());
        wait(|| gate.entered.load(Ordering::Acquire));
        for i in 0..50 {
            worker.restart(format!("/ignored-{i}").into());
        }
        worker.restart("/latest".into());
        assert_eq!(
            lock(&worker.shared.requests).pending.as_ref().unwrap().1,
            Path::new("/latest")
        );
        assert!(matches!(
            worker.rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(!backend.saw("list", "/latest"));
        assert_eq!(limiter.active.load(Ordering::Acquire), 1);
        gate.release();
        assert_eq!(listing(&worker), [OsString::from("latest")]);
        assert_eq!(
            lock(&backend.calls)
                .iter()
                .filter(|((kind, _), _)| *kind == "list")
                .count(),
            2
        );
        let calls = lock(&backend.calls);
        assert!(calls.iter().all(|(_, thread)| *thread == calls[0].1));
    }

    #[test]
    fn stale_stat_indices_and_queued_jobs_are_rejected_after_restart() {
        let backend = Arc::new(Fake::default());
        let limiter = Limiter::new(1);
        let gate = backend.block("stat", "/old/item");
        let mut worker = Worker::with_backend("/old".into(), backend.clone(), limiter);
        assert_eq!(listing(&worker), [OsString::from("old")]);
        assert!(
            worker
                .tx
                .try_send(Job::Stat(vec![(999, "item".into())]))
                .is_ok()
        );
        wait(|| gate.entered.load(Ordering::Acquire));
        assert!(
            worker
                .tx
                .try_send(Job::Summary(888, "queued".into()))
                .is_ok()
        );
        worker.restart("/new".into());
        assert!(lock(&worker.shared.requests).jobs.is_empty());
        gate.release();
        assert_eq!(listing(&worker), [OsString::from("new")]);
        assert!(!backend.saw("stat", "/old/queued"));
    }

    #[test]
    fn more_than_four_models_retry_capacity_without_false_done() {
        let backend = Arc::new(Fake::default());
        let limiter = Limiter::new(4);
        let gates: Vec<_> = (0..4)
            .map(|i| backend.block("list", &format!("/blocked-{i}")))
            .collect();
        let mut workers: Vec<_> = (0..4)
            .map(|i| {
                Worker::with_backend(
                    format!("/blocked-{i}").into(),
                    backend.clone(),
                    limiter.clone(),
                )
            })
            .collect();
        wait(|| {
            gates
                .iter()
                .all(|gate| gate.entered.load(Ordering::Acquire))
        });
        let mut waiting = Worker::with_backend("/waiting".into(), backend.clone(), limiter.clone());
        wait(|| {
            while let Ok(update) = waiting.rx.try_recv() {
                match update {
                    Update::Waiting(true) => return true,
                    Update::Done => panic!("capacity must not report a completed listing"),
                    _ => {}
                }
            }
            false
        });
        waiting.restart("/replacement".into());
        let sixth = Worker::with_backend("/sixth".into(), backend.clone(), limiter.clone());
        assert_eq!(limiter.active.load(Ordering::Acquire), 4);
        assert!(!backend.saw("list", "/replacement"));
        for (i, worker) in workers.iter_mut().enumerate() {
            worker.restart(format!("/restarted-{i}").into());
        }
        for gate in gates {
            gate.release();
        }
        for (i, worker) in workers.iter().enumerate() {
            assert_eq!(listing(worker), [OsString::from(format!("restarted-{i}"))]);
        }
        assert_eq!(listing(&waiting), [OsString::from("replacement")]);
        assert_eq!(listing(&sixth), [OsString::from("sixth")]);
        assert!(!backend.saw("list", "/waiting"));
        wait(|| limiter.active.load(Ordering::Acquire) == 0);
        // All six Model lifetimes remain alive, but idle threads hold no permit.
        assert!(
            workers[0]
                .tx
                .try_send(Job::Stat(vec![(0, "item".into())]))
                .is_ok()
        );
        wait(|| matches!(workers[0].rx.try_recv(), Ok(Update::Stats(_))));
    }

    #[test]
    fn offline_probe_retains_one_thread_and_resumes_latest_path_when_released() {
        let backend = Arc::new(Fake::default());
        let gate = backend.block("free", "/offline");
        let limiter = Limiter::new(1);
        let mut worker = Worker::with_backend("/offline".into(), backend.clone(), limiter.clone());
        // Done and first entries are observable even with a blocked space probe.
        assert_eq!(listing(&worker), [OsString::from("offline")]);
        wait(|| gate.entered.load(Ordering::Acquire));
        for i in 0..100 {
            worker.restart(format!("/pending-{i}").into());
        }
        worker.restart("/recovered".into());
        assert_eq!(limiter.active.load(Ordering::Acquire), 1);
        assert!(!backend.saw("list", "/recovered"));
        gate.release();
        assert_eq!(listing(&worker), [OsString::from("recovered")]);
        let calls = lock(&backend.calls);
        assert!(calls.iter().all(|(_, thread)| *thread == calls[0].1));
    }
    #[test]
    fn dual_and_columns_transfer_one_spare_while_its_backend_is_blocked() {
        use crate::model::{Action, Model, View};
        let backend = Arc::new(Fake::default());
        let gate = backend.block("list", "/fixture-dual-owner/child");
        let limiter = Limiter::new(1);
        let spare = Worker::with_backend("/initial".into(), backend.clone(), limiter.clone());
        let identity = Arc::as_ptr(&spare.shared) as usize;
        let mut m = Model::new("/fixture-dual-owner".into());
        m.worker.suspend();
        m.view = View::Columns;
        m.entries = vec![Entry {
            name: "child".into(),
            kind: Kind::Directory,
            metadata: None,
            stat_attempted: true,
        }];
        m.order = vec![0];
        m.selected = Some(0);
        m.selection.insert(0);
        m.loaded = true;
        m.columns.worker = Some(spare);
        m.pump_columns();
        wait(|| gate.entered.load(Ordering::Acquire));
        for _ in 0..12 {
            m.action(Action::DualPane);
            assert!(m.dual.enabled);
            assert!(m.columns.worker.is_none());
            assert_eq!(
                Arc::as_ptr(
                    &m.dual
                        .other
                        .as_ref()
                        .unwrap()
                        .worker
                        .as_ref()
                        .unwrap()
                        .shared
                ) as usize,
                identity
            );
            m.action(Action::View(View::Columns));
            assert!(!m.dual.enabled);
            m.pump_columns();
            assert!(m.dual.other.as_ref().unwrap().worker.is_none());
            assert_eq!(
                Arc::as_ptr(&m.columns.worker.as_ref().unwrap().shared) as usize,
                identity
            );
            assert_eq!(limiter.active.load(Ordering::Acquire), 1);
        }
        m.action(Action::View(View::List));
        m.pump_columns();
        gate.release();
        wait(|| limiter.active.load(Ordering::Acquire) == 0);
        let calls = lock(&backend.calls);
        assert!(calls.iter().all(|(_, id)| *id == calls[0].1));
    }
}
