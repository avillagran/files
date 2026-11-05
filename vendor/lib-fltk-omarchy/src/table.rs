//! Virtual table and tree in Omarchy's row style, on top of a
//! [`KeyboardPanel`]: only visible rows are drawn, the cursor and the
//! multi-selection are kept by stable IDs, and every key of the panel works
//! (Ctrl/Shift ranges, PageUp/PageDown, Home/End, Ctrl+A, Enter).
//!
//! ```no_run
//! use fltk_omarchy::{panel::KeyboardPanel, table::{Column, Table}};
//! let files = vec![("Cargo.toml", 1_204u64), ("README.md", 2_048)];
//! let panel = KeyboardPanel::new("files");
//! let n = files.len();
//! panel.set_items(n, |i| i as u64, move |id| (id < n as u64).then_some(id as usize));
//! let table = Table::new(&panel, vec![Column::new("Name", 0).flex(), Column::new("Size", 90).right()],
//!     move |row, col| if col == 0 { files[row].0.to_string() } else { format!("{} B", files[row].1) })
//!     .on_sort(|col, ascending| println!("sort by {col}, ascending: {ascending}"));
//! ```
//!
//! With a [`ListModel`], [`Table::from_model`] and [`Tree::from_model`]
//! bind the panel to the model and redraw only the rows that changed (see
//! [`crate::model`]).

use crate::model::{Changes, Invalidations, ListModel, RowSlots};
use crate::a11y::{self, Action, Role};
use crate::panel::{ItemId, KeyboardPanel};
use crate::paint;
use crate::theme;
use crate::widgets::glyph;
use fltk::{app, draw, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::{Rc, Weak};

const BAR: i32 = 4;

fn row_height() -> i32 {
    theme::with(|t| t.font.body + 4 + 2 * t.spacing.lg)
}

/// Row background: selected fill, cursor (hover-cursor fill, ring while the
/// keyboard is in use), or hover.
fn draw_row_bg(t: &theme::Theme, x: i32, y: i32, w: i32, h: i32, selected: bool, cursor: bool, hot: bool, ring: bool) {
    let under = t.palette.background;
    let c = &t.controls;
    if selected {
        paint::fill(x, y, w, h, t.radius, t.fill(&c.selected, under));
    }
    if (cursor && ring) || hot {
        let base = if selected { t.fill(&c.selected, under) } else { under };
        paint::fill(x, y, w, h, t.radius, t.fill(&c.hover, base));
    }
    if cursor && ring {
        paint::stroke(x, y, w, h, t.radius, c.focus.border_width.max(1), t.border(&c.focus, under));
    }
}

fn scrollbar(t: &theme::Theme, x: i32, y: i32, h: i32, top: i64, content: i64) {
    if content <= h as i64 || h <= 0 {
        return;
    }
    let max = content - h as i64;
    let thumb = ((h as f64 / content as f64) * h as f64).max(24.0) as i32;
    let ty = y + ((top as f64 / max as f64) * (h - thumb) as f64) as i32;
    paint::fill(x, ty, BAR, thumb, BAR / 2, t.palette.foreground.over(t.palette.background, 0.25));
}

/// Key of a table's header row among its accessible rows (item IDs are the
/// other keys; an item with this ID would hide the header).
const HEADER_KEY: u64 = u64::MAX - 7;

fn scroll_into_view(top: &mut i64, index: usize, row_h: i32, view_h: i32) {
    let y = index as i64 * row_h as i64;
    if y < *top {
        *top = y;
    } else if y + row_h as i64 > *top + view_h as i64 {
        *top = y + row_h as i64 - view_h as i64;
    }
}

/// Keeps the cursor row in view when it moves.
fn follow_cursor(panel: &KeyboardPanel, last: &mut Option<ItemId>, top: &mut i64, row_h: i32, view_h: i32) {
    let cur = panel.cursor();
    if cur == *last {
        return;
    }
    *last = cur;
    if let Some(i) = panel.cursor_index() {
        let y = i as i64 * row_h as i64;
        if y < *top {
            *top = y;
        } else if y + row_h as i64 > *top + view_h as i64 {
            *top = y + row_h as i64 - view_h as i64;
        }
    }
}

// =================================================================== Table

/// A table column: `Column::new("Name", 0).flex()`, `Column::new("Size", 90).right()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub title: String,
    /// Width in pixels (ignored for flex columns).
    pub width: i32,
    pub flex: bool,
    pub right: bool,
}

impl Column {
    pub fn new(title: &str, width: i32) -> Column {
        Column { title: title.into(), width: width.max(24), flex: false, right: false }
    }
    /// Takes the remaining width (shared between flex columns).
    pub fn flex(mut self) -> Self {
        self.flex = true;
        self
    }
    /// Right-aligned cells (numbers, sizes).
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }
}

type CellFn = Box<dyn Fn(usize, usize) -> String>;

struct TState {
    cols: Vec<Column>,
    cell: CellFn,
    icon: Option<Box<dyn Fn(usize) -> String>>,
    panel: KeyboardPanel,
    top: i64,
    motion: crate::scroll::Scroll,
    hover: Option<usize>,
    sort: Option<(usize, bool)>,
    on_sort: Option<Box<dyn FnMut(usize, bool)>>,
    resizing: Option<(usize, i32, i32)>,
    last_cursor: Option<ItemId>,
    drawn: usize,
    slots: RowSlots,
    inval: Invalidations,
}

/// Virtual table: header (click to sort, drag a column edge to resize) and
/// rows from `cell(row, column)`, with the panel's cursor and selection.
#[derive(Clone)]
pub struct Table {
    widget: Widget,
    st: Rc<RefCell<TState>>,
}
fltk::widget_extends!(Table, Widget, widget);

