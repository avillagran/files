//! Registry of widget descriptions, per-window AccessKit state and the
//! once-per-loop sync. Everything here runs on the UI thread except the
//! small queue the platform adapters fill from their own threads.

use super::{platform, tree, Action, Live, Node, Request, Value};
use accesskit as ak;
use fltk::{app, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type DescribeFn = Rc<dyn Fn(&mut Node)>;
type ActFn = Rc<RefCell<Box<dyn FnMut(&Request) -> bool>>>;

struct Entry {
    /// Deletion-tracked handle (fltk's widget tracker): a reused address
    /// never inherits a dead widget's entry.
    w: Widget,
    id: u64,
    describe: Option<DescribeFn>,
    act: Option<ActFn>,
    click: Option<Rc<dyn Fn()>>,
    name: Option<String>,
    description: Option<String>,
}

/// What the tree builder needs from an entry (cloned out of the registry,
/// so describe callbacks may register widgets themselves).
pub(crate) struct Info {
    pub id: u64,
    pub describe: Option<DescribeFn>,
    pub name: Option<String>,
    pub description: Option<String>,
}

thread_local! {
    static REG: RefCell<HashMap<usize, Entry>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static INSERTS: Cell<u32> = const { Cell::new(0) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    static ADAPTERS_OFF: Cell<bool> = const { Cell::new(false) };
    static DEBUG: Cell<bool> = const { Cell::new(false) };
    static WINDOWS: RefCell<Vec<WinState>> = const { RefCell::new(Vec::new()) };
    static DIRTY: Cell<bool> = const { Cell::new(false) };
    static LAST_FOCUS: Cell<usize> = const { Cell::new(0) };
    static POPUPS: RefCell<Vec<(Widget, usize)>> = const { RefCell::new(Vec::new()) };
    static ANNOUNCE: RefCell<Option<(String, Live, bool)>> = const { RefCell::new(None) };
}

/// Set by adapter threads (activation, actions); read by the UI check.
static WAKE: AtomicBool = AtomicBool::new(false);
static QUEUE: Mutex<Vec<ak::ActionRequest>> = Mutex::new(Vec::new());

pub(crate) fn wake() {
    WAKE.store(true, Ordering::SeqCst);
    app::awake();
}

pub(crate) fn queue_action(r: ak::ActionRequest) {
    if let Ok(mut q) = QUEUE.lock() {
        q.push(r);
    }
    wake();
}

fn ptr<W: WidgetExt + ?Sized>(w: &W) -> usize {
    w.as_widget_ptr() as usize
}

/// A plain `Widget` handle (deletion-tracked) for any fltk widget.
pub(crate) fn base<W: WidgetExt + ?Sized>(w: &W) -> Widget {
    // SAFETY: `w` is a live widget.
    unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) }
}

/// Pixels per FLTK unit of a window (FLTK's per-screen scale).
pub(crate) fn scale_of(w: &Widget) -> f64 {
    let s = w
        .as_window()
        .map(|w| app::screen_scale(w.screen_num()) as f64)
        .unwrap_or(1.0);
    if s.is_finite() && s > 0.0 {
        s
    } else {
        1.0
    }
}

fn shown(w: &Widget) -> bool {
    w.as_window().is_some_and(|w| w.shown())
}

fn with_entry<W: WidgetExt + ?Sized, R>(w: &W, f: impl FnOnce(&mut Entry) -> R) -> R {
    let p = ptr(w);
    REG.with(|reg| {
        let mut reg = reg.borrow_mut();
        if reg.get(&p).is_some_and(|e| e.w.was_deleted()) {
            reg.remove(&p);
        }
        if !reg.contains_key(&p) {
            // Keep the registry bounded when widgets come and go (popups).
            let n = INSERTS.with(|c| c.replace(c.get().wrapping_add(1)));
            if n % 256 == 255 {
                reg.retain(|_, e| !e.w.was_deleted());
            }
            let id = NEXT_ID.with(|c| c.replace(c.get() + 1));
            // SAFETY: `w` is a live widget; the copy only tracks its deletion.
            let tracked = unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) };
            reg.insert(
                p,
                Entry {
                    w: tracked,
                    id,
                    describe: None,
                    act: None,
                    click: None,
                    name: None,
                    description: None,
                },
            );
        }
        f(reg.get_mut(&p).expect("entry"))
    })
}

