//! Keyboard panels: focusable regions inside one widget (a canvas that
//! draws several lists, a grid, a dual pane) with a virtual cursor and a
//! selection kept by stable item IDs, never one widget per item.
//!
//! Each panel is a real Tab stop: register the panels of a widget with
//! [`crate::focus::host`] and Tab / Shift+Tab walk them in order, then the
//! next control. While a panel has focus, its keys go to the panel first
//! (any modifiers); keys it does not use go on as usual (the host widget's
//! own handler, window shortcuts), so a shortcut is never handled twice.
//!
//! ```no_run
//! use fltk::{prelude::*, widget::Widget};
//! use fltk_omarchy::{focus, panel::{KeyboardPanel, PanelLayout}};
//! let left = KeyboardPanel::new("left");
//! let right = KeyboardPanel::new("right").layout(PanelLayout::Grid { columns: 4 });
//! let names = vec![10u64, 11, 12];              // stable IDs from your model
//! let ids = names.clone();
//! left.set_items(names.len(), move |i| ids[i], move |id| names.iter().position(|&n| n == id));
//! let mut canvas = Widget::default().with_size(600, 400);
//! canvas.handle(|_, ev| matches!(ev, fltk::enums::Event::Focus | fltk::enums::Event::Unfocus));
//! focus::host(&canvas, &[&left, &right]);       // two Tab stops in one widget
//! left.on_activate(|id| println!("open {id}"));
//! // draw: left.cursor(), left.is_selected(id), left.show_ring()
//! ```
//!
//! Keys while a panel has focus:
//!
//! | Key | Effect |
//! |---|---|
//! | Up / Down (Left / Right in a grid) | move the cursor; the selection becomes that item |
//! | Ctrl + those keys | move the cursor only (selection unchanged) |
//! | Shift + those keys | select the range from the anchor to the cursor |
//! | Ctrl+Shift + those keys | add that range to the selection |
//! | PageUp / PageDown, Home / End | same, by a page (`set_page_rows`) or to the ends |
//! | Ctrl+Space | toggle the item under the cursor |
//! | Ctrl+A | select everything |
//! | Enter | `on_activate(cursor)` |
//!
//! At an edge (Up on the first row, Left on the first column…) the key goes
//! to `on_edge`, then to the arrow scopes and window shortcuts. `on_key`
//! sees every key first and can take it.

use fltk::{enums::{Key, Shortcut}, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

/// A stable item identity chosen by the app (an inode, a hash of the raw
/// name, a row key). Selection and cursor survive reordering and refresh.
pub type ItemId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelLayout {
    /// One column: Up/Down move, Left/Right are edges.
    List,
    /// Row-major grid with this many columns.
    Grid { columns: usize },
}

type IdAt = Box<dyn Fn(usize) -> ItemId>;
type IndexOf = Box<dyn Fn(ItemId) -> Option<usize>>;

struct Source {
    len: usize,
    id_at: IdAt,
    index_of: IndexOf,
}

#[derive(Default)]
struct Callbacks {
    change: Option<Box<dyn FnMut()>>,
    activate: Option<Box<dyn FnMut(ItemId)>>,
    edge: Option<Box<dyn FnMut(Key) -> bool>>,
    key: Option<Box<dyn FnMut(Key, Shortcut) -> bool>>,
    focus: Option<Box<dyn FnMut(bool)>>,
}

struct Inner {
    name: String,
    layout: PanelLayout,
    page_rows: usize,
    source: Option<Source>,
    cursor: Option<ItemId>,
    anchor: Option<ItemId>,
    selection: BTreeSet<ItemId>,
    host: Option<Widget>,
}

/// One focusable region with a cursor and an ID selection. Cloning shares it.
#[derive(Clone)]
pub struct KeyboardPanel {
    inner: Rc<RefCell<Inner>>,
    cbs: Rc<RefCell<Callbacks>>,
}

impl PartialEq for KeyboardPanel {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for KeyboardPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i = self.inner.borrow();
        write!(f, "KeyboardPanel({:?}, cursor {:?}, {} selected)", i.name, i.cursor, i.selection.len())
    }
}

impl KeyboardPanel {
    pub fn new(name: &str) -> KeyboardPanel {
        KeyboardPanel {
            inner: Rc::new(RefCell::new(Inner {
                name: name.into(),
                layout: PanelLayout::List,
                page_rows: 10,
                source: None,
                cursor: None,
                anchor: None,
                selection: BTreeSet::new(),
                host: None,
            })),
            cbs: Rc::default(),
        }
    }