impl Table {
    /// `cell(row, column)` gives the text of a cell; `row` is the display
    /// index of the panel's items (`panel.id_at(row)` is its ID).
    pub fn new(panel: &KeyboardPanel, columns: Vec<Column>, cell: impl Fn(usize, usize) -> String + 'static) -> Table {
        let mut widget = Widget::default().with_size(480, 260);
        widget.set_frame(FrameType::NoBox);
        widget.set_visible_focus();
        let st = Rc::new(RefCell::new(TState {
            cols: columns,
            cell: Box::new(cell),
            icon: None,
            panel: panel.clone(),
            top: 0,
            motion: crate::scroll::Scroll::new(),
            hover: None,
            sort: None,
            on_sort: None,
            resizing: None,
            last_cursor: None,
            drawn: 0,
            slots: RowSlots::default(),
            inval: Invalidations::default(),
        }));
        let s2 = st.clone();
        widget.draw(move |w| table_draw(w, &s2));
        let s3 = st.clone();
        widget.handle(move |w, ev| table_handle(w, ev, &s3));
        crate::focus::host(&widget, &[panel]);
        let t = Table { widget, st };
        t.describe();
        t
    }

    /// Screen readers: a grid with a header row and the rows in view (plus
    /// the cursor row), each with its row index in the whole table.
    fn describe(&self) {
        let (st, me) = (self.st.clone(), self.widget.clone());
        a11y::describe(&self.widget, move |n| {
            let Ok(s) = st.try_borrow() else { return };
            let (rh, hh) = (row_height(), header_h());
            let len = s.panel.len();
            n.role(Role::Grid).counts(len + 1, s.cols.len()).multiselectable(true).focusable();
            let cols = col_layout(&s.cols, me.w() - BAR - 4);
            let m = crate::layout::Mirror::of(&me);
            n.child(HEADER_KEY, |h| {
                h.role(Role::Row).row_index(0).bounds(me.x(), me.y(), me.w(), hh);
                for (ci, (c, (cx, cw))) in s.cols.iter().zip(cols.iter()).enumerate() {
                    h.child(ci as u64, |cell| {
                        cell.role(Role::ColumnHeader).name(&c.title).column_index(ci).bounds(m.x(me.x() + cx, *cw), me.y(), *cw, hh).action(Action::Click);
                        if let Some((_, asc)) = s.sort.filter(|(sc, _)| *sc == ci) {
                            cell.sort(asc);
                        }
                    });
                }
            });
            let body_h = me.h() - hh;
            let first = (s.top / rh as i64).max(0) as usize;
            let mut rows: Vec<usize> = (first..(first + (body_h / rh + 2).max(0) as usize).min(len)).collect();
            if let Some(c) = s.panel.cursor_index().filter(|c| !rows.contains(c)) {
                rows.push(c);
                rows.sort_unstable();
            }
            let cursor = s.panel.cursor();
            for idx in rows {
                let Some(id) = s.panel.id_at(idx) else { continue };
                let ry = me.y() + hh + (idx as i64 * rh as i64 - s.top) as i32;
                n.child(id, |r| {
                    r.role(Role::Row).row_index(idx + 1).name(&(s.cell)(idx, 0)).selected(s.panel.is_selected(id)).bounds(m.x(me.x(), me.w() - BAR - 4), ry, me.w() - BAR - 4, rh)
                        .action(Action::Click).action(Action::Focus).action(Action::ScrollIntoView);
                    for (ci, (cx, cw)) in cols.iter().enumerate().take(s.cols.len()) {
                        r.child(ci as u64, |cell| {
                            cell.role(Role::Cell).name(&(s.cell)(idx, ci)).column_index(ci).bounds(m.x(me.x() + cx, *cw), ry, *cw, rh);
                        });
                    }
                    if cursor == Some(id) {
                        r.focused();
                    }
                });
            }
        });
        let (st, mut me) = (self.st.clone(), self.widget.clone());
        a11y::on_action(&self.widget, move |r| {
            let panel = st.borrow().panel.clone();
            match (r.path.as_slice(), r.action) {
                ([HEADER_KEY, ci], Action::Click) => {
                    let ci = *ci as usize;
                    if ci >= st.borrow().cols.len() { return false; }
                    let asc = match st.borrow().sort { Some((c, a)) if c == ci => !a, _ => true };
                    st.borrow_mut().sort = Some((ci, asc));
                    let f = st.borrow_mut().on_sort.take();
                    if let Some(mut f) = f {
                        f(ci, asc);
                        let mut s = st.borrow_mut();
                        if s.on_sort.is_none() { s.on_sort = Some(f); }
                    }
                }
                ([id, ..], Action::Click) => panel.click(*id, false, false),
                ([id, ..], Action::Focus) => { panel.focus(); panel.set_cursor(Some(*id)); }
                ([id, ..], Action::ScrollIntoView) => {
                    let Some(i) = panel.index_of(*id) else { return false };
                    let mut s = st.borrow_mut();
                    scroll_into_view(&mut s.top, i, row_height(), me.h() - header_h());
                    let top = s.top;
                    s.motion.jump(top);
                }
                _ => return false,
            }
            me.redraw();
            true
        });
    }

    /// A glyph for the first column of each row.
    pub fn icon(self, f: impl Fn(usize) -> String + 'static) -> Self {
        self.st.borrow_mut().icon = Some(Box::new(f));
        self
    }

    /// Called when a header is clicked: `(column, ascending)`. Sort your
    /// data and call `panel.set_items` again; the selection follows the IDs.
    pub fn on_sort(self, f: impl FnMut(usize, bool) + 'static) -> Self {
        self.st.borrow_mut().on_sort = Some(Box::new(f));
        self
    }

    /// Current sort (column, ascending), as shown in the header.
    pub fn sort(&self) -> Option<(usize, bool)> {
        self.st.borrow().sort
    }

    /// Sets the sort shown in the header (without calling `on_sort`).
    pub fn set_sort(&self, s: Option<(usize, bool)>) {
        self.st.borrow_mut().sort = s;
        self.widget.clone().redraw();
    }

    /// Renames the columns (in order; extra titles are ignored), e.g. from
    /// `i18n::bind` so the header follows the language.
    pub fn set_titles(&self, titles: &[&str]) {
        for (c, t) in self.st.borrow_mut().cols.iter_mut().zip(titles) {
            c.title = t.to_string();
        }
        self.widget.clone().redraw();
    }

    pub fn column_widths(&self) -> Vec<i32> {
        let s = self.st.borrow();
        col_layout(&s.cols, self.widget.w() - BAR - 4).into_iter().map(|(_, w)| w).collect()
    }

    /// Rows drawn so far (proves virtualization in tests).
    pub fn rows_drawn(&self) -> usize {
        self.st.borrow().drawn
    }

    pub fn scroll_offset(&self) -> i64 {
        self.st.borrow().top
    }