pub(crate) fn set_describe<W: WidgetExt + ?Sized>(w: &W, f: DescribeFn) {
    with_entry(w, |e| e.describe = Some(f));
}

pub(crate) fn set_action<W: WidgetExt + ?Sized>(w: &W, f: Box<dyn FnMut(&Request) -> bool>) {
    with_entry(w, |e| e.act = Some(Rc::new(RefCell::new(f))));
}

pub(crate) fn set_click<W: WidgetExt + ?Sized>(w: &W, f: Rc<dyn Fn()>) {
    with_entry(w, |e| e.click = Some(f));
}

pub(crate) fn set_name<W: WidgetExt + ?Sized>(w: &W, name: &str) {
    with_entry(w, |e| e.name = (!name.is_empty()).then(|| name.to_string()));
    mark_changed();
}

pub(crate) fn set_description<W: WidgetExt + ?Sized>(w: &W, text: &str) {
    with_entry(w, |e| {
        e.description = (!text.is_empty()).then(|| text.to_string())
    });
    mark_changed();
}

pub(crate) fn info(w: &Widget) -> Info {
    with_entry(w, |e| Info {
        id: e.id,
        describe: e.describe.clone(),
        name: e.name.clone(),
        description: e.description.clone(),
    })
}

pub(crate) fn mark_changed() {
    DIRTY.with(|d| d.set(true));
}

pub(crate) fn popup_of<L: WidgetExt + ?Sized, T: WidgetExt + ?Sized>(layer: &L, trigger: &T) {
    // SAFETY: `layer` is live; the copy only tracks its deletion.
    let l = unsafe { Widget::from_widget_ptr(layer.as_widget_ptr()) };
    POPUPS.with(|p| {
        let mut p = p.borrow_mut();
        p.retain(|(l, _)| !l.was_deleted() && l.parent().is_some());
        p.push((l, ptr(trigger)));
    });
}

pub(crate) fn has_popup<T: WidgetExt + ?Sized>(trigger: &T) -> bool {
    let t = ptr(trigger);
    POPUPS.with(|p| {
        p.borrow()
            .iter()
            .any(|(l, tr)| *tr == t && !l.was_deleted() && l.parent().is_some())
    })
}

pub(crate) fn announce(text: &str, live: Live) {
    ANNOUNCE.with(|a| {
        let mut a = a.borrow_mut();
        // The same text twice is still read: alternate an invisible mark.
        let flip = a.as_ref().is_some_and(|(t, _, f)| t == text && !*f);
        *a = Some((text.to_string(), live, flip));
    });
    mark_changed();
}

/// The live region text for the window with root `root` (the focused one).
pub(crate) fn announcement(root: u64) -> Option<(String, Live)> {
    let focused_root = app::focus()
        .map(|f| match f.top_window() {
            Some(t) => base(&*t),
            None => base(&f),
        })
        .map(|w| info(&w).id);
    let first = app::first_window().map(|w| info(&base(&w)).id);
    if focused_root.or(first) != Some(root) {
        return None;
    }
    ANNOUNCE.with(|a| {
        a.borrow().as_ref().map(|(t, l, flip)| {
            (
                if *flip {
                    format!("{t}\u{200b}")
                } else {
                    t.clone()
                },
                *l,
            )
        })
    })
}

// ------------------------------------------------------------ windows

/// Activation state shared with a window's platform adapter.
#[derive(Default)]
pub(crate) struct Activation {
    /// Bumped on every (re)activation: the next update must be a full tree.
    pub generation: AtomicU64,
    pub active: AtomicBool,
}

struct WinState {
    win: Widget,
    adapter: platform::Adapter,
    shared: Arc<Activation>,
    seen: u64,
    sent: HashMap<ak::NodeId, ak::Node>,
    routes: HashMap<u64, tree::Route>,
    focused: Option<bool>,
    bounds: Option<(i32, i32, i32, i32)>,
}

