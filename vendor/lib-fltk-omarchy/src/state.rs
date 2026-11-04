//! Reactive values for the UI thread (Rc-based, not Send).
//!
//! ```no_run
//! use fltk_omarchy::State;
//! let count = State::new(0);
//! let label = count.map(|c| format!("Clicked {c} times"));
//! count.update(|c| *c += 1);
//! assert_eq!(label.get(), "Clicked 1 times");
//! ```

use fltk::{prelude::*, widget::Widget};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

type Listener<T> = Rc<RefCell<dyn FnMut(&T)>>;

struct Inner<T> {
    value: T,
    listeners: Vec<(u64, Listener<T>)>,
    next_id: u64,
    notifying: bool,
}

/// A shared, observable value. Cloning a `State` clones the handle.
pub struct State<T: 'static> {
    inner: Rc<RefCell<Inner<T>>>,
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
        Self { inner: Rc::new(RefCell::new(Inner { value, listeners: Vec::new(), next_id: 0, notifying: false })) }
    }

    pub fn get(&self) -> T {
        self.inner.borrow().value.clone()
    }

    /// Reads without cloning.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.borrow().value)
    }

    /// Sets the value; listeners run only if it changed.
    pub fn set(&self, value: T) {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.value == value {
                return;
            }
            assert!(!inner.notifying, "State changed from inside its own listener (feedback loop)");
            inner.value = value;
            inner.notifying = true;
        }
        let (value, listeners): (T, Vec<Listener<T>>) = {
            let inner = self.inner.borrow();
            (inner.value.clone(), inner.listeners.iter().map(|(_, l)| l.clone()).collect())
        };
        for l in listeners {
            // A listener already running (re-entrant set) is skipped, not re-borrowed.
            if let Ok(mut f) = l.try_borrow_mut() {
                f(&value);
            }
        }
        self.inner.borrow_mut().notifying = false;
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut v = self.get();
        f(&mut v);
        self.set(v);
    }

    /// Calls `f` now and on every change.
    pub fn watch(&self, mut f: impl FnMut(&T) + 'static) -> Subscription {
        let v = self.get();
        f(&v);
        self.subscribe(f)
    }

    /// Calls `f` on every change (not now).
    pub fn subscribe(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        let id = {
            let mut inner = self.inner.borrow_mut();
            inner.next_id += 1;
            let id = inner.next_id;
            inner.listeners.push((id, Rc::new(RefCell::new(f))));
            id
        };
        let weak: Weak<RefCell<Inner<T>>> = Rc::downgrade(&self.inner);
        Subscription {
            cancel: Some(Box::new(move || {
                if let Some(inner) = weak.upgrade() {
                    inner.borrow_mut().listeners.retain(|(i, _)| *i != id);
                }
            })),
        }
    }

    pub fn listener_count(&self) -> usize {
        self.inner.borrow().listeners.len()
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
        sub.cancel();
        s.set(9);
        assert_eq!(hits.get(), 2);
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
}

thread_local! {
    static TIED: RefCell<Vec<(Widget, Subscription)>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f` on every change of `state` while the fltk widget `w` exists
/// (and once now when `now`). The binding belongs to the widget, not to the
/// Rust handle: `flex.fit(&*Toggle::new(..))` keeps following its State
/// after the temporary handle is dropped, and the listener goes away when
/// the widget is deleted.
pub fn bind_widget<W: WidgetExt, T: Clone + PartialEq + 'static>(w: &W, state: &State<T>, now: bool, mut f: impl FnMut(&T) + 'static) {
    // SAFETY: `w` is a live widget; the copy only tracks its deletion.
    let guard = unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) };
    let alive = guard.clone();
    let mut run = move |v: &T| if !alive.was_deleted() { f(v) };
    if now {
        state.with(|v| run(v));
    }
    let sub = state.subscribe(run);
    // Drop dead bindings outside the registry borrow (their cancel touches States).
    let dead: Vec<(Widget, Subscription)> = TIED.with(|t| {
        let mut t = t.borrow_mut();
        let (dead, live): (Vec<_>, Vec<_>) = std::mem::take(&mut *t).into_iter().partition(|(w, _)| w.was_deleted());
        *t = live;
        t.push((guard, sub));
        dead
    });
    drop(dead);
}

/// Number of widget bindings currently registered (tests).
pub fn widget_binding_count() -> usize {
    TIED.with(|t| t.borrow().iter().filter(|(w, _)| !w.was_deleted()).count())
}