    /// Scrolls to an offset in pixels (clamped when drawn).
    pub fn set_scroll_offset(&self, px: i64) {
        let mut s = self.st.borrow_mut();
        s.top = px.max(0);
        let top = s.top;
        s.motion.jump(top);
        drop(s);
        self.widget.clone().redraw();
    }

    /// A table showing a [`ListModel`]: binds `panel` to it (cursor and
    /// selection by ID) and builds cells from items with `cell(item,
    /// column)`. A change redraws only the visible rows whose item changed
    /// or moved; inserts above the viewport keep what is on screen.
    pub fn from_model<K, T>(panel: &KeyboardPanel, model: &ListModel<K, T>, columns: Vec<Column>, cell: impl Fn(&T, usize) -> String + 'static) -> Table
    where
        K: Clone + Eq + Hash + 'static,
        T: Clone + PartialEq + 'static,
    {
        model.bind_panel(panel);
        let m = model.clone();
        let table = Table::new(panel, columns, move |row, col| m.with_at(row, |t| cell(t, col)).unwrap_or_default());
        let weak = Rc::downgrade(&table.st);
        let mut w = table.widget.clone();
        model.listen(move |m, changes| {
            let Some(st) = weak.upgrade().filter(|_| !w.was_deleted()) else { return false };
            let (a, b) = (m.clone(), m.clone());
            let hh = header_h();
            let mut s = st.borrow_mut();
            let s = &mut *s;
            refit_rows(&mut w, hh, &mut s.slots, &mut s.top, &mut s.motion, &mut s.hover, &mut s.inval, m.len(), changes, &move |i| a.id_at(i), &move |id| b.index_of(id));
            true
        });
        table
    }

    /// How model changes were redrawn so far (model tables; tests, benchmarks).
    pub fn invalidations(&self) -> Invalidations {
        self.st.borrow().inval
    }
}

/// Shared by model-backed Table and List-like views: anchors `top`, moves
/// the hover with the content and invalidates only the changed row slots
/// of the body (which starts `body_y` px below the widget top).
#[allow(clippy::too_many_arguments)]
fn refit_rows(
    w: &mut Widget,
    body_y: i32,
    slots: &mut RowSlots,
    top: &mut i64,
    motion: &mut crate::scroll::Scroll,
    hover: &mut Option<usize>,
    inval: &mut Invalidations,
    len: usize,
    changes: &Changes,
    id_at: &dyn Fn(usize) -> Option<ItemId>,
    index_of: &dyn Fn(ItemId) -> Option<usize>,
) {
    let rh = row_height();
    let body_h = w.h() - body_y;
    let old = *top;
    let r = crate::model::refit(slots, old, rh, body_h, len, changes, id_at, index_of);
    if r.top != old {
        *hover = hover.and_then(|h| usize::try_from(h as i64 + (r.top - old) / rh as i64).ok()).filter(|h| *h < len);
        motion.shift(r.top - old);
        *top = r.top;
    }
    if r.full {
        inval.full += 1;
        w.redraw();
        return;
    }
    if r.slots.is_empty() {
        inval.none += 1;
    }
    inval.rows += r.slots.len();
    let off = r.top.rem_euclid(rh as i64) as i32;
    let (y0, y1) = (w.y() + body_y, w.y() + w.h());
    for k in r.slots {
        let y = (y0 + k as i32 * rh - off).max(y0);
        let bottom = (y0 + (k as i32 + 1) * rh - off).min(y1);
        if bottom > y {
            w.set_damage_area(Damage::User1, w.x(), y, w.w(), bottom - y);
        }
    }
    if r.bar {
        let x = crate::layout::Mirror::of(&*w).x(w.x() + w.w() - BAR, BAR);
        w.set_damage_area(Damage::User1, x, y0, BAR, body_h);
    }
}

/// (x offset, width) of each column for a total width.
fn col_layout(cols: &[Column], total: i32) -> Vec<(i32, i32)> {
    let fixed: i32 = cols.iter().filter(|c| !c.flex).map(|c| c.width).sum();
    let nflex = cols.iter().filter(|c| c.flex).count() as i32;
    let flex_w = if nflex > 0 { ((total - fixed) / nflex).max(40) } else { 0 };
    let mut x = 0;
    cols.iter().map(|c| {
        let w = if c.flex { flex_w } else { c.width };
        let r = (x, w);
        x += w;
        r
    }).collect()
}

fn header_h() -> i32 {
    theme::with(|t| t.font.caption + 2 * t.spacing.lg)
}

fn table_draw(w: &mut Widget, st: &Rc<RefCell<TState>>) {
    let rh = row_height();
    let hh = header_h();
    let mut guard = st.borrow_mut();
    let s = &mut *guard;
    let body_h = w.h() - hh;
    s.panel.set_page_rows((body_h / rh).max(1) as usize);
    follow_cursor(&s.panel, &mut s.last_cursor, &mut s.top, rh, body_h);
    let n = s.panel.len();
    let content = n as i64 * rh as i64;
    s.top = s.motion.position(s.top, (content - body_h as i64).max(0));
    if s.motion.running() { crate::anim::request_redraw(w); }
    let ring = s.panel.show_ring();
    let cols = col_layout(&s.cols, w.w() - BAR - 4);
    // Right to left: the first column on the right, cells aligned to their
    // mirrored side, the scrollbar on the left.
    let m = crate::layout::Mirror::of(w);
    theme::with(|t| {
        let fg = t.palette.foreground;
        let under = t.palette.background;
        let pad = t.spacing.md + t.spacing.sm;
        // Header.
        for (i, (c, (cx, cw))) in s.cols.iter().zip(cols.iter()).enumerate() {
            let x = w.x() + cx;
            let align = m.align(if c.right { Align::Right } else { Align::Left });
            let sorted = s.sort.filter(|(sc, _)| *sc == i).map(|(_, asc)| asc);
            let arrow = if sorted.is_some() { t.font.caption + t.spacing.sm } else { 0 };
            // Sorted column: a chevron next to the title (up = ascending).
            let (tx, ax) = if c.right { (x + pad + arrow, x + pad) } else { (x + pad, x + cw - pad - arrow) };
            let tw = cw - 2 * pad - arrow;
            paint::text_elided(&c.title, m.x(tx, tw), w.y(), tw, hh, t.font.caption, true, fg.darker(1.4), align);
            if let Some(asc) = sorted {
                paint::glyph(if asc { glyph::CHEVRON_UP } else { glyph::CHEVRON_DOWN }, m.x(ax, arrow), w.y(), arrow, hh, t.font.caption, t.palette.accent);
            }
            if i + 1 < cols.len() {
                paint::fill(m.x(x + cw - 1, 1), w.y() + hh / 4, 1, hh / 2, 0, fg.over(under, 0.15));
            }
        }
        paint::hairline(w.x(), w.y() + hh - 1, w.w(), fg, under, 0.12);
        // Rows.
        let by = w.y() + hh;
        draw::push_clip(w.x(), by, w.w(), body_h);
        let first = (s.top / rh as i64) as usize;
        let visible = (body_h / rh + 2) as usize;
        let cursor = s.panel.cursor();
        s.slots.begin(s.top, visible);
        for k in 0..visible {
            let idx = first + k;
            if idx >= n { s.slots.drawn(k, None); continue; }
            let Some(id) = s.panel.id_at(idx) else { break };
            let ry = by + (idx as i64 * rh as i64 - s.top) as i32;
            // Partial redraws (one changed row) draw only the rows in the damaged area.
            if !draw::not_clipped(w.x(), ry, w.w(), rh) { continue; }
            s.slots.drawn(k, Some(id));
            let rw = w.w() - BAR - 4;
            draw_row_bg(t, m.x(w.x(), rw), ry, rw, rh, s.panel.is_selected(id), cursor == Some(id), s.hover == Some(idx), ring);
            for (ci, (c, (cx, cw))) in s.cols.iter().zip(cols.iter()).enumerate() {
                let mut tx = w.x() + cx + pad;
                let mut tw = cw - 2 * pad;
                if ci == 0 {
                    if let Some(icon) = s.icon.as_ref() {
                        let g = icon(idx);
                        if !g.is_empty() {
                            let cell = (22.0 * t.font.base as f64 / 12.0).round() as i32;
                            paint::glyph(&g, m.x(tx, cell), ry, cell, rh, t.font.title, fg);
                            tx += cell + t.spacing.md;
                            tw -= cell + t.spacing.md;
                        }
                    }
                }
                let text = (s.cell)(idx, ci);
                let color = if ci == 0 { fg } else { fg.darker(1.25) };
                paint::text_elided(&text, m.x(tx, tw.max(0)), ry, tw.max(0), rh, t.font.body, false, color, m.align(if c.right { Align::Right } else { Align::Left }));
            }
            s.drawn += 1;
        }
        draw::pop_clip();
        scrollbar(t, m.x(w.x() + w.w() - BAR, BAR), by, body_h, s.top, content);
        if crate::focus::show_ring(w) && cursor.is_none() {
            paint::stroke(w.x(), by, w.w(), body_h, t.radius, 1, t.border(&t.controls.focus, under));
        }
    });
}

fn table_handle(w: &mut Widget, ev: Event, st: &Rc<RefCell<TState>>) -> bool {
    let rh = row_height();
    let hh = header_h();
    // Mirrored tables are hit-tested in their left-to-right geometry.
    let m = crate::layout::Mirror::of(w);
    let (ex, ey) = (m.point(app::event_x()) - w.x(), app::event_y() - w.y());
    let row_at = |s: &TState| -> Option<usize> {
        if ey < hh { return None; }
        let i = ((ey - hh) as i64 + s.top) / rh as i64;
        (i >= 0 && (i as usize) < s.panel.len()).then_some(i as usize)
    };
    if matches!(ev, Event::Push | Event::KeyDown) {
        let mut s = st.borrow_mut(); let top = s.top; s.motion.jump(top);
    }
    match ev {
        Event::Focus | Event::Unfocus => { w.redraw(); true }
        Event::Enter => true,
        Event::Move => {
            let h = row_at(&st.borrow());
            if st.borrow().hover != h { st.borrow_mut().hover = h; w.redraw(); }
            true
        }
        Event::Leave => { st.borrow_mut().hover = None; w.redraw(); true }
        Event::MouseWheel => {
            let mut s = st.borrow_mut();
            let max = (s.panel.len() as i64 * rh as i64 - (w.h() - header_h()) as i64).max(0);
            let top = s.top;
            s.top = s.motion.wheel(top, app::event_dy_value() as f64 * 3.0 * rh as f64, max);
            drop(s);
            w.redraw();
            true
        }
        Event::Push => {
            let _ = w.take_focus();
            let cols = col_layout(&st.borrow().cols, w.w() - BAR - 4);
            if ey < hh {
                // Column edge: resize; title: sort.
                if let Some(i) = cols.iter().position(|(x, cw)| (ex - (x + cw)).abs() <= 4) {
                    if !st.borrow().cols[i].flex {
                        st.borrow_mut().resizing = Some((i, m.point(app::event_x()), cols[i].1));
                        return true;
                    }
                }
                if let Some(i) = cols.iter().position(|(x, cw)| ex >= *x && ex < x + cw) {
                    let asc = match st.borrow().sort { Some((c, a)) if c == i => !a, _ => true };
                    st.borrow_mut().sort = Some((i, asc));
                    let f = st.borrow_mut().on_sort.take();
                    if let Some(mut f) = f {
                        f(i, asc);
                        let mut s = st.borrow_mut();
                        if s.on_sort.is_none() { s.on_sort = Some(f); }
                    }
                    w.redraw();
                }
                return true;
            }
            let (row, panel) = { let s = st.borrow(); (row_at(&s), s.panel.clone()) };
            if let Some(id) = row.and_then(|r| panel.id_at(r)) {
                panel.click(id, crate::keys::primary_held(), app::is_event_shift());
                if app::event_clicks() {
                    panel.activate(id);
                }
            }
            true
        }
        Event::Drag => {
            let r = st.borrow().resizing;
            if let Some((i, x0, w0)) = r {
                st.borrow_mut().cols[i].width = (w0 + m.point(app::event_x()) - x0).max(40);
                w.redraw();
            }
            true
        }
        Event::Released => { st.borrow_mut().resizing = None; true }
        _ => false,
    }
}

// ==================================================================== Tree

/// A node of a [`Tree`].
#[derive(Debug, Clone, PartialEq)]
pub struct TreeNode {
    pub id: ItemId,
    pub label: String,
    pub icon: String,
    pub detail: String,
    /// Shows the expander; children are asked for only when expanded.
    pub has_children: bool,
}

impl TreeNode {
    pub fn new(id: ItemId, label: &str) -> TreeNode {
        TreeNode { id, label: label.into(), icon: String::new(), detail: String::new(), has_children: false }
    }
    pub fn icon(mut self, g: &str) -> Self { self.icon = g.into(); self }
    pub fn detail(mut self, d: &str) -> Self { self.detail = d.into(); self }
    pub fn branch(mut self) -> Self { self.has_children = true; self }
}

#[derive(Clone)]
struct Visible {
    depth: u16,
    node: TreeNode,
    parent: Option<ItemId>,
}

type ChildrenFn = Box<dyn Fn(Option<ItemId>) -> Vec<TreeNode>>;

/// Parent/children index of a model-backed tree (rebuilt on structural changes).
#[derive(Default)]
struct TreeIndex {
    children: HashMap<Option<ItemId>, Vec<ItemId>>,
    parent: HashMap<ItemId, Option<ItemId>>,
}

struct TrState {
    panel: KeyboardPanel,
    children: ChildrenFn,
    rows: Rc<RefCell<Vec<Visible>>>,
    index: Rc<RefCell<HashMap<ItemId, usize>>>,
    expanded: HashSet<ItemId>,
    /// Children already asked for (None = roots); see `refresh_node`.
    cache: HashMap<Option<ItemId>, Vec<TreeNode>>,
    top: i64,
    motion: crate::scroll::Scroll,
    hover: Option<usize>,
    last_cursor: Option<ItemId>,
    drawn: usize,
    slots: RowSlots,
    inval: Invalidations,
    // Branches opening and closing: rows below slide, new rows arrive.
    rm: crate::rowmotion::RowMotion<Visible>,
}

/// Virtual tree: `children(None)` gives the roots, `children(Some(id))`
/// the children of a node, asked for only when it is expanded. Keys: the
/// panel's, plus Right (expand, then first child) and Left (collapse, then
/// parent). Click the chevron to expand or collapse.
#[derive(Clone)]
pub struct Tree {
    widget: Widget,
    st: Rc<RefCell<TrState>>,
}
fltk::widget_extends!(Tree, Widget, widget);

impl Tree {
    pub fn new(panel: &KeyboardPanel, children: impl Fn(Option<ItemId>) -> Vec<TreeNode> + 'static) -> Tree {
        let mut widget = Widget::default().with_size(320, 260);
        widget.set_frame(FrameType::NoBox);
        widget.set_visible_focus();
        let st = Rc::new(RefCell::new(TrState {
            panel: panel.clone(),
            children: Box::new(children),
            rows: Rc::default(),
            index: Rc::default(),
            expanded: HashSet::new(),
            cache: HashMap::new(),
            top: 0,
            motion: crate::scroll::Scroll::new(),
            hover: None,
            last_cursor: None,
            drawn: 0,
            slots: RowSlots::default(),
            inval: Invalidations::default(),
            rm: Default::default(),
        }));
        let tree = Tree { widget, st };
        tree.refresh();
        // Left/Right: expand, collapse and move between parent and child;
        // the app's own on_edge still runs when the tree does not use the key.
        let prev = RefCell::new(panel.replace_edge(None));
        let weak: Weak<RefCell<TrState>> = Rc::downgrade(&tree.st);
        let mut redraw = tree.widget.clone();
        panel.on_edge(move |k| {
            let Some(st) = weak.upgrade() else { return false };
            let me = Tree { widget: redraw.clone(), st };
            let used = me.edge(k);
            redraw.redraw();
            if used { true } else { prev.borrow_mut().as_mut().is_some_and(|f| f(k)) }
        });
        let s2 = tree.st.clone();
        tree.widget.clone().draw(move |w| tree_draw(w, &s2));
        let s3 = tree.st.clone();
        tree.widget.clone().handle(move |w, ev| tree_handle(w, ev, &s3));
        crate::focus::host(&tree.widget, &[panel]);
        tree.describe();
        tree
    }