pub(crate) fn install() {
    if INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    let off = std::env::var("OMARCHY_FLTK_A11Y")
        .is_ok_and(|v| matches!(v.trim(), "0" | "off" | "false" | "no"));
    ADAPTERS_OFF.with(|o| o.set(off));
    DEBUG.with(|d| d.set(std::env::var_os("OMARCHY_FLTK_A11Y_DEBUG").is_some()));
    app::add_check(|_| check());
}

pub(crate) fn is_active() -> bool {
    WINDOWS.with(|w| {
        w.borrow()
            .iter()
            .any(|s| s.shared.active.load(Ordering::SeqCst))
    })
}

/// Shown top-level windows (no menus, tooltips or subwindows).
fn top_windows() -> Vec<Widget> {
    app::windows()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.parent().is_none() && !w.is_override() && w.shown())
        .map(|w| base(&w))
        .collect()
}

/// Runs before every wait of the event loop (and in `App::pump`).
fn check() {
    let mut list = WINDOWS.with(|w| std::mem::take(&mut *w.borrow_mut()));
    list.retain(|s| !s.win.was_deleted() && (s.adapter.is_recorder() || shown(&s.win)));
    if !ADAPTERS_OFF.with(|o| o.get()) {
        for w in top_windows() {
            if !list.iter().any(|s| ptr(&s.win) == ptr(&w)) {
                if let Some(s) = attach(&w, None) {
                    list.push(s);
                }
            }
        }
    }
    let ran = run_actions(&list);
    let woke = WAKE.swap(false, Ordering::SeqCst);
    let active = list.iter().any(|s| s.shared.active.load(Ordering::SeqCst));
    if active {
        let focus = app::focus().map_or(0, |f| f.as_widget_ptr() as usize);
        let focus_moved = LAST_FOCUS.with(|l| l.replace(focus)) != focus;
        // Everything changes with focus, an action or an explicit change;
        // otherwise only windows that repaint (or were just activated).
        let all = DIRTY.with(|d| d.replace(false)) || woke || ran || focus_moved;
        for s in &mut list {
            let active = s.shared.active.load(Ordering::SeqCst);
            if active
                && (all || s.win.damage() || s.shared.generation.load(Ordering::SeqCst) != s.seen)
            {
                sync(s);
            }
        }
    }
    WINDOWS.with(|w| {
        let mut w = w.borrow_mut();
        // Windows attached while we ran (tests) go after the old ones.
        let mut added = std::mem::take(&mut *w);
        list.append(&mut added);
        *w = list;
    });
}

fn attach(w: &Widget, recorder: Option<platform::Recorder>) -> Option<WinState> {
    let shared = Arc::new(Activation::default());
    let adapter = match recorder {
        Some(r) => {
            shared.generation.store(1, Ordering::SeqCst);
            shared.active.store(true, Ordering::SeqCst);
            platform::Adapter::Recorder(r)
        }
        None => platform::Adapter::new(w, shared.clone())?,
    };
    Some(WinState {
        win: w.clone(),
        adapter,
        shared,
        seen: 0,
        sent: HashMap::new(),
        routes: HashMap::new(),
        focused: None,
        bounds: None,
    })
}

fn window_focused(w: &Widget) -> bool {
    let Some(f) = app::focus() else { return false };
    f.as_widget_ptr() as usize == ptr(w) || f.top_window().is_some_and(|t| ptr(&*t) == ptr(w))
}