    pub fn layout(self, layout: PanelLayout) -> Self {
        self.set_layout(layout);
        self
    }

    /// Changes List/Grid (e.g. the view mode or the column count after a resize).
    pub fn set_layout(&self, layout: PanelLayout) {
        self.inner.borrow_mut().layout = match layout {
            PanelLayout::Grid { columns } => PanelLayout::Grid { columns: columns.max(1) },
            l => l,
        };
    }

    /// Rows moved by PageUp/PageDown (the rows that fit in the view).
    pub fn set_page_rows(&self, rows: usize) {
        self.inner.borrow_mut().page_rows = rows.max(1);
    }

    pub fn name(&self) -> String {
        self.inner.borrow().name.clone()
    }

    /// The items, in display order: `len`, the ID at an index, and the index
    /// of an ID (None when it is gone). Call again after every refresh,
    /// sort or filter: the cursor, anchor and selection keep their IDs and
    /// drop those that no longer exist.
    pub fn set_items(&self, len: usize, id_at: impl Fn(usize) -> ItemId + 'static, index_of: impl Fn(ItemId) -> Option<usize> + 'static) {
        {
            let mut i = self.inner.borrow_mut();
            let src = Source { len, id_at: Box::new(id_at), index_of: Box::new(index_of) };
            let alive = |id: &ItemId| (src.index_of)(*id).is_some_and(|x| x < len);
            i.selection.retain(alive);
            if i.cursor.is_some_and(|c| !alive(&c)) {
                i.cursor = None;
            }
            if i.anchor.is_some_and(|c| !alive(&c)) {
                i.anchor = None;
            }
            i.source = Some(src);
        }
        self.changed();
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().source.as_ref().map_or(0, |s| s.len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// ID at a display index.
    pub fn id_at(&self, index: usize) -> Option<ItemId> {
        let i = self.inner.borrow();
        let s = i.source.as_ref()?;
        (index < s.len).then(|| (s.id_at)(index))
    }

    /// Display index of an ID.
    pub fn index_of(&self, id: ItemId) -> Option<usize> {
        let i = self.inner.borrow();
        let s = i.source.as_ref()?;
        (s.index_of)(id).filter(|x| *x < s.len)
    }

    // ---- state

    pub fn cursor(&self) -> Option<ItemId> {
        self.inner.borrow().cursor
    }

    pub fn cursor_index(&self) -> Option<usize> {
        self.cursor().and_then(|c| self.index_of(c))
    }

    /// Moves the cursor (no selection change). None clears it.
    pub fn set_cursor(&self, id: Option<ItemId>) {
        let id = id.filter(|i| self.index_of(*i).is_some());
        self.inner.borrow_mut().cursor = id;
        self.changed();
    }

    pub fn anchor(&self) -> Option<ItemId> {
        self.inner.borrow().anchor
    }

    pub fn is_selected(&self, id: ItemId) -> bool {
        self.inner.borrow().selection.contains(&id)
    }

    pub fn selection_len(&self) -> usize {
        self.inner.borrow().selection.len()
    }

    /// Selected IDs in display order.
    pub fn selection(&self) -> Vec<ItemId> {
        let sel: Vec<ItemId> = self.inner.borrow().selection.iter().copied().collect();
        let mut v: Vec<(usize, ItemId)> = sel.into_iter().filter_map(|id| self.index_of(id).map(|i| (i, id))).collect();
        v.sort_unstable();
        v.into_iter().map(|(_, id)| id).collect()
    }

    pub fn select_only(&self, id: ItemId) {
        if self.index_of(id).is_none() {
            return;
        }
        {
            let mut i = self.inner.borrow_mut();
            i.selection.clear();
            i.selection.insert(id);
            i.cursor = Some(id);
            i.anchor = Some(id);
        }
        self.changed();
    }

    pub fn toggle(&self, id: ItemId) {
        if self.index_of(id).is_none() {
            return;
        }
        {
            let mut i = self.inner.borrow_mut();
            if !i.selection.remove(&id) {
                i.selection.insert(id);
            }
            i.anchor = Some(id);
        }
        self.changed();
    }

    /// Selects the items between two IDs (inclusive, display order).
    /// `add` keeps the current selection.
    pub fn select_range(&self, from: ItemId, to: ItemId, add: bool) {
        let (Some(a), Some(b)) = (self.index_of(from), self.index_of(to)) else { return };
        let ids: Vec<ItemId> = (a.min(b)..=a.max(b)).filter_map(|x| self.id_at(x)).collect();
        {
            let mut i = self.inner.borrow_mut();
            if !add {
                i.selection.clear();
            }
            i.selection.extend(ids);
        }
        self.changed();
    }

    pub fn select_all(&self) {
        let ids: Vec<ItemId> = (0..self.len()).filter_map(|x| self.id_at(x)).collect();
        self.inner.borrow_mut().selection.extend(ids);
        self.changed();
    }

    pub fn clear_selection(&self) {
        self.inner.borrow_mut().selection.clear();
        self.changed();
    }

    pub fn invert_selection(&self) {
        let ids: Vec<ItemId> = (0..self.len()).filter_map(|x| self.id_at(x)).collect();
        {
            let mut i = self.inner.borrow_mut();
            let old = std::mem::take(&mut i.selection);
            i.selection = ids.into_iter().filter(|id| !old.contains(id)).collect();
        }
        self.changed();
    }

    /// Mouse click on an item: plain = only this one, Ctrl = toggle,
    /// Shift = range from the anchor (Ctrl+Shift adds it). Also focuses the
    /// panel.
    pub fn click(&self, id: ItemId, ctrl: bool, shift: bool) {
        if self.index_of(id).is_none() {
            return;
        }
        let anchor = self.anchor();
        match (ctrl, shift, anchor) {
            (_, true, Some(a)) => {
                self.select_range(a, id, ctrl);
                self.inner.borrow_mut().cursor = Some(id);
                self.changed();
            }
            (true, _, _) => {
                self.toggle(id);
                self.inner.borrow_mut().cursor = Some(id);
                self.changed();
            }
            _ => self.select_only(id),
        }
        self.focus();
    }

    /// Runs `on_activate(id)` (double click in a table or tree).
    pub fn activate(&self, id: ItemId) {
        let f = self.cbs.borrow_mut().activate.take();
        if let Some(mut f) = f {
            f(id);
            let mut c = self.cbs.borrow_mut();
            if c.activate.is_none() {
                c.activate = Some(f);
            }
        }
    }

    /// Takes over Left/Right edges (trees use this to expand and collapse);
    /// returns the previous handler.
    pub(crate) fn replace_edge(&self, f: Option<Box<dyn FnMut(Key) -> bool>>) -> Option<Box<dyn FnMut(Key) -> bool>> {
        std::mem::replace(&mut self.cbs.borrow_mut().edge, f)
    }

    // ---- focus

    /// Gives keyboard focus to this panel (its host widget takes focus).
    pub fn focus(&self) {
        crate::focus::focus_panel(self);
    }

    /// True when this panel is the focused region of the focused widget.
    pub fn has_focus(&self) -> bool {
        crate::focus::is_panel_focused(self)
    }

    /// Draw the focus ring now? (has focus and the keyboard is in use)
    pub fn show_ring(&self) -> bool {
        self.has_focus() && crate::focus::keyboard_mode()
    }

    pub(crate) fn set_host(&self, host: Option<Widget>) {
        self.inner.borrow_mut().host = host;
    }

    pub(crate) fn host(&self) -> Option<Widget> {
        self.inner.borrow().host.clone().filter(|h| !h.was_deleted())
    }

    // ---- callbacks

    /// After any cursor or selection change (redraw, scroll the cursor
    /// into view). The host widget is redrawn automatically.
    pub fn on_change(&self, f: impl FnMut() + 'static) -> &Self {
        self.cbs.borrow_mut().change = Some(Box::new(f));
        self
    }

    /// This panel gained (`true`) or lost (`false`) keyboard focus: Tab,
    /// Shift+Tab, [`KeyboardPanel::focus`], a host change, focus given back
    /// by a menu, a click. Runs synchronously, before the app-wide
    /// [`crate::focus::on_panel_focus`] listeners; the losing panel first.
    pub fn on_focus(&self, f: impl FnMut(bool) + 'static) -> &Self {
        self.cbs.borrow_mut().focus = Some(Box::new(f));
        self
    }

    pub(crate) fn notify_focus(&self, focused: bool) {
        // Taken out while it runs, so it may call back into this panel.
        let f = self.cbs.borrow_mut().focus.take();
        if let Some(mut f) = f {
            f(focused);
            let mut c = self.cbs.borrow_mut();
            if c.focus.is_none() {
                c.focus = Some(f);
            }
        }
    }

    /// Enter on the cursor item.
    pub fn on_activate(&self, f: impl FnMut(ItemId) + 'static) -> &Self {
        self.cbs.borrow_mut().activate = Some(Box::new(f));
        self
    }

    /// A move key at an edge (Left on the first column, Up on the first row…);
    /// return true when used (e.g. focus the other panel).
    pub fn on_edge(&self, f: impl FnMut(Key) -> bool + 'static) -> &Self {
        self.cbs.borrow_mut().edge = Some(Box::new(f));
        self
    }

    /// Sees every key first while the panel has focus; return true to take it.
    pub fn on_key(&self, f: impl FnMut(Key, Shortcut) -> bool + 'static) -> &Self {
        self.cbs.borrow_mut().key = Some(Box::new(f));
        self
    }

    fn changed(&self) {
        if let Some(mut h) = self.host() {
            h.redraw();
        }
        let f = self.cbs.borrow_mut().change.take();
        if let Some(mut f) = f {
            f();
            let mut c = self.cbs.borrow_mut();
            if c.change.is_none() {
                c.change = Some(f);
            }
        }
    }

    fn edge(&self, k: Key) -> bool {
        let f = self.cbs.borrow_mut().edge.take();
        let Some(mut f) = f else { return false };
        let used = f(k);
        let mut c = self.cbs.borrow_mut();
        if c.edge.is_none() {
            c.edge = Some(f);
        }
        used
    }

    // ---- keys

    /// Handles a key as if pressed while the panel has focus; true when used.
    /// The router calls it; tests may too.
    pub fn handle_key(&self, k: Key, mods: Shortcut) -> bool {
        let hook = self.cbs.borrow_mut().key.take();
        if let Some(mut f) = hook {
            let used = f(k, mods);
            {
                let mut c = self.cbs.borrow_mut();
                if c.key.is_none() {
                    c.key = Some(f);
                }
            }
            if used {
                return true;
            }
        }
        if mods.intersects(Shortcut::Alt | Shortcut::Meta) {
            return false;
        }
        let ctrl = mods.contains(Shortcut::Ctrl);
        let shift = mods.contains(Shortcut::Shift);
        let len = self.len();
        if ctrl && !shift && k == Key::from_char('a') {
            self.select_all();
            return true;
        }
        if ctrl && !shift && k == Key::from_char(' ') {
            if let Some(c) = self.cursor() {
                self.toggle(c);
            }
            return true;
        }
        if matches!(k, Key::Enter | Key::KPEnter) && !ctrl && !shift {
            let Some(c) = self.cursor() else { return false };
            let f = self.cbs.borrow_mut().activate.take();
            if let Some(mut f) = f {
                f(c);
                let mut cb = self.cbs.borrow_mut();
                if cb.activate.is_none() {
                    cb.activate = Some(f);
                }
                return true;
            }
            return false;
        }
        if !matches!(k, Key::Up | Key::Down | Key::Left | Key::Right | Key::PageUp | Key::PageDown | Key::Home | Key::End) {
            return false;
        }
        if len == 0 {
            return matches!(k, Key::Up | Key::Down | Key::Left | Key::Right) && self.edge(k);
        }
        let (layout, page) = {
            let i = self.inner.borrow();
            (i.layout, i.page_rows)
        };
        let cols = match layout { PanelLayout::List => 1, PanelLayout::Grid { columns } => columns.max(1) };
        let cur = self.cursor_index();
        let target: Option<usize> = match (cur, k) {
            (None, _) => Some(0), // first key lands on the first item
            (Some(c), Key::Up) => (c >= cols).then(|| c - cols),
            (Some(c), Key::Down) => {
                if c + cols < len { Some(c + cols) } else if c / cols < (len - 1) / cols { Some(len - 1) } else { None }
            }
            (Some(c), Key::Left) => (cols > 1 && c % cols != 0).then(|| c - 1),
            (Some(c), Key::Right) => (cols > 1 && c % cols != cols - 1 && c + 1 < len).then(|| c + 1),
            (Some(c), Key::PageUp) => Some(c.saturating_sub(page * cols)),
            (Some(c), Key::PageDown) => Some((c + page * cols).min(len - 1)),
            (Some(_), Key::Home) => Some(0),
            (Some(_), Key::End) => Some(len - 1),
            _ => None,
        };
        let Some(t) = target else { return self.edge(k) };
        let Some(id) = self.id_at(t) else { return false };
        match (ctrl, shift) {
            (true, false) => {
                self.inner.borrow_mut().cursor = Some(id);
                self.changed();
            }
            (_, true) => {
                let anchor = self.anchor().or(self.cursor()).unwrap_or(id);
                self.select_range(anchor, id, ctrl);
                let mut i = self.inner.borrow_mut();
                i.anchor = Some(anchor);
                i.cursor = Some(id);
                drop(i);
                self.changed();
            }
            (false, false) => self.select_only(id),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(n: usize, layout: PanelLayout) -> KeyboardPanel {
        let p = KeyboardPanel::new("t").layout(layout);
        // IDs are not indexes: 100 + 7*i.
        p.set_items(n, |i| 100 + 7 * i as u64, move |id| (id >= 100 && (id - 100) % 7 == 0).then(|| ((id - 100) / 7) as usize));
        p
    }
    const N: Shortcut = Shortcut::None;

    #[test]
    fn list_cursor_selection_and_ranges() {
        let p = panel(10, PanelLayout::List);
        assert!(p.handle_key(Key::Down, N));
        assert_eq!(p.cursor_index(), Some(0));
        p.handle_key(Key::Down, N);
        p.handle_key(Key::Down, N);
        assert_eq!((p.cursor(), p.selection()), (Some(114), vec![114]));
        p.handle_key(Key::Down, Shortcut::Shift);
        p.handle_key(Key::Down, Shortcut::Shift);
        assert_eq!(p.selection(), vec![114, 121, 128]);
        p.handle_key(Key::Down, Shortcut::Ctrl); // cursor only
        assert_eq!((p.cursor_index(), p.selection_len()), (Some(5), 3));
        p.handle_key(Key::from_char(' '), Shortcut::Ctrl);
        assert_eq!(p.selection(), vec![114, 121, 128, 135]);
        p.handle_key(Key::End, N);
        assert_eq!(p.selection(), vec![163]);
        assert!(!p.handle_key(Key::Down, N), "edge without on_edge is not used");
        assert!(!p.handle_key(Key::Left, N), "Left is an edge in a list");
        p.invert_selection();
        assert_eq!(p.selection_len(), 9);
        p.handle_key(Key::from_char('a'), Shortcut::Ctrl);
        assert_eq!(p.selection_len(), 10);
        assert!(!p.handle_key(Key::Tab, Shortcut::Ctrl), "Ctrl+Tab is not a panel key");
    }

    #[test]
    fn grid_moves_and_partial_last_row() {
        let p = panel(10, PanelLayout::Grid { columns: 4 }); // rows: 0-3, 4-7, 8-9
        p.handle_key(Key::Home, N);
        p.handle_key(Key::Right, N);
        p.handle_key(Key::Down, N);
        assert_eq!(p.cursor_index(), Some(5));
        p.handle_key(Key::Right, N);
        p.handle_key(Key::Right, N);
        assert_eq!(p.cursor_index(), Some(7));
        assert!(!p.handle_key(Key::Right, N), "last column is an edge");
        p.handle_key(Key::Down, N);
        assert_eq!(p.cursor_index(), Some(9), "Down into a shorter last row lands on the last item");
        let edges = Rc::new(RefCell::new(Vec::new()));
        let e2 = edges.clone();
        p.on_edge(move |k| { e2.borrow_mut().push(k); true });
        assert!(p.handle_key(Key::Down, N));
        assert_eq!(*edges.borrow(), vec![Key::Down]);
    }

    #[test]
    fn ids_survive_reorder_and_removal() {
        let p = panel(5, PanelLayout::List);
        p.click(114, false, false);
        p.click(128, false, true); // shift: 114..=128
        assert_eq!(p.selection(), vec![114, 121, 128]);
        // Reverse the order and drop 121.
        let ids = vec![128u64, 114, 107, 100];
        let ids2 = ids.clone();
        p.set_items(ids.len(), move |i| ids[i], move |id| ids2.iter().position(|&x| x == id));
        assert_eq!(p.selection(), vec![128, 114]);
        assert_eq!(p.cursor(), Some(128));
        p.handle_key(Key::Down, N);
        assert_eq!(p.cursor(), Some(114), "cursor moves in the new order");
    }
}
