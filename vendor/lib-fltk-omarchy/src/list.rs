//! Virtualized list in Omarchy's row style. Only visible rows are built,
//! so 1 000 000 rows cost the same per frame as 20.
//!
//! ```no_run
//! use fltk_omarchy::{list::{List, ListRow}, State};
//! let count = State::new(100_000usize);
//! let selected = State::new(None::<usize>);
//! let list = List::new(&count, |i| ListRow::new(&format!("Item {i}")).detail(&format!("{} KB", i * 3)))
//!     .bind_selected(&selected)
//!     .on_activate(|i| println!("open {i}"));
//! ```
//!
//! For data that changes often, show a [`ListModel`] instead of a count:
//! [`List::from_model`] redraws only the rows whose item changed, keeps the
//! scroll position when items are inserted above the viewport and keeps
//! the selection on its item when rows move (see [`crate::model`]).

use crate::model::{Changes, Invalidations, ListModel, RowSlots};
use crate::a11y::{self, Action, Role};
use crate::paint;
use crate::panel::ItemId;
use crate::state::State;
use crate::theme;
use crate::widgets::draw_row;
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::rc::Rc;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListRow {
    pub icon: String,
    pub label: String,
    pub detail: String,
}

impl ListRow {
    pub fn new(label: &str) -> ListRow {
        ListRow { label: label.into(), ..Default::default() }
    }
    pub fn icon(mut self, g: &str) -> Self { self.icon = g.into(); self }
    pub fn detail(mut self, d: &str) -> Self { self.detail = d.into(); self }
}

struct Inner {
    row: Box<dyn Fn(usize) -> ListRow>,
    count: usize,
    top: i64, // scroll offset in px
    motion: crate::scroll::Scroll,
    row_h: i32,
    hover: Option<usize>,
    selected: Option<usize>,
    fetched: usize,
    dragging_thumb: Option<(i32, i64)>,
    // Model-backed lists (from_model): stable IDs, the selected ID and the
    // IDs drawn per row slot, to redraw only rows that changed.
    ids: Option<IdSource>,
    sel_id: Option<ItemId>,
    slots: RowSlots,
    inval: Invalidations,
    // Model lists: rows arrive, leave, make room and fly (by item ID).
    rm: crate::rowmotion::RowMotion<ListRow>,
}

type IdAtFn = Box<dyn Fn(usize) -> Option<ItemId>>;
type IndexOfFn = Box<dyn Fn(ItemId) -> Option<usize>>;

struct IdSource {
    id_at: IdAtFn,
    index_of: IndexOfFn,
}

impl Inner {
    /// Sets the selected index and remembers its ID (model lists).
    fn set_selected(&mut self, idx: Option<usize>) {
        self.selected = idx;
        self.sel_id = idx.and_then(|x| self.ids.as_ref().and_then(|s| (s.id_at)(x)));
    }
}

#[derive(Clone)]
pub struct List {
    widget: Widget,
    inner: Rc<RefCell<Inner>>,
    select_cb: Rc<RefCell<Option<Box<dyn FnMut(usize)>>>>,
    activate_cb: Rc<RefCell<Option<Box<dyn FnMut(usize)>>>>,
    selected_state: Rc<RefCell<Option<State<Option<usize>>>>>,
    activations: Rc<Cell<usize>>,
}
fltk::widget_extends!(List, Widget, widget);

const BAR: i32 = 4;

impl List {
    pub fn new(count: &State<usize>, row: impl Fn(usize) -> ListRow + 'static) -> List {
        let list = List::build(count.get(), row);
        let (inn, mut w) = (list.inner.clone(), list.widget.clone());
        crate::state::bind_widget(&list.widget, count, false, move |n| {
            let mut i = inn.borrow_mut();
            i.count = *n;
            if i.selected.is_some_and(|s| s >= *n) { i.set_selected(None); }
            drop(i);
            w.redraw();
        });
        list
    }

