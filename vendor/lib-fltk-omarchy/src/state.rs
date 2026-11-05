//! Reactive values for the UI thread (Rc-based, not Send).
//!
//! ```no_run
//! use fltk_omarchy::State;
//! let count = State::new(0);
//! let label = count.map(|c| format!("Clicked {c} times"));
//! count.update(|c| *c += 1);
//! assert_eq!(label.get(), "Clicked 1 times");
//! ```
//!
//! **Equal values never notify**: `set` compares with `PartialEq` first
//! ([`State::set_if_changed`] tells whether it did).
//!
//! **Batches.** [`batch`] defers every notification until the closure
//! returns, then each changed State notifies once (and each
//! [`crate::model::ListModel`] once, with all its changes). Work posted with
//! [`crate::task::post`] and the feeds ([`StateFeed`], [`TextFeed`],
//! [`crate::model::Feed`]) already run inside one batch per event-loop
//! iteration, so a burst of updates costs one notification and one redraw.
//!
//! **Streaming text.** [`State::push_str`] appends in place (no clone of
//! the whole string) and listeners registered with
//! [`State::subscribe_edit`] receive an [`Edit::Appended`] hint telling
//! where the new text starts, so a text view only lays out the tail.
//!
//! ```no_run
//! use fltk_omarchy::state::{Edit, State};
//! let text = State::new(String::new());
//! text.subscribe_edit(|t, edit| match edit {
//!     Edit::Appended { from } => println!("new tail: {:?}", &t[from..]),
//!     _ => println!("whole text: {} bytes", t.len()),
//! }).forever();
//! text.push_str("Hello");   // Appended { from: 0 }
//! text.push_str(", world"); // Appended { from: 5 }
//! ```

use fltk::{prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What changed in a value, as told to [`State::subscribe_edit`] listeners.
///
/// Consumers that can update incrementally (a text view, a log) look at the
/// hint; anything else treats every edit as [`Edit::Replaced`]. Edits made
/// inside one [`batch`] are merged with [`Edit::then`]: appends stay an
/// append from the earliest offset, anything else becomes `Replaced`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// The value may have changed anywhere.
    Replaced,
    /// Only new content at the end: `value[from..]` is new and everything
    /// before `from` is unchanged. Offsets are bytes for `String`
    /// (always a char boundary) and items for `Vec`.
    Appended { from: usize },
}

impl Edit {
    /// This edit followed by `later`, as one edit.
    pub fn then(self, later: Edit) -> Edit {
        match (self, later) {
            (Edit::Appended { from: a }, Edit::Appended { from: b }) => Edit::Appended { from: a.min(b) },
            _ => Edit::Replaced,
        }
    }

    /// The part of a value of length `len` that is new: `from..len` for an
    /// append, `0..len` otherwise.
    pub fn new_range(self, len: usize) -> Range<usize> {
        match self {
            Edit::Appended { from } => from.min(len)..len,
            Edit::Replaced => 0..len,
        }
    }
}

