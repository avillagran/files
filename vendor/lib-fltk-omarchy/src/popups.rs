//! Anchored popups in the omarchy-shell popup style (`[popups]` surface,
//! 28 px rows, hover fill): `PopupMenu` (header Preferences/Actions menus
//! and context menus), `SearchableDropdown`, `ShortcutHelp` (F1 overlay)
//! and `OperationToast` (progress with pause / cancel).

use crate::a11y::{self, Action, Live, Role};
use crate::overlay::{bg, card, close_layer, open_layer};
use crate::paint;
use crate::state::State;
use crate::theme;
use crate::widgets::glyph;
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn s(t: &theme::Theme, px: f64) -> i32 {
    (px * t.font.base as f64 / 12.0).round() as i32
}

// =============================================================== PopupMenu

/// One entry of a [`PopupMenu`].
#[derive(Debug, Clone, PartialEq)]
pub struct PopupItem {
    pub icon: String,
    pub label: String,
    /// Shortcut hint shown dimmed on the right ("Ctrl+C"), as given to
    /// [`PopupItem::shortcut`]: specs with `Mod` are shown the platform's
    /// way ([`crate::keys::label`]).
    pub shortcut: String,
    pub enabled: bool,
    /// Shows "✓" on the right (radio/check entries).
    pub checked: bool,
    /// Draws a separator after this entry.
    pub divider: bool,
    /// The app's name for the entry ("copy"), given to
    /// [`PopupMenu::open_id`]'s callback; empty unless set with [`PopupItem::id`].
    pub id: String,
    /// The entries of the submenu this entry opens ([`PopupItem::submenu`]);
    /// empty for an entry that is picked.
    pub children: Vec<PopupItem>,
}

impl PopupItem {
    pub fn new(icon: &str, label: &str) -> PopupItem {
        PopupItem {
            icon: icon.into(),
            label: label.into(),
            shortcut: String::new(),
            enabled: true,
            checked: false,
            divider: false,
            id: String::new(),
            children: Vec::new(),
        }
    }

    /// An entry that opens a submenu of `children` (which can hold
    /// submenus too). It shows a chevron on the trailing side instead of
    /// a shortcut or a check; choosing it opens the submenu, it is never
    /// picked. Open menus with submenus with [`PopupMenu::open_path`] or
    /// [`PopupMenu::open_id`] to know which entry inside was chosen.
    pub fn submenu(icon: &str, label: &str, children: Vec<PopupItem>) -> PopupItem {
        PopupItem { children, ..PopupItem::new(icon, label) }
    }

    pub fn shortcut(mut self, s: &str) -> Self { self.shortcut = crate::keys::label(s); self }
    pub fn checked(mut self, on: bool) -> Self { self.checked = on; self }
    pub fn disabled(mut self) -> Self { self.enabled = false; self }
    pub fn divider(mut self) -> Self { self.divider = true; self }
    /// Names the entry for [`PopupMenu::open_id`] ("copy", "share.email").
    pub fn id(mut self, id: &str) -> Self { self.id = id.into(); self }

    /// True when the entry opens a submenu.
    pub fn is_submenu(&self) -> bool {
        !self.children.is_empty()
    }
}

/// How a popup ended (see [`PopupHandle::on_dismiss`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismiss {
    /// An entry was chosen; `on_pick` ran first. The index is into the
    /// top-level items: for an entry inside a submenu it is the root entry
    /// that holds it ([`PopupHandle::picked_path`] has the whole path).
    Picked(usize),
    /// Esc.
    Escape,
    /// A click outside the card.
    ClickOutside,
    /// [`PopupHandle::close`] (e.g. the data the menu was built from changed).
    Closed,
}

type Rect = (i32, i32, i32, i32);

struct HandleState {
    open: bool,
    rect: Option<Rect>,
    reason: Option<Dismiss>,
    on_dismiss: Option<Box<dyn FnOnce(Dismiss)>>,
    /// Open submenus (PopupMenu): their layers, cards and the entries that
    /// opened them, from the root.
    subs: Vec<(Widget, Rect, usize)>,
    /// The path of the picked entry (PopupMenu).
    picked: Option<Vec<usize>>,
}

impl HandleState {
    fn open(rect: Rect) -> HandleState {
        HandleState { open: true, rect: Some(rect), reason: None, on_dismiss: None, subs: Vec::new(), picked: None }
    }
}

/// Handle of an open [`PopupMenu`]. Dropping it leaves the menu open.
/// Callbacks never run after the menu is gone: a menu whose window was
/// deleted is simply not open any more ([`PopupHandle::is_open`] is false).
#[derive(Clone)]
pub struct PopupHandle {
    layer: Option<Widget>,
    state: Rc<RefCell<HandleState>>,
}

impl PopupHandle {
    fn closed_now() -> PopupHandle {
        let mut s = HandleState::open((0, 0, 0, 0));
        (s.open, s.rect, s.reason) = (false, None, Some(Dismiss::Closed));
        PopupHandle { layer: None, state: Rc::new(RefCell::new(s)) }
    }

    /// Called once when the menu closes, with the reason. If it is already
    /// closed, `f` runs now.
    pub fn on_dismiss(self, f: impl FnOnce(Dismiss) + 'static) -> Self {
        let done = self.state.borrow().reason;
        match done {
            Some(r) => f(r),
            None => self.state.borrow_mut().on_dismiss = Some(Box::new(f)),
        }
        self
    }

    /// Closes the menu (and its open submenus) without picking (reason
    /// [`Dismiss::Closed`]); focus goes back to the control that had it.
    /// No-op when already closed.
    pub fn close(&self) {
        if let Some(l) = &self.layer {
            finish(&self.state, l, Dismiss::Closed);
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.borrow().open && self.layer.as_ref().is_some_and(|l| !l.was_deleted())
    }

    /// The card rectangle in window coordinates (x, y, w, h).
    pub fn rect(&self) -> Option<(i32, i32, i32, i32)> {
        self.state.borrow().rect
    }

    /// The cards of the open submenus, outermost first (empty when none is
    /// open or the menu is closed).
    pub fn submenu_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        if !self.is_open() {
            return Vec::new();
        }
        self.state.borrow().subs.iter().map(|s| s.1).collect()
    }

    /// The entries whose submenus are open, from the root: `[2, 0]` when
    /// the third root entry's submenu is open and, in it, the first
    /// entry's submenu. Empty when none is open.
    pub fn submenu_path(&self) -> Vec<usize> {
        if !self.is_open() {
            return Vec::new();
        }
        self.state.borrow().subs.iter().map(|s| s.2).collect()
    }

    /// The path of the picked entry (indices from the root, `vec![i]` in a
    /// flat menu), once [`Dismiss::Picked`].
    pub fn picked_path(&self) -> Option<Vec<usize>> {
        self.state.borrow().picked.clone()
    }

    /// Why it closed (None while open).
    pub fn dismissed(&self) -> Option<Dismiss> {
        self.state.borrow().reason
    }
}

fn finish(state: &Rc<RefCell<HandleState>>, layer: &Widget, reason: Dismiss) -> bool {
    finish_then(state, layer, reason, || {})
}

/// Closes the popup and its submenus (deepest first, so focus walks back
/// to where it was), runs `picked` (the pick callback), then `on_dismiss`.
fn finish_then(state: &Rc<RefCell<HandleState>>, layer: &Widget, reason: Dismiss, picked: impl FnOnce()) -> bool {
    let subs = {
        let mut s = state.borrow_mut();
        if !s.open {
            return false;
        }
        s.open = false;
        if layer.was_deleted() {
            // The window went away with the menu: no callbacks.
            s.on_dismiss = None;
            s.subs.clear();
            return false;
        }
        s.reason = Some(reason);
        std::mem::take(&mut s.subs)
    };
    for (l, _, _) in subs.iter().rev() {
        if !l.was_deleted() {
            crate::focus::pop_modal(l);
            close_layer(l);
        }
    }
    crate::focus::pop_modal(layer);
    close_layer(layer);
    picked();
    let f = state.borrow_mut().on_dismiss.take();
    if let Some(f) = f {
        f(reason);
    }
    true
}

/// Menu card anchored to a widget (below it, or above when there is more
/// room; right-aligned when the anchor is in the right half, like
/// omarchy-shell's PopupCard) or opened at a point (context menu).
///
/// Right to left (the anchor's direction) the entries are mirrored: glyph
/// and label on the right, shortcut and check on the left, and `open_at`
/// grows to the left of the point.
///
/// The card always fits the window: when the entries do not fit, it is
/// clamped to the window and scrolls (wheel, keys), keeping the active
/// entry visible; long labels end in "…" and keep their shortcut.
/// Keys: Up/Down or j/k (skip disabled, wrap), PageUp/PageDown, Home/End,
/// Enter or Space picks, Esc closes; Tab stays in the menu. A click outside closes.
/// `open` returns a [`PopupHandle`]: `.on_dismiss(|reason| ..)`, `.close()`.
///
/// **Submenus** ([`PopupItem::submenu`]) group long menus. A submenu is a
/// second card in the same window, beside its entry (on the right; on the
/// left when there is no room, and right to left), clamped to the window
/// and scrolling like the root card. Hovering a submenu entry opens it
/// after [`crate::anim::ms::SUBMENU_OPEN`] and closes a sibling's; moving
/// the pointer toward an open submenu keeps it open
/// ([`crate::anim::ms::SUBMENU_AIM`]); a click opens it at once. Keys act
/// on the innermost open card: Right (Left right to left), Enter or Space
/// on a submenu entry opens it on its first entry; Left (Right right to
/// left) or Esc closes that submenu and goes back to its entry; Esc on the
/// root card closes the menu. Picking any entry closes every card. Use
/// [`PopupMenu::open_path`] (indices from the root) or
/// [`PopupMenu::open_id`] ([`PopupItem::id`]) for such menus: `open`
/// reports the top-level index only.
///
/// ```no_run
/// use fltk_omarchy::popups::{PopupItem, PopupMenu};
/// use fltk_omarchy::widgets::{glyph, Button};
/// # let anchor = Button::new("Actions");
/// PopupMenu::open_id(&*anchor, vec![
///     PopupItem::new(glyph::FOLDER, "Open").id("open"),
///     PopupItem::submenu(glyph::FILE, "Edit", vec![
///         PopupItem::new("", "Copy").shortcut("Mod+C").id("copy"),
///         PopupItem::new("", "Paste").shortcut("Mod+V").id("paste"),
///     ]),
/// ], |id| println!("picked {id}"));
/// ```
pub struct PopupMenu;

struct MenuGeo {
    rect: Rect,
    /// Content-relative (y, h) of each entry.
    rows: Vec<(i32, i32)>,
    /// Inner padding (top/bottom) and the visible content height.
    pad: i32,
    view_h: i32,
    content_h: i32,
}