    /// A list showing a [`ListModel`]: `row` builds the visible rows from
    /// their items. A change redraws only the visible rows whose item
    /// changed or moved; items inserted or removed above the viewport keep
    /// what is on screen (the view stays on its first visible item unless
    /// it is at the very top); the selection stays on its item.
    ///
    /// ```no_run
    /// # use fltk_omarchy::{list::{List, ListRow}, model::ListModel};
    /// let files = ListModel::new(|f: &(String, u64)| f.0.clone());
    /// let list = List::from_model(&files, |f| ListRow::new(&f.0).detail(&format!("{} B", f.1)));
    /// files.push(("notes.md".into(), 120));
    /// ```
    pub fn from_model<K, T>(model: &ListModel<K, T>, row: impl Fn(&T) -> ListRow + 'static) -> List
    where
        K: Clone + Eq + Hash + 'static,
        T: Clone + PartialEq + 'static,
    {
        let m = model.clone();
        let list = List::build(model.len(), move |i| m.with_at(i, &row).unwrap_or_default());
        let (a, b) = (model.clone(), model.clone());
        list.inner.borrow_mut().ids = Some(IdSource { id_at: Box::new(move |i| a.id_at(i)), index_of: Box::new(move |id| b.index_of(id)) });
        let weak = Rc::downgrade(&list.inner);
        let sel_state = Rc::downgrade(&list.selected_state);
        let mut w = list.widget.clone();
        model.listen(move |m, changes| {
            let Some(inn) = weak.upgrade().filter(|_| !w.was_deleted()) else { return false };
            let moved = apply_changes(&inn, &mut w, m.len(), changes);
            if let (Some(sel), Some(s)) = (moved, sel_state.upgrade()) {
                let st = s.borrow().clone();
                if let Some(st) = st { st.set(sel); }
            }
            true
        });
        list
    }

    fn build(count: usize, row: impl Fn(usize) -> ListRow + 'static) -> List {
        let mut widget = Widget::default().with_size(240, 200);
        widget.set_frame(FrameType::NoBox);
        widget.set_visible_focus();
        let row_h = theme::with(|t| t.font.body + 4 + 2 * t.spacing.xl);
        let inner = Rc::new(RefCell::new(Inner {
            row: Box::new(row), count, top: 0, motion: crate::scroll::Scroll::new(), row_h, hover: None, selected: None, fetched: 0, dragging_thumb: None,
            ids: None, sel_id: None, slots: RowSlots::default(), inval: Invalidations::default(),
            rm: Default::default(),
        }));
        let list = List {
            widget,
            inner,
            select_cb: Rc::default(),
            activate_cb: Rc::default(),
            selected_state: Rc::default(),
            activations: Rc::default(),
        };
        list.install();
        list.describe();
        list
    }

    /// Screen readers: a list box with the visible rows (and the selected
    /// one), each with its position in the whole list.
    fn describe(&self) {
        let (inn, me) = (self.inner.clone(), self.widget.clone());
        a11y::describe(&self.widget, move |n| {
            let Ok(i) = inn.try_borrow() else { return };
            n.role(Role::ListBox).focusable();
            let rh = i.row_h.max(1) as i64;
            let first = (i.top / rh).max(0) as usize;
            let last = ((i.top + me.h() as i64) / rh + 1).max(0) as usize;
            let mut rows: Vec<usize> = (first..=last.min(i.count.saturating_sub(1))).filter(|r| *r < i.count).collect();
            if let Some(s) = i.selected.filter(|s| *s < i.count && !rows.contains(s)) {
                rows.push(s);
                rows.sort_unstable();
            }
            for idx in rows {
                let r = (i.row)(idx);
                let y = me.y() + (idx as i64 * rh - i.top) as i32;
                n.child(idx as u64, |o| {
                    let m = crate::layout::Mirror::of(&me);
                    o.role(Role::ListBoxOption).name(&r.label).description(&r.detail).selected(i.selected == Some(idx))
                        .position(idx + 1, i.count).bounds(m.x(me.x(), me.w() - BAR - 4), y, me.w() - BAR - 4, i.row_h)
                        .action(Action::Click).action(Action::Focus).action(Action::ScrollIntoView);
                    if i.selected == Some(idx) {
                        o.focused();
                    }
                });
            }
        });
        let me = self.clone();
        a11y::on_action(&self.widget, move |r| {
            let Some(idx) = r.key().map(|k| k as usize).filter(|k| *k < me.count()) else { return false };
            let mut l = me.clone();
            match r.action {
                Action::Click | Action::Focus => {
                    if r.action == Action::Focus {
                        let _ = l.widget.take_focus();
                    }
                    l.select_visible(idx);
                }
                Action::ScrollIntoView => l.scroll_to(idx),
                _ => return false,
            }
            true
        });
    }

    /// Selects like a click (scrolls only as much as needed, fires on_select).
    fn select_visible(&mut self, idx: usize) {
        self.scroll_to(idx);
        self.inner.borrow_mut().selected = Some(idx);
        if let Some(s) = self.selected_state.borrow().as_ref() { s.set(Some(idx)); }
        if let Some(f) = self.select_cb.borrow_mut().as_mut() { f(idx); }
        self.widget.redraw();
    }

    fn scroll_to(&mut self, idx: usize) {
        let h = self.widget.h() as i64;
        let mut i = self.inner.borrow_mut();
        let rh = i.row_h as i64;
        let y = idx as i64 * rh;
        if y < i.top { i.top = y } else if y + rh > i.top + h { i.top = y + rh - h }
        let top = i.top;
        i.motion.jump(top);
        drop(i);
        self.widget.redraw();
    }

    fn install(&self) {
        let inn = self.inner.clone();
        let mut w = self.widget.clone();
        w.draw(move |w| {
            let mut i = inn.borrow_mut();
            // Right to left: rows mirrored, the scrollbar on the left.
            let m = crate::layout::Mirror::of(w);
            theme::with(|t| {
                let under = t.palette.background;
                fltk::draw::push_clip(w.x(), w.y(), w.w(), w.h());
                let max_top = (i.count as i64 * i.row_h as i64 - w.h() as i64).max(0);
                let top = i.top;
                i.top = i.motion.position(top, max_top);
                if i.motion.running() { crate::anim::request_redraw(w); }
                let first = (i.top / i.row_h as i64) as usize;
                let visible = (w.h() / i.row_h + 2) as usize;
                let text_w = w.w() - BAR - 4;
                let top = i.top;
                i.slots.begin(top, visible);
                // Row motion (model lists, full redraws): where each row in view is now.
                let full = w.damage_type().contains(Damage::All);
                let moving = full && i.ids.is_some();
                if moving {
                    let rows: Vec<(ItemId, i32)> = (0..visible)
                        .map(|k| first + k)
                        .filter(|idx| *idx < i.count)
                        .filter_map(|idx| {
                            let id = i.ids.as_ref().and_then(|s| (s.id_at)(idx))?;
                            Some((id, (idx as i64 * i.row_h as i64 - i.top) as i32))
                        })
                        .collect();
                    let inn = &mut *i;
                    let ids = inn.ids.as_ref();
                    inn.rm.begin(&rows, |id| ids.is_some_and(|s| (s.index_of)(id).is_some()));
                }
                let drift = (22.0 * t.font.base as f64 / 12.0).round();
                let mut drawn = Vec::new();
                let mut lifted = Vec::new();
                for k in 0..visible {
                    let idx = first + k;
                    if idx >= i.count { i.slots.drawn(k, None); continue; }
                    let base = w.y() + (idx as i64 * i.row_h as i64 - i.top) as i32;
                    let id = i.ids.as_ref().and_then(|s| (s.id_at)(idx));
                    let (dy, opacity, lift) = match (moving, id) { (true, Some(id)) => i.rm.at(id, drift), _ => (0, 1.0, 0.0) };
                    let ry = base + dy;
                    // Partial redraws (one changed row) build only the rows in the damaged area.
                    if !fltk::draw::not_clipped(w.x(), ry, w.w(), i.row_h) { continue; }
                    let r = (i.row)(idx);
                    i.slots.drawn(k, id);
                    i.fetched += 1;
                    if lift > 0.0 {
                        lifted.push((idx, ry, lift, r.clone()));
                    } else {
                        draw_row(t, m.x(w.x(), text_w), ry, text_w, i.row_h, &r.icon, &r.label, &r.detail, i.hover == Some(idx), i.selected == Some(idx), under, m.on);
                        if opacity < 1.0 {
                            paint::alpha_fill(m.x(w.x(), text_w), ry, text_w, i.row_h, under, 1.0 - opacity);
                        }
                    }
                    if let (true, Some(id)) = (moving && crate::anim::enabled(), id) {
                        drawn.push((id, ry - w.y(), r));
                    }
                }
                if moving {
                    for g in &i.rm.ghosts {
                        let (dy, o) = g.sample(drift);
                        let ry = w.y() + g.y + dy;
                        draw_row(t, m.x(w.x(), text_w), ry, text_w, i.row_h, &g.row.icon, &g.row.label, &g.row.detail, false, false, under, m.on);
                        paint::alpha_fill(m.x(w.x(), text_w), ry, text_w, i.row_h, under, 1.0 - o);
                    }
                    // Rows in flight on top: a lifted card (o3 T3RowFlight).
                    for (idx, ry, bow, r) in lifted {
                        let x = m.x(w.x(), text_w);
                        paint::alpha_fill(x + 2, ry + (5.0 * bow) as i32, text_w, i.row_h, theme::Rgb(0), 0.32 * bow);
                        let card = t.palette.foreground.over(under, 0.06);
                        paint::fill(x, ry, text_w, i.row_h, t.radius, card);
                        draw_row(t, x, ry, text_w, i.row_h, &r.icon, &r.label, &r.detail, false, i.selected == Some(idx), card, m.on);
                        paint::stroke(x, ry, text_w, i.row_h, t.radius, 1, t.palette.accent.over(card, 0.55 * bow));
                    }
                    i.rm.end(drawn);
                    if i.rm.running() { crate::anim::request_redraw(w); }
                }
                if max_top > 0 {
                    let track = w.h();
                    let thumb = ((w.h() as f64 / (i.count as f64 * i.row_h as f64)) * track as f64).max(24.0) as i32;
                    let ty = w.y() + ((i.top as f64 / max_top as f64) * (track - thumb) as f64) as i32;
                    paint::fill(m.x(w.x() + w.w() - BAR, BAR), ty, BAR, thumb, BAR / 2, t.palette.foreground.over(under, 0.25));
                }
                fltk::draw::pop_clip();
                if crate::focus::show_ring(w) {
                    // Ring on the cursor row, or around the list when there is none.
                    let ring = t.border(&t.controls.focus, under);
                    let bw = t.controls.focus.border_width.max(1);
                    let row = i.selected.map(|s| w.y() + (s as i64 * i.row_h as i64 - i.top) as i32)
                        .filter(|ry| *ry + i.row_h > w.y() && *ry < w.y() + w.h());
                    match row {
                        Some(ry) => {
                            fltk::draw::push_clip(w.x(), w.y(), w.w(), w.h());
                            paint::stroke(m.x(w.x(), text_w), ry, text_w, i.row_h, t.radius, bw, ring);
                            fltk::draw::pop_clip();
                        }
                        None => paint::stroke(w.x(), w.y(), w.w(), w.h(), t.radius, bw, ring),
                    }
                }
            });
        });

        let (inn, sel_cb, act_cb, sel_state, acts) =
            (self.inner.clone(), self.select_cb.clone(), self.activate_cb.clone(), self.selected_state.clone(), self.activations.clone());
        w.handle(move |w, ev| {
            let wy = w.y();
            let row_at = move |i: &Inner, ey: i32| -> Option<usize> {
                let idx = ((ey - wy) as i64 + i.top) / i.row_h as i64;
                (idx >= 0 && (idx as usize) < i.count).then_some(idx as usize)
            };
            let select = |idx: usize, w: &mut Widget| {
                {
                    let mut i = inn.borrow_mut();
                    i.set_selected(Some(idx));
                    let y = idx as i64 * i.row_h as i64;
                    if y < i.top { i.top = y } else if y + i.row_h as i64 > i.top + w.h() as i64 { i.top = y + i.row_h as i64 - w.h() as i64 }
                }
                if let Some(s) = sel_state.borrow().as_ref() { s.set(Some(idx)); }
                if let Some(f) = sel_cb.borrow_mut().as_mut() { f(idx); }
                w.redraw();
            };
            let activate = |idx: usize| {
                acts.set(acts.get() + 1);
                if let Some(f) = act_cb.borrow_mut().as_mut() { f(idx); }
            };
            match ev {
                Event::Enter => true,
                Event::Focus | Event::Unfocus => { w.redraw(); true }
                Event::Move => {
                    let h = row_at(&inn.borrow(), app::event_y());
                    if inn.borrow().hover != h { inn.borrow_mut().hover = h; w.redraw(); }
                    true
                }
                Event::Leave => { inn.borrow_mut().hover = None; w.redraw(); true }
                Event::Push => {
                    let _ = w.take_focus();
                    { let mut i = inn.borrow_mut(); let top = i.top; i.motion.jump(top); }
                    if crate::layout::Mirror::of(w).point(app::event_x()) >= w.x() + w.w() - BAR - 4 {
                        let top = inn.borrow().top;
                        inn.borrow_mut().dragging_thumb = Some((app::event_y(), top));
                        return true;
                    }
                    let idx = row_at(&inn.borrow(), app::event_y());
                    if let Some(idx) = idx {
                        select(idx, w);
                        if app::event_clicks() { activate(idx); }
                    }
                    true
                }
                Event::Drag => {
                    let d = inn.borrow().dragging_thumb;
                    if let Some((y0, top0)) = d {
                        let mut i = inn.borrow_mut();
                        let content = i.count as f64 * i.row_h as f64;
                        i.top = top0 + ((app::event_y() - y0) as f64 * content / w.h() as f64) as i64;
                        drop(i);
                        w.redraw();
                    }
                    true
                }
                Event::Released => { inn.borrow_mut().dragging_thumb = None; true }
                Event::MouseWheel => {
                    let mut i = inn.borrow_mut();
                    let max = (i.count as i64 * i.row_h as i64 - w.h() as i64).max(0);
                    let (top, delta) = (i.top, app::event_dy_value() as f64 * 3.0 * i.row_h as f64);
                    i.top = i.motion.wheel(top, delta, max);
                    drop(i);
                    w.redraw();
                    true
                }
                Event::KeyDown => {
                    { let mut i = inn.borrow_mut(); let top = i.top; i.motion.jump(top); }
                    let (count, cur, page) = { let i = inn.borrow(); (i.count, i.selected, (w.h() / i.row_h).max(1) as usize) };
                    if count == 0 { return false; }
                    let k = app::event_key();
                    let next = match k {
                        Key::Down => Some(cur.map_or(0, |c| (c + 1).min(count - 1))),
                        Key::Up => Some(cur.map_or(0, |c| c.saturating_sub(1))),
                        Key::PageDown => Some(cur.map_or(0, |c| (c + page).min(count - 1))),
                        Key::PageUp => Some(cur.map_or(0, |c| c.saturating_sub(page))),
                        Key::Home => Some(0),
                        Key::End => Some(count - 1),
                        _ if k == Key::from_char('j') => Some(cur.map_or(0, |c| (c + 1).min(count - 1))),
                        _ if k == Key::from_char('k') => Some(cur.map_or(0, |c| c.saturating_sub(1))),
                        Key::Enter | Key::KPEnter => { if let Some(c) = cur { activate(c); } return true; }
                        _ => None,
                    };
                    if let Some(n) = next { select(n, w); return true; }
                    false
                }
                _ => false,
            }
        });
    }

    pub fn row_height(self, px: i32) -> Self {
        self.inner.borrow_mut().row_h = px.max(8);
        self
    }

    pub fn on_select(self, f: impl FnMut(usize) + 'static) -> Self {
        *self.select_cb.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Double click or Enter.
    pub fn on_activate(self, f: impl FnMut(usize) + 'static) -> Self {
        *self.activate_cb.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Two-way selection binding.
    pub fn bind_selected(self, s: &State<Option<usize>>) -> Self {
        *self.selected_state.borrow_mut() = Some(s.clone());
        let (inn, mut w) = (self.inner.clone(), self.widget.clone());
        crate::state::bind_widget(&self.widget, s, true, move |v| {
            let mut i = inn.borrow_mut();
            if i.selected == *v { return; } // a model moved the selected item
            i.set_selected(*v);
            drop(i);
            w.redraw();
        });
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.inner.borrow().selected
    }

    /// Selects and scrolls to `idx` (fires on_select).
    pub fn select(&mut self, idx: usize) {
        let row_h = self.inner.borrow().row_h as i64;
        {
            let mut i = self.inner.borrow_mut();
            if idx >= i.count { return; }
            i.set_selected(Some(idx));
            i.top = idx as i64 * row_h - (self.widget.h() as i64 - row_h) / 2;
        }
        if let Some(s) = self.selected_state.borrow().as_ref() { s.set(Some(idx)); }
        if let Some(f) = self.select_cb.borrow_mut().as_mut() { f(idx); }
        self.widget.redraw();
    }

    pub fn activate_selected(&self) {
        if let Some(i) = self.selected() {
            self.activations.set(self.activations.get() + 1);
            if let Some(f) = self.activate_cb.borrow_mut().as_mut() { f(i); }
        }
    }

    /// Total rows built for drawing so far (proves virtualization in tests).
    pub fn rows_built(&self) -> usize {
        self.inner.borrow().fetched
    }

    pub fn count(&self) -> usize {
        self.inner.borrow().count
    }

    /// Stable ID of the selected item (model lists, see [`List::from_model`]).
    pub fn selected_id(&self) -> Option<ItemId> {
        self.inner.borrow().sel_id
    }

    /// Scroll offset in pixels.
    pub fn scroll_offset(&self) -> i64 {
        self.inner.borrow().top
    }

    /// Scrolls to an offset in pixels (clamped when drawn).
    pub fn set_scroll_offset(&mut self, px: i64) {
        {
            let mut i = self.inner.borrow_mut();
            i.top = px.max(0);
            let top = i.top;
            i.motion.jump(top);
        }
        self.widget.redraw();
    }

    /// How model changes were redrawn so far (model lists; tests, benchmarks).
    pub fn invalidations(&self) -> Invalidations {
        self.inner.borrow().inval
    }
}

/// Model changes on a list: anchor the scroll offset, follow the selected
/// ID, and invalidate only the row slots that now show something else.
/// Returns the new selected index when it moved.
fn apply_changes(inn: &Rc<RefCell<Inner>>, w: &mut Widget, len: usize, changes: &Changes) -> Option<Option<usize>> {
    let mut guard = inn.borrow_mut();
    let i = &mut *guard;
    let ids = i.ids.as_ref()?;
    let old_top = i.top;
    let r = crate::model::refit(&mut i.slots, old_top, i.row_h, w.h(), len, changes, &*ids.id_at, &*ids.index_of);
    let rh = i.row_h.max(1) as i64;
    if r.top != old_top {
        // The pointer did not move: the hovered row is the one now under it.
        i.hover = i.hover.and_then(|h| usize::try_from(h as i64 + (r.top - old_top) / rh).ok()).filter(|h| *h < len);
        i.motion.shift(r.top - old_top);
        i.top = r.top;
    }
    i.count = len;
    let structural = changes.is_structural() || !changes.removed().is_empty();
    if structural {
        let flown: Vec<ItemId> = changes.ops().iter().filter_map(|op| match op {
            crate::model::Change::Moved { to, .. } => (ids.id_at)(*to),
            _ => None,
        }).collect();
        i.rm.changed(changes.inserted(), &flown);
    }
    let mut moved = None;
    if changes.is_structural() || !changes.removed().is_empty() {
        let now = i.sel_id.and_then(|id| (ids.index_of)(id));
        if now.is_none() {
            i.sel_id = None;
        }
        if now != i.selected {
            i.selected = now;
            moved = Some(now);
        }
    }
    // Rows that move are drawn in full while they move.
    if r.full || (structural && crate::anim::enabled()) {
        i.inval.full += 1;
        w.redraw();
        return moved;
    }
    if r.slots.is_empty() {
        i.inval.none += 1;
    }
    i.inval.rows += r.slots.len();
    let off = (r.top.rem_euclid(rh)) as i32;
    let (y0, y1) = (w.y(), w.y() + w.h());
    for k in r.slots {
        let y = (w.y() + k as i32 * i.row_h - off).max(y0);
        let bottom = (w.y() + (k as i32 + 1) * i.row_h - off).min(y1);
        if bottom > y {
            w.set_damage_area(Damage::User1, w.x(), y, w.w(), bottom - y);
        }
    }
    if r.bar {
        let x = crate::layout::Mirror::of(&*w).x(w.x() + w.w() - BAR, BAR);
        w.set_damage_area(Damage::User1, x, w.y(), BAR, w.h());
    }
    moved
}