fn sync(s: &mut WinState) {
    let focused = window_focused(&s.win);
    if s.focused != Some(focused) {
        s.focused = Some(focused);
        s.adapter.set_focused(focused);
    }
    let b = (s.win.x(), s.win.y(), s.win.w(), s.win.h());
    if s.bounds != Some(b) {
        s.bounds = Some(b);
        let scale = scale_of(&s.win);
        s.adapter.set_bounds(b, scale);
    }
    let (win, shared, seen, sent, routes) =
        (&s.win, &s.shared, &mut s.seen, &mut s.sent, &mut s.routes);
    s.adapter.update_if_active(|| {
        // Read inside the factory: the adapter holds its lock, so a
        // concurrent (re)activation is either seen here or comes after.
        let gen = shared.generation.load(Ordering::SeqCst);
        let t0 = std::time::Instant::now();
        let built = tree::build(win);
        let debug = DEBUG.with(|d| d.get());
        *routes = built.routes;
        let focus = ak::NodeId(built.focus);
        if gen != *seen {
            *seen = gen;
            *sent = built.nodes.iter().cloned().collect();
            if debug {
                eprintln!(
                    "a11y: full tree {} nodes in {} us",
                    built.nodes.len(),
                    t0.elapsed().as_micros()
                );
            }
            let mut t = ak::TreeInfo::new(ak::NodeId(built.root));
            t.toolkit_name = Some("lib-fltk-omarchy".into());
            t.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
            return ak::TreeUpdate {
                nodes: built.nodes,
                tree: Some(t),
                tree_id: ak::TreeId::ROOT,
                focus,
            };
        }
        let mut changed = Vec::new();
        let mut next = HashMap::with_capacity(built.nodes.len());
        for (id, n) in built.nodes {
            if sent.get(&id) != Some(&n) {
                changed.push((id, n.clone()));
            }
            next.insert(id, n);
        }
        *sent = next;
        if debug {
            eprintln!(
                "a11y: update {} of {} nodes in {} us",
                changed.len(),
                sent.len(),
                t0.elapsed().as_micros()
            );
        }
        ak::TreeUpdate {
            nodes: changed,
            tree: None,
            tree_id: ak::TreeId::ROOT,
            focus,
        }
    });
}

// ------------------------------------------------------------ actions

fn value_of(d: Option<ak::ActionData>) -> Option<Value> {
    match d? {
        ak::ActionData::Value(v) => Some(Value::Text(v.into())),
        ak::ActionData::NumericValue(n) => Some(Value::Number(n)),
        ak::ActionData::SetTextSelection(sel) => Some(Value::Selection {
            anchor: sel.anchor.character_index,
            focus: sel.focus.character_index,
        }),
        _ => None,
    }
}

/// Runs queued AT requests; true when any ran.
fn run_actions(list: &[WinState]) -> bool {
    let reqs: Vec<ak::ActionRequest> = QUEUE
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    let mut ran = false;
    for r in reqs {
        let route = list
            .iter()
            .find_map(|s| s.routes.get(&r.target_node.0).cloned());
        // Text runs belong to their text node: selection requests name a run.
        let (route, data) = match route {
            Some(rt) => (rt, r.data),
            None => continue,
        };
        let Some(action) = tree::action_from(r.action) else {
            continue;
        };
        let value = match (&data, action) {
            (Some(ak::ActionData::SetTextSelection(sel)), _) => text_selection(list, sel),
            _ => value_of(data),
        };
        ran |= dispatch(
            &route.widget,
            Request {
                action,
                path: route.path,
                value,
            },
        );
    }
    ran
}

/// A selection over text runs, in characters of the text of the node
/// that holds the runs: `Selection` when that is the widget's own node,
/// `TextRange` (with the key path of each holder) for virtual children.
fn text_selection(list: &[WinState], sel: &ak::TextSelection) -> Option<Value> {
    let place = |p: &ak::TextPosition| -> Option<(Vec<u64>, usize)> {
        for s in list {
            let Some((owner, node)) = s.sent.iter().find(|(_, n)| n.children().contains(&p.node))
            else {
                continue;
            };
            let mut at = 0;
            for c in node.children() {
                if *c == p.node {
                    let path = s.routes.get(&owner.0).map(|r| r.path.clone()).unwrap_or_default();
                    return Some((path, at + p.character_index));
                }
                at += s.sent.get(c).map_or(0, |n| n.character_lengths().len());
            }
        }
        None
    };
    let (anchor, focus) = (place(&sel.anchor)?, place(&sel.focus)?);
    Some(if anchor.0.is_empty() && focus.0.is_empty() {
        Value::Selection {
            anchor: anchor.1,
            focus: focus.1,
        }
    } else {
        Value::TextRange { anchor, focus }
    })
}