    /// Screen readers: a tree with the items in view (plus the cursor
    /// item), each with its level, expanded state and place among siblings.
    fn describe(&self) {
        let (st, me) = (self.st.clone(), self.widget.clone());
        a11y::describe(&self.widget, move |n| {
            let Ok(s) = st.try_borrow() else { return };
            let rh = row_height();
            n.role(Role::Tree).multiselectable(true).focusable();
            let rows = s.rows.borrow();
            let first = (s.top / rh as i64).max(0) as usize;
            let mut shown: Vec<usize> = (first..(first + (me.h() / rh + 2).max(0) as usize).min(rows.len())).collect();
            if let Some(c) = s.panel.cursor_index().filter(|c| *c < rows.len() && !shown.contains(c)) {
                shown.push(c);
                shown.sort_unstable();
            }
            // Position among siblings: one pass over the visible rows.
            let mut seen: HashMap<Option<ItemId>, usize> = HashMap::new();
            let mut pos = vec![0usize; rows.len()];
            for (i, v) in rows.iter().enumerate() {
                let c = seen.entry(v.parent).or_insert(0);
                *c += 1;
                pos[i] = *c;
            }
            let cursor = s.panel.cursor();
            for idx in shown {
                let v = &rows[idx];
                let id = v.node.id;
                let ry = me.y() + (idx as i64 * rh as i64 - s.top) as i32;
                n.child(id, |t| {
                    t.role(Role::TreeItem).name(&v.node.label).description(&v.node.detail).level(v.depth as usize + 1)
                        .selected(s.panel.is_selected(id)).position(pos[idx], seen[&v.parent])
                        .bounds(crate::layout::Mirror::of(&me).x(me.x(), me.w() - BAR - 4), ry, me.w() - BAR - 4, rh)
                        .action(Action::Click).action(Action::Focus).action(Action::ScrollIntoView);
                    if v.node.has_children {
                        let open = s.expanded.contains(&id);
                        t.expanded(open).action(if open { Action::Collapse } else { Action::Expand });
                    }
                    if cursor == Some(id) {
                        t.focused();
                    }
                });
            }
        });
        let me = self.clone();
        a11y::on_action(&self.widget, move |r| {
            let Some(id) = r.key() else { return false };
            let panel = me.st.borrow().panel.clone();
            match r.action {
                Action::Click => panel.click(id, false, false),
                Action::Focus => { panel.focus(); panel.set_cursor(Some(id)); }
                Action::Expand => me.expand(id),
                Action::Collapse => me.collapse(id),
                Action::ScrollIntoView => {
                    let Some(i) = panel.index_of(id) else { return false };
                    let h = me.widget.h();
                    let mut s = me.st.borrow_mut();
                    scroll_into_view(&mut s.top, i, row_height(), h);
                    let top = s.top;
                    s.motion.jump(top);
                }
                _ => return false,
            }
            me.widget.clone().redraw();
            true
        });
    }

