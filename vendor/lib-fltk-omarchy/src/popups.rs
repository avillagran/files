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
}

impl PopupItem {
    pub fn new(icon: &str, label: &str) -> PopupItem {
        PopupItem { icon: icon.into(), label: label.into(), shortcut: String::new(), enabled: true, checked: false, divider: false }
    }
    pub fn shortcut(mut self, s: &str) -> Self { self.shortcut = crate::keys::label(s); self }
    pub fn checked(mut self, on: bool) -> Self { self.checked = on; self }
    pub fn disabled(mut self) -> Self { self.enabled = false; self }
    pub fn divider(mut self) -> Self { self.divider = true; self }
}

/// How a popup ended (see [`PopupHandle::on_dismiss`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismiss {
    /// An entry was chosen (index into the items); `on_pick` ran first.
    Picked(usize),
    /// Esc.
    Escape,
    /// A click outside the card.
    ClickOutside,
    /// [`PopupHandle::close`] (e.g. the data the menu was built from changed).
    Closed,
}

struct HandleState {
    open: bool,
    rect: Option<(i32, i32, i32, i32)>,
    reason: Option<Dismiss>,
    on_dismiss: Option<Box<dyn FnOnce(Dismiss)>>,
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
        PopupHandle { layer: None, state: Rc::new(RefCell::new(HandleState { open: false, rect: None, reason: Some(Dismiss::Closed), on_dismiss: None })) }
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

    /// Closes the menu without picking (reason [`Dismiss::Closed`]); focus
    /// goes back to the control that had it. No-op when already closed.
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

    /// Why it closed (None while open).
    pub fn dismissed(&self) -> Option<Dismiss> {
        self.state.borrow().reason
    }
}