/// The pick callback inside a menu: the path and the picked entry.
type PickFn = Box<dyn FnOnce(Vec<usize>, &PopupItem)>;
/// What a pointer intent does when its delay has passed.
type Intent = Rc<dyn Fn(&Rc<MenuCtl>)>;

impl PopupMenu {
    /// Opens under `anchor` (or above it when there is more room).
    /// `on_pick(index)` receives the index into `items`; for an entry inside
    /// a submenu, the index of the top-level entry that holds it (use
    /// [`PopupMenu::open_path`] or [`PopupMenu::open_id`] for submenus).
    pub fn open<A: WidgetExt>(anchor: &A, items: Vec<PopupItem>, on_pick: impl FnOnce(usize) + 'static) -> PopupHandle {
        Self::open_path(anchor, items, move |p| on_pick(p[0]))
    }

    /// Like [`PopupMenu::open`], with the path of the picked entry: indices
    /// from the root (`vec![i]` for a top-level entry, `vec![2, 0]` for the
    /// first entry of the third entry's submenu).
    pub fn open_path<A: WidgetExt>(anchor: &A, items: Vec<PopupItem>, on_pick: impl FnOnce(Vec<usize>) + 'static) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let w = Self::width(&items).min(win.w() - 4).max(1);
        let right_half = anchor.x() + anchor.w() / 2 > win.w() / 2;
        let x = if right_half { anchor.x() + anchor.w() - w } else { anchor.x() };
        Self::open_rect(anchor, items, x, (anchor.y(), anchor.y() + anchor.h()), w, Box::new(move |p, _| on_pick(p)))
    }

    /// Like [`PopupMenu::open`], with the [`PopupItem::id`] of the picked
    /// entry (empty for an entry without one): no index arithmetic for
    /// menus with submenus.
    pub fn open_id<A: WidgetExt>(anchor: &A, items: Vec<PopupItem>, on_pick: impl FnOnce(&str) + 'static) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let w = Self::width(&items).min(win.w() - 4).max(1);
        let right_half = anchor.x() + anchor.w() / 2 > win.w() / 2;
        let x = if right_half { anchor.x() + anchor.w() - w } else { anchor.x() };
        Self::open_rect(anchor, items, x, (anchor.y(), anchor.y() + anchor.h()), w, Box::new(move |_, it| on_pick(&it.id)))
    }

    /// Opens at window coordinates (e.g. the mouse position for a context
    /// menu). `on_pick` as in [`PopupMenu::open`].
    pub fn open_at<A: WidgetExt>(anchor: &A, x: i32, y: i32, items: Vec<PopupItem>, on_pick: impl FnOnce(usize) + 'static) -> PopupHandle {
        Self::open_at_path(anchor, x, y, items, move |p| on_pick(p[0]))
    }

    /// [`PopupMenu::open_at`] with the path of the picked entry
    /// ([`PopupMenu::open_path`]).
    pub fn open_at_path<A: WidgetExt>(anchor: &A, x: i32, y: i32, items: Vec<PopupItem>, on_pick: impl FnOnce(Vec<usize>) + 'static) -> PopupHandle {
        Self::open_at_inner(anchor, x, y, items, Box::new(move |p, _| on_pick(p)))
    }

    /// [`PopupMenu::open_at`] with the id of the picked entry
    /// ([`PopupMenu::open_id`]).
    pub fn open_at_id<A: WidgetExt>(anchor: &A, x: i32, y: i32, items: Vec<PopupItem>, on_pick: impl FnOnce(&str) + 'static) -> PopupHandle {
        Self::open_at_inner(anchor, x, y, items, Box::new(move |_, it| on_pick(&it.id)))
    }

    fn open_at_inner<A: WidgetExt>(anchor: &A, x: i32, y: i32, items: Vec<PopupItem>, on_pick: PickFn) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let w = Self::width(&items).min(win.w() - 4).max(1);
        // Right to left the card grows to the left of the point.
        let x = if crate::layout::is_rtl(anchor) { x - w } else { x };
        Self::open_rect(anchor, items, x, (y, y), w, on_pick)
    }

    /// Natural width: widest label + shortcut + icon and trailing columns
    /// (the check column also holds a submenu entry's chevron).
    fn width(items: &[PopupItem]) -> i32 {
        theme::with(|t| {
            let pad = t.spacing.control_padding_x;
            let mut w = s(t, 200.0);
            for it in items {
                let lw = crate::widgets::measure_offscreen(&it.label, t.font.body);
                let sw = if it.shortcut.is_empty() || it.is_submenu() { 0 } else { crate::widgets::measure_offscreen(&it.shortcut, t.font.caption) + t.spacing.xl };
                w = w.max(2 * pad + s(t, 22.0) + t.spacing.lg + lw + sw + s(t, 18.0) + 4 + SCROLLBAR);
            }
            w
        })
    }

    /// Content-relative rows and the content height.
    fn rows(t: &theme::Theme, items: &[PopupItem]) -> (Vec<(i32, i32)>, i32) {
        let row = t.spacing.popup_row_height;
        let mut rows = Vec::with_capacity(items.len());
        let mut y = 0;
        for it in items {
            rows.push((y, row));
            y += row + if it.divider { t.spacing.md + 1 } else { 0 };
        }
        (rows, y)
    }

    /// Lays the card out inside the window: below `span.1` if it fits, else
    /// above `span.0` if it fits, else on the larger side, clamped and scrolling.
    fn geometry(items: &[PopupItem], win_h: i32, x: i32, span: (i32, i32), w: i32) -> MenuGeo {
        theme::with(|t| {
            let row = t.spacing.popup_row_height;
            let pad = 2 + t.spacing.xxs;
            let (rows, content_h) = Self::rows(t, items);
            let full = content_h + 2 * pad;
            let margin = 2;
            let below = win_h - margin - (span.1 + 2);
            let above = span.0 - 2 - margin;
            let (top, h) = if full <= below {
                (span.1 + 2, full)
            } else if full <= above {
                (span.0 - 2 - full, full)
            } else if below >= above && below >= row + 2 * pad {
                (span.1 + 2, below)
            } else if above >= row + 2 * pad {
                (margin, above)
            } else {
                // No room on either side: use the whole window height.
                (margin, (win_h - 2 * margin).max(1).min(full))
            };
            MenuGeo { rect: (x, top, w, h), rows, pad, view_h: (h - 2 * pad).max(0), content_h }
        })
    }

    /// A submenu's card: beside `parent` (overlapping its border a little),
    /// on the trailing side if it fits, else on the other side; its first
    /// entry level with the parent entry at `row_y`; clamped to the window
    /// (and scrolling when taller).
    fn sub_geometry(items: &[PopupItem], win: (i32, i32), parent: Rect, row_y: i32, rtl: bool) -> MenuGeo {
        let (win_w, win_h) = win;
        let w = Self::width(items).min(win_w - 4).max(1);
        theme::with(|t| {
            let pad = 2 + t.spacing.xxs;
            let (rows, content_h) = Self::rows(t, items);
            let margin = 2;
            let h = (content_h + 2 * pad).min(win_h - 2 * margin).max(1);
            let top = (row_y - pad).min(win_h - margin - h).max(margin);
            let overlap = SUBMENU_OVERLAP;
            let right = parent.0 + parent.2 - overlap;
            let left = parent.0 + overlap - w;
            let fits_right = right + w <= win_w - margin;
            let fits_left = left >= margin;
            let x = match (rtl, fits_right, fits_left) {
                (false, true, _) | (true, true, false) => right,
                (true, _, true) | (false, false, true) => left,
                // Room on neither side: the larger one, overlapping the parent.
                _ => if win_w - margin - right >= parent.0 + overlap - margin { right } else { left },
            };
            let x = x.clamp(margin, (win_w - w - margin).max(margin));
            MenuGeo { rect: (x, top, w, h), rows, pad, view_h: (h - 2 * pad).max(0), content_h }
        })
    }

    fn open_rect<A: WidgetExt>(anchor: &A, items: Vec<PopupItem>, x: i32, span: (i32, i32), w: i32, on_pick: PickFn) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let x = x.clamp(2, (win.w() - w - 2).max(2));
        let g = Self::geometry(&items, win.h(), x, span, w);
        let Some(layer) = open_layer(anchor, None, true) else { return PopupHandle::closed_now() };
        let state = Rc::new(RefCell::new(HandleState::open(g.rect)));
        let handle = PopupHandle { layer: Some(layer.clone()), state: state.clone() };
        let first = items.iter().position(|i| i.enabled).map(|i| i as i32).unwrap_or(-1);
        let root = Rc::new(Level::new(layer, items, g, None, first));
        // The anchor's direction: right to left, glyphs on the right,
        // shortcuts and checks on the left, the scrollbar on the left,
        // submenus open to the left.
        let ctl = Rc::new(MenuCtl {
            state,
            levels: RefCell::new(vec![root.clone()]),
            on_pick: RefCell::new(Some(on_pick)),
            rtl: crate::layout::is_rtl(anchor),
            gen: Cell::new(0),
            last: Cell::new(None),
        });
        ctl.install(&root);
        handle
    }

    /// Content and visible heights the menu would get (tests, layout checks):
    /// `(card height, content height)`.
    pub fn measure(items: &[PopupItem], window_h: i32, anchor_bottom: i32) -> (i32, i32) {
        let g = Self::geometry(items, window_h, 0, (anchor_bottom, anchor_bottom), 200);
        (g.rect.3, g.content_h)
    }
}

/// Width of the popup scrollbar column.
const SCROLLBAR: i32 = 6;
/// How far a submenu card overlaps its parent card.
const SUBMENU_OVERLAP: i32 = 3;

/// One card of an open menu: the root or a submenu.
struct Level {
    layer: Widget,
    items: Vec<PopupItem>,
    geo: MenuGeo,
    /// The entry of the previous card that opened it (None for the root).
    parent: Option<usize>,
    /// The highlighted entry (-1: none).
    cur: Cell<i32>,
    scroll: Cell<i32>,
    motion: RefCell<crate::scroll::Scroll>,
}

impl Level {
    fn new(layer: Widget, items: Vec<PopupItem>, geo: MenuGeo, parent: Option<usize>, cur: i32) -> Level {
        Level { layer, items, geo, parent, cur: Cell::new(cur), scroll: Cell::new(0), motion: RefCell::new(crate::scroll::Scroll::new()) }
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        let (rx, ry, rw, rh) = self.geo.rect;
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    }