    /// Asks again for every node's children (call after the data changes);
    /// expanded nodes stay expanded, cursor and selection follow their IDs.
    pub fn refresh(&self) {
        self.st.borrow_mut().cache.clear();
        self.rebuild();
    }

    /// Asks again only for the children of `id` (None = the roots), e.g.
    /// after one folder changed.
    pub fn refresh_node(&self, id: Option<ItemId>) {
        self.st.borrow_mut().cache.remove(&id);
        self.rebuild();
    }

    /// Rebuilds the visible rows from the cache, asking only for children
    /// not seen yet.
    fn rebuild(&self) {
        let mut out = Vec::new();
        {
            let mut guard = self.st.borrow_mut();
            fn walk(s: &mut TrState, parent: Option<ItemId>, depth: u16, out: &mut Vec<Visible>) {
                if !s.cache.contains_key(&parent) {
                    let kids = (s.children)(parent);
                    s.cache.insert(parent, kids);
                }
                let kids = s.cache[&parent].clone();
                for node in kids {
                    let (id, open) = (node.id, node.has_children && s.expanded.contains(&node.id));
                    out.push(Visible { depth, node, parent });
                    if open {
                        walk(s, Some(id), depth + 1, out);
                    }
                }
            }
            walk(&mut guard, None, 0, &mut out);
        }
        let (rows, index, panel) = {
            let s = self.st.borrow();
            (s.rows.clone(), s.index.clone(), s.panel.clone())
        };
        *index.borrow_mut() = out.iter().enumerate().map(|(i, v)| (v.node.id, i)).collect();
        let n = out.len();
        *rows.borrow_mut() = out;
        let (r2, i2) = (rows.clone(), index.clone());
        panel.set_items(n, move |i| r2.borrow()[i].node.id, move |id| i2.borrow().get(&id).copied());
        self.widget.clone().redraw();
    }