// ------------------------------------------------------------------ batch

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static QUEUE: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f` with notifications deferred: every State (and ListModel)
/// changed inside notifies once when the outermost batch ends, with the
/// final value (and the merged [`Edit`]). Values are updated immediately,
/// so `get` inside the batch sees them; derived states (`map`, `combine`)
/// and widgets follow at the end. Nested batches join the outer one.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    DEPTH.with(|d| d.set(d.get() + 1));
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let left = DEPTH.with(|d| {
                let v = d.get().saturating_sub(1);
                d.set(v);
                v
            });
            // After a panic the queue is kept; the next batch delivers it.
            if left == 0 && !std::thread::panicking() {
                flush_batch();
            }
        }
    }
    let _guard = Guard;
    f()
}

/// True inside [`batch`] (notifications are being deferred).
pub fn in_batch() -> bool {
    DEPTH.with(|d| d.get() > 0)
}

fn flush_batch() {
    loop {
        let jobs = QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if jobs.is_empty() {
            return;
        }
        for job in jobs {
            job();
        }
    }
}

/// Queues a notification for the end of the current batch (models).
pub(crate) fn defer(job: Box<dyn FnOnce()>) {
    QUEUE.with(|q| q.borrow_mut().push(job));
}

// ------------------------------------------------------------------ State

type Listener<T> = Rc<RefCell<dyn FnMut(&T, Edit)>>;

struct Inner<T> {
    value: RefCell<T>,
    listeners: RefCell<Vec<(u64, Listener<T>)>>,
    next_id: Cell<u64>,
    notifying: Cell<bool>,
    /// Merged edit waiting for the end of a batch (Some = queued).
    pending: Cell<Option<Edit>>,
}

/// A shared, observable value. Cloning a `State` clones the handle.
pub struct State<T: 'static> {
    inner: Rc<Inner<T>>,
}

/// A State handle that does not keep the value alive.
pub(crate) struct WeakState<T: 'static> {
    inner: Weak<Inner<T>>,
}

impl<T: Clone + PartialEq + 'static> WeakState<T> {
    pub(crate) fn upgrade(&self) -> Option<State<T>> {
        self.inner.upgrade().map(|inner| State { inner })
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// Keeps a subscription alive; dropping it (or calling `cancel`) stops updates.
#[must_use = "dropping a Subscription unsubscribes immediately; store it or call .forever()"]
pub struct Subscription {
    cancel: Option<Box<dyn FnOnce()>>,
}

impl Subscription {
    pub(crate) fn new(cancel: impl FnOnce() + 'static) -> Subscription {
        Subscription { cancel: Some(Box::new(cancel)) }
    }

    pub fn cancel(mut self) {
        if let Some(c) = self.cancel.take() {
            c()
        }
    }

    /// Keeps the subscription for the lifetime of the State.
    pub fn forever(mut self) {
        self.cancel = None;
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(c) = self.cancel.take() {
            c()
        }
    }
}

impl<T: Clone + PartialEq + 'static> State<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(value),
                listeners: RefCell::new(Vec::new()),
                next_id: Cell::new(0),
                notifying: Cell::new(false),
                pending: Cell::new(None),
            }),
        }
    }

    pub fn get(&self) -> T {
        self.inner.value.borrow().clone()
    }

    /// Reads without cloning.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.borrow())
    }

    /// Sets the value; listeners run only if it changed.
    pub fn set(&self, value: T) {
        self.set_if_changed(value);
    }

    /// Sets the value when it differs (`PartialEq`); returns whether it
    /// changed (and listeners were or, in a batch, will be notified).
    pub fn set_if_changed(&self, value: T) -> bool {
        if *self.inner.value.borrow() == value {
            return false;
        }
        self.check_not_notifying();
        *self.inner.value.borrow_mut() = value;
        self.changed(Edit::Replaced);
        true
    }

    /// Changes a copy and sets it (notifies only if it differs). For large
    /// values prefer [`State::modify`], which edits in place.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut v = self.get();
        f(&mut v);
        self.set(v);
    }

    /// Edits the value in place, without cloning it. `f` returns what it
    /// changed (`None` = nothing: no notification). `f` must not read this
    /// State (it is borrowed mutably while `f` runs).
    ///
    /// ```no_run
    /// # use fltk_omarchy::state::{Edit, State};
    /// let log = State::new(Vec::<String>::new());
    /// log.modify(|v| { let from = v.len(); v.push("line".into()); Some(Edit::Appended { from }) });
    /// ```
    pub fn modify(&self, f: impl FnOnce(&mut T) -> Option<Edit>) -> bool {
        self.check_not_notifying();
        let edit = f(&mut self.inner.value.borrow_mut());
        match edit {
            Some(e) => {
                self.changed(e);
                true
            }
            None => false,
        }
    }

    fn check_not_notifying(&self) {
        assert!(!self.inner.notifying.get(), "State changed from inside its own listener (feedback loop)");
    }

    fn changed(&self, edit: Edit) {
        if in_batch() {
            let queued = self.inner.pending.get();
            self.inner.pending.set(Some(queued.map_or(edit, |p| p.then(edit))));
            if queued.is_none() {
                let inner = self.inner.clone();
                defer(Box::new(move || {
                    if let Some(e) = inner.pending.take() {
                        State { inner }.notify(e);
                    }
                }));
            }
            return;
        }
        // A batch interrupted by a panic may have left an edit pending.
        let edit = self.inner.pending.take().map_or(edit, |p| p.then(edit));
        self.notify(edit);
    }

    fn notify(&self, edit: Edit) {
        let listeners: Vec<Listener<T>> = self.inner.listeners.borrow().iter().map(|(_, l)| l.clone()).collect();
        if listeners.is_empty() {
            return;
        }
        struct Reset<'a>(&'a Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        self.inner.notifying.set(true);
        let _reset = Reset(&self.inner.notifying);
        // Listeners read the value in place: no clone, even for a 1 MB string.
        let value = self.inner.value.borrow();
        for l in listeners {
            // A listener already running (re-entrant set) is skipped, not re-borrowed.
            if let Ok(mut f) = l.try_borrow_mut() {
                f(&value, edit);
            }
        }
    }

    /// Calls `f` now and on every change.
    pub fn watch(&self, mut f: impl FnMut(&T) + 'static) -> Subscription {
        self.with(&mut f);
        self.subscribe(f)
    }

    /// Calls `f` on every change (not now).
    pub fn subscribe(&self, mut f: impl FnMut(&T) + 'static) -> Subscription {
        self.subscribe_edit(move |v, _| f(v))
    }

    /// Calls `f` on every change with what changed ([`Edit`]): incremental
    /// consumers (text views, logs) only process `edit.new_range(len)`.
    pub fn subscribe_edit(&self, f: impl FnMut(&T, Edit) + 'static) -> Subscription {
        let id = self.inner.next_id.get() + 1;
        self.inner.next_id.set(id);
        self.inner.listeners.borrow_mut().push((id, Rc::new(RefCell::new(f))));
        let weak: Weak<Inner<T>> = Rc::downgrade(&self.inner);
        Subscription::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.listeners.borrow_mut().retain(|(i, _)| *i != id);
            }
        })
    }

    /// [`State::subscribe_edit`], called once now with `Edit::Replaced`.
    pub fn watch_edit(&self, mut f: impl FnMut(&T, Edit) + 'static) -> Subscription {
        self.with(|v| f(v, Edit::Replaced));
        self.subscribe_edit(f)
    }

    pub(crate) fn downgrade(&self) -> WeakState<T> {
        WeakState { inner: Rc::downgrade(&self.inner) }
    }

    pub fn listener_count(&self) -> usize {
        self.inner.listeners.borrow().len()
    }

    /// Derived state that follows this one.
    pub fn map<U: Clone + PartialEq + 'static>(&self, f: impl Fn(&T) -> U + 'static) -> State<U> {
        let out = State::new(self.with(&f));
        let target = Rc::downgrade(&out.inner);
        self.subscribe(move |v| {
            if let Some(inner) = target.upgrade() {
                State { inner }.set(f(v));
            }
        })
        .forever();
        out
    }
}