/// Routes a request to the widget's handler, then to the defaults
/// (click callback, focus, plain fltk behavior). True when handled.
pub(crate) fn dispatch(w: &Widget, r: Request) -> bool {
    if w.was_deleted() {
        return false;
    }
    let (act, click) = REG.with(|reg| {
        reg.borrow()
            .get(&ptr(w))
            .filter(|e| !e.w.was_deleted())
            .map(|e| (e.act.clone(), e.click.clone()))
            .unwrap_or((None, None))
    });
    if let Some(a) = act {
        // Not re-entrant: a handler that triggers another request skips it.
        let used = a.try_borrow_mut().map(|mut f| f(&r)).unwrap_or(false);
        if used {
            mark_changed();
            return true;
        }
    }
    if tree::panel_action(w, &r) {
        mark_changed();
        return true;
    }
    if !r.path.is_empty() {
        return false;
    }
    let mut w = w.clone();
    let used = match r.action {
        Action::Focus => w.take_focus().is_ok(),
        Action::Click if !w.active_r() => false,
        Action::Click => match click {
            Some(c) => {
                c();
                true
            }
            None => generic_click(&mut w),
        },
        Action::SetValue
        | Action::Increment
        | Action::Decrement
        | Action::SetTextSelection
        | Action::ReplaceSelectedText => generic_value(&mut w, &r),
        _ => false,
    };
    if used {
        crate::focus::sync_panels(crate::focus::PanelFocusCause::Other);
        mark_changed();
    }
    used
}

fn generic_click(w: &mut Widget) -> bool {
    use fltk::button;
    let p = w.as_widget_ptr();
    if let Some(mut b) = button::Button::from_dyn_widget_ptr(p as _) {
        let toggles = button::LightButton::from_dyn_widget_ptr(p as _).is_some()
            || matches!(
                b.get_type::<button::ButtonType>(),
                button::ButtonType::Toggle | button::ButtonType::Radio
            );
        if toggles {
            if b.get_type::<button::ButtonType>() == button::ButtonType::Radio
                || button::RoundButton::from_dyn_widget_ptr(p as _).is_some()
            {
                // Fl_Button::setonly: clear the other radio buttons of the group.
                if let Some(g) = b.parent() {
                    for i in 0..g.children() {
                        let Some(c) = g.child(i) else { continue };
                        if let Some(mut o) =
                            button::Button::from_dyn_widget_ptr(c.as_widget_ptr() as _)
                        {
                            if o.as_widget_ptr() != b.as_widget_ptr()
                                && o.get_type::<button::ButtonType>() == button::ButtonType::Radio
                            {
                                o.set_value(false);
                            }
                        }
                    }
                }
                b.set_value(true);
            } else {
                b.set_value(!b.value());
            }
        }
        b.do_callback();
        return true;
    }
    if fltk::input::Input::from_dyn_widget_ptr(p as _).is_some() {
        return w.take_focus().is_ok();
    }
    w.do_callback();
    true
}

fn generic_value(w: &mut Widget, r: &Request) -> bool {
    use fltk::{input, misc, valuator};
    let p = w.as_widget_ptr();
    if let Some(mut i) = input::Input::from_dyn_widget_ptr(p as _) {
        let text = i.value();
        match (&r.action, &r.value) {
            (Action::SetValue, Some(Value::Text(t))) => {
                i.set_value(t);
                i.do_callback();
            }
            (Action::SetTextSelection, Some(Value::Selection { anchor, focus })) => {
                let _ = i.set_position(tree::byte_index(&text, *focus) as i32);
                let _ = i.set_mark(tree::byte_index(&text, *anchor) as i32);
            }
            (Action::ReplaceSelectedText, Some(Value::Text(t))) => {
                let _ = i.replace(i.position(), i.mark(), t);
                i.do_callback();
            }
            _ => return false,
        }
        i.redraw();
        return true;
    }
    let step_of = |s: f64, lo: f64, hi: f64| if s > 0.0 { s } else { (hi - lo).abs() / 100.0 };
    let apply = |v: f64, lo: f64, hi: f64, s: f64| -> Option<f64> {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let n = match (&r.action, &r.value) {
            (Action::SetValue, Some(Value::Number(n))) => *n,
            (Action::SetValue, Some(Value::Text(t))) => t.trim().parse().ok()?,
            (Action::Increment, _) => v + step_of(s, lo, hi),
            (Action::Decrement, _) => v - step_of(s, lo, hi),
            _ => return None,
        };
        Some(n.clamp(lo, hi))
    };
    macro_rules! valuator {
        ($t:ty) => {
            if let Some(mut s) = <$t>::from_dyn_widget_ptr(p as _) {
                let Some(n) = apply(s.value(), s.minimum(), s.maximum(), s.step()) else {
                    return false;
                };
                s.set_value(n);
                s.do_callback();
                s.redraw();
                return true;
            }
        };
    }
    valuator!(misc::Spinner);
    valuator!(valuator::Slider);
    valuator!(valuator::Dial);
    valuator!(valuator::Roller);
    valuator!(valuator::Counter);
    valuator!(valuator::ValueInput);
    false
}