    /// The entry under a window point.
    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        let (rx, ry, rw, _) = self.geo.rect;
        let top = ry + self.geo.pad;
        if x < rx || x >= rx + rw || y < top || y >= top + self.geo.view_h {
            return None;
        }
        let cy = y - top + self.scroll.get();
        self.geo.rows.iter().position(|(r, h)| cy >= *r && cy < r + h)
    }

    /// Window y of an entry's top.
    fn row_y(&self, i: usize) -> i32 {
        self.geo.rect.1 + self.geo.pad + self.geo.rows[i].0 - self.scroll.get()
    }

    fn max_scroll(&self) -> i32 {
        (self.geo.content_h - self.geo.view_h).max(0)
    }

    fn reveal(&self, i: i32) {
        if i < 0 {
            return;
        }
        let (ry, rh) = self.geo.rows[i as usize];
        let sc = self.scroll.get();
        if ry < sc {
            self.scroll.set(ry)
        } else if ry + rh > sc + self.geo.view_h {
            self.scroll.set((ry + rh - self.geo.view_h).min(self.max_scroll()))
        }
    }

    fn enabled(&self) -> Vec<i32> {
        self.items.iter().enumerate().filter(|(_, i)| i.enabled).map(|(n, _)| n as i32).collect()
    }

    /// Next / previous enabled entry, wrapping (from none: the first / last).
    fn step(&self, d: i32) {
        let n = self.items.len() as i32;
        if !self.items.iter().any(|i| i.enabled) {
            return;
        }
        let mut c = self.cur.get();
        if c < 0 {
            c = if d > 0 { -1 } else { n };
        }
        for _ in 0..n {
            c = (c + d).rem_euclid(n.max(1));
            if self.items[c as usize].enabled {
                break;
            }
        }
        self.cur.set(c);
    }

    /// Page moves: by the rows that fit, no wrap, landing on an enabled entry.
    fn page(&self, dir: i32) {
        let enabled = self.enabled();
        if enabled.is_empty() {
            return;
        }
        let cur = self.cur.get();
        let rows = (self.geo.view_h / theme::with(|t| t.spacing.popup_row_height)).max(1);
        let target = (cur.max(0) + dir * rows).clamp(0, self.items.len() as i32 - 1);
        let pick = if dir > 0 {
            enabled.iter().copied().filter(|&e| e <= target).max().filter(|&e| e > cur).or_else(|| enabled.iter().copied().find(|&e| e > cur))
        } else {
            enabled.iter().copied().filter(|&e| e >= target).min().filter(|&e| e < cur).or_else(|| enabled.iter().copied().rev().find(|&e| e < cur))
        };
        if let Some(p) = pick {
            self.cur.set(p)
        }
    }

    fn draw(&self, l: &mut Widget, rtl: bool) {
        let gd = &self.geo;
        let mut motion = self.motion.borrow_mut();
        self.scroll.set(motion.position(self.scroll.get() as i64, self.max_scroll() as i64) as i32);
        if motion.running() { crate::anim::request_redraw(l); }
        drop(motion);
        if let Some(win) = l.top_window() {
            if (l.w(), l.h()) != (win.w(), win.h()) { l.resize(0, 0, win.w(), win.h()); }
        }
        let sc = self.scroll.get();
        let cur = self.cur.get();
        theme::with(|t| {
            let (x, y, w, h) = gd.rect;
            let m = crate::layout::Mirror::new(rtl, x, w);
            card(t, x, y, w, h, &t.popups, t.controls.normal.border_width.max(1));
            let fill = t.popups.fill(bg(t));
            let fg = t.popups.text;
            let inset = 2;
            let pad = t.spacing.control_padding_x;
            let scrolls = gd.content_h > gd.view_h;
            let right_edge = x + w - pad - if scrolls { SCROLLBAR } else { 0 };
            let top = y + gd.pad;
            fltk::draw::push_clip(x + 1, top, w - 2, gd.view_h);
            for (i, (item, &(oy, rh))) in self.items.iter().zip(gd.rows.iter()).enumerate() {
                let ry = top + oy - sc;
                if ry + rh < top || ry > top + gd.view_h { continue; }
                if i as i32 == cur && item.enabled {
                    let hw = w - 2 * inset - if scrolls { SCROLLBAR } else { 0 };
                    paint::fill(m.x(x + inset, hw), ry, hw, rh, t.radius, t.fill(&t.controls.hover, fill));
                }
                let color = if item.enabled { fg } else { fg.darker(2.0) };
                let cell = s(t, 22.0);
                if !item.icon.is_empty() {
                    paint::glyph(&item.icon, m.x(x + pad, cell), ry, cell, rh, t.font.icon, color);
                }
                let tx = x + pad + cell + t.spacing.lg;
                let mut right = right_edge;
                if item.is_submenu() {
                    // A chevron toward the side the submenu opens on, in the check column.
                    let cw = s(t, 14.0);
                    paint::glyph(m.glyph(glyph::SUBMENU, glyph::SUBMENU_RTL), m.x(right - cw, cw), ry, cw, rh, t.font.body, color);
                    right -= s(t, 18.0);
                } else {
                    if item.checked {
                        paint::glyph(glyph::CHECKMARK, m.x(right - s(t, 14.0), s(t, 14.0)), ry, s(t, 14.0), rh, t.font.body, t.palette.accent);
                        right -= s(t, 18.0);
                    }
                    if !item.shortcut.is_empty() {
                        let sw = paint::measure(&item.shortcut, t.font.caption, false);
                        paint::text(&item.shortcut, m.x(right - sw, sw), ry, sw, rh, t.font.caption, false, fg.darker(1.4), m.align(Align::Right));
                        right -= sw + t.spacing.xl;
                    }
                }
                let lw = (right - tx).max(0);
                paint::text_elided(&item.label, m.x(tx, lw), ry, lw, rh, t.font.body, false, color, m.align(Align::Left));
                if item.divider {
                    paint::hairline(x + inset + pad, ry + rh + t.spacing.md / 2, w - 2 * (inset + pad), fg, fill, 0.12);
                }
            }
            fltk::draw::pop_clip();
            if scrolls {
                let track = gd.view_h;
                let thumb = ((gd.view_h as f64 / gd.content_h as f64) * track as f64).max(16.0) as i32;
                let max = (gd.content_h - gd.view_h).max(1);
                let ty = top + ((sc as f64 / max as f64) * (track - thumb) as f64) as i32;
                paint::fill(m.x(x + w - SCROLLBAR - 2, SCROLLBAR - 2), ty, SCROLLBAR - 2, thumb, (SCROLLBAR - 2) / 2, fg.over(fill, 0.3));
            }
        });
    }
}

/// An open menu: its cards (root first), shared by their layers. Pointer
/// events reach the topmost layer and are routed to the card under the
/// pointer; keys reach the focused (innermost) card.
struct MenuCtl {
    state: Rc<RefCell<HandleState>>,
    levels: RefCell<Vec<Rc<Level>>>,
    on_pick: RefCell<Option<PickFn>>,
    rtl: bool,
    /// Generation of the pending pointer intent (submenu open/close, aim):
    /// a newer one cancels it.
    gen: Cell<u64>,
    /// The last pointer position over the menu (submenu aim).
    last: Cell<Option<(i32, i32)>>,
}

impl MenuCtl {
    fn open(&self) -> bool {
        self.state.borrow().open
    }

    fn levels(&self) -> Vec<Rc<Level>> {
        self.levels.borrow().clone()
    }

    fn depth_of(&self, level: &Rc<Level>) -> Option<usize> {
        self.levels.borrow().iter().position(|l| Rc::ptr_eq(l, level))
    }

    /// The handle's view of the open submenus.
    fn sync(&self) {
        let subs = self.levels.borrow().iter().skip(1).map(|l| (l.layer.clone(), l.geo.rect, l.parent.unwrap_or(0))).collect();
        self.state.borrow_mut().subs = subs;
    }

    fn redraw(&self) {
        for l in self.levels() {
            l.layer.clone().redraw();
        }
    }

    /// Cancels a pending pointer intent; returns the new generation.
    fn bump(&self) -> u64 {
        let g = self.gen.get() + 1;
        self.gen.set(g);
        g
    }

    /// Closes the submenus from `depth` on (deepest first: focus walks back
    /// to the card under them).
    fn close_from(&self, depth: usize) {
        let gone: Vec<Rc<Level>> = {
            let mut lv = self.levels.borrow_mut();
            if depth == 0 || depth >= lv.len() {
                return;
            }
            lv.drain(depth..).collect()
        };
        self.sync();
        for l in gone.iter().rev() {
            if !l.layer.was_deleted() {
                crate::focus::pop_modal(&l.layer);
                close_layer(&l.layer);
            }
        }
        self.redraw();
    }

    /// Opens the submenu of entry `row` of card `depth` (closing deeper
    /// ones). From the keyboard its first enabled entry is highlighted.
    fn open_sub(self: &Rc<Self>, depth: usize, row: usize, keyboard: bool) {
        self.bump();
        self.close_from(depth + 1);
        let Some(parent) = self.levels().get(depth).cloned() else { return };
        let Some(item) = parent.items.get(row) else { return };
        if !item.enabled || !item.is_submenu() {
            return;
        }
        let Some(win) = parent.layer.top_window() else { return };
        parent.cur.set(row as i32);
        parent.reveal(row as i32);
        let row_y = parent.row_y(row).clamp(parent.geo.rect.1, parent.geo.rect.1 + parent.geo.rect.3);
        let items = item.children.clone();
        let g = PopupMenu::sub_geometry(&items, (win.w(), win.h()), parent.geo.rect, row_y, self.rtl);
        let Some(layer) = open_layer(&parent.layer, None, true) else { return };
        let first = if keyboard { items.iter().position(|i| i.enabled).map(|i| i as i32).unwrap_or(-1) } else { -1 };
        let level = Rc::new(Level::new(layer, items, g, Some(row), first));
        self.levels.borrow_mut().push(level.clone());
        self.sync();
        self.install(&level);
        self.redraw();
    }

    /// The path of entry `row` of card `depth`, from the root.
    fn path(&self, depth: usize, row: usize) -> Vec<usize> {
        let mut p: Vec<usize> = self.levels.borrow().iter().take(depth + 1).skip(1).filter_map(|l| l.parent).collect();
        p.push(row);
        p
    }

    /// Chooses entry `row` of card `depth`: opens a submenu, or picks a
    /// leaf (closes every card, then `on_pick`, then `on_dismiss`).
    fn activate(self: &Rc<Self>, depth: usize, row: usize, keyboard: bool) {
        let Some(level) = self.levels().get(depth).cloned() else { return };
        let Some(item) = level.items.get(row) else { return };
        if !item.enabled {
            return;
        }
        if item.is_submenu() {
            self.open_sub(depth, row, keyboard);
            return;
        }
        self.bump();
        let path = self.path(depth, row);
        let item = item.clone();
        let root = self.levels.borrow()[0].layer.clone();
        self.state.borrow_mut().picked = Some(path.clone());
        let f = self.on_pick.borrow_mut().take();
        let done = finish_then(&self.state, &root, Dismiss::Picked(path[0]), move || {
            if let Some(f) = f {
                f(path, &item);
            }
        });
        if !done {
            self.state.borrow_mut().picked = None;
        }
    }