impl State<String> {
    /// Appends in place (no copy of the existing text); listeners get
    /// `Edit::Appended { from: old_len }`.
    pub fn push_str(&self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.modify(|v| {
            let from = v.len();
            v.push_str(s);
            Some(Edit::Appended { from })
        });
    }

    /// Sets a full text received from a protocol that re-sends it: when the
    /// new text extends the current one, only the tail is stored and
    /// listeners get `Edit::Appended`; otherwise it is a plain `set`.
    pub fn set_text(&self, text: &str) {
        let extends = self.with(|cur| text.len() > cur.len() && text.as_bytes().starts_with(cur.as_bytes()));
        if extends {
            let from = self.with(|cur| cur.len());
            self.push_str(&text[from..]);
        } else if self.with(|cur| cur != text) {
            self.set(text.to_string());
        }
    }
}

impl<T: Clone + PartialEq + 'static> State<Vec<T>> {
    /// Appends one item in place; listeners get `Edit::Appended`.
    pub fn push(&self, item: T) {
        self.modify(|v| {
            let from = v.len();
            v.push(item);
            Some(Edit::Appended { from })
        });
    }

    /// Appends items in place; listeners get `Edit::Appended` (none if empty).
    pub fn extend(&self, items: impl IntoIterator<Item = T>) {
        self.modify(|v| {
            let from = v.len();
            v.extend(items);
            (v.len() > from).then_some(Edit::Appended { from })
        });
    }
}

