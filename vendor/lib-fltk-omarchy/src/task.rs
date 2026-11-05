//! Background work with cooperative cancellation and UI-thread callbacks.
//!
//! ```no_run
//! use fltk_omarchy::task::{Task, Outcome};
//! let task = Task::new(|ctx| {
//!     for i in 0..100 {
//!         ctx.sleep(std::time::Duration::from_millis(20))?; // returns Err if cancelled
//!         ctx.progress((i + 1) as f64 / 100.0);
//!     }
//!     Ok("done".to_string())
//! })
//! .on_progress(|p| println!("{p}"))
//! .on_done(|outcome| match outcome {
//!     Outcome::Done(v) => println!("{v}"),
//!     Outcome::Failed(e) => eprintln!("{e}"),
//!     Outcome::Cancelled => println!("cancelled"),
//! })
//! .start();
//! task.cancel();
//! ```
//! Work closures run on a bounded worker pool (see [`set_max_workers`]);
//! `on_progress`/`on_done` run on the UI thread from the app loop.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- UI queue

type Job = Box<dyn FnOnce() + Send>;
static POSTED: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());

/// Thread-safe: runs `f` on the UI thread on the next loop iteration.
pub fn post(f: impl FnOnce() + Send + 'static) {
    POSTED.lock().unwrap().push_back(Box::new(f));
    fltk::app::awake();
}

/// Runs everything posted so far. Called by the app loop (and tests).
///
/// The jobs run inside one [`crate::state::batch`]: a burst of posted
/// updates (a background task streaming events) notifies each State and
/// model once, so the UI redraws once per loop iteration.
pub fn drain() -> usize {
    crate::state::batch(|| {
        let mut n = 0;
        loop {
            let job = POSTED.lock().unwrap().pop_front();
            match job {
                Some(j) => {
                    j();
                    n += 1;
                }
                None => return n,
            }
        }
    })
}

// ------------------------------------------------------------ UI sinks
//
// A sink is a UI-thread closure that a background thread can wake with a
// plain id (`Send`): the feeds (`state::StateFeed`, `state::TextFeed`,
// `model::Feed`) queue data behind a mutex and wake their sink once until
// it drains. Optionally rate-limited: a wake sooner than `min_interval`
// after the last run arms one fltk timeout instead (no timer while idle).

struct Sink {
    run: Rc<RefCell<dyn FnMut() -> bool>>,
    min_interval: Arc<AtomicU64>, // nanoseconds, 0 = every loop iteration
    last: Option<Instant>,
    armed: bool,
}

thread_local! {
    static SINKS: RefCell<HashMap<u64, Sink>> = RefCell::new(HashMap::new());
}
static NEXT_SINK: AtomicU64 = AtomicU64::new(1);

/// Id of a UI-thread sink; `wake` is thread-safe.
#[derive(Debug, Clone)]
pub(crate) struct SinkId {
    id: u64,
    pub(crate) min_interval: Arc<AtomicU64>,
}

impl SinkId {
    /// Thread-safe: runs the sink on the UI thread on the next loop iteration.
    pub(crate) fn wake(&self) {
        let id = self.id;
        post(move || run_sink(id, false));
    }
}

/// Registers `f` on this (the UI) thread. `f` returns false to unregister.
pub(crate) fn register_sink(f: impl FnMut() -> bool + 'static) -> SinkId {
    let id = NEXT_SINK.fetch_add(1, Ordering::Relaxed);
    let min_interval = Arc::new(AtomicU64::new(0));
    SINKS.with(|s| s.borrow_mut().insert(id, Sink { run: Rc::new(RefCell::new(f)), min_interval: min_interval.clone(), last: None, armed: false }));
    SinkId { id, min_interval }
}

fn run_sink(id: u64, from_timer: bool) {
    enum Next {
        Run(Rc<RefCell<dyn FnMut() -> bool>>),
        Wait(Duration),
        Skip,
    }
    let next = SINKS.with(|s| {
        let mut s = s.borrow_mut();
        let Some(sink) = s.get_mut(&id) else { return Next::Skip };
        if from_timer {
            sink.armed = false;
        } else if sink.armed {
            return Next::Skip; // the timer will run it
        }
        let min = Duration::from_nanos(sink.min_interval.load(Ordering::Relaxed));
        if let Some(last) = sink.last {
            let since = last.elapsed();
            if since < min {
                sink.armed = true;
                return Next::Wait(min - since);
            }
        }
        sink.last = Some(Instant::now());
        Next::Run(sink.run.clone())
    });
    match next {
        Next::Skip => {}
        Next::Wait(d) => {
            fltk::app::add_timeout3(d.as_secs_f64(), move |_| crate::state::batch(|| run_sink(id, true)));
        }
        Next::Run(f) => {
            let keep = f.try_borrow_mut().map(|mut f| f()).unwrap_or(true);
            if !keep {
                SINKS.with(|s| s.borrow_mut().remove(&id));
            }
        }
    }
}