    /// Runs `f` once `deadline` (on the animation clock) has passed, unless
    /// a newer pointer intent replaced this one or the menu closed. With a
    /// frozen clock ([`crate::anim::clock`]) it checks every frame, so
    /// tests step through the delay with `clock::advance`.
    fn after(self: &Rc<Self>, gen: u64, deadline: std::time::Instant, f: Intent) {
        let now = crate::anim::now();
        let wait = if crate::anim::clock::frozen() { 1.0 / 60.0 } else { deadline.saturating_duration_since(now).as_secs_f64().max(0.001) };
        let me = Rc::downgrade(self);
        app::add_timeout3(wait, move |_| {
            let Some(me) = me.upgrade() else { return };
            if me.gen.get() != gen || !me.open() {
                return;
            }
            if crate::anim::now() >= deadline {
                f(&me);
            } else {
                me.after(gen, deadline, f.clone());
            }
        });
    }

    /// The pointer rests on entry `row` of card `depth`: after the hover
    /// delay, its submenu replaces the open one (or the open one closes).
    fn hover_intent(self: &Rc<Self>, depth: usize, row: usize) {
        let gen = self.bump();
        let deadline = crate::anim::now() + std::time::Duration::from_millis(crate::anim::ms::SUBMENU_OPEN as u64);
        self.after(gen, deadline, Rc::new(move |me: &Rc<MenuCtl>| me.settle(depth, row)));
    }

    fn settle(self: &Rc<Self>, depth: usize, row: usize) {
        let levels = self.levels();
        if depth >= levels.len() || levels.get(depth + 1).is_some_and(|c| c.parent == Some(row)) {
            return;
        }
        self.close_from(depth + 1);
        let it = &levels[depth].items[row];
        if it.enabled && it.is_submenu() {
            self.open_sub(depth, row, false);
        }
    }

    /// Pointer motion at window `(x, y)`; `aim`: keep an open submenu while
    /// the pointer heads for it.
    fn pointer(self: &Rc<Self>, x: i32, y: i32, aim: bool) {
        let prev = self.last.replace(Some((x, y)));
        let levels = self.levels();
        let Some(d) = (0..levels.len()).rev().find(|&d| levels[d].contains(x, y)) else { return };
        let lv = &levels[d];
        // The cards under it stay on the entries that opened the ones above.
        for k in 0..d {
            if let Some(p) = levels[k + 1].parent {
                if levels[k].cur.replace(p as i32) != p as i32 {
                    levels[k].layer.clone().redraw();
                }
            }
        }
        let row = lv.row_at(x, y);
        let child = levels.get(d + 1);
        if let Some(c) = child {
            if row.is_some() && row == c.parent {
                self.bump();
                if lv.cur.replace(row.unwrap_or(0) as i32) != row.unwrap_or(0) as i32 {
                    lv.layer.clone().redraw();
                }
                return;
            }
            if aim && prev.is_some_and(|p| p != (x, y) && aiming(p, (x, y), c.geo.rect, lv.geo.rect)) {
                // Heading for the open submenu: wait; if the pointer settles
                // here instead, act on where it is then.
                let gen = self.bump();
                let deadline = crate::anim::now() + std::time::Duration::from_millis(crate::anim::ms::SUBMENU_AIM as u64);
                self.after(gen, deadline, Rc::new(|me: &Rc<MenuCtl>| {
                    if let Some((x, y)) = me.last.get() {
                        me.pointer_settled(x, y);
                    }
                }));
                return;
            }
        }
        let Some(row) = row else { return };
        if lv.cur.replace(row as i32) != row as i32 {
            lv.layer.clone().redraw();
        }
        if child.is_some() || (lv.items[row].enabled && lv.items[row].is_submenu()) {
            self.hover_intent(d, row);
        } else {
            self.bump();
        }
    }

    /// After the aim grace: what the pointer is on now takes effect at once.
    fn pointer_settled(self: &Rc<Self>, x: i32, y: i32) {
        let levels = self.levels();
        let Some(d) = (0..levels.len()).rev().find(|&d| levels[d].contains(x, y)) else { return };
        let Some(row) = levels[d].row_at(x, y) else { return };
        levels[d].cur.set(row as i32);
        self.settle(d, row);
        self.redraw();
    }

    fn push(self: &Rc<Self>, x: i32, y: i32) {
        self.bump();
        let levels = self.levels();
        let Some(d) = (0..levels.len()).rev().find(|&d| levels[d].contains(x, y)) else {
            let root = levels[0].layer.clone();
            finish(&self.state, &root, Dismiss::ClickOutside);
            return;
        };
        let lv = &levels[d];
        match lv.row_at(x, y) {
            // The entry of the open submenu: it stays open.
            Some(r) if levels.get(d + 1).is_some_and(|c| c.parent == Some(r)) => self.close_from(d + 2),
            Some(r) => {
                lv.cur.set(r as i32);
                if lv.items[r].enabled {
                    self.activate(d, r, false);
                } else {
                    self.close_from(d + 1);
                }
            }
            None => self.close_from(d + 1),
        }
        if self.open() {
            self.redraw();
        }
    }

    /// A key on the innermost card. False: not a menu key (the menu closed
    /// and the key goes on to the app).
    fn key(self: &Rc<Self>) -> bool {
        self.bump();
        let levels = self.levels();
        let d = levels.len() - 1;
        let lv = levels[d].clone();
        let k = app::event_key();
        let cmd = crate::overlay::command_mods();
        // Left / Right as the reader means them (mirrored right to left).
        let lk = crate::layout::Mirror::new(self.rtl, 0, 0).key(k);
        let cur = lv.cur.get();
        let on_sub = cur >= 0 && lv.items[cur as usize].enabled && lv.items[cur as usize].is_submenu();
        let space = k == Key::from_char(' ') && !app::event_state().intersects(Shortcut::Ctrl | Shortcut::Alt | Shortcut::Meta | Shortcut::Shift);
        match k {
            Key::Escape if !cmd => {
                if d > 0 {
                    self.close_from(d);
                } else {
                    finish(&self.state, &lv.layer, Dismiss::Escape);
                }
                return true;
            }
            Key::Tab => return true, // Tab stays in the menu
            Key::Left | Key::Right if !cmd && lk == Key::Right && on_sub => { self.open_sub(d, cur as usize, true); return true; }
            Key::Left | Key::Right if !cmd && lk == Key::Left && d > 0 => { self.close_from(d); return true; }
            // Inside a submenu Left / Right are menu keys even when they do nothing.
            Key::Left | Key::Right if !cmd && d > 0 => return true,
            Key::Enter | Key::KPEnter if !cmd => { if cur >= 0 { self.activate(d, cur as usize, true); } return true; }
            _ if space => { if cur >= 0 { self.activate(d, cur as usize, true); } return true; }
            Key::Down if !cmd => lv.step(1),
            Key::Up if !cmd => lv.step(-1),
            Key::PageDown if !cmd => lv.page(1),
            Key::PageUp if !cmd => lv.page(-1),
            Key::Home if !cmd => { if let Some(&e) = lv.enabled().first() { lv.cur.set(e) } }
            Key::End if !cmd => { if let Some(&e) = lv.enabled().last() { lv.cur.set(e) } }
            _ if crate::overlay::plain_letter(k, 'j') => lv.step(1),
            _ if crate::overlay::plain_letter(k, 'k') => lv.step(-1),
            _ => {
                // Not a menu key (F1, F10, Ctrl+…): close the menu, give focus
                // back and let the key reach the app's shortcuts.
                let root = levels[0].layer.clone();
                finish(&self.state, &root, Dismiss::Closed);
                return false;
            }
        }
        lv.reveal(lv.cur.get());
        lv.layer.clone().redraw();
        true
    }

    fn wheel(&self, x: i32, y: i32) {
        let levels = self.levels();
        let d = (0..levels.len()).rev().find(|&d| levels[d].contains(x, y)).unwrap_or(levels.len() - 1);
        let lv = &levels[d];
        let row = theme::with(|t| t.spacing.popup_row_height);
        let to = lv.motion.borrow_mut().wheel(lv.scroll.get() as i64, app::event_dy_value() as f64 * row as f64, lv.max_scroll() as i64);
        lv.scroll.set(to as i32);
        if lv.max_scroll() > 0 && d + 1 < levels.len() {
            // Its entries scroll away from the submenus they opened.
            self.bump();
            self.close_from(d + 1);
        }
        lv.layer.clone().redraw();
    }

    fn handle(self: &Rc<Self>, ev: Event) -> bool {
        if !self.open() {
            return matches!(ev, Event::Focus | Event::Enter);
        }
        if matches!(ev, Event::Push | Event::KeyDown) {
            for l in self.levels() {
                l.motion.borrow_mut().jump(l.scroll.get() as i64);
            }
        }
        let (ex, ey) = app::event_coords();
        match ev {
            // FLTK sends Enter, not Move, to a layer the pointer reaches first
            // (a submenu that just opened under it).
            Event::Move | Event::Enter => { self.pointer(ex, ey, true); true }
            Event::Push => { self.push(ex, ey); true }
            Event::MouseWheel => { self.wheel(ex, ey); true }
            Event::KeyDown => self.key(),
            Event::Focus => true,
            _ => false,
        }
    }