fn finish(state: &Rc<RefCell<HandleState>>, layer: &Widget, reason: Dismiss) -> bool {
    {
        let mut s = state.borrow_mut();
        if !s.open {
            return false;
        }
        s.open = false;
        if layer.was_deleted() {
            // The window went away with the menu: no callbacks.
            s.on_dismiss = None;
            return false;
        }
        s.reason = Some(reason);
    }
    crate::focus::pop_modal(layer);
    close_layer(layer);
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
/// Enter picks, Esc closes; Tab stays in the menu. A click outside closes.
/// `open` returns a [`PopupHandle`]: `.on_dismiss(|reason| ..)`, `.close()`.
pub struct PopupMenu;

struct MenuGeo {
    rect: (i32, i32, i32, i32),
    /// Content-relative (y, h) of each entry.
    rows: Vec<(i32, i32)>,
    /// Inner padding (top/bottom) and the visible content height.
    pad: i32,
    view_h: i32,
    content_h: i32,
}

impl PopupMenu {
    /// Opens under `anchor` (or above it when there is more room).
    /// `on_pick(index)` receives the index into `items`.
    pub fn open<A: WidgetExt>(anchor: &A, items: Vec<PopupItem>, on_pick: impl FnOnce(usize) + 'static) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let w = Self::width(&items).min(win.w() - 4).max(1);
        let right_half = anchor.x() + anchor.w() / 2 > win.w() / 2;
        let x = if right_half { anchor.x() + anchor.w() - w } else { anchor.x() };
        Self::open_rect(anchor, items, x, (anchor.y(), anchor.y() + anchor.h()), w, on_pick)
    }

    /// Opens at window coordinates (e.g. the mouse position for a context menu).
    pub fn open_at<A: WidgetExt>(anchor: &A, x: i32, y: i32, items: Vec<PopupItem>, on_pick: impl FnOnce(usize) + 'static) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let w = Self::width(&items).min(win.w() - 4).max(1);
        // Right to left the card grows to the left of the point.
        let x = if crate::layout::is_rtl(anchor) { x - w } else { x };
        Self::open_rect(anchor, items, x, (y, y), w, on_pick)
    }

    /// Natural width: widest label + shortcut + icon and check columns.
    fn width(items: &[PopupItem]) -> i32 {
        theme::with(|t| {
            let pad = t.spacing.control_padding_x;
            let mut w = s(t, 200.0);
            for it in items {
                let lw = crate::widgets::measure_offscreen(&it.label, t.font.body);
                let sw = if it.shortcut.is_empty() { 0 } else { crate::widgets::measure_offscreen(&it.shortcut, t.font.caption) + t.spacing.xl };
                w = w.max(2 * pad + s(t, 22.0) + t.spacing.lg + lw + sw + s(t, 18.0) + 4 + SCROLLBAR);
            }
            w
        })
    }

    /// Lays the card out inside the window: below `span.1` if it fits, else
    /// above `span.0` if it fits, else on the larger side, clamped and scrolling.
    fn geometry(items: &[PopupItem], win_h: i32, x: i32, span: (i32, i32), w: i32) -> MenuGeo {
        theme::with(|t| {
            let row = t.spacing.popup_row_height;
            let pad = 2 + t.spacing.xxs;
            let mut rows = Vec::with_capacity(items.len());
            let mut y = 0;
            for it in items {
                rows.push((y, row));
                y += row + if it.divider { t.spacing.md + 1 } else { 0 };
            }
            let content_h = y;
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

    fn open_rect<A: WidgetExt>(anchor: &A, items: Vec<PopupItem>, x: i32, span: (i32, i32), w: i32, on_pick: impl FnOnce(usize) + 'static) -> PopupHandle {
        let Some(win) = anchor.top_window() else { return PopupHandle::closed_now() };
        let x = x.clamp(2, (win.w() - w - 2).max(2));
        let g = Rc::new(Self::geometry(&items, win.h(), x, span, w));
        let Some(mut layer) = open_layer(anchor, None, true) else { return PopupHandle::closed_now() };
        let state = Rc::new(RefCell::new(HandleState { open: true, rect: Some(g.rect), reason: None, on_dismiss: None }));
        let handle = PopupHandle { layer: Some(layer.clone()), state: state.clone() };
        let items = Rc::new(items);
        let current = Rc::new(Cell::new(items.iter().position(|i| i.enabled).map(|i| i as i32).unwrap_or(-1)));
        let scroll = Rc::new(Cell::new(0i32));
        let motion = Rc::new(RefCell::new(crate::scroll::Scroll::new()));
        // The anchor's direction: right to left, glyphs on the right,
        // shortcuts and checks on the left, the scrollbar on the left.
        let rtl = crate::layout::is_rtl(anchor);

        let (it, cur, sc, gd, mo) = (items.clone(), current.clone(), scroll.clone(), g.clone(), motion.clone());
        crate::overlay::draw_layer(&mut layer, move |l| {
            let mut motion = mo.borrow_mut();
            sc.set(motion.position(sc.get() as i64, (gd.content_h - gd.view_h).max(0) as i64) as i32);
            if motion.running() { crate::anim::request_redraw(l); }
            drop(motion);
            if let Some(win) = l.top_window() {
                if (l.w(), l.h()) != (win.w(), win.h()) { l.resize(0, 0, win.w(), win.h()); }
            }
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
                for (i, (item, &(oy, rh))) in it.iter().zip(gd.rows.iter()).enumerate() {
                    let ry = top + oy - sc.get();
                    if ry + rh < top || ry > top + gd.view_h { continue; }
                    if i as i32 == cur.get() && item.enabled {
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
                    if item.checked {
                        paint::glyph(glyph::CHECKMARK, m.x(right - s(t, 14.0), s(t, 14.0)), ry, s(t, 14.0), rh, t.font.body, t.palette.accent);
                        right -= s(t, 18.0);
                    }
                    if !item.shortcut.is_empty() {
                        let sw = paint::measure(&item.shortcut, t.font.caption, false);
                        paint::text(&item.shortcut, m.x(right - sw, sw), ry, sw, rh, t.font.caption, false, fg.darker(1.4), m.align(Align::Right));
                        right -= sw + t.spacing.xl;
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
                    let ty = top + ((sc.get() as f64 / max as f64) * (track - thumb) as f64) as i32;
                    paint::fill(m.x(x + w - SCROLLBAR - 2, SCROLLBAR - 2), ty, SCROLLBAR - 2, thumb, (SCROLLBAR - 2) / 2, fg.over(fill, 0.3));
                }
            });
        });

        let on_pick = Rc::new(RefCell::new(Some(Box::new(on_pick) as Box<dyn FnOnce(usize)>)));
        let (it, cur, sc, gd) = (items.clone(), current.clone(), scroll.clone(), g.clone());
        a11y::describe(&layer, move |n| {
            let (x, y, w, h) = gd.rect;
            n.role(Role::Menu).bounds(x, y, w, h);
            let c = cur.get().max(0) as usize;
            let top = y + gd.pad;
            for i in c.saturating_sub(100)..(c + 101).min(it.len()) {
                let item = &it[i];
                let (oy, rh) = gd.rows[i];
                n.child(i as u64, |m| {
                    m.role(if item.checked { Role::MenuItemCheckBox } else { Role::MenuItem }).name(&item.label).shortcut(&item.shortcut)
                        .disabled(!item.enabled).position(i + 1, it.len()).action(Action::Click).action(Action::Focus);
                    if item.checked {
                        m.toggled(true);
                    }
                    let ry = top + oy - sc.get();
                    if ry + rh > top && ry < top + gd.view_h {
                        m.bounds(x + 2, ry, w - 4, rh);
                    }
                    if i as i32 == cur.get() {
                        m.focused();
                    }
                });
            }
        });
        let (it, cur, st, op, l5) = (items.clone(), current.clone(), state.clone(), on_pick.clone(), layer.clone());
        a11y::on_action(&layer, move |r| {
            let Some(i) = r.key().map(|k| k as usize).filter(|i| *i < it.len()) else { return false };
            match r.action {
                Action::Click if it[i].enabled => {
                    // Same order as a pick with the keyboard: close, then the callbacks.
                    {
                        let mut s = st.borrow_mut();
                        if !s.open { return false; }
                        s.open = false;
                        s.reason = Some(Dismiss::Picked(i));
                    }
                    close_layer(&l5);
                    if let Some(f) = op.borrow_mut().take() { f(i); }
                    let f = st.borrow_mut().on_dismiss.take();
                    if let Some(f) = f { f(Dismiss::Picked(i)); }
                }
                Action::Focus | Action::ScrollIntoView if it[i].enabled => { cur.set(i as i32); l5.clone().redraw(); }
                _ => return false,
            }
            true
        });
        let (it, cur, sc, gd, st) = (items.clone(), current.clone(), scroll.clone(), g.clone(), state.clone());
        layer.handle(move |l, ev| {
            if matches!(ev, Event::Push | Event::KeyDown) { motion.borrow_mut().jump(sc.get() as i64); }
            let (x, y, w, h) = gd.rect;
            let max_scroll = (gd.content_h - gd.view_h).max(0);
            let at = |ey: i32| -> Option<usize> {
                let (ex, _) = app::event_coords();
                let top = y + gd.pad;
                if ex < x || ex >= x + w || ey < top || ey >= top + gd.view_h { return None; }
                let cy = ey - top + sc.get();
                gd.rows.iter().position(|(ry, rh)| cy >= *ry && cy < ry + rh)
            };
            let reveal = |i: i32| {
                if i < 0 { return; }
                let (ry, rh) = gd.rows[i as usize];
                if ry < sc.get() { sc.set(ry) } else if ry + rh > sc.get() + gd.view_h { sc.set((ry + rh - gd.view_h).min(max_scroll)) }
            };
            let pick = |l: &Widget, i: usize| {
                if !it[i].enabled { return; }
                // Close first (focus goes back), then the callbacks.
                {
                    let mut s = st.borrow_mut();
                    if !s.open { return; }
                    s.open = false;
                    s.reason = Some(Dismiss::Picked(i));
                }
                close_layer(l);
                if let Some(f) = on_pick.borrow_mut().take() { f(i); }
                let f = st.borrow_mut().on_dismiss.take();
                if let Some(f) = f { f(Dismiss::Picked(i)); }
            };
            let enabled: Vec<i32> = it.iter().enumerate().filter(|(_, i)| i.enabled).map(|(n, _)| n as i32).collect();
            let step = |d: i32| {
                let n = it.len() as i32;
                if enabled.is_empty() { return; }
                let mut c = cur.get().max(0);
                for _ in 0..n {
                    c = (c + d).rem_euclid(n.max(1));
                    if it[c as usize].enabled { break; }
                }
                cur.set(c);
            };
            // Page moves: by the rows that fit, no wrap, landing on an enabled entry.
            let page = |dir: i32| {
                if enabled.is_empty() { return; }
                let rows = (gd.view_h / theme::with(|t| t.spacing.popup_row_height)).max(1);
                let target = (cur.get().max(0) + dir * rows).clamp(0, it.len() as i32 - 1);
                let pick = if dir > 0 {
                    enabled.iter().copied().filter(|&e| e <= target).max().filter(|&e| e > cur.get()).or_else(|| enabled.iter().copied().find(|&e| e > cur.get()))
                } else {
                    enabled.iter().copied().filter(|&e| e >= target).min().filter(|&e| e < cur.get()).or_else(|| enabled.iter().copied().rev().find(|&e| e < cur.get()))
                };
                if let Some(p) = pick { cur.set(p) }
            };
            match ev {
                Event::Move => {
                    if let Some(i) = at(app::event_y()) { if cur.get() != i as i32 { cur.set(i as i32); l.redraw(); } }
                    true
                }
                Event::Push => {
                    let (ex, ey) = app::event_coords();
                    if ex >= x && ex < x + w && ey >= y && ey < y + h {
                        if let Some(i) = at(ey) { pick(l, i); }
                    } else {
                        finish(&st, l, Dismiss::ClickOutside);
                    }
                    true
                }
                Event::MouseWheel => {
                    let row = theme::with(|t| t.spacing.popup_row_height);
                    sc.set(motion.borrow_mut().wheel(sc.get() as i64, app::event_dy_value() as f64 * row as f64, max_scroll as i64) as i32);
                    l.redraw();
                    true
                }
                Event::KeyDown => {
                    let k = app::event_key();
                    let cmd = crate::overlay::command_mods();
                    match k {
                        Key::Escape if !cmd => { finish(&st, l, Dismiss::Escape); return true; }
                        Key::Tab => return true, // Tab stays in the menu
                        Key::Down if !cmd => step(1),
                        Key::Up if !cmd => step(-1),
                        Key::PageDown if !cmd => page(1),
                        Key::PageUp if !cmd => page(-1),
                        Key::Home if !cmd => { if let Some(&e) = enabled.first() { cur.set(e) } }
                        Key::End if !cmd => { if let Some(&e) = enabled.last() { cur.set(e) } }
                        Key::Enter | Key::KPEnter if !cmd => { if cur.get() >= 0 { pick(l, cur.get() as usize); } return true; }
                        _ if crate::overlay::plain_letter(k, 'j') => step(1),
                        _ if crate::overlay::plain_letter(k, 'k') => step(-1),
                        _ => {
                            // Not a menu key (F1, F10, Ctrl+…): close the menu, give focus
                            // back and let the key reach the app's shortcuts.
                            finish(&st, l, Dismiss::Closed);
                            return false;
                        }
                    }
                    reveal(cur.get());
                    l.redraw();
                    true
                }
                Event::Focus | Event::Enter => true,
                _ => false,
            }
        });
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
        let state = Rc::new(RefCell::new(HandleState { open: true, rect: Some((x, y, w, h)), reason: None, on_dismiss: None }));
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
