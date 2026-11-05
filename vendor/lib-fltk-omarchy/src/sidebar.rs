//! `Sidebar`: a navigation rail that expands on hover.
//!
//! Collapsed it is a rail of icons; resting the pointer on it (hover intent
//! [`anim::ms::HOVER_OPEN`], 350 ms) expands it to show the labels, and it
//! collapses [`anim::ms::HOVER_CLOSE`] (140 ms) after the pointer leaves.
//! Keyboard focus inside expands it at once; Esc collapses it. Pinned, it
//! stays expanded. Selected = accent bar on the start edge + selected fill.
//!
//! - [`SidebarMode::Push`] (default): the rail's width changes in its row
//!   and the content beside it makes room. The width animates through the
//!   toolkit's automatic layout motion (190 ms OutCubic), like o3's
//!   sidebar reveal and Omarchy's tray drawer, which push too.
//! - [`SidebarMode::Overlay`]: the rail keeps its width; the expanded
//!   panel slides over the content (a non-modal layer).
//!
//! Right to left, the rail is on the right (its row mirrors), the accent
//! bar on its right edge and an overlay panel grows to the left.
//!
//! ```no_run
//! use fltk::prelude::*;
//! use fltk_omarchy::{layout::{self, FlexExt}, widgets::*, State};
//! let view = State::new(0usize);
//! let mut row = layout::row();
//! let side = Sidebar::new(vec![
//!     SidebarItem::new(glyph::FOLDER, "Files"),
//!     SidebarItem::new(glyph::SEARCH, "Search"),
//!     SidebarItem::new(glyph::SETTINGS, "Settings").footer(),
//! ], &view);
//! row.fit(&*side);
//! let content = fltk::frame::Frame::default(); // takes the rest
//! row.end();
//! ```

use crate::a11y::{self, Action, Role};
use crate::anim::{self, Anim, Ease};
use crate::paint;
use crate::state::State;
use crate::theme::{self, Rgb, Theme};
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::rc::Rc;

/// One entry of a [`Sidebar`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SidebarItem {
    pub icon: String,
    pub label: String,
    pub badge: String,
    pub footer: bool,
}

impl SidebarItem {
    pub fn new(icon: &str, label: &str) -> SidebarItem {
        SidebarItem {
            icon: icon.into(),
            label: label.into(),
            ..Default::default()
        }
    }
    /// A short count or mark at the end of the row (expanded).
    pub fn badge(mut self, b: &str) -> Self {
        self.badge = b.into();
        self
    }
    /// Kept at the bottom of the rail (settings, account).
    pub fn footer(mut self) -> Self {
        self.footer = true;
        self
    }
}

/// How a [`Sidebar`] expands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarMode {
    /// The rail grows in its row; the content beside it makes room.
    #[default]
    Push,
    /// The rail keeps its width; the expanded panel covers the content.
    Overlay,
}

struct Sb {
    items: Vec<SidebarItem>,
    hover: Option<usize>,
    cursor: usize,
    hovered: bool,
    layer_hovered: bool,
    open: bool,
    gen: u64,
    rail: i32,
    wide: i32,
    mode: SidebarMode,
    layer: Option<Widget>,
    layer_w: Anim<f64>,
    on_activate: Option<Box<dyn FnMut(usize)>>,
}

/// Navigation rail that expands on hover. See the [module docs](self).
#[derive(Clone)]
pub struct Sidebar {
    inner: Widget,
    st: Rc<RefCell<Sb>>,
    selected: State<usize>,
    pinned: State<bool>,
    expanded: State<bool>,
}
fltk::widget_extends!(Sidebar, Widget, inner);

fn row_h(t: &Theme) -> i32 {
    t.spacing.control_height + t.spacing.md
}

/// Rows from the top (`false`) and from the bottom (footer, `true`):
/// content-relative y of each item.
fn rows(items: &[SidebarItem], h: i32, t: &Theme) -> Vec<i32> {
    let rh = row_h(t);
    let pad = t.spacing.md;
    let mut top = pad;
    let feet = items.iter().filter(|i| i.footer).count() as i32;
    let mut bottom = h - pad - feet * rh;
    items
        .iter()
        .map(|it| {
            if it.footer {
                let y = bottom;
                bottom += rh;
                y
            } else {
                let y = top;
                top += rh;
                y
            }
        })
        .collect()
}

fn surface(t: &Theme) -> Rgb {
    t.palette.foreground.over(t.palette.background, 0.04)
}