    /// Draw, events and the accessible description of a card's layer.
    fn install(self: &Rc<Self>, level: &Rc<Level>) {
        let mut layer = level.layer.clone();
        let (lv, rtl) = (level.clone(), self.rtl);
        crate::overlay::draw_layer(&mut layer, move |l| lv.draw(l, rtl));

        let (lv, me) = (level.clone(), Rc::downgrade(self));
        a11y::describe(&layer, move |n| {
            let gd = &lv.geo;
            let (x, y, w, h) = gd.rect;
            n.role(Role::Menu).bounds(x, y, w, h);
            let me = me.upgrade();
            // A submenu is named after its entry.
            if let (Some(me), Some(p)) = (&me, lv.parent) {
                let levels = me.levels.borrow();
                if let Some(d) = levels.iter().position(|l| Rc::ptr_eq(l, &lv)).filter(|&d| d > 0) {
                    if let Some(it) = levels[d - 1].items.get(p) {
                        n.name(&it.label);
                    }
                }
            }
            let open_child = me.as_ref().and_then(|me| {
                let levels = me.levels.borrow();
                let d = levels.iter().position(|l| Rc::ptr_eq(l, &lv))?;
                levels.get(d + 1).and_then(|c| c.parent)
            });
            let c = lv.cur.get().max(0) as usize;
            let top = y + gd.pad;
            let sc = lv.scroll.get();
            let it = &lv.items;
            for i in c.saturating_sub(100)..(c + 101).min(it.len()) {
                let item = &it[i];
                let (oy, rh) = gd.rows[i];
                n.child(i as u64, |m| {
                    m.role(if item.checked && !item.is_submenu() { Role::MenuItemCheckBox } else { Role::MenuItem }).name(&item.label).shortcut(&item.shortcut)
                        .disabled(!item.enabled).position(i + 1, it.len()).action(Action::Click).action(Action::Focus);
                    if item.is_submenu() {
                        let open = open_child == Some(i);
                        m.has_popup(true).expanded(open).action(if open { Action::Collapse } else { Action::Expand });
                    } else if item.checked {
                        m.toggled(true);
                    }
                    let ry = top + oy - sc;
                    if ry + rh > top && ry < top + gd.view_h {
                        m.bounds(x + 2, ry, w - 4, rh);
                    }
                    if i as i32 == lv.cur.get() {
                        m.focused();
                    }
                });
            }
        });

        let (lv, me) = (level.clone(), Rc::downgrade(self));
        a11y::on_action(&layer, move |r| {
            let Some(me) = me.upgrade() else { return false };
            let Some(d) = me.depth_of(&lv).filter(|_| me.open()) else { return false };
            let Some(i) = r.key().map(|k| k as usize).filter(|i| *i < lv.items.len()) else { return false };
            let item = &lv.items[i];
            let open = me.levels.borrow().get(d + 1).is_some_and(|c| c.parent == Some(i));
            match r.action {
                Action::Click if item.enabled => me.activate(d, i, true),
                Action::Expand if item.enabled && item.is_submenu() => me.open_sub(d, i, true),
                Action::Collapse if open => me.close_from(d + 1),
                Action::Focus | Action::ScrollIntoView if item.enabled => {
                    lv.cur.set(i as i32);
                    lv.reveal(i as i32);
                    lv.layer.clone().redraw();
                }
                _ => return false,
            }
            true
        });

        // The layers keep the menu alive (it holds no strong reference to them).
        let me = self.clone();
        layer.handle(move |_, ev| me.handle(ev));
    }
}

/// True when the pointer, moving from `from` to `to` over `parent`, heads
/// for the submenu card `sub`: `to` lies in the triangle between `from` and
/// the submenu's near edge.
fn aiming(from: (i32, i32), to: (i32, i32), sub: Rect, parent: Rect) -> bool {
    let right = sub.0 + sub.2 / 2 >= parent.0 + parent.2 / 2;
    let ex = if right { sub.0 } else { sub.0 + sub.2 };
    let (a, b, c) = (from, (ex, sub.1 - 4), (ex, sub.1 + sub.3 + 4));
    let cross = |p: (i32, i32), q: (i32, i32), r: (i32, i32)| -> i64 {
        (q.0 - p.0) as i64 * (r.1 - p.1) as i64 - (q.1 - p.1) as i64 * (r.0 - p.0) as i64
    };
    let (d1, d2, d3) = (cross(a, b, to), cross(b, c, to), cross(c, a, to));
    let neg = d1 < 0 || d2 < 0 || d3 < 0;
    let pos = d1 > 0 || d2 > 0 || d3 > 0;
    !(neg && pos)
}

// ====================================================== SearchableDropdown

/// Omarchy `SearchableDropdown`: Dropdown trigger (260 px) whose popup has
/// a search line on top (type to filter, label or description), "No
/// matches" when empty, rows with an optional description line.
#[derive(Clone)]
pub struct SearchableDropdown {
    inner: Widget,
    options: Rc<Vec<(String, String)>>,
    value: State<Option<usize>>,
}
fltk::widget_extends!(SearchableDropdown, Widget, inner);

impl SearchableDropdown {
    /// `options`: (label, description). `value`: selected index or None.
    pub fn new(placeholder: &str, options: Vec<(String, String)>, value: &State<Option<usize>>) -> SearchableDropdown {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        inner.set_visible_focus();
        let options = Rc::new(options);
        let hover = Rc::new(Cell::new(false));
        let (opts, v, hv, ph) = (options.clone(), value.clone(), hover.clone(), placeholder.to_string());
        inner.draw(move |w| {
            theme::with(|t| {
                let focused = crate::focus::show_ring(w);
                let st = if focused { &t.controls.focus } else if hv.get() { &t.controls.hover } else { &t.controls.normal };
                let under = bg(t);
                paint::surface(w.x(), w.y(), w.w(), w.h(), t.radius, Some(t.fill(st, under)), Some((t.border(st, under), st.border_width)));
                let fg = t.palette.foreground;
                let pad = t.spacing.control_padding_x + 1;
                let chev = paint::measure(glyph::CHEVRON_DOWN, t.font.body, false);
                let (text, color) = match v.get().and_then(|i| opts.get(i)) {
                    Some((l, _)) => (l.clone(), fg),
                    None => (ph.clone(), fg.darker(1.5)),
                };
                let m = crate::layout::Mirror::of(w);
                let tw = w.w() - 2 * pad - chev - t.spacing.md;
                paint::text_elided(&text, m.x(w.x() + pad, tw), w.y(), tw, w.h(), t.font.body, false, color, m.align(Align::Left));
                paint::glyph(glyph::CHEVRON_DOWN, m.x(w.x() + w.w() - 9 - chev, chev), w.y(), chev, w.h(), t.font.body, fg.darker(1.2));
            });
        });
        let (opts, v, hv) = (options.clone(), value.clone(), hover.clone());
        inner.handle(move |w, ev| match ev {
            Event::Enter => { hv.set(true); w.redraw(); true }
            Event::Leave => { hv.set(false); w.redraw(); true }
            Event::Focus | Event::Unfocus => { w.redraw(); true }
            Event::Push => { let _ = w.take_focus(); open_search(w, &opts, &v); true }
            Event::KeyDown if matches!(app::event_key(), Key::Enter | Key::KPEnter | Key::Down) => { open_search(w, &opts, &v); true }
            _ => false,
        });
        let (w, h) = theme::with(|t| (t.spacing.searchable_dropdown_width, t.spacing.control_height));
        inner.set_size(w, h);
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        let (opts, v, me, ph) = (options.clone(), value.clone(), inner.clone(), placeholder.to_string());
        a11y::describe(&inner, move |n| {
            let text = v.get().and_then(|i| opts.get(i)).map(|(l, _)| l.clone()).unwrap_or_default();
            n.role(Role::ComboBox).name(&ph).placeholder(&ph).value(&text).expanded(a11y::has_popup(&me)).focusable()
                .action(Action::Click).action(Action::Expand);
        });
        let (opts, v, mut me) = (options.clone(), value.clone(), inner.clone());
        a11y::on_action(&inner, move |r| {
            if !matches!(r.action, Action::Click | Action::Expand) || a11y::has_popup(&me) {
                return false;
            }
            open_search(&mut me, &opts, &v);
            true
        });
        SearchableDropdown { inner, options, value: value.clone() }
    }

    pub fn selected(&self) -> Option<usize> {
        self.value.get()
    }

    pub fn open(&mut self) {
        let (o, v) = (self.options.clone(), self.value.clone());
        open_search(&mut self.inner, &o, &v);
    }

    /// Filter used by the popup (case-insensitive substring on label or description).
    pub fn filter(options: &[(String, String)], q: &str) -> Vec<usize> {
        let q = q.trim().to_lowercase();
        options.iter().enumerate()
            .filter(|(_, (l, d))| q.is_empty() || l.to_lowercase().contains(&q) || d.to_lowercase().contains(&q))
            .map(|(i, _)| i).collect()
    }
}