/// Derived state from two sources.
pub fn combine<A, B, U>(a: &State<A>, b: &State<B>, f: impl Fn(&A, &B) -> U + 'static) -> State<U>
where
    A: Clone + PartialEq + 'static,
    B: Clone + PartialEq + 'static,
    U: Clone + PartialEq + 'static,
{
    let f = Rc::new(f);
    let out = State::new(a.with(|x| b.with(|y| f(x, y))));
    for which in 0..2 {
        let (a2, b2, f2) = (a.clone(), b.clone(), f.clone());
        let target = Rc::downgrade(&out.inner);
        let cb = move || {
            if let Some(inner) = target.upgrade() {
                State { inner }.set(a2.with(|x| b2.with(|y| f2(x, y))));
            }
        };
        if which == 0 { a.subscribe(move |_| cb()).forever() } else { b.subscribe(move |_| cb()).forever() }
    }
    out
}

// ------------------------------------------------------------- thread feeds

/// Sets a [`State`] from any thread: `Send + Sync + Clone`. The UI thread
/// applies only the latest value, once per event-loop iteration, inside a
/// [`batch`]. Create it on the UI thread with [`State::feed`].
///
/// ```no_run
/// # use fltk_omarchy::State;
/// let status = State::new(String::new());
/// let feed = status.feed();
/// std::thread::spawn(move || for i in 0..1000 { feed.set(format!("step {i}")) });
/// ```
pub struct StateFeed<T: Send + 'static> {
    slot: Arc<FeedSlot<Option<T>>>,
}

/// The UI-side handle of a feed's slot (set once the sink is registered).
pub(crate) type SlotCell<P> = Rc<RefCell<Option<Arc<FeedSlot<P>>>>>;

/// Shared by every feed: data pushed from threads behind a mutex, one
/// wake of the UI-thread sink until it drains.
pub(crate) struct FeedSlot<P> {
    pub(crate) pending: Mutex<P>,
    scheduled: AtomicBool,
    pub(crate) alive: AtomicBool,
    pub(crate) sink: crate::task::SinkId,
}

impl<P: Default> FeedSlot<P> {
    pub(crate) fn new(sink: crate::task::SinkId) -> Arc<Self> {
        Arc::new(FeedSlot { pending: Mutex::new(P::default()), scheduled: AtomicBool::new(false), alive: AtomicBool::new(true), sink })
    }

    pub(crate) fn push(&self, f: impl FnOnce(&mut P)) {
        if !self.alive.load(Ordering::Relaxed) {
            return;
        }
        f(&mut self.pending.lock().unwrap());
        if !self.scheduled.swap(true, Ordering::AcqRel) {
            self.sink.wake();
        }
    }

    pub(crate) fn set_max_rate(&self, hz: f64) {
        let ns = if hz > 0.0 { std::time::Duration::from_secs_f64(1.0 / hz).as_nanos() as u64 } else { 0 };
        self.sink.min_interval.store(ns, Ordering::Relaxed);
    }

    /// UI thread: takes what was pushed so far.
    pub(crate) fn take(&self) -> P {
        // Cleared first: a push after this schedules another drain.
        self.scheduled.store(false, Ordering::Release);
        std::mem::take(&mut *self.pending.lock().unwrap())
    }
}

impl<T: Send + 'static> Clone for StateFeed<T> {
    fn clone(&self) -> Self {
        StateFeed { slot: self.slot.clone() }
    }
}