    pub fn is_expanded(&self, id: ItemId) -> bool {
        self.st.borrow().expanded.contains(&id)
    }

    pub fn expand(&self, id: ItemId) {
        if self.st.borrow_mut().expanded.insert(id) {
            // Omarchy KeyboardPanel growing toward new rows: 200 ms.
            self.st.borrow_mut().rm.reflowed(crate::anim::ms::REVEAL);
            self.rebuild();
            self.widget.clone().redraw();
        }
    }

    pub fn collapse(&self, id: ItemId) {
        if self.st.borrow_mut().expanded.remove(&id) {
            self.st.borrow_mut().rm.reflowed(crate::anim::ms::REVEAL);
            self.rebuild();
            self.widget.clone().redraw();
        }
    }

    pub fn toggle(&self, id: ItemId) {
        if self.is_expanded(id) { self.collapse(id) } else { self.expand(id) }
    }

    /// Visible rows (expanded nodes included).
    pub fn visible_len(&self) -> usize {
        self.st.borrow().rows.borrow().len()
    }

    /// The node at a visible row.
    pub fn node(&self, row: usize) -> Option<TreeNode> {
        self.st.borrow().rows.borrow().get(row).map(|v| v.node.clone())
    }

    /// Depth of a visible row (0 = root).
    pub fn depth(&self, row: usize) -> Option<u16> {
        self.st.borrow().rows.borrow().get(row).map(|v| v.depth)
    }

    /// Rows drawn so far (proves virtualization in tests).
    pub fn rows_drawn(&self) -> usize {
        self.st.borrow().drawn
    }

    pub fn scroll_offset(&self) -> i64 {
        self.st.borrow().top
    }

    /// Scrolls to an offset in pixels (clamped when drawn).
    pub fn set_scroll_offset(&self, px: i64) {
        let mut s = self.st.borrow_mut();
        s.top = px.max(0);
        let top = s.top;
        s.motion.jump(top);
        drop(s);
        self.widget.clone().redraw();
    }

    /// How model changes were redrawn so far (model trees; tests, benchmarks).
    pub fn invalidations(&self) -> Invalidations {
        self.st.borrow().inval
    }