fn open_search(trigger: &mut Widget, options: &Rc<Vec<(String, String)>>, value: &State<Option<usize>>) {
    let Some(mut layer) = open_layer(trigger, None, true) else { return };
    let query = Rc::new(RefCell::new(String::new()));
    let visible = Rc::new(RefCell::new(SearchableDropdown::filter(options, "")));
    let cur = Rc::new(Cell::new(0usize));
    let top = Rc::new(Cell::new(0usize));
    let (tx, ty, tw) = (trigger.x(), trigger.y() + trigger.h() + 2, trigger.w());
    // Right to left: query and rows right-aligned.
    let al = crate::layout::Mirror::new(crate::layout::is_rtl(trigger), tx, tw).align(Align::Left);
    let geo = move || theme::with(|t| {
        let row = t.spacing.popup_row_height.max(t.font.body + t.font.caption + 12);
        let header = t.spacing.popup_row_height + t.spacing.xl;
        let rows = 6usize;
        let h = (header + 1 + rows as i32 * (row + t.spacing.sm) + 4).max(220);
        (row, header, rows, h)
    });
    let (opts, q, vis, cu, tp) = (options.clone(), query.clone(), visible.clone(), cur.clone(), top.clone());
    crate::overlay::draw_layer(&mut layer, move |l| {
        if let Some(win) = l.top_window() { if (l.w(), l.h()) != (win.w(), win.h()) { l.resize(0, 0, win.w(), win.h()); } }
        let (row, header, rows, h) = geo();
        theme::with(|t| {
            card(t, tx, ty, tw, h, &t.popups, t.controls.normal.border_width.max(1));
            let fill = t.popups.fill(bg(t));
            let fg = t.popups.text;
            let m = t.spacing.md;
            let fh = header - 2 * m;
            let st = &t.controls.focus;
            paint::surface(tx + m, ty + m, tw - 2 * m, fh, t.radius, Some(t.fill(st, fill)), Some((t.border(st, fill), st.border_width)));
            let qs = q.borrow();
            let (text, color) = if qs.is_empty() { (crate::i18n::tr("Search…"), fg.darker(1.6)) } else { (format!("{qs}▏"), fg) };
            paint::text_elided(&text, tx + m + t.spacing.control_padding_x, ty + m, tw - 2 * m - 2 * t.spacing.control_padding_x, fh, t.font.body, false, color, al);
            paint::hairline(tx + 2, ty + header, tw - 4, fg, fill, 0.10);
            let vis = vis.borrow();
            let ly = ty + header + 1 + 2;
            if vis.is_empty() {
                paint::text(&crate::i18n::tr("No matches"), tx, ly, tw, h - header - 4, t.font.body, false, fg.darker(1.6), Align::Center);
                return;
            }
            for slot in 0..rows {
                let vi = tp.get() + slot;
                let Some(&idx) = vis.get(vi) else { break };
                let (label, desc) = &opts[idx];
                let ry = ly + slot as i32 * (row + t.spacing.sm);
                if vi == cu.get() {
                    paint::fill(tx + 2, ry, tw - 4, row, t.radius, t.fill(&t.controls.hover, fill));
                }
                let pad = t.spacing.control_padding_x;
                if desc.is_empty() {
                    paint::text_elided(label, tx + pad, ry, tw - 2 * pad, row, t.font.body, false, fg, al);
                } else {
                    let lh = t.font.body + 4;
                    let top_y = ry + (row - lh - t.font.caption - 4 - t.spacing.xxs) / 2;
                    paint::text_elided(label, tx + pad, top_y, tw - 2 * pad, lh, t.font.body, false, fg, al);
                    paint::text_elided(desc, tx + pad, top_y + lh + t.spacing.xxs, tw - 2 * pad, t.font.caption + 4, t.font.caption, false, fg.darker(1.5), al);
                }
            }
        });
    });
    a11y::popup_of(&layer, trigger);
    let (opts, q, vis, cu, tp) = (options.clone(), query.clone(), visible.clone(), cur.clone(), top.clone());
    a11y::describe(&layer, move |n| {
        let (row, header, rows, h) = geo();
        let gap = theme::with(|t| t.spacing.sm);
        n.role(Role::ListBox).description(&q.borrow()).bounds(tx, ty, tw, h);
        let vis = vis.borrow();
        let c = cu.get();
        for vi in c.saturating_sub(50)..(c + 51).min(vis.len()) {
            let (label, desc) = &opts[vis[vi]];
            let slot = vi as i32 - tp.get() as i32;
            n.child(vis[vi] as u64, |o| {
                o.role(Role::ListBoxOption).name(label).description(desc).selected(vi == c).position(vi + 1, vis.len()).action(Action::Click);
                if (0..rows as i32).contains(&slot) {
                    o.bounds(tx + 2, ty + header + 3 + slot * (row + gap), tw - 4, row);
                }
                if vi == c {
                    o.focused();
                }
            });
        }
    });
    let (v, mut trig, l2) = (value.clone(), trigger.clone(), layer.clone());
    a11y::on_action(&layer, move |r| {
        let (Some(i), Action::Click) = (r.key(), r.action) else { return false };
        v.set(Some(i as usize));
        close_layer(&l2);
        let _ = trig.take_focus();
        trig.redraw();
        true
    });
    let (opts, q, vis, cu, tp, v) = (options.clone(), query.clone(), visible.clone(), cur.clone(), top.clone(), value.clone());
    let mut trig = trigger.clone();
    layer.handle(move |l, ev| {
        let (row, header, rows, h) = geo();
        let refilter = |l: &mut Widget| { *vis.borrow_mut() = SearchableDropdown::filter(&opts, &q.borrow()); cu.set(0); tp.set(0); l.redraw(); };
        let mut finish = |l: &Widget, pick: Option<usize>| {
            if let Some(i) = pick { v.set(Some(i)); }
            close_layer(l);
            let _ = trig.take_focus();
            trig.redraw();
        };
        let ensure = |c: usize| { if c < tp.get() { tp.set(c) } else if c >= tp.get() + rows { tp.set(c + 1 - rows) } };
        match ev {
            Event::KeyDown => {
                let k = app::event_key();
                let n = vis.borrow().len();
                let cmd = crate::overlay::command_mods();
                match k {
                    Key::Escape if !cmd => finish(l, None),
                    Key::Tab => {}
                    Key::Down if !cmd => { if n > 0 { cu.set((cu.get() + 1).min(n - 1)); ensure(cu.get()); l.redraw(); } }
                    Key::Up if !cmd => { cu.set(cu.get().saturating_sub(1)); ensure(cu.get()); l.redraw(); }
                    Key::Enter | Key::KPEnter if !cmd => { let p = vis.borrow().get(cu.get()).copied(); finish(l, p); }
                    Key::BackSpace if !cmd => { q.borrow_mut().pop(); refilter(l); }
                    _ => {
                        let t = app::event_text();
                        if !cmd && !t.is_empty() && t.chars().all(|c| !c.is_control()) {
                            q.borrow_mut().push_str(&t);
                            refilter(l);
                        } else {
                            // Not search input: close, then the key goes on to the app.
                            finish(l, None);
                            return false;
                        }
                    }
                }
                true
            }
            Event::Move => {
                let ly = ty + header + 1 + 2;
                let (ex, ey) = app::event_coords();
                if ex >= tx && ex < tx + tw && ey >= ly {
                    let vi = tp.get() + ((ey - ly) / (row + 4)) as usize;
                    if vi < vis.borrow().len() && vi != cu.get() { cu.set(vi); l.redraw(); }
                }
                true
            }
            Event::Push => {
                let (ex, ey) = app::event_coords();
                if ex < tx || ex >= tx + tw || ey < ty || ey >= ty + h { finish(l, None); }
                else if ey > ty + header { let p = vis.borrow().get(cu.get()).copied(); finish(l, p); }
                true
            }
            Event::MouseWheel => {
                let d = match app::event_dy() { app::MouseWheel::Down => 1i32, app::MouseWheel::Up => -1, _ => 0 };
                let max_top = vis.borrow().len().saturating_sub(rows);
                tp.set((tp.get() as i32 + d).clamp(0, max_top as i32) as usize);
                l.redraw();
                true
            }
            Event::Focus | Event::Enter => true,
            _ => false,
        }
    });
}

// ============================================================ ShortcutHelp

/// F1-style help card: sections of (keys, description) rows over a scrim.
/// Keys are drawn as chips. Esc, F1 or a click closes. Up/Down, PageUp/
/// PageDown, Home/End and the wheel scroll, bounded to the content.
pub struct ShortcutHelp;

impl ShortcutHelp {
    /// `sections`: (title, [(keys, description)]). Keys with `Mod` are
    /// shown the platform's way ([`crate::keys::label`]: "Ctrl+," on Linux
    /// and Windows, "⌘," on macOS).
    pub fn open<A: WidgetExt>(anchor: &A, title: &str, sections: Vec<(String, Vec<(String, String)>)>) -> ShortcutHelpHandle {
        let sections: Vec<(String, Vec<(String, String)>)> =
            sections.into_iter().map(|(s, rows)| (s, rows.into_iter().map(|(k, d)| (crate::keys::label(&k), d)).collect())).collect();
        let none = || ShortcutHelpHandle { scroll: Rc::default(), extent: Rc::default() };
        let Some(mut layer) = open_layer(anchor, None, true) else { return none() };
        let scroll = Rc::new(Cell::new(0i32));
        // (content height, visible height), measured while drawing.
        let extent = Rc::new(Cell::new((0i32, 0i32)));
        let title = title.to_string();
        let (t2, secs) = (title.clone(), sections.clone());
        a11y::describe(&layer, move |n| {
            n.role(Role::Dialog).name(&t2).modal(true);
            for (si, (sec, rows)) in secs.iter().enumerate() {
                n.child(si as u64, |g| {
                    g.role(Role::Group).name(sec);
                    for (ri, (keys, desc)) in rows.iter().enumerate() {
                        g.child(ri as u64, |l| {
                            l.role(Role::Label).name(&format!("{keys}: {desc}"));
                        });
                    }
                });
            }
        });
        let (sc, ext) = (scroll.clone(), extent.clone());
        let (scroll_out, extent_out) = (scroll.clone(), extent.clone());
        // Right to left: key chips on the right, descriptions on their left.
        let rtl = crate::layout::is_rtl(anchor);
        crate::overlay::draw_layer(&mut layer, move |l| {
            if let Some(win) = l.top_window() { if (l.w(), l.h()) != (win.w(), win.h()) { l.resize(0, 0, win.w(), win.h()); } }
            theme::with(|t| {
                crate::overlay::draw_scrim(l, t.menu.background, 0.5);
                let cw = s(t, 520.0).min(l.w() - 2 * t.gaps_out.max(8));
                let ch = (l.h() as f64 * 0.8) as i32;
                let (cx, cy) = ((l.w() - cw) / 2, (l.h() - ch) / 2);
                let fill = t.menu.fill(bg(t));
                paint::surface(cx, cy, cw, ch, t.radius, Some(fill), Some((t.menu.border.over(fill, t.menu.border_alpha), 2)));
                let pad = t.spacing.panel_padding;
                let fg = t.menu.text;
                let m = crate::layout::Mirror::new(rtl, cx, cw);
                let al = m.align(Align::Left);
                paint::text_elided(&title, cx + pad, cy + pad, cw - 2 * pad, t.font.heading + 8, t.font.heading, true, fg, al);
                let top = cy + pad + t.font.heading + 8 + t.spacing.lg;
                let bottom = cy + ch - pad;
                // Content height first, so scrolling stops at the last row.
                let row = t.font.body + t.spacing.xl;
                let content: i32 = sections.iter().map(|(_, r)| t.font.caption + 6 + t.spacing.sm + r.len() as i32 * row + t.spacing.lg).sum();
                let view = bottom - top;
                ext.set((content, view));
                sc.set(sc.get().clamp(0, (content - view).max(0)));
                fltk::draw::push_clip(cx + pad, top, cw - 2 * pad, bottom - top);
                let mut y = top - sc.get();
                let row = t.font.body + t.spacing.xl;
                let key_col = s(t, 170.0);
                for (sec, rows) in &sections {
                    paint::text(&sec.to_uppercase(), cx + pad, y, cw - 2 * pad, t.font.caption + 6, t.font.caption, true, fg.darker(1.4), al);
                    y += t.font.caption + 6 + t.spacing.sm;
                    for (keys, desc) in rows {
                        // A key combination reads left to right in every language
                        // ("Ctrl+S"): its chips keep their order, the column moves.
                        let mut kx = cx + pad;
                        let parts: Vec<&str> = keys.split('+').collect();
                        let plus = s(t, 10.0);
                        let kws: Vec<i32> = parts.iter().map(|p| paint::measure(p, t.font.caption, true) + t.spacing.lg).collect();
                        let total = kws.iter().sum::<i32>() + plus * (parts.len() as i32 - 1).max(0);
                        if rtl {
                            kx = cx + cw - pad - total;
                        }
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                paint::text("+", kx, y, plus, row, t.font.caption, false, fg.darker(1.4), Align::Center);
                                kx += plus;
                            }
                            let kw = kws[i];
                            let kh = t.font.caption + t.spacing.md + 2;
                            paint::surface(kx, y + (row - kh) / 2, kw, kh, t.radius.min(4), Some(t.fill(&t.controls.normal, fill)),
                                           Some((t.border(&t.controls.normal, fill), 1)));
                            paint::text(part, kx, y + (row - kh) / 2, kw, kh, t.font.caption, true, fg, Align::Center);
                            kx += kw;
                        }
                        let dw = cw - 2 * pad - key_col;
                        paint::text_elided(desc, m.x(cx + pad + key_col, dw), y, dw, row, t.font.body, false, fg, al);
                        y += row;
                    }
                    y += t.spacing.lg;
                }
                fltk::draw::pop_clip();
                if content > view && view > 0 {
                    let thumb = ((view as f64 / content as f64) * view as f64).max(16.0) as i32;
                    let ty = top + ((sc.get() as f64 / (content - view) as f64) * (view - thumb) as f64) as i32;
                    paint::fill(m.x(cx + cw - pad / 2 - 2, 4), ty, 4, thumb, 2, fg.over(fill, 0.3));
                }
            });
        });
        layer.handle(move |l, ev| {
            let (content, view) = extent.get();
            let max = (content - view).max(0);
            let mut go = |v: i32| { scroll.set(v.clamp(0, max)); l.redraw(); };
            let line = theme::with(|t| t.font.body + t.spacing.xl);
            match ev {
            Event::KeyDown => {
                let cmd = crate::overlay::command_mods();
                match app::event_key() {
                    Key::Escape | Key::F1 if !cmd => close_layer(l),
                    Key::Tab => {}
                    Key::Down if !cmd => go(scroll.get() + line),
                    Key::Up if !cmd => go(scroll.get() - line),
                    Key::PageDown if !cmd => go(scroll.get() + (view - line).max(line)),
                    Key::PageUp if !cmd => go(scroll.get() - (view - line).max(line)),
                    Key::Home if !cmd => go(0),
                    Key::End if !cmd => go(max),
                    _ => {
                        // Any other key closes the help and reaches the app.
                        close_layer(l);
                        return false;
                    }
                }
                true
            }
            Event::MouseWheel => {
                let d = app::event_dy_value().saturating_mul(40);
                go(scroll.get() + d);
                true
            }
            Event::Push => { close_layer(l); true }
            Event::Focus | Event::Enter => true,
            _ => false,
            }
        });
        ShortcutHelpHandle { scroll: scroll_out, extent: extent_out }
    }
}