impl<T: Send + 'static> StateFeed<T> {
    /// Thread-safe. Replaces any value not yet applied.
    pub fn set(&self, value: T) {
        self.slot.push(|p| *p = Some(value));
    }

    /// Applies at most `hz` times per second (0 = every loop iteration,
    /// the default); pushes in between wait for one fltk timeout.
    pub fn max_rate(self, hz: f64) -> Self {
        self.slot.set_max_rate(hz);
        self
    }

    pub fn set_max_rate(&self, hz: f64) {
        self.slot.set_max_rate(hz);
    }

    /// False once the State is gone (pushes are then dropped).
    pub fn is_connected(&self) -> bool {
        self.slot.alive.load(Ordering::Relaxed)
    }
}

impl<T: Clone + PartialEq + Send + 'static> State<T> {
    /// A thread-safe setter for this State (see [`StateFeed`]). Call on the
    /// UI thread.
    pub fn feed(&self) -> StateFeed<T> {
        let weak = Rc::downgrade(&self.inner);
        let cell: SlotCell<Option<T>> = Rc::default();
        let c2 = cell.clone();
        let sink = crate::task::register_sink(move || {
            let Some(slot) = c2.borrow().clone() else { return true };
            let Some(inner) = weak.upgrade() else {
                slot.alive.store(false, Ordering::Relaxed);
                return false;
            };
            if let Some(v) = slot.take() {
                State { inner }.set(v);
            }
            true
        });
        let slot = FeedSlot::new(sink);
        *cell.borrow_mut() = Some(slot.clone());
        StateFeed { slot }
    }
}

#[derive(Default)]
struct TextPending {
    replace: Option<String>,
    append: String,
}

/// Streams text into a `State<String>` from any thread (`Send + Sync +
/// Clone`): chunks pushed between two event-loop iterations are joined and
/// appended once, and listeners get one [`Edit::Appended`]. Create it on
/// the UI thread with [`State::text_feed`].
///
/// ```no_run
/// # use fltk_omarchy::State;
/// let answer = State::new(String::new());
/// let feed = answer.text_feed();
/// std::thread::spawn(move || for tok in ["Hel", "lo", "!"] { feed.push_str(tok) });
/// ```
#[derive(Clone)]
pub struct TextFeed {
    slot: Arc<FeedSlot<TextPending>>,
}

impl TextFeed {
    /// Thread-safe append.
    pub fn push_str(&self, s: &str) {
        if !s.is_empty() {
            self.slot.push(|p| p.append.push_str(s));
        }
    }

    /// Thread-safe: replaces the whole text (drops chunks not yet applied).
    pub fn set(&self, text: String) {
        self.slot.push(|p| {
            p.replace = Some(text);
            p.append.clear();
        });
    }

    /// Applies at most `hz` times per second (0 = every loop iteration,
    /// the default); pushes in between wait for one fltk timeout.
    pub fn max_rate(self, hz: f64) -> Self {
        self.slot.set_max_rate(hz);
        self
    }

    pub fn set_max_rate(&self, hz: f64) {
        self.slot.set_max_rate(hz);
    }

    /// False once the State is gone (pushes are then dropped).
    pub fn is_connected(&self) -> bool {
        self.slot.alive.load(Ordering::Relaxed)
    }
}