    /// A tree over a flat [`ListModel`]: `parent(item)` gives the key of
    /// an item's parent (None, or a key not in the model: a root), `node`
    /// its row (`TreeNode::id` is set to the model's ID; mark parents with
    /// `.branch()`). Children keep the model's order. An update that keeps
    /// an item's parent and shape redraws only its row; anything else
    /// rebuilds the visible rows, keeping the first visible item in place,
    /// the expanded nodes, the cursor and the selection.
    pub fn from_model<K, T>(panel: &KeyboardPanel, model: &ListModel<K, T>, node: impl Fn(&T) -> TreeNode + 'static, parent: impl Fn(&T) -> Option<K> + 'static) -> Tree
    where
        K: Clone + Eq + Hash + 'static,
        T: Clone + PartialEq + 'static,
    {
        let node = Rc::new(node);
        let parent = Rc::new(parent);
        let index: Rc<RefCell<Option<TreeIndex>>> = Rc::default();
        let parent_id = {
            let parent = parent.clone();
            Rc::new(move |m: &ListModel<K, T>, id: ItemId, t: &T| parent(t).and_then(|k| m.id_of(&k)).filter(|p| *p != id))
        };
        let children = {
            let (m, node, index, parent_id) = (model.clone(), node.clone(), index.clone(), parent_id.clone());
            move |p: Option<ItemId>| -> Vec<TreeNode> {
                if index.borrow().is_none() {
                    let mut ix = TreeIndex::default();
                    let ids: Vec<ItemId> = (0..m.len()).filter_map(|i| m.id_at(i)).collect();
                    for id in ids {
                        let par = m.with_id(id, |t| parent_id(&m, id, t)).flatten();
                        ix.children.entry(par).or_default().push(id);
                        ix.parent.insert(id, par);
                    }
                    *index.borrow_mut() = Some(ix);
                }
                let ix = index.borrow();
                let kids = ix.as_ref().and_then(|ix| ix.children.get(&p));
                kids.map(|ids| ids.iter().filter_map(|&id| m.with_id(id, |t| TreeNode { id, ..node(t) })).collect()).unwrap_or_default()
            }
        };
        let tree = Tree::new(panel, children);
        let weak = Rc::downgrade(&tree.st);
        let w = tree.widget.clone();
        model.listen(move |m, changes| {
            let Some(st) = weak.upgrade().filter(|_| !w.was_deleted()) else { return false };
            let me = Tree { widget: w.clone(), st };
            let mut rebuild = changes.is_structural() || !changes.removed().is_empty();
            let mut patched = Vec::new();
            if !rebuild {
                let ix = index.borrow();
                for &id in changes.updated() {
                    let now = m.with_id(id, |t| (parent_id(m, id, t), TreeNode { id, ..node(t) }));
                    match (ix.as_ref().and_then(|ix| ix.parent.get(&id)), now) {
                        (Some(old), Some((par, n))) if *old == par => patched.push(n),
                        _ => {
                            rebuild = true;
                            break;
                        }
                    }
                }
            }
            if rebuild || !me.patch_nodes(patched) {
                *index.borrow_mut() = None;
                me.refresh_anchored();
            }
            true
        });
        tree
    }

    /// Replaces cached nodes in place and redraws their visible rows.
    /// False when a node changed shape (expander) and the rows must be rebuilt.
    fn patch_nodes(&self, nodes: Vec<TreeNode>) -> bool {
        let rh = row_height();
        let mut damage = Vec::new();
        {
            let mut guard = self.st.borrow_mut();
            let s = &mut *guard;
            let (rows, index) = (s.rows.clone(), s.index.clone());
            let mut rows = rows.borrow_mut();
            for n in nodes {
                let row = index.borrow().get(&n.id).copied();
                if let Some(r) = row {
                    let v = &mut rows[r];
                    if v.node.has_children != n.has_children {
                        return false;
                    }
                    if let Some(kids) = s.cache.get_mut(&v.parent) {
                        if let Some(k) = kids.iter_mut().find(|k| k.id == n.id) {
                            *k = n.clone();
                        }
                    }
                    v.node = n;
                    let y = self.widget.y() + (r as i64 * rh as i64 - s.top) as i32;
                    if y + rh > self.widget.y() && y < self.widget.y() + self.widget.h() {
                        damage.push(y);
                    }
                } else {
                    // Under a collapsed node: only the cache (seen on expand).
                    for kids in s.cache.values_mut() {
                        if let Some(k) = kids.iter_mut().find(|k| k.id == n.id) {
                            if k.has_children != n.has_children {
                                return false;
                            }
                            *k = n.clone();
                        }
                    }
                }
            }
            if damage.is_empty() {
                s.inval.none += 1;
            }
            s.inval.rows += damage.len();
        }
        let mut w = self.widget.clone();
        for y in damage {
            let top = y.max(w.y());
            let bottom = (y + rh).min(w.y() + w.h());
            w.set_damage_area(Damage::User1, w.x(), top, w.w(), bottom - top);
        }
        true
    }

    /// `refresh`, keeping the first visible item where it is on screen.
    fn refresh_anchored(&self) {
        let rh = row_height() as i64;
        let (top, anchor) = {
            let s = self.st.borrow();
            (s.top, s.slots.anchor(s.top))
        };
        self.refresh();
        let mut s = self.st.borrow_mut();
        s.slots.invalidate();
        s.inval.full += 1;
        if top > 0 {
            let at = anchor.and_then(|id| s.index.borrow().get(&id).copied());
            if let Some(i) = at {
                let new_top = i as i64 * rh + top.rem_euclid(rh);
                s.motion.shift(new_top - top);
                s.top = new_top;
            }
        }
    }