/// State of an open [`ShortcutHelp`] (tests, scripted tours).
#[derive(Clone)]
pub struct ShortcutHelpHandle {
    scroll: Rc<Cell<i32>>,
    extent: Rc<Cell<(i32, i32)>>,
}

impl ShortcutHelpHandle {
    /// Current scroll offset in pixels.
    pub fn scroll(&self) -> i32 {
        self.scroll.get()
    }
    /// Largest scroll offset (content height − visible height).
    pub fn max_scroll(&self) -> i32 {
        let (c, v) = self.extent.get();
        (c - v).max(0)
    }
}

// ========================================================== OperationToast

/// Long operation card at the window's bottom-right, bottom-left right to left (copy/move/delete):
/// title, detail line, accent progress bar and Pause/Resume + Cancel
/// buttons. Not modal; update it from the UI thread (e.g. task progress).
/// Keyboard: Tab reaches it (or call [`OperationToast::focus`] from a
/// shortcut); Left/Right pick Pause or Cancel, Enter/Space press, Esc gives
/// focus back without cancelling.
#[derive(Clone)]
pub struct OperationToast {
    layer: Widget,
    data: Rc<RefCell<ToastData>>,
}

struct ToastData {
    title: String,
    detail: String,
    progress: f64,
    paused: bool,
    hover: i32,
    /// Keyboard cursor: 0 = Pause/Resume, 1 = Cancel.
    kb: i32,
    on_pause: Option<Box<dyn FnMut(bool)>>,
    on_cancel: Option<Box<dyn FnMut()>>,
    /// Pause, Resume, Cancel button labels and the paused state text set
    /// by the app; None: the toolkit's, in the current language.
    labels: Option<[String; 4]>,
    indeterminate: bool,
    started: std::time::Instant,
}

impl ToastData {
    fn labels(&self) -> [String; 4] {
        use crate::i18n::tr;
        self.labels.clone().unwrap_or_else(|| [tr("Pause"), tr("Resume"), tr("Cancel"), tr("Paused")])
    }
}

/// Toast rectangle in its window: the bottom-right corner, bottom-left
/// right to left.
fn toast_rect(win_w: i32, win_h: i32, rtl: bool) -> (i32, i32, i32, i32) {
    let (w, h, m) = toast_size(win_w);
    let x = if rtl { m } else { win_w - w - m };
    (x, win_h - h - m, w, h)
}

/// Toast size for a window width: (w, h, margin).
fn toast_size(win_w: i32) -> (i32, i32, i32) {
    theme::with(|t| {
        let m = t.gaps_out.max(8);
        let w = s(t, 380.0).min(win_w - 2 * m).max(120);
        (w, t.spacing.popup_padding * 2 + t.font.title + 6 + t.font.body_small + 6 + t.spacing.lg + 4 + t.spacing.lg + t.spacing.control_height, m)
    })
}