impl State<String> {
    /// A thread-safe appender for this State (see [`TextFeed`]). Call on
    /// the UI thread.
    pub fn text_feed(&self) -> TextFeed {
        let weak = Rc::downgrade(&self.inner);
        let cell: SlotCell<TextPending> = Rc::default();
        let c2 = cell.clone();
        let sink = crate::task::register_sink(move || {
            let Some(slot) = c2.borrow().clone() else { return true };
            let Some(inner) = weak.upgrade() else {
                slot.alive.store(false, Ordering::Relaxed);
                return false;
            };
            let p = slot.take();
            let s = State { inner };
            if let Some(r) = p.replace {
                s.set(r);
            }
            s.push_str(&p.append);
            true
        });
        let slot = FeedSlot::new(sink);
        *cell.borrow_mut() = Some(slot.clone());
        TextFeed { slot }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn notifies_on_change_only() {
        let s = State::new(1);
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        let sub = s.subscribe(move |_| h.set(h.get() + 1));
        s.set(1);
        s.set(2);
        s.update(|v| *v += 1);
        assert_eq!(hits.get(), 2);
        assert!(!s.set_if_changed(3), "equal value: no change");
        assert!(s.set_if_changed(4));
        assert_eq!(hits.get(), 3);
        sub.cancel();
        s.set(9);
        assert_eq!(hits.get(), 3);
        assert_eq!(s.listener_count(), 0);
    }

    #[test]
    fn map_and_combine() {
        let a = State::new(2);
        let b = State::new("x".to_string());
        let m = a.map(|v| v * 10);
        let c = combine(&a, &b, |x, y| format!("{y}{x}"));
        a.set(3);
        b.set("y".into());
        assert_eq!(m.get(), 30);
        assert_eq!(c.get(), "y3");
    }

    #[test]
    fn dropping_subscription_unsubscribes() {
        let s = State::new(0);
        {
            let _sub = s.subscribe(|_| {});
            assert_eq!(s.listener_count(), 1);
        }
        assert_eq!(s.listener_count(), 0);
    }

    #[test]
    fn batch_coalesces_into_one_notification_per_state() {
        let a = State::new(0);
        let b = State::new(String::new());
        let doubled = a.map(|v| v * 2);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s2 = seen.clone();
        let _sub = a.subscribe(move |v| s2.borrow_mut().push(*v));
        let edits = Rc::new(RefCell::new(Vec::new()));
        let e2 = edits.clone();
        let _sub2 = b.subscribe_edit(move |t, e| e2.borrow_mut().push((t.clone(), e)));
        let r = batch(|| {
            for i in 1..=100 {
                a.set(i);
                b.push_str("ab");
            }
            batch(|| a.set(500)); // nested: joins the outer batch
            assert_eq!(a.get(), 500, "values update immediately");
            assert_eq!(doubled.get(), 0, "derived states follow at the end");
            assert!(seen.borrow().is_empty());
            7
        });
        assert_eq!(r, 7);
        assert_eq!(*seen.borrow(), vec![500], "one notification with the final value");
        assert_eq!(doubled.get(), 1000);
        assert_eq!(edits.borrow().len(), 1);
        assert_eq!(edits.borrow()[0].1, Edit::Appended { from: 0 }, "appends merge into one append");
        assert_eq!(edits.borrow()[0].0.len(), 200);
        // A set back to the original value inside a batch still notifies once (values differ in between).
        assert!(!in_batch());
    }

    #[test]
    fn append_hints_and_set_text() {
        let s = State::new(String::from("Hel"));
        let edits = Rc::new(RefCell::new(Vec::new()));
        let e2 = edits.clone();
        let _sub = s.subscribe_edit(move |t, e| e2.borrow_mut().push((e, t[e.new_range(t.len())].to_string())));
        s.push_str("lo");
        s.set_text("Hello, world");
        s.set_text("Hello, world"); // unchanged: nothing
        s.set_text("Bye");
        s.push_str("");
        assert_eq!(
            *edits.borrow(),
            vec![
                (Edit::Appended { from: 3 }, "lo".to_string()),
                (Edit::Appended { from: 5 }, ", world".to_string()),
                (Edit::Replaced, "Bye".to_string()),
            ]
        );
        assert_eq!(Edit::Appended { from: 4 }.then(Edit::Replaced), Edit::Replaced);
        let v = State::new(vec![1, 2]);
        let hint = Rc::new(Cell::new(None));
        let h2 = hint.clone();
        let _s = v.subscribe_edit(move |_, e| h2.set(Some(e)));
        v.extend([3, 4]);
        assert_eq!(hint.get(), Some(Edit::Appended { from: 2 }));
        v.extend(std::iter::empty());
        v.push(5);
        assert_eq!((hint.get(), v.get()), (Some(Edit::Appended { from: 4 }), vec![1, 2, 3, 4, 5]));
    }

    #[test]
    fn listeners_read_without_clone_and_may_read_the_state() {
        #[derive(PartialEq)]
        struct NoClone(u32);
        impl Clone for NoClone {
            fn clone(&self) -> Self {
                panic!("value cloned during notification")
            }
        }
        let s = State::new(NoClone(0));
        let s2 = s.clone();
        let read = Rc::new(Cell::new(0));
        let r2 = read.clone();
        let _sub = s.subscribe(move |v| r2.set(v.0 + s2.with(|x| x.0)));
        s.modify(|v| {
            v.0 = 21;
            Some(Edit::Replaced)
        });
        assert_eq!(read.get(), 42);
    }

    #[test]
    #[should_panic(expected = "feedback loop")]
    fn setting_inside_own_listener_panics() {
        let s = State::new(0);
        let s2 = s.clone();
        s.subscribe(move |v| s2.set(v + 1)).forever();
        s.set(1);
    }
}

/// A widget binding: the widget it belongs to, its subscription and the
/// bound State itself, so a temporary (`.bind(&s.map(..))`) lives as long
/// as the widget.
type Tie = (Widget, Subscription, Box<dyn std::any::Any>);

thread_local! {
    static TIED: RefCell<Vec<Tie>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f` on every change of `state` while the fltk widget `w` exists
/// (and once now when `now`). The binding belongs to the widget, not to the
/// Rust handle: `flex.fit(&*Toggle::new(..))` keeps following its State
/// after the temporary handle is dropped, and the listener goes away when
/// the widget is deleted. The binding also keeps the State alive, so a
/// derived temporary (`Label::new("").bind(&count.map(..))`) keeps updating.
pub fn bind_widget<W: WidgetExt, T: Clone + PartialEq + 'static>(w: &W, state: &State<T>, now: bool, mut f: impl FnMut(&T) + 'static) {
    // SAFETY: `w` is a live widget; the copy only tracks its deletion.
    let guard = unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) };
    let alive = guard.clone();
    let mut run = move |v: &T| if !alive.was_deleted() { f(v) };
    if now {
        state.with(|v| run(v));
    }
    let sub = state.subscribe(run);
    tie(guard, sub, Box::new(state.clone()));
}

/// [`bind_widget`] with the change hint ([`Edit`]): `f` runs now with
/// `Edit::Replaced`, then on every change with what changed.
pub(crate) fn bind_widget_edit<W: WidgetExt, T: Clone + PartialEq + 'static>(w: &W, state: &State<T>, mut f: impl FnMut(&T, Edit) + 'static) {
    // SAFETY: `w` is a live widget; the copy only tracks its deletion.
    let guard = unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) };
    let alive = guard.clone();
    let mut run = move |v: &T, e: Edit| if !alive.was_deleted() { f(v, e) };
    state.with(|v| run(v, Edit::Replaced));
    let sub = state.subscribe_edit(run);
    tie(guard, sub, Box::new(state.clone()));
}

/// Keeps `sub` (and `keep`, the bound source) while the widget lives
/// (shared by States and models).
pub(crate) fn tie(guard: Widget, sub: Subscription, keep: Box<dyn std::any::Any>) {
    // Drop dead bindings outside the registry borrow (their cancel touches States).
    let dead: Vec<Tie> = TIED.with(|t| {
        let mut t = t.borrow_mut();
        let (dead, live): (Vec<_>, Vec<_>) = std::mem::take(&mut *t).into_iter().partition(|(w, _, _)| w.was_deleted());
        *t = live;
        t.push((guard, sub, keep));
        dead
    });
    drop(dead);
}

/// Number of widget bindings currently registered (tests).
pub fn widget_binding_count() -> usize {
    TIED.with(|t| t.borrow().iter().filter(|(w, _, _)| !w.was_deleted()).count())
}