/// Paints the sidebar in `(x, y, w, h)` (`w` between the rail and the
/// expanded width), mirrored when `rtl`.
fn paint_rail(s: &Sb, sel: usize, (x, y, w, h): (i32, i32, i32, i32), rtl: bool, ring: bool) {
    theme::with(|t| {
        let m = crate::layout::Mirror::new(rtl, x, w);
        let bg = surface(t);
        paint::fill(x, y, w, h, 0, bg);
        let fg = t.palette.foreground;
        // Hairline on the edge facing the content.
        paint::fill(m.x(x + w - 1, 1), y, 1, h, 0, fg.over(bg, 0.12));
        // Labels fade in as the rail widens (none at rail width).
        let reveal = if s.wide > s.rail {
            ((w - s.rail) as f64 / (s.wide - s.rail) as f64).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let rh = row_h(t);
        let inset = t.spacing.sm;
        fltk::draw::push_clip(x, y, w, h);
        for (i, (it, ry)) in s.items.iter().zip(rows(&s.items, h, t)).enumerate() {
            let ry = y + ry;
            let (bx, bw) = (x + inset, (w - 2 * inset).max(0));
            let selected = i == sel;
            if selected {
                paint::fill(
                    m.x(bx, bw),
                    ry + 2,
                    bw,
                    rh - 4,
                    t.radius,
                    t.fill(&t.controls.selected, bg),
                );
                paint::fill(m.x(x + 2, 3), ry + 6, 3, rh - 12, 1, t.palette.accent);
            } else if s.hover == Some(i) {
                paint::fill(
                    m.x(bx, bw),
                    ry + 2,
                    bw,
                    rh - 4,
                    t.radius,
                    t.fill(&t.controls.hover, bg),
                );
            }
            if ring && i == s.cursor {
                paint::stroke(
                    m.x(bx, bw),
                    ry + 2,
                    bw,
                    rh - 4,
                    t.radius,
                    t.controls.focus.border_width.max(1),
                    t.border(&t.controls.focus, bg),
                );
            }
            let ink = if selected { t.palette.accent } else { fg };
            paint::glyph(&it.icon, m.x(x, s.rail), ry, s.rail, rh, t.font.icon, ink);
            if reveal > 0.0 {
                let text = fg.over(bg, reveal);
                let tx = x + s.rail;
                let bw2 = if it.badge.is_empty() {
                    0
                } else {
                    paint::measure(&it.badge, t.font.caption, false) + t.spacing.md
                };
                let lw = (x + w - inset - t.spacing.md - bw2 - tx).max(0);
                paint::text_elided(
                    &it.label,
                    m.x(tx, lw),
                    ry,
                    lw,
                    rh,
                    t.font.body,
                    selected,
                    text,
                    m.align(Align::Left),
                );
                if bw2 > 0 {
                    let bx2 = x + w - inset - t.spacing.md - bw2 + t.spacing.md;
                    paint::text(
                        &it.badge,
                        m.x(bx2, bw2 - t.spacing.md),
                        ry,
                        bw2 - t.spacing.md,
                        rh,
                        t.font.caption,
                        false,
                        fg.darker(1.3).over(bg, reveal),
                        m.align(Align::Right),
                    );
                }
            }
        }
        fltk::draw::pop_clip();
    });
}

fn hit(s: &Sb, top: i32, h: i32, ey: i32) -> Option<usize> {
    theme::with(|t| {
        let rh = row_h(t);
        rows(&s.items, h, t)
            .iter()
            .position(|&ry| ey >= top + ry && ey < top + ry + rh)
    })
}

impl Sidebar {
    /// A rail with `items`; `selected` is the selected item's index.
    pub fn new(items: Vec<SidebarItem>, selected: &State<usize>) -> Sidebar {
        let (rail, wide) = theme::with(|t| {
            (
                t.spacing.control_height + 2 * t.spacing.lg,
                240.max(t.font.body * 20),
            )
        });
        let mut inner = Widget::default().with_size(rail, 240);
        inner.set_frame(FrameType::NoBox);
        inner.set_visible_focus();
        let st = Rc::new(RefCell::new(Sb {
            items,
            hover: None,
            cursor: selected.get(),
            hovered: false,
            layer_hovered: false,
            open: false,
            gen: 0,
            rail,
            wide,
            mode: SidebarMode::Push,
            layer: None,
            layer_w: Anim::new(),
            on_activate: None,
        }));
        let sb = Sidebar {
            inner,
            st,
            selected: selected.clone(),
            pinned: State::new(false),
            expanded: State::new(false),
        };
        sb.install();
        sb
    }

    /// Width collapsed (the rail) and expanded.
    pub fn widths(self, rail: i32, expanded: i32) -> Self {
        {
            let mut s = self.st.borrow_mut();
            s.rail = rail.max(1);
            s.wide = expanded.max(s.rail);
        }
        let mut w = self.inner.clone();
        let h = w.h();
        w.set_size(if self.expanded.get() { expanded } else { rail }, h);
        self
    }

    /// Push (default) or overlay, see [`SidebarMode`].
    pub fn mode(self, m: SidebarMode) -> Self {
        self.st.borrow_mut().mode = m;
        self
    }

    /// Keeps it expanded while `pinned` is true (two-way: [`Sidebar::set_pinned`]).
    pub fn bind_pinned(mut self, pinned: &State<bool>) -> Self {
        self.pinned = pinned.clone();
        let me = self.clone();
        crate::state::bind_widget(&self.inner, pinned, true, move |_| me.apply());
        self
    }

    /// Runs when an item is picked (click, Enter, Space), after `selected` changed.
    pub fn on_activate(self, f: impl FnMut(usize) + 'static) -> Self {
        self.st.borrow_mut().on_activate = Some(Box::new(f));
        self
    }

    pub fn set_pinned(&self, on: bool) {
        self.pinned.set(on);
        self.apply();
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned.get()
    }

    /// True while expanded (pinned, hovered or focused). Bind it to follow.
    pub fn expanded(&self) -> State<bool> {
        self.expanded.clone()
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    /// Expands now (as hover would, without the delay).
    pub fn expand(&self) {
        self.st.borrow_mut().open = true;
        self.apply();
    }

    /// Collapses unless pinned.
    pub fn collapse(&self) {
        self.st.borrow_mut().open = false;
        self.apply();
    }

    pub fn items(&self) -> Vec<SidebarItem> {
        self.st.borrow().items.clone()
    }

    pub fn set_items(&mut self, items: Vec<SidebarItem>) {
        {
            let mut s = self.st.borrow_mut();
            s.cursor = s.cursor.min(items.len().saturating_sub(1));
            s.items = items;
        }
        self.inner.redraw();
    }

    /// Simulates the pointer resting on (`true`) or leaving it, through the
    /// same path as real Enter/Leave events, hover intent included.
    pub fn hover(&self, on: bool) {
        if on {
            self.entered()
        } else {
            self.left()
        }
    }

    fn entered(&self) {
        let gen = {
            let mut s = self.st.borrow_mut();
            s.hovered = true;
            s.gen += 1;
            s.gen
        };
        let me = self.clone();
        app::add_timeout3(anim::ms::HOVER_OPEN as f64 / 1000.0, move |_| {
            let go = {
                let s = me.st.borrow();
                s.gen == gen && (s.hovered || s.layer_hovered)
            };
            if go && !me.inner.was_deleted() {
                me.expand();
            }
        });
    }

    fn left(&self) {
        let gen = {
            let mut s = self.st.borrow_mut();
            s.hovered = false;
            s.hover = None;
            s.gen += 1;
            s.gen
        };
        self.inner.clone().redraw();
        let me = self.clone();
        app::add_timeout3(anim::ms::HOVER_CLOSE as f64 / 1000.0, move |_| {
            if me.inner.was_deleted() {
                return;
            }
            let stay = {
                let s = me.st.borrow();
                s.gen != gen || s.hovered || s.layer_hovered
            };
            let held =
                crate::focus::is_focused(&me.inner) || crate::overlay::modal_open_over(&me.inner);
            if !stay && !held {
                me.collapse();
            }
        });
    }

    fn activate(&self, i: usize) {
        if i >= self.st.borrow().items.len() {
            return;
        }
        self.st.borrow_mut().cursor = i;
        self.selected.set(i);
        let f = self.st.borrow_mut().on_activate.take();
        if let Some(mut f) = f {
            f(i);
            let mut s = self.st.borrow_mut();
            if s.on_activate.is_none() {
                s.on_activate = Some(f);
            }
        }
        self.inner.clone().redraw();
    }

    /// Expanded = pinned or open; moves the width (push) or the layer (overlay).
    fn apply(&self) {
        let want = self.pinned.get() || self.st.borrow().open;
        if self.expanded.get() != want {
            self.expanded.set(want);
        }
        let (mode, rail, wide) = {
            let s = self.st.borrow();
            (s.mode, s.rail, s.wide)
        };
        match mode {
            SidebarMode::Push => {
                let target = if want { wide } else { rail };
                if self.inner.w() != target
                    && !crate::layout::refit(&self.inner, (target, self.inner.h()))
                {
                    {
                        let mut w = self.inner.clone();
                        let h = w.h();
                        w.set_size(target, h);
                        if let Some(mut p) = w.parent() {
                            p.redraw();
                        }
                    }
                }
            }
            SidebarMode::Overlay => {
                if want {
                    self.open_layer();
                }
                if let Some(mut l) = self.st.borrow().layer.clone() {
                    l.redraw();
                }
            }
        }
        a11y::changed(&self.inner);
        self.inner.clone().redraw();
    }

    fn open_layer(&self) {
        if self
            .st
            .borrow()
            .layer
            .as_ref()
            .is_some_and(|l| !l.was_deleted())
        {
            return;
        }
        let (rail, wide) = {
            let s = self.st.borrow();
            (s.rail, s.wide)
        };
        let rtl = crate::layout::is_rtl(&self.inner);
        let x = if rtl {
            self.inner.x() + self.inner.w() - wide
        } else {
            self.inner.x()
        };
        let Some(mut layer) = crate::overlay::open_layer(
            &self.inner,
            Some((x, self.inner.y(), wide, self.inner.h())),
            false,
        ) else {
            return;
        };
        {
            let mut s = self.st.borrow_mut();
            s.layer_w = Anim::new();
            s.layer_w.jump(rail as f64);
            s.layer = Some(layer.clone());
        }
        let me = self.clone();
        layer.draw(move |l| {
            let mut s = me.st.borrow_mut();
            let want = me.expanded.get();
            let (rail, wide) = (s.rail as f64, s.wide as f64);
            let w = s
                .layer_w
                .get(
                    if want { wide } else { rail },
                    anim::ms::LAYOUT,
                    Ease::OutCubic,
                    l,
                )
                .round() as i32;
            let closing = !want && !s.layer_w.running();
            let rtl = crate::layout::is_rtl(&me.inner);
            let x = if rtl { l.x() + l.w() - w } else { l.x() };
            let ring = crate::focus::show_ring(&me.inner);
            paint_rail(&s, me.selected.get(), (x, l.y(), w, l.h()), rtl, ring);
            if closing {
                s.layer = None;
                drop(s);
                let l2 = l.clone();
                app::add_timeout3(0.0, move |_| crate::overlay::close_layer(&l2));
            }
        });
        let me = self.clone();
        layer.handle(move |l, ev| {
            let width = me.st.borrow_mut().layer_w.target().unwrap_or(0.0) as i32;
            let rtl = crate::layout::is_rtl(&me.inner);
            let inside_panel = {
                let ex = app::event_x();
                if rtl {
                    ex >= l.x() + l.w() - width
                } else {
                    ex < l.x() + width
                }
            };
            match ev {
                Event::Enter | Event::Move => {
                    let h = if inside_panel {
                        hit(&me.st.borrow(), l.y(), l.h(), app::event_y())
                    } else {
                        None
                    };
                    let was = me.st.borrow().layer_hovered;
                    {
                        let mut s = me.st.borrow_mut();
                        s.layer_hovered = inside_panel;
                        s.hover = h;
                    }
                    if was && !inside_panel {
                        me.left();
                    }
                    l.redraw();
                    inside_panel
                }
                Event::Leave => {
                    me.st.borrow_mut().layer_hovered = false;
                    me.left();
                    true
                }
                Event::Push if inside_panel => {
                    if let Some(i) = hit(&me.st.borrow(), l.y(), l.h(), app::event_y()) {
                        me.activate(i);
                    }
                    true
                }
                Event::MouseWheel => inside_panel,
                _ => false,
            }
        });
    }

    fn install(&self) {
        let (st, sel, me) = (self.st.clone(), self.selected.clone(), self.clone());
        let mut inner = self.inner.clone();
        inner.draw(move |w| {
            let s = st.borrow();
            let overlay_open = s.mode == SidebarMode::Overlay && s.layer.is_some();
            let ring = crate::focus::show_ring(w) && !overlay_open;
            paint_rail(
                &s,
                sel.get(),
                (w.x(), w.y(), w.w(), w.h()),
                crate::layout::is_rtl(w),
                ring,
            );
            let _ = &me;
        });
        let me = self.clone();
        inner.handle(move |w, ev| match ev {
            Event::Enter => {
                me.entered();
                true
            }
            Event::Leave => {
                me.left();
                true
            }
            Event::Move => {
                let h = hit(&me.st.borrow(), w.y(), w.h(), app::event_y());
                if me.st.borrow().hover != h {
                    me.st.borrow_mut().hover = h;
                    w.redraw();
                }
                true
            }
            Event::Push => {
                let _ = w.take_focus();
                if let Some(i) = hit(&me.st.borrow(), w.y(), w.h(), app::event_y()) {
                    me.activate(i);
                }
                true
            }
            Event::Focus => {
                me.st.borrow_mut().open = true;
                me.apply();
                true
            }
            Event::Unfocus => {
                let hovered = {
                    let s = me.st.borrow();
                    s.hovered || s.layer_hovered
                };
                if !hovered {
                    me.collapse();
                } else {
                    w.redraw();
                }
                true
            }
            Event::KeyDown => {
                let n = me.st.borrow().items.len();
                if n == 0 {
                    return false;
                }
                let cur = me.st.borrow().cursor.min(n - 1);
                let next = match app::event_key() {
                    Key::Up => Some(cur.saturating_sub(1)),
                    Key::Down => Some((cur + 1).min(n - 1)),
                    Key::Home => Some(0),
                    Key::End => Some(n - 1),
                    Key::Enter | Key::KPEnter => {
                        me.activate(cur);
                        return true;
                    }
                    Key::Escape => {
                        if me.is_expanded() && !me.is_pinned() {
                            me.collapse();
                            return true;
                        }
                        return false;
                    }
                    _ if app::event_key() == Key::from_char(' ') => {
                        me.activate(cur);
                        return true;
                    }
                    _ => None,
                };
                match next {
                    Some(i) => {
                        me.st.borrow_mut().cursor = i;
                        if !me.is_expanded() {
                            me.expand();
                        }
                        w.redraw();
                        if let Some(mut l) = me.st.borrow().layer.clone() {
                            l.redraw();
                        }
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        });
        let mut w2 = self.inner.clone();
        crate::state::bind_widget(&self.inner, &self.selected, false, move |_| w2.redraw());
        let (st, sel, ex, me) = (
            self.st.clone(),
            self.selected.clone(),
            self.expanded.clone(),
            self.inner.clone(),
        );
        a11y::describe(&self.inner, move |n| {
            let Ok(s) = st.try_borrow() else { return };
            n.role(Role::TabList)
                .name(&crate::i18n::tr("Sidebar"))
                .expanded(ex.get())
                .focusable();
            let h = me.h();
            let rh = theme::with(row_h);
            let ys = theme::with(|t| rows(&s.items, h, t));
            let m = crate::layout::Mirror::of(&me);
            for (i, (it, ry)) in s.items.iter().zip(ys).enumerate() {
                n.child(i as u64, |c| {
                    c.role(Role::Tab)
                        .name(&it.label)
                        .selected(sel.get() == i)
                        .position(i + 1, s.items.len())
                        .bounds(m.x(me.x(), me.w()), me.y() + ry, me.w(), rh)
                        .action(Action::Click)
                        .action(Action::Focus)
                        .focusable();
                    if !it.badge.is_empty() {
                        c.description(&it.badge);
                    }
                    if s.cursor == i {
                        c.focused();
                    }
                });
            }
        });
        let me = self.clone();
        a11y::on_action(&self.inner, move |r| {
            let Some(i) = r
                .key()
                .map(|k| k as usize)
                .filter(|i| *i < me.st.borrow().items.len())
            else {
                return false;
            };
            match r.action {
                Action::Click => me.activate(i),
                Action::Focus => {
                    me.st.borrow_mut().cursor = i;
                    let _ = me.inner.clone().take_focus();
                }
                _ => return false,
            }
            true
        });
    }

    /// The item under the keyboard cursor.
    pub fn cursor(&self) -> usize {
        self.st.borrow().cursor
    }

    /// The overlay panel's current width (overlay mode; 0 when closed).
    pub fn overlay_width(&self) -> i32 {
        let s = self.st.borrow();
        match &s.layer {
            Some(l) if !l.was_deleted() => s.layer_w.target().map_or(0, |v| v as i32),
            _ => 0,
        }
    }
}