impl OperationToast {
    pub fn show<A: WidgetExt>(anchor: &A, title: &str) -> Option<OperationToast> {
        let win = anchor.top_window()?;
        let rtl = crate::layout::is_rtl(anchor);
        let rect = toast_rect(win.w(), win.h(), rtl);
        let mut layer = open_layer(anchor, Some(rect), false)?;
        let data = Rc::new(RefCell::new(ToastData { title: title.into(), detail: String::new(), progress: 0.0, paused: false, hover: -1, kb: 0, on_pause: None, on_cancel: None,
            labels: None, indeterminate: false, started: std::time::Instant::now() }));
        let lb = data.clone();
        let buttons = Rc::new(move |l: &Widget| -> [(i32, i32, i32, i32); 2] {
            let d = lb.borrow();
            theme::with(|t| {
                let bh = t.spacing.control_height;
                // Wide enough for the longest label (localized labels can be longer).
                let fit = |a: &str| crate::widgets::measure_offscreen(a, t.font.body) + 2 * t.spacing.control_padding_x + 2;
                let labels = d.labels();
                let b0 = fit(&labels[0]).max(fit(&labels[1])).max(s(t, 96.0));
                let b1 = fit(&labels[2]).max(s(t, 96.0));
                let y = l.y() + l.h() - t.spacing.popup_padding - bh;
                let x1 = l.x() + l.w() - t.spacing.popup_padding - b1;
                // Right to left: Cancel at the left end, Pause on its right.
                let m = crate::layout::Mirror::new(rtl, l.x(), l.w());
                [m.rect((x1 - t.spacing.md - b0, y, b0, bh)), m.rect((x1, y, b1, bh))]
            })
        });
        let (d, buttons2, buttons3) = (data.clone(), buttons.clone(), buttons.clone());
        crate::overlay::draw_layer(&mut layer, move |l| {
            let buttons = &buttons2;
            // Stay in the bottom-right corner when the window is resized.
            if let Some(win) = l.top_window() {
                let want = toast_rect(win.w(), win.h(), rtl);
                if (l.x(), l.y(), l.w(), l.h()) != want {
                    l.resize(want.0, want.1, want.2, want.3);
                    if let Some(mut p) = l.parent() { p.redraw(); }
                    return;
                }
            }
            let d = d.borrow();
            theme::with(|t| {
                card(t, l.x(), l.y(), l.w(), l.h(), &t.notifications, 2);
                let fill = t.notifications.fill(bg(t));
                let fg = t.notifications.text;
                let p = t.spacing.popup_padding;
                let (x, mut y, w) = (l.x() + p, l.y() + p, l.w() - 2 * p);
                let m = crate::layout::Mirror::new(rtl, l.x(), l.w());
                paint::text_elided(&d.title, x, y, w, t.font.title + 6, t.font.title, true, fg, m.align(Align::Left));
                y += t.font.title + 6;
                let pct = if d.indeterminate { String::new() } else { format!("{}%", (d.progress.clamp(0.0, 1.0) * 100.0).round()) };
                let labels = d.labels();
                let detail = if d.paused { format!("{} · {}", labels[3], d.detail) } else { d.detail.clone() };
                let dw = w - s(t, 48.0);
                paint::text_elided(&detail, m.x(x, dw), y, dw, t.font.body_small + 6, t.font.body_small, false, fg.darker(1.4), m.align(Align::Left));
                paint::text(&pct, x, y, w, t.font.body_small + 6, t.font.body_small, true, fg, m.align(Align::Right));
                y += t.font.body_small + 6 + t.spacing.lg;
                paint::fill(x, y, w, 4, 2, t.fill(&t.controls.selected, fill));
                let bar = if d.paused { fg.darker(1.6) } else { t.notifications.border };
                if d.indeterminate {
                    // A third of the track sliding back and forth (static when transitions are off or paused).
                    let seg = (w / 3).max(8);
                    let k = if crate::anim::enabled() && !d.paused {
                        let ph = (d.started.elapsed().as_secs_f64() / 1.2) % 2.0;
                        crate::anim::Ease::OutCubic.apply(if ph < 1.0 { ph } else { 2.0 - ph })
                    } else { 0.5 };
                    paint::fill(m.x(x + ((w - seg) as f64 * k).round() as i32, seg), y, seg, 4, 2, bar);
                    if crate::anim::enabled() && !d.paused { crate::anim::request_redraw(l); }
                } else {
                    let fw = (w as f64 * d.progress.clamp(0.0, 1.0)).round() as i32;
                    paint::fill(m.x(x, fw), y, fw, 4, 2, bar);
                }
                let focused = crate::focus::show_ring(l);
                for (i, (bx, by, bw, bh)) in buttons(l).iter().copied().enumerate() {
                    let hot = d.hover == i as i32 || (focused && d.kb == i as i32);
                    let (label, color) = if i == 0 {
                        (if d.paused { labels[1].as_str() } else { labels[0].as_str() }, fg)
                    } else {
                        (labels[2].as_str(), t.palette.urgent)
                    };
                    let st = if focused && d.kb == i as i32 { &t.controls.focus } else if hot { &t.controls.hover } else { &t.controls.normal };
                    paint::surface(bx, by, bw, bh, t.radius, Some(t.fill(st, fill)), Some((t.border(st, fill), st.border_width.max(1))));
                    paint::text(label, bx, by, bw, bh, t.font.body, false, color, Align::Center);
                }
            });
        });
        let d = data.clone();
        layer.handle(move |l, ev| {
            let hit = |b: (i32, i32, i32, i32)| { let (x, y) = app::event_coords(); x >= b.0 && x < b.0 + b.2 && y >= b.1 && y < b.1 + b.3 };
            let bs = buttons(l);
            match ev {
                Event::Enter => true,
                Event::Move => {
                    let h = bs.iter().position(|b| hit(*b)).map(|i| i as i32).unwrap_or(-1);
                    if d.borrow().hover != h { d.borrow_mut().hover = h; l.redraw(); }
                    true
                }
                Event::Leave => { d.borrow_mut().hover = -1; l.redraw(); true }
                Event::Push => {
                    if hit(bs[0]) { press_toast(&d, l, 0) } else if hit(bs[1]) { press_toast(&d, l, 1) }
                    true
                }
                Event::Focus => { crate::focus::remember(l); l.redraw(); true }
                Event::Unfocus => { l.redraw(); true }
                Event::KeyDown => {
                    let k = app::event_key();
                    match k {
                        Key::Left | Key::Right => { let kb = d.borrow().kb; d.borrow_mut().kb = 1 - kb; l.redraw(); true }
                        Key::Enter | Key::KPEnter => { let kb = d.borrow().kb; press_toast(&d, l, kb); true }
                        Key::Escape => {
                            // SAFETY: a Fl_Window is a Fl_Widget.
                            let win = l.top_window().map(|w| unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) });
                            crate::focus::give_back(l, win);
                            true
                        }
                        _ if k == Key::from_char(' ') => { let kb = d.borrow().kb; press_toast(&d, l, kb); true }
                        _ => false,
                    }
                }
                _ => false,
            }
        });
        layer.set_visible_focus();
        let (d, lw, b3) = (data.clone(), layer.clone(), buttons3);
        a11y::describe(&layer, move |n| {
            let d = d.borrow();
            let labels = d.labels();
            let detail = if d.paused { format!("{} · {}", labels[3], d.detail) } else { d.detail.clone() };
            n.role(Role::Status).live(Live::Polite).name(&d.title).description(&detail).focusable();
            let focused = crate::focus::is_focused(&lw);
            n.child(0, |p| {
                p.role(Role::ProgressIndicator).name(&d.title);
                if !d.indeterminate {
                    let v = d.progress.clamp(0.0, 1.0);
                    p.numeric(v, 0.0, 1.0, 0.0).value(&format!("{}%", (v * 100.0).round()));
                }
            });
            let bs = b3(&lw);
            for (i, label) in [if d.paused { &labels[1] } else { &labels[0] }, &labels[2]].into_iter().enumerate() {
                let (bx, by, bw, bh) = bs[i];
                n.child(i as u64 + 1, |b| {
                    b.role(Role::Button).name(label).bounds(bx, by, bw, bh).action(Action::Click).focusable();
                    if focused && d.kb == i as i32 {
                        b.focused();
                    }
                });
            }
        });
        let (d, lw) = (data.clone(), layer.clone());
        a11y::on_action(&layer, move |r| {
            match (r.action, r.key()) {
                (Action::Click, Some(k @ 1..=2)) => press_toast(&d, &lw, k as i32 - 1),
                (Action::Focus, Some(k @ 1..=2)) => {
                    d.borrow_mut().kb = k as i32 - 1;
                    let _ = lw.clone().take_focus();
                }
                _ => return false,
            }
            true
        });
        Some(OperationToast { layer, data })
    }

    /// Moves keyboard focus to the toast (bind it to a shortcut so a long
    /// operation can be paused or cancelled without the mouse).
    pub fn focus(&mut self) {
        let _ = self.layer.take_focus();
    }

    pub fn has_focus(&self) -> bool {
        crate::focus::is_focused(&self.layer)
    }

    pub fn on_pause(self, f: impl FnMut(bool) + 'static) -> Self { self.data.borrow_mut().on_pause = Some(Box::new(f)); self }
    pub fn on_cancel(self, f: impl FnMut() + 'static) -> Self { self.data.borrow_mut().on_cancel = Some(Box::new(f)); self }

    pub fn set_progress(&mut self, p: f64) { self.data.borrow_mut().progress = p; self.layer.redraw(); }

    /// Overrides the button labels and the paused text. Not needed for
    /// translation: by default they come from the toolkit catalog
    /// (`Pause`, `Resume`, `Cancel`, `Paused`) and follow the locale live.
    pub fn labels(self, pause: &str, resume: &str, cancel: &str, paused: &str) -> Self {
        self.data.borrow_mut().labels = Some([pause.into(), resume.into(), cancel.into(), paused.into()]);
        self.layer.clone().redraw();
        self
    }

    /// Progress of unknown length: a sliding bar instead of a percentage.
    pub fn set_indeterminate(&mut self, on: bool) {
        self.data.borrow_mut().indeterminate = on;
        self.layer.redraw();
    }

    /// Card rectangle in window coordinates (tests).
    pub fn rect(&self) -> (i32, i32, i32, i32) {
        (self.layer.x(), self.layer.y(), self.layer.w(), self.layer.h())
    }
    pub fn set_detail(&mut self, s: &str) { self.data.borrow_mut().detail = s.into(); self.layer.redraw(); }
    pub fn set_title(&mut self, s: &str) { self.data.borrow_mut().title = s.into(); self.layer.redraw(); }
    pub fn is_paused(&self) -> bool { self.data.borrow().paused }

    /// Same as pressing Pause/Resume (tests, keyboard shortcuts).
    pub fn toggle_pause(&mut self) {
        press_toast(&self.data, &self.layer, 0);
    }

    /// Same as pressing Cancel.
    pub fn cancel(&mut self) {
        press_toast(&self.data, &self.layer, 1);
    }

    pub fn close(&self) {
        close_layer(&self.layer);
    }
}

/// Runs the toast button `i` (0 = Pause/Resume, 1 = Cancel).
fn press_toast(d: &Rc<RefCell<ToastData>>, l: &Widget, i: i32) {
    if i == 0 {
        let paused = { let mut dd = d.borrow_mut(); dd.paused = !dd.paused; dd.paused };
        let cb = d.borrow_mut().on_pause.take();
        if let Some(mut f) = cb { f(paused); d.borrow_mut().on_pause = Some(f); }
    } else {
        let cb = d.borrow_mut().on_cancel.take();
        if let Some(mut f) = cb { f(); d.borrow_mut().on_cancel = Some(f); }
    }
    l.clone().redraw();
}

// =============================================================== PopupCard

/// Omarchy `PopupCard`: a card anchored to a widget (or a point) holding
/// any widgets, clamped to the window. Esc or a click outside closes it;
/// Tab and Shift+Tab stay inside while it is open; focus goes back to the
/// control that had it. Returns the same [`PopupHandle`] as menus.
///
/// ```no_run
/// use fltk_omarchy::{layout::FlexExt, popups::PopupCard, widgets::*, State};
/// # let anchor = Button::new("Volume");
/// let vol = State::new(0.6);
/// let card = PopupCard::open(&*anchor, 260, 110, |col| {
///     col.fit(&*Label::section("Output"));
///     let _ = Slider::new(&vol, 0.0, 1.0, 0.05);
/// }).on_dismiss(|why| println!("closed: {why:?}"));
/// ```
pub struct PopupCard;

impl PopupCard {
    /// Opens a `w × h` card below `anchor` (above it when there is more
    /// room; right-aligned in the right half). `build` adds widgets to the
    /// card's column, like an `AppWindow` body.
    pub fn open<A: WidgetExt>(anchor: &A, w: i32, h: i32, build: impl FnOnce(&mut fltk::group::Flex)) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let w = w.min(win.w() - 4).max(1);
        let right_half = anchor.x() + anchor.w() / 2 > win.w() / 2;
        let x = if right_half { anchor.x() + anchor.w() - w } else { anchor.x() };
        Self::open_rect(anchor, x, (anchor.y(), anchor.y() + anchor.h()), w, h, build)
    }

    /// Opens at window coordinates (e.g. where the user clicked).
    pub fn open_at<A: WidgetExt>(anchor: &A, x: i32, y: i32, w: i32, h: i32, build: impl FnOnce(&mut fltk::group::Flex)) -> PopupHandle {
        Self::open_rect(anchor, x, (y, y), w, h, build)
    }

    fn open_rect<A: WidgetExt>(anchor: &A, x: i32, span: (i32, i32), w: i32, h: i32, build: impl FnOnce(&mut fltk::group::Flex)) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let Some(mut wg) = fltk::group::Group::from_dyn_widget_ptr(win.as_widget_ptr()) else { return PopupHandle::closed_now() };
        let (ww, wh) = (win.w(), win.h());
        let h = h.min(wh - 4).max(1);
        let x = x.clamp(2, (ww - w - 2).max(2));
        let below = wh - 2 - (span.1 + 2);
        let y = if h <= below { span.1 + 2 } else if h <= span.0 - 4 { span.0 - 2 - h } else { (wh - h) / 2 };
        let prev_focus = crate::focus::current();
        let prev_group = fltk::group::Group::try_current();
        fltk::group::Group::set_current(None::<&fltk::group::Group>);
        let mut layer = fltk::group::Group::new(0, 0, ww, wh, None);
        layer.set_frame(FrameType::NoBox);
        let mut card = fltk::group::Group::new(x, y, w, h, None);
        card.set_frame(FrameType::NoBox);
        let pad = theme::with(|t| t.spacing.popup_padding);
        let mut col = fltk::group::Flex::new(x + pad, y + pad, w - 2 * pad, h - 2 * pad, None).column();
        col.set_spacing(theme::with(|t| t.spacing.md));
        build(&mut col);
        col.end();
        card.end();
        card.draw(|c| {
            theme::with(|t| card_surface(t, c.x(), c.y(), c.w(), c.h()));
            c.draw_children();
        });
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        card.super_draw(false);
        layer.end();
        wg.add(&layer);
        if let Some(g) = prev_group {
            g.begin();
        }
        // SAFETY: the layer is a live widget; the copy only tracks it.
        let lw = unsafe { Widget::from_widget_ptr(layer.as_widget_ptr()) };
        let state = Rc::new(RefCell::new(HandleState::open((x, y, w, h))));
        let handle = PopupHandle { layer: Some(lw.clone()), state: state.clone() };
        let (st, rect) = (state.clone(), (x, y, w, h));
        layer.handle(move |l, ev| {
            // SAFETY: as above.
            let me = unsafe { Widget::from_widget_ptr(l.as_widget_ptr()) };
            match ev {
                Event::Push => {
                    let (ex, ey) = app::event_coords();
                    let inside = ex >= rect.0 && ex < rect.0 + rect.2 && ey >= rect.1 && ey < rect.1 + rect.3;
                    if !inside {
                        crate::focus::pop_modal(&me);
                        finish(&st, &me, Dismiss::ClickOutside);
                        return true;
                    }
                    false // the card's widgets get the click
                }
                Event::KeyDown | Event::Shortcut if app::event_key() == Key::Escape => {
                    crate::focus::pop_modal(&me);
                    finish(&st, &me, Dismiss::Escape);
                    true
                }
                _ => false,
            }
        });
        crate::focus::push_modal(&layer);
        a11y::describe(&layer, |n| {
            n.role(Role::Dialog).modal(true);
        });
        // Focus goes into the card; it comes back to `prev_focus` on close.
        if let Some(p) = prev_focus {
            crate::focus::remember_for(&lw, p);
        }
        if crate::focus::first(&wg).is_none() {
            app::set_focus(&layer);
        }
        wg.redraw();
        handle
    }
}

fn card_surface(t: &theme::Theme, x: i32, y: i32, w: i32, h: i32) {
    card(t, x, y, w, h, &t.popups, t.controls.normal.border_width.max(1));
}