// ------------------------------------------------------------ testing

pub mod testing {
    //! Records what would be sent to a window's platform adapter
    //! (unit and UI tests; no D-Bus or window system needed).
    use super::*;
    pub use accesskit;
    pub use accesskit_consumer;

    /// Updates sent to one window, applied to a local copy of the tree.
    #[derive(Clone)]
    pub struct Recorder {
        log: Rc<RefCell<Vec<ak::TreeUpdate>>>,
        applied: Rc<Cell<usize>>,
        tree: Rc<RefCell<HashMap<ak::NodeId, ak::Node>>>,
        focus: Rc<Cell<Option<ak::NodeId>>>,
    }

    /// Starts recording `window` as if an AT had just activated it (also
    /// when it is not shown). Installs the sync check if needed.
    pub fn record<W: WindowExt>(window: &W) -> Recorder {
        install();
        let win = base(window);
        let rec = Recorder {
            log: Rc::default(),
            applied: Rc::default(),
            tree: Rc::default(),
            focus: Rc::default(),
        };
        let state = attach(
            &win,
            Some(platform::Recorder {
                log: rec.log.clone(),
            }),
        )
        .expect("recorder");
        WINDOWS.with(|w| {
            let mut w = w.borrow_mut();
            w.retain(|s| !s.win.was_deleted() && ptr(&s.win) != ptr(&win));
            w.push(state);
        });
        rec
    }

    impl Recorder {
        /// Runs the per-loop sync now (as the event loop would before waiting).
        pub fn sync(&self) {
            mark_changed();
            check();
            let log = self.log.borrow();
            let mut t = self.tree.borrow_mut();
            for u in log.iter().skip(self.applied.get()) {
                if u.tree.is_some() {
                    t.clear();
                }
                for (id, n) in &u.nodes {
                    t.insert(*id, n.clone());
                }
                self.focus.set(Some(u.focus));
            }
            self.applied.set(log.len());
        }

        /// The updates sent since the last call (each sync sends at most one).
        pub fn take(&self) -> Vec<ak::TreeUpdate> {
            self.applied.set(0);
            std::mem::take(&mut *self.log.borrow_mut())
        }

        /// The node with this role and label (or value, for labels).
        pub fn find(&self, role: ak::Role, name: &str) -> Option<ak::NodeId> {
            let t = self.tree.borrow();
            let mut hits: Vec<ak::NodeId> = t
                .iter()
                .filter(|(_, n)| {
                    n.role() == role
                        && (n.label() == Some(name)
                            || (role == ak::Role::Label && n.value() == Some(name)))
                })
                .map(|(id, _)| *id)
                .collect();
            hits.sort();
            hits.into_iter().next()
        }

        pub fn node(&self, id: ak::NodeId) -> Option<ak::Node> {
            self.tree.borrow().get(&id).cloned()
        }

        pub fn focus(&self) -> Option<ak::NodeId> {
            self.focus.get()
        }

        /// Sends an action as the AT would (from another thread), then syncs.
        pub fn act(&self, target: ak::NodeId, action: ak::Action, data: Option<ak::ActionData>) {
            queue_action(ak::ActionRequest {
                action,
                target_tree: ak::TreeId::ROOT,
                target_node: target,
                data,
            });
            self.sync();
        }
    }
}