/// Number of registered sinks (tests).
pub fn sink_count() -> usize {
    SINKS.with(|s| s.borrow().len())
}

// ---------------------------------------------------------------- outcome

/// Final result delivered to `on_done`.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome<R> {
    Done(R),
    Failed(String),
    Cancelled,
}

/// Returned by [`Ctx::check`] / [`Ctx::sleep`] after cancellation; use `?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl From<Cancelled> for String {
    fn from(_: Cancelled) -> String {
        "cancelled".into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

struct Shared {
    cancel: AtomicBool,
    status: Mutex<Status>,
    progress: Mutex<Option<f64>>,
    progress_posted: AtomicBool,
    finished: Condvar,
}

/// Passed to the work closure. `Send + Sync`.
pub struct Ctx {
    id: u64,
    shared: Arc<Shared>,
}

impl Ctx {
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() { Err(Cancelled) } else { Ok(()) }
    }

    /// Sleeps in short steps; returns `Err(Cancelled)` early on cancel.
    pub fn sleep(&self, d: Duration) -> Result<(), Cancelled> {
        let end = Instant::now() + d;
        loop {
            self.check()?;
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            std::thread::sleep(left.min(Duration::from_millis(20)));
        }
    }

    /// Reports progress (0..1 by convention). Coalesced: the UI sees the latest value.
    pub fn progress(&self, value: f64) {
        *self.shared.progress.lock().unwrap() = Some(value);
        if !self.shared.progress_posted.swap(true, Ordering::AcqRel) {
            let (id, shared) = (self.id, self.shared.clone());
            post(move || {
                shared.progress_posted.store(false, Ordering::Release);
                let v = shared.progress.lock().unwrap().take();
                if let (Some(v), false) = (v, shared.cancel.load(Ordering::Relaxed)) {
                    deliver(id, Event::Progress(v));
                }
            });
        }
    }
}

// ------------------------------------------------- UI-side callback registry

enum Event {
    Progress(f64),
    Finished(Box<dyn Any + Send>),
}

struct Callbacks {
    progress: Option<Box<dyn FnMut(f64)>>,
    done: Option<Box<dyn FnOnce(Box<dyn Any + Send>)>>,
}

thread_local! {
    static CALLBACKS: RefCell<HashMap<u64, Callbacks>> = RefCell::new(HashMap::new());
}

fn deliver(id: u64, ev: Event) {
    match ev {
        Event::Progress(v) => {
            let cb = CALLBACKS.with(|c| c.borrow_mut().get_mut(&id).and_then(|cb| cb.progress.take()));
            if let Some(mut f) = cb {
                f(v);
                CALLBACKS.with(|c| {
                    if let Some(cb) = c.borrow_mut().get_mut(&id) {
                        cb.progress = Some(f);
                    }
                });
            }
        }
        Event::Finished(any) => {
            let cb = CALLBACKS.with(|c| c.borrow_mut().remove(&id));
            if let Some(Callbacks { done: Some(f), .. }) = cb {
                f(any);
            }
        }
    }
}

// ------------------------------------------------------------------ pool

static MAX_WORKERS: AtomicUsize = AtomicUsize::new(2);
static MAX_QUEUED: AtomicUsize = AtomicUsize::new(64);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct Pool {
    queue: VecDeque<Job>,
    workers: Vec<JoinHandle<()>>,
    idle: usize,
    closed: bool,
    live: Vec<Arc<Shared>>,
}

static POOL: Mutex<Pool> = Mutex::new(Pool { queue: VecDeque::new(), workers: Vec::new(), idle: 0, closed: false, live: Vec::new() });
static WAKE: Condvar = Condvar::new();

/// Maximum concurrent worker threads (default 2). Threads start lazily.
pub fn set_max_workers(n: usize) {
    MAX_WORKERS.store(n.max(1), Ordering::Relaxed);
}

/// Maximum tasks waiting for a worker (default 64); more makes `start` fail.
pub fn set_max_queued(n: usize) {
    MAX_QUEUED.store(n.max(1), Ordering::Relaxed);
}

pub fn worker_count() -> usize {
    POOL.lock().unwrap().workers.len()
}

fn worker_loop() {
    loop {
        let job = {
            let mut pool = POOL.lock().unwrap();
            loop {
                if let Some(j) = pool.queue.pop_front() {
                    break Some(j);
                }
                if pool.closed {
                    break None;
                }
                pool.idle += 1;
                pool = WAKE.wait(pool).unwrap();
                pool.idle -= 1;
            }
        };
        match job {
            Some(j) => j(),
            None => return,
        }
    }
}