    fn edge(&self, k: Key) -> bool {
        let panel = self.st.borrow().panel.clone();
        let Some(i) = panel.cursor_index() else { return false };
        let Some(v) = self.st.borrow().rows.borrow().get(i).cloned() else { return false };
        match k {
            Key::Right if v.node.has_children && !self.is_expanded(v.node.id) => {
                self.expand(v.node.id);
                true
            }
            Key::Right if v.node.has_children => {
                let child = self.st.borrow().rows.borrow().get(i + 1).filter(|c| c.parent == Some(v.node.id)).map(|c| c.node.id);
                if let Some(c) = child {
                    panel.select_only(c);
                }
                true
            }
            Key::Left if self.is_expanded(v.node.id) => {
                self.collapse(v.node.id);
                true
            }
            Key::Left => match v.parent {
                Some(p) => {
                    panel.select_only(p);
                    true
                }
                None => false,
            },
            _ => false,
        }
    }
}

fn indent() -> i32 {
    theme::with(|t| (16.0 * t.font.base as f64 / 12.0).round() as i32)
}

fn tree_draw(w: &mut Widget, st: &Rc<RefCell<TrState>>) {
    let rh = row_height();
    let mut guard = st.borrow_mut();
    let s = &mut *guard;
    s.panel.set_page_rows((w.h() / rh).max(1) as usize);
    follow_cursor(&s.panel, &mut s.last_cursor, &mut s.top, rh, w.h());
    let rows = s.rows.clone();
    let rows = rows.borrow();
    let content = rows.len() as i64 * rh as i64;
    s.top = s.motion.position(s.top, (content - w.h() as i64).max(0));
    if s.motion.running() { crate::anim::request_redraw(w); }
    let ring = s.panel.show_ring();
    let cursor = s.panel.cursor();
    // Right to left: indentation from the right, expanders point left.
    let m = crate::layout::Mirror::of(w);
    theme::with(|t| {
        let fg = t.palette.foreground;
        draw::push_clip(w.x(), w.y(), w.w(), w.h());
        let first = (s.top / rh as i64) as usize;
        let visible = (w.h() / rh + 2) as usize;
        let ind = indent();
        let pad = t.spacing.md;
        s.slots.begin(s.top, visible);
        let rw = w.w() - BAR - 4;
        // One row: background, expander, icon, label and detail.
        let paint_row = |s: &TrState, v: &Visible, ry: i32, idx: Option<usize>| {
            draw_row_bg(t, m.x(w.x(), rw), ry, rw, rh, s.panel.is_selected(v.node.id), cursor == Some(v.node.id), idx.is_some() && s.hover == idx, ring);
            let mut x = w.x() + pad + v.depth as i32 * ind;
            if v.node.has_children {
                let open = s.expanded.contains(&v.node.id);
                let closed = m.glyph(glyph::CHEVRON_RIGHT, glyph::CHEVRON_LEFT);
                paint::glyph(if open { glyph::CHEVRON_DOWN } else { closed }, m.x(x, ind), ry, ind, rh, t.font.body_small, fg.darker(1.3));
            }
            x += ind;
            if !v.node.icon.is_empty() {
                let cell = (22.0 * t.font.base as f64 / 12.0).round() as i32;
                paint::glyph(&v.node.icon, m.x(x, cell), ry, cell, rh, t.font.title, fg);
                x += cell + t.spacing.md;
            }
            let right = w.x() + rw - pad;
            let mut lw = right - x;
            if !v.node.detail.is_empty() {
                let dw = paint::measure(&v.node.detail, t.font.caption, false).min(lw / 2);
                paint::text_elided(&v.node.detail, m.x(right - dw, dw), ry, dw, rh, t.font.caption, false, fg.darker(1.4), m.align(Align::Right));
                lw -= dw + t.spacing.lg;
            }
            paint::text_elided(&v.node.label, m.x(x, lw.max(0)), ry, lw.max(0), rh, t.font.body, false, fg, m.align(Align::Left));
        };
        // Branches opening or closing (full redraws): where each row in view is now.
        let moving = w.damage_type().contains(Damage::All);
        if moving {
            let inview: Vec<(ItemId, i32)> = (0..visible)
                .filter_map(|k| rows.get(first + k).map(|v| (v.node.id, ((first + k) as i64 * rh as i64 - s.top) as i32)))
                .collect();
            let index = s.index.clone();
            s.rm.begin(&inview, |id| index.borrow().contains_key(&id));
        }
        let under = t.palette.background;
        let drift = (22.0 * t.font.base as f64 / 12.0).round();
        let mut drawn = Vec::new();
        for k in 0..visible {
            let idx = first + k;
            let Some(v) = rows.get(idx) else { s.slots.drawn(k, None); continue };
            let (dy, opacity, _) = if moving { s.rm.at(v.node.id, drift) } else { (0, 1.0, 0.0) };
            let ry = w.y() + (idx as i64 * rh as i64 - s.top) as i32 + dy;
            // Partial redraws (one changed row) draw only the rows in the damaged area.
            if !draw::not_clipped(w.x(), ry, w.w(), rh) { continue; }
            s.slots.drawn(k, Some(v.node.id));
            paint_row(s, v, ry, Some(idx));
            if opacity < 1.0 {
                paint::alpha_fill(m.x(w.x(), rw), ry, rw, rh, under, 1.0 - opacity);
            }
            if moving && crate::anim::enabled() {
                drawn.push((v.node.id, ry - w.y(), v.clone()));
            }
            s.drawn += 1;
        }
        if moving {
            for g in &s.rm.ghosts {
                let (dy, o) = g.sample(drift);
                let ry = w.y() + g.y + dy;
                paint_row(s, &g.row, ry, None);
                paint::alpha_fill(m.x(w.x(), rw), ry, rw, rh, under, 1.0 - o);
            }
            s.rm.end(drawn);
            if s.rm.running() { crate::anim::request_redraw(w); }
        }
        draw::pop_clip();
        scrollbar(t, m.x(w.x() + w.w() - BAR, BAR), w.y(), w.h(), s.top, content);
    });
}

fn tree_handle(w: &mut Widget, ev: Event, st: &Rc<RefCell<TrState>>) -> bool {
    let rh = row_height();
    let ex = crate::layout::Mirror::of(w).point(app::event_x()) - w.x();
    let ey = app::event_y() - w.y();
    let row_at = |s: &TrState| -> Option<usize> {
        let i = (ey as i64 + s.top) / rh as i64;
        (i >= 0 && (i as usize) < s.rows.borrow().len()).then_some(i as usize)
    };
    if matches!(ev, Event::Push | Event::KeyDown) {
        let mut s = st.borrow_mut(); let top = s.top; s.motion.jump(top);
    }
    match ev {
        Event::Focus | Event::Unfocus => { w.redraw(); true }
        Event::Enter => true,
        Event::Move => {
            let h = row_at(&st.borrow());
            if st.borrow().hover != h { st.borrow_mut().hover = h; w.redraw(); }
            true
        }
        Event::Leave => { st.borrow_mut().hover = None; w.redraw(); true }
        Event::MouseWheel => {
            let mut s = st.borrow_mut();
            let max = (s.rows.borrow().len() as i64 * rh as i64 - w.h() as i64).max(0);
            let top = s.top;
            s.top = s.motion.wheel(top, app::event_dy_value() as f64 * 3.0 * rh as f64, max);
            drop(s);
            w.redraw();
            true
        }
        Event::Push => {
            let _ = w.take_focus();
            let (row, panel) = { let s = st.borrow(); (row_at(&s), s.panel.clone()) };
            let Some(r) = row else { return true };
            let v = st.borrow().rows.borrow()[r].clone();
            let pad = theme::with(|t| t.spacing.md);
            let chev_x = pad + v.depth as i32 * indent();
            let tree = Tree { widget: w.clone(), st: st.clone() };
            if v.node.has_children && ex >= chev_x && ex < chev_x + indent() {
                tree.toggle(v.node.id);
                panel.select_only(v.node.id);
                panel.focus();
                return true;
            }
            panel.click(v.node.id, crate::keys::primary_held(), app::is_event_shift());
            if app::event_clicks() {
                if v.node.has_children { tree.toggle(v.node.id) } else { panel.activate(v.node.id) }
            }
            true
        }
        _ => false,
    }
}
