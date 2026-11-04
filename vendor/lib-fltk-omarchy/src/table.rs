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

use crate::panel::{ItemId, KeyboardPanel};
use crate::paint;
use crate::theme;
use crate::widgets::glyph;
use fltk::{app, draw, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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
        }));
        let s2 = st.clone();
        widget.draw(move |w| table_draw(w, &s2));
        let s3 = st.clone();
        widget.handle(move |w, ev| table_handle(w, ev, &s3));
        crate::focus::host(&widget, &[panel]);
        Table { widget, st }
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
    theme::with(|t| {
        let fg = t.palette.foreground;
        let under = t.palette.background;
        let pad = t.spacing.md + t.spacing.sm;
        // Header.
        for (i, (c, (cx, cw))) in s.cols.iter().zip(cols.iter()).enumerate() {
            let x = w.x() + cx;
            let align = if c.right { Align::Right } else { Align::Left };
            let sorted = s.sort.filter(|(sc, _)| *sc == i).map(|(_, asc)| asc);
            let arrow = if sorted.is_some() { t.font.caption + t.spacing.sm } else { 0 };
            // Sorted column: a chevron next to the title (up = ascending).
            let (tx, ax) = if c.right { (x + pad + arrow, x + pad) } else { (x + pad, x + cw - pad - arrow) };
            paint::text_elided(&c.title, tx, w.y(), cw - 2 * pad - arrow, hh, t.font.caption, true, fg.darker(1.4), align);
            if let Some(asc) = sorted {
                paint::glyph(if asc { glyph::CHEVRON_UP } else { glyph::CHEVRON_DOWN }, ax, w.y(), arrow, hh, t.font.caption, t.palette.accent);
            }
            if i + 1 < cols.len() {
                paint::fill(x + cw - 1, w.y() + hh / 4, 1, hh / 2, 0, fg.over(under, 0.15));
            }
        }
        paint::hairline(w.x(), w.y() + hh - 1, w.w(), fg, under, 0.12);
        // Rows.
        let by = w.y() + hh;
        draw::push_clip(w.x(), by, w.w(), body_h);
        let first = (s.top / rh as i64) as usize;
        let visible = (body_h / rh + 2) as usize;
        let cursor = s.panel.cursor();
        for k in 0..visible {
            let idx = first + k;
            if idx >= n { break; }
            let Some(id) = s.panel.id_at(idx) else { break };
            let ry = by + (idx as i64 * rh as i64 - s.top) as i32;
            let rw = w.w() - BAR - 4;
            draw_row_bg(t, w.x(), ry, rw, rh, s.panel.is_selected(id), cursor == Some(id), s.hover == Some(idx), ring);
            for (ci, (c, (cx, cw))) in s.cols.iter().zip(cols.iter()).enumerate() {
                let mut tx = w.x() + cx + pad;
                let mut tw = cw - 2 * pad;
                if ci == 0 {
                    if let Some(icon) = s.icon.as_ref() {
                        let g = icon(idx);
                        if !g.is_empty() {
                            let cell = (22.0 * t.font.base as f64 / 12.0).round() as i32;
                            paint::glyph(&g, tx, ry, cell, rh, t.font.title, fg);
                            tx += cell + t.spacing.md;
                            tw -= cell + t.spacing.md;
                        }
                    }
                }
                let text = (s.cell)(idx, ci);
                let color = if ci == 0 { fg } else { fg.darker(1.25) };
                paint::text_elided(&text, tx, ry, tw.max(0), rh, t.font.body, false, color, if c.right { Align::Right } else { Align::Left });
            }
            s.drawn += 1;
        }
        draw::pop_clip();
        scrollbar(t, w.x() + w.w() - BAR, by, body_h, s.top, content);
        if crate::focus::show_ring(w) && cursor.is_none() {
            paint::stroke(w.x(), by, w.w(), body_h, t.radius, 1, t.border(&t.controls.focus, under));
        }
    });
}

fn table_handle(w: &mut Widget, ev: Event, st: &Rc<RefCell<TState>>) -> bool {
    let rh = row_height();
    let hh = header_h();
    let (ex, ey) = (app::event_x() - w.x(), app::event_y() - w.y());
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
                        st.borrow_mut().resizing = Some((i, app::event_x(), cols[i].1));
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
                panel.click(id, app::is_event_ctrl(), app::is_event_shift());
                if app::event_clicks() {
                    panel.activate(id);
                }
            }
            true
        }
        Event::Drag => {
            let r = st.borrow().resizing;
            if let Some((i, x0, w0)) = r {
                st.borrow_mut().cols[i].width = (w0 + app::event_x() - x0).max(40);
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
        tree
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
            self.rebuild();
        }
    }

    pub fn collapse(&self, id: ItemId) {
        if self.st.borrow_mut().expanded.remove(&id) {
            self.rebuild();
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
    theme::with(|t| {
        let fg = t.palette.foreground;
        draw::push_clip(w.x(), w.y(), w.w(), w.h());
        let first = (s.top / rh as i64) as usize;
        let visible = (w.h() / rh + 2) as usize;
        let ind = indent();
        let pad = t.spacing.md;
        for k in 0..visible {
            let idx = first + k;
            let Some(v) = rows.get(idx) else { break };
            let ry = w.y() + (idx as i64 * rh as i64 - s.top) as i32;
            let rw = w.w() - BAR - 4;
            draw_row_bg(t, w.x(), ry, rw, rh, s.panel.is_selected(v.node.id), cursor == Some(v.node.id), s.hover == Some(idx), ring);
            let mut x = w.x() + pad + v.depth as i32 * ind;
            if v.node.has_children {
                let open = s.expanded.contains(&v.node.id);
                paint::glyph(if open { glyph::CHEVRON_DOWN } else { glyph::CHEVRON_RIGHT }, x, ry, ind, rh, t.font.body_small, fg.darker(1.3));
            }
            x += ind;
            if !v.node.icon.is_empty() {
                let cell = (22.0 * t.font.base as f64 / 12.0).round() as i32;
                paint::glyph(&v.node.icon, x, ry, cell, rh, t.font.title, fg);
                x += cell + t.spacing.md;
            }
            let right = w.x() + rw - pad;
            let mut lw = right - x;
            if !v.node.detail.is_empty() {
                let dw = paint::measure(&v.node.detail, t.font.caption, false).min(lw / 2);
                paint::text_elided(&v.node.detail, right - dw, ry, dw, rh, t.font.caption, false, fg.darker(1.4), Align::Right);
                lw -= dw + t.spacing.lg;
            }
            paint::text_elided(&v.node.label, x, ry, lw.max(0), rh, t.font.body, false, fg, Align::Left);
            s.drawn += 1;
        }
        draw::pop_clip();
        scrollbar(t, w.x() + w.w() - BAR, w.y(), w.h(), s.top, content);
    });
}

fn tree_handle(w: &mut Widget, ev: Event, st: &Rc<RefCell<TrState>>) -> bool {
    let rh = row_height();
    let (ex, ey) = (app::event_x() - w.x(), app::event_y() - w.y());
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
            panel.click(v.node.id, app::is_event_ctrl(), app::is_event_shift());
            if app::event_clicks() {
                if v.node.has_children { tree.toggle(v.node.id) } else { panel.activate(v.node.id) }
            }
            true
        }
        _ => false,
    }
}