/// Cancels every task and joins workers for up to `timeout`.
/// Returns how many workers did not stop in time (they are detached).
pub fn shutdown(timeout: Duration) -> usize {
    let workers = {
        let mut pool = POOL.lock().unwrap();
        pool.closed = true;
        for s in &pool.live {
            s.cancel.store(true, Ordering::Relaxed);
        }
        pool.queue.clear();
        std::mem::take(&mut pool.workers)
    };
    WAKE.notify_all();
    let deadline = Instant::now() + timeout;
    let mut stuck = 0;
    for w in workers {
        while !w.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if w.is_finished() {
            let _ = w.join();
        } else {
            stuck += 1;
        }
    }
    let mut pool = POOL.lock().unwrap();
    pool.closed = false; // allow reuse (tests, new app)
    pool.live.clear();
    stuck
}

// ------------------------------------------------------------------ task

type Work<R> = Box<dyn FnOnce(&Ctx) -> Result<R, String> + Send>;

/// Builder for a background task. Call [`Task::start`] to run it.
pub struct Task<R: Send + 'static> {
    work: Work<R>,
    progress: Option<Box<dyn FnMut(f64)>>,
    done: Option<Box<dyn FnOnce(Outcome<R>)>>,
}

/// Handle to a started task. Cheap to clone; `Send + Sync`.
#[derive(Clone)]
pub struct TaskHandle {
    shared: Arc<Shared>,
}

impl TaskHandle {
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }

    pub fn status(&self) -> Status {
        *self.shared.status.lock().unwrap()
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.status(), Status::Done | Status::Failed | Status::Cancelled)
    }

    /// Blocks the calling thread until finished or timeout. For tests/non-UI threads.
    pub fn wait(&self, timeout: Duration) -> Status {
        let guard = self.shared.status.lock().unwrap();
        let (guard, _) = self
            .shared
            .finished
            .wait_timeout_while(guard, timeout, |s| matches!(s, Status::Pending | Status::Running))
            .unwrap();
        *guard
    }
}

impl<R: Send + 'static> Task<R> {
    pub fn new(work: impl FnOnce(&Ctx) -> Result<R, String> + Send + 'static) -> Self {
        Self { work: Box::new(work), progress: None, done: None }
    }

    pub fn on_progress(mut self, f: impl FnMut(f64) + 'static) -> Self {
        self.progress = Some(Box::new(f));
        self
    }

    pub fn on_done(mut self, f: impl FnOnce(Outcome<R>) + 'static) -> Self {
        self.done = Some(Box::new(f));
        self
    }

    /// Queues the task. Panics if more than `set_max_queued` tasks are waiting.
    pub fn start(self) -> TaskHandle {
        self.try_start().expect("too many queued tasks (see task::set_max_queued)")
    }

    pub fn try_start(self) -> Result<TaskHandle, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared {
            cancel: AtomicBool::new(false),
            status: Mutex::new(Status::Pending),
            progress: Mutex::new(None),
            progress_posted: AtomicBool::new(false),
            finished: Condvar::new(),
        });
        let done = self.done.map(|f| {
            Box::new(move |any: Box<dyn Any + Send>| {
                if let Ok(o) = any.downcast::<Outcome<R>>() {
                    f(*o)
                }
            }) as Box<dyn FnOnce(Box<dyn Any + Send>)>
        });
        CALLBACKS.with(|c| c.borrow_mut().insert(id, Callbacks { progress: self.progress, done }));

        let work = self.work;
        let sh = shared.clone();
        let job: Job = Box::new(move || {
            let ctx = Ctx { id, shared: sh.clone() };
            let outcome = if ctx.is_cancelled() {
                Outcome::Cancelled
            } else {
                *sh.status.lock().unwrap() = Status::Running;
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ctx))) {
                    _ if ctx.is_cancelled() => Outcome::Cancelled,
                    Ok(Ok(v)) => Outcome::Done(v),
                    Ok(Err(e)) => Outcome::Failed(e),
                    Err(p) => Outcome::Failed(
                        p.downcast_ref::<&str>().map(|s| s.to_string())
                            .or_else(|| p.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "task panicked".into()),
                    ),
                }
            };
            let status = match outcome {
                Outcome::Done(_) => Status::Done,
                Outcome::Failed(_) => Status::Failed,
                Outcome::Cancelled => Status::Cancelled,
            };
            post(move || deliver(id, Event::Finished(Box::new(outcome))));
            *sh.status.lock().unwrap() = status;
            sh.finished.notify_all();
            POOL.lock().unwrap().live.retain(|s| !Arc::ptr_eq(s, &sh));
        });

        let mut pool = POOL.lock().unwrap();
        if pool.queue.len() >= MAX_QUEUED.load(Ordering::Relaxed) {
            CALLBACKS.with(|c| c.borrow_mut().remove(&id));
            return Err(format!("{} tasks already queued", pool.queue.len()));
        }
        pool.live.push(shared.clone());
        pool.queue.push_back(job);
        if pool.idle == 0 && pool.workers.len() < MAX_WORKERS.load(Ordering::Relaxed) {
            let handle = std::thread::Builder::new().name("omarchy-worker".into()).spawn(worker_loop).map_err(|e| e.to_string())?;
            pool.workers.push(handle);
        }
        drop(pool);
        WAKE.notify_one();
        Ok(TaskHandle { shared })
    }
}
