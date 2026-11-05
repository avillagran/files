//! Layout helpers on top of `fltk::group::Flex` with Omarchy spacing.
//!
//! Components get their Omarchy implicit size at creation; `fit` fixes a
//! child to that size along the flex axis, everything else stretches.
//!
//! ```no_run
//! use fltk::prelude::*;
//! use fltk_omarchy::{layout::*, widgets::*};
//! let mut row = row();                    // gap = control-gap (8), height = control-height
//! let ok = Button::new("OK").primary();
//! row.fit(&*ok);                          // natural width; the rest stretches
//! row.end();
//! ```
//!
//! ## Direction
//!
//! Rows follow the writing direction of the language
//! ([`crate::i18n::direction`]): in Arabic, Hebrew, Persian or Urdu the
//! first child of a row is on the right, and the components draw mirrored
//! (icons, switches, sliders, list columns, scrollbars, menus). A locale
//! switch re-lays out the rows of open windows, no rebuild. The child order
//! stays the reading order, so Tab still goes from the start to the end.
//!
//! Things that keep their left-to-right look in any language (media
//! controls, code, numbers, a timeline) get an override, which also holds
//! for everything inside:
//!
//! ```no_run
//! use fltk_omarchy::layout::{self, Direction, FlexExt};
//! let mut transport = layout::row();
//! transport.direction(Direction::Ltr);
//! ```
//!
//! Custom widgets read [`direction_of`] (or [`Mirror::of`]) when they draw.
//!
//! ## Motion
//!
//! When animations are on ([`crate::anim::enabled`]), the toolkit's rows
//! and columns animate their own changes: a `fixed` size that changes, a
//! child shown, hidden or added slides and fades over 190 ms OutCubic
//! instead of jumping. Nothing to call: `row()`, `column()`, `section()`,
//! containers you [`FlexExt::fit`] into and every `Flex` of an
//! [`AppWindow`](crate::AppWindow) at `end` do it (not a `Flex` with its
//! own draw callback). [`set_visible`] also collapses and fades out what
//! it hides. Opt out per container with [`FlexExt::animated`]`(false)` or
//! [`set_animated`] (a whole window too). The first layout, window
//! resizes and direction flips never animate. docs/ANIMATION.md.
//!
//! ```no_run
//! use fltk::prelude::*;
//! use fltk_omarchy::layout::{self, FlexExt};
//! # let mut row = layout::row();
//! # let sidebar = fltk::frame::Frame::default();
//! row.fixed(&sidebar, 240);   // was 56: slides to 240 px
//! row.layout();
//! layout::set_visible(&sidebar, false); // collapses and fades out
//! ```

use crate::theme;
use fltk::{group::Flex, prelude::*};

/// Vertical stack with the panel gap (14 px at base 12).
pub fn column() -> Flex {
    let mut f = Flex::default().column();
    f.set_spacing(theme::with(|t| t.spacing.panel_gap));
    crate::motion::install(&f);
    f
}

/// Vertical stack with the small section gap (6 px), for a header + rows.
pub fn section() -> Flex {
    let mut f = Flex::default().column();
    f.set_spacing(theme::with(|t| t.spacing.md));
    crate::motion::install(&f);
    f
}

/// Horizontal row of controls (gap 8, control height).
pub fn row() -> Flex {
    let (gap, h) = theme::with(|t| (t.spacing.control_gap, t.spacing.control_height));
    let mut f = Flex::default().row().with_size(100, h);
    f.set_spacing(gap);
    mirror(&f);
    f
}

/// Flexible empty space.
pub fn spacer() -> fltk::frame::Frame {
    fltk::frame::Frame::default()
}

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    // Natural (w, h) recorded when a child is fitted, before Flex layout
    // changes its geometry.
    // The tracker tells a live entry from one left by a deleted widget whose
    // address fltk reused.
    static NATURAL: RefCell<HashMap<usize, (fltk::widget::Widget, (i32, i32))>> = RefCell::new(HashMap::new());
}

fn key<W: WidgetExt>(w: &W) -> usize {
    w.as_widget_ptr() as usize
}

fn record<W: WidgetExt>(w: &W, size: (i32, i32)) {
    // SAFETY: `w` is a live widget; the copy only tracks its deletion.
    let t = unsafe { <fltk::widget::Widget as WidgetBase>::from_widget_ptr(w.as_widget_ptr()) };
    NATURAL.with(|n| {
        let mut n = n.borrow_mut();
        if n.len() > 256 {
            n.retain(|_, (t, _)| !t.was_deleted());
        }
        n.insert(key(w), (t, size));
    });
}

/// Records a widget's natural size (done by `fit`; call it for widgets you
/// size yourself before putting them in a fitted container).
pub fn remember<W: WidgetExt>(w: &W) {
    record(w, (w.w(), w.h()));
}

fn natural_of<W: WidgetExt>(w: &W) -> Option<(i32, i32)> {
    NATURAL.with(|n| n.borrow().get(&key(w)).filter(|(t, _)| !t.was_deleted()).map(|(_, s)| *s))
}

/// Updates the natural size of a fitted widget whose content changed and
/// re-fixes it in its parent flex. Returns false (and does nothing) for
/// widgets that were not fitted yet.
pub fn refit<W: WidgetExt>(w: &W, natural: (i32, i32)) -> bool {
    let fitted = NATURAL.with(|n| {
        let mut n = n.borrow_mut();
        n.get_mut(&key(w)).filter(|(t, _)| !t.was_deleted()).map(|v| v.1 = natural).is_some()
    });
    if !fitted {
        return false;
    }
    if let Some(parent) = w.parent() {
        if let Some(mut p) = Flex::from_dyn_widget(&parent) {
            p.fixed(w, if is_column(&p) { natural.1 } else { natural.0 });
            p.layout();
            p.redraw();
        }
    }
    true
}

fn is_column(f: &Flex) -> bool {
    f.get_type::<fltk::group::FlexType>() == fltk::group::FlexType::Column
}

pub trait FlexExt {
    /// Fixes `child` to its implicit size along this flex's axis.
    fn fit<W: WidgetExt>(&mut self, child: &W);
    /// Ends a nested flex and fixes it in its parent flex at its natural size.
    fn close_fitted(&mut self);
    /// Forces the direction of this container and everything inside it,
    /// whatever the language: `row.direction(Direction::Ltr)` for media
    /// controls, code, numbers or a timeline. See [`set_direction`].
    fn direction(&mut self, d: Direction) -> &mut Self;
    /// Turns automatic layout motion off (`false`) or on again for this
    /// container and everything inside it. See [`set_animated`].
    fn animated(&mut self, on: bool) -> &mut Self;
}

impl FlexExt for Flex {
    fn fit<W: WidgetExt>(&mut self, child: &W) {
        // Creation order decides the parent in fltk; fitting a widget created
        // elsewhere moves it here, so the call reads as "put this in the row".
        if child.parent().map_or(true, |p| p.as_widget_ptr() != self.as_widget_ptr()) {
            self.add(child);
        }
        let (w, h) = natural_of(child).unwrap_or_else(|| {
            remember(child);
            (child.w(), child.h())
        });
        self.fixed(child, if is_column(self) { h } else { w });
        crate::motion::install_if_free(self);
        if !is_column(self) {
            register(self);
            // Shown already: FLTK would lay it out left to right at the next draw.
            if self.visible_r() {
                relayout_if_mirrored(self);
            }
        }
    }

    fn direction(&mut self, d: Direction) -> &mut Self {
        crate::motion::install_if_free(self);
        if !is_column(self) {
            register(self);
        }
        set_direction(self, Some(d));
        self
    }

    fn animated(&mut self, on: bool) -> &mut Self {
        set_animated(self, on);
        self
    }

    fn close_fitted(&mut self) {
        self.end();
        crate::motion::install_if_free(self);
        if !is_column(self) {
            register(self);
        }
        let (w, h) = natural(self);
        record(self, (w, h));
        let me = self.clone();
        if let Some(parent) = self.parent() {
            if let Some(mut p) = Flex::from_dyn_widget(&parent) {
                let size = if is_column(&p) { h } else { w };
                p.fixed(&me, size);
            }
        }
    }
}

/// Natural size of a flex from its fitted children (flexible children add
/// nothing along the axis).
fn natural(f: &Flex) -> (i32, i32) {
    let column = is_column(f);
    let kids: Vec<(i32, i32)> = (0..f.children())
        .filter_map(|i| f.child(i))
        .filter(|c| c.visible())
        .map(|c| natural_of(&c).unwrap_or((0, 0)))
        .collect();
    let (l, t, r, b) = f.margins();
    let gaps = f.spacing() * (kids.len() as i32 - 1).max(0);
    let main: i32 = kids.iter().map(|k| if column { k.1 } else { k.0 }).sum::<i32>() + gaps;
    let cross = kids.iter().map(|k| if column { k.0 } else { k.1 }).max().unwrap_or(0);
    if column { (cross + l + r, main + t + b) } else { (main + l + r, cross + t + b) }
}

// =============================================================== direction

/// Writing direction of a layout (the language's, see [`crate::i18n::direction`]).
pub use crate::i18n::Direction;

// ================================================================== motion

pub use crate::motion::{animated, animating, set_animated, set_visible};
pub use crate::sidebar::{Sidebar, SidebarItem, SidebarMode};
use fltk::enums::{Align, Key};
use fltk::widget::Widget;

thread_local! {
    // Containers with a forced direction (deletion-tracked copies).
    static OVERRIDES: RefCell<Vec<(Widget, Direction)>> = const { RefCell::new(Vec::new()) };
    // Rows that mirror their children in a right-to-left direction.
    static MIRRORS: RefCell<HashMap<usize, Flex>> = RefCell::new(HashMap::new());
    static PRUNE_AT: std::cell::Cell<usize> = const { std::cell::Cell::new(256) };
    static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn as_widget<W: WidgetExt>(w: &W) -> Widget {
    // SAFETY: `w` is a live widget; the copy is only read or compared.
    unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) }
}

/// Forces the direction of `w` and of everything inside it (`None`
/// follows the language again). Rows inside are laid out again and `w`
/// redraws. Same as [`FlexExt::direction`] for any widget (a canvas whose
/// `KeyboardPanel` grid should not swap Left/Right, a media bar…).
pub fn set_direction<W: WidgetExt>(w: &W, d: Option<Direction>) {
    let p = w.as_widget_ptr() as usize;
    OVERRIDES.with(|o| {
        let mut o = o.borrow_mut();
        o.retain(|(t, _)| !t.was_deleted() && t.as_widget_ptr() as usize != p);
        if let Some(d) = d {
            o.push((as_widget(w), d));
        }
    });
    relayout_all();
    as_widget(w).redraw();
}

/// The direction `w` lays out and draws in: the nearest override on `w` or
/// its parents, else the language's.
pub fn direction_of<W: WidgetExt>(w: &W) -> Direction {
    let forced = OVERRIDES.with(|o| {
        let o = o.borrow();
        if o.is_empty() {
            return None;
        }
        let mut cur = Some(as_widget(w));
        while let Some(c) = cur {
            let p = c.as_widget_ptr();
            if let Some((_, d)) = o.iter().find(|(t, _)| t.as_widget_ptr() == p && !t.was_deleted()) {
                return Some(*d);
            }
            cur = c.parent().map(|g| as_widget(&g));
        }
        None
    });
    forced.unwrap_or_else(crate::i18n::direction)
}

/// True when `w` lays out right to left ([`direction_of`]).
pub fn is_rtl<W: WidgetExt>(w: &W) -> bool {
    direction_of(w).is_rtl()
}

/// Left/Right as the user means them in `w`'s direction: swapped when it
/// is right to left, so code written for "Right = next" works mirrored.
pub fn logical_key<W: WidgetExt>(w: &W, k: Key) -> Key {
    if is_rtl(w) { Mirror::swap_key(k) } else { k }
}

/// Mirrors x coordinates inside a box when the direction is right to left.
/// Components compute their geometry left to right and pass every x through
/// [`Mirror::x`] (spans) or [`Mirror::point`] (the pointer), and their text
/// alignment through [`Mirror::align`].
///
/// ```no_run
/// # use fltk::{prelude::*, enums::Align, widget::Widget};
/// # use fltk_omarchy::layout::Mirror;
/// # let w = Widget::default();
/// let m = Mirror::of(&w);
/// let icon_x = m.x(w.x() + 8, 22);              // on the right in Arabic
/// let align = m.align(Align::Left);             // Right in Arabic
/// let ex = m.point(fltk::app::event_x());       // hit-test in LTR terms
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mirror {
    pub on: bool,
    x: i32,
    w: i32,
}

impl Mirror {
    /// For widget `w` in its direction ([`direction_of`]).
    pub fn of<W: WidgetExt>(w: &W) -> Mirror {
        Mirror { on: is_rtl(w), x: w.x(), w: w.w() }
    }

    /// For the box `x..x + w`, mirrored when `on`.
    pub fn new(on: bool, x: i32, w: i32) -> Mirror {
        Mirror { on, x, w }
    }

    /// Left edge of the span `x..x + w` (laid out left to right).
    pub fn x(&self, x: i32, w: i32) -> i32 {
        if self.on { 2 * self.x + self.w - x - w } else { x }
    }

    /// A point (pointer x), mirrored: hit-test mirrored widgets with their
    /// left-to-right geometry.
    pub fn point(&self, x: i32) -> i32 {
        if self.on { 2 * self.x + self.w - 1 - x } else { x }
    }

    /// `(x, y, w, h)` with its x mirrored.
    pub fn rect(&self, r: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
        (self.x(r.0, r.2), r.1, r.2, r.3)
    }

    /// Left and Right alignment swapped when mirrored (Center, Top… kept).
    pub fn align(&self, a: Align) -> Align {
        if !self.on {
            return a;
        }
        let (l, r) = (a.contains(Align::Left), a.contains(Align::Right));
        match (l, r) {
            (true, false) => (a & !Align::Left) | Align::Right,
            (false, true) => (a & !Align::Right) | Align::Left,
            _ => a,
        }
    }

    /// Left and Right swapped when mirrored.
    pub fn key(&self, k: Key) -> Key {
        if self.on { Self::swap_key(k) } else { k }
    }

    /// A glyph that points along the line (a chevron, "›"): its mirror
    /// image when mirrored.
    pub fn glyph<'a>(&self, ltr: &'a str, rtl: &'a str) -> &'a str {
        if self.on { rtl } else { ltr }
    }

    fn swap_key(k: Key) -> Key {
        match k {
            Key::Left => Key::Right,
            Key::Right => Key::Left,
            k => k,
        }
    }
}

/// Makes a row follow the direction: in right to left its children are
/// placed from the right (same sizes, same order). [`row`] calls it;
/// [`FlexExt::fit`] and [`FlexExt::close_fitted`] install the resize part
/// (they keep a draw callback the app set). Call it for a plain
/// `Flex::default().row()` you fill another way. It sets the flex's resize
/// callback and its draw callback (run before FLTK's); a custom draw
/// callback on a mirrored row calls [`relayout_if_mirrored`] first.
pub fn mirror(f: &Flex) {
    if !register(f) {
        return;
    }
    // The toolkit's draw: lays the row out (mirrored) and animates changes.
    crate::motion::install(f);
}

/// Registers the row and its resize hook; false if it was already.
pub(crate) fn register(f: &Flex) -> bool {
    let p = f.as_widget_ptr() as usize;
    let new = MIRRORS.with(|m| {
        let mut m = m.borrow_mut();
        if m.get(&p).is_some_and(|x| !x.was_deleted()) {
            return false;
        }
        m.insert(p, f.clone());
        let limit = PRUNE_AT.with(|c| c.get());
        if m.len() > limit {
            m.retain(|_, x| !x.was_deleted());
            PRUNE_AT.with(|c| c.set((m.len() * 2).max(256)));
        }
        true
    });
    if new {
        watch();
        // The motion hook's resize callback mirrors too.
        if !crate::motion::installed(f) {
            let mut f2 = f.clone();
            f2.resize_callback(|f, _, _, _, _| flip(f));
        }
    }
    new
}

/// Registers every `Flex` row inside `g` (any depth) like [`FlexExt::fit`]
/// does, so rows built with plain fltk-rs mirror too. [`AppWindow::end`]
/// calls it for the window. It sets their resize callback: a row with its
/// own resize callback should not be inside (or force `Direction::Ltr` and
/// mirror it yourself).
///
/// [`AppWindow::end`]: crate::AppWindow::end
pub fn mirror_rows<G: GroupExt>(g: &G) {
    fn walk(g: &fltk::group::Group) {
        for i in 0..g.children() {
            let Some(c) = g.child(i) else { continue };
            let Some(sub) = c.as_group() else { continue };
            if let Some(mut f) = Flex::from_dyn_widget(&c) {
                crate::motion::install_if_free(&f);
                if !is_column(&f) && register(&f) && f.visible_r() {
                    relayout_if_mirrored(&mut f);
                }
            }
            walk(&sub);
        }
    }
    if let Some(g) = g.as_group() {
        walk(&g);
    }
}

/// For custom draw callbacks of mirrored rows: lays the row out again when
/// it is right to left (FLTK lays a `Flex` out left to right while drawing
/// after `fixed`, `add`, `hide`…). No-op left to right.
pub fn relayout_if_mirrored(f: &mut Flex) {
    if !is_column(f) && is_rtl(f) {
        // resize → Fl_Flex::layout (left to right) → resize callback → flip.
        let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
        f.resize(x, y, w, h);
    }
}

/// Mirrors a row's children after FLTK laid it out (resize hooks).
pub(crate) fn flip_row(f: &mut Flex) {
    flip(f)
}

/// Mirrors the children of a row laid out left to right by FLTK.
fn flip(f: &mut Flex) {
    if is_column(f) || f.children() == 0 || !is_rtl(f) {
        return;
    }
    let (x0, w0) = (f.x(), f.w());
    for i in 0..f.children() {
        if let Some(mut c) = f.child(i) {
            let nx = 2 * x0 + w0 - c.x() - c.w();
            if nx != c.x() {
                let (y, w, h) = (c.y(), c.w(), c.h());
                c.resize(nx, y, w, h);
            }
        }
    }
}

/// Lays out every registered row again (direction change).
fn relayout_all() {
    let rows: Vec<Flex> = MIRRORS.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|_, x| !x.was_deleted());
        let mut v: Vec<Flex> = m.values().cloned().collect();
        // Outer rows first (parents before children: fewer passes).
        v.sort_by_key(|f| {
            let mut d = 0;
            let mut p = f.parent();
            while let Some(g) = p {
                d += 1;
                p = g.parent();
            }
            d
        });
        v
    });
    for mut f in rows {
        let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
        // Left to right again first, then mirrored if it is right to left now.
        f.resize(x, y, w, h);
        f.redraw();
    }
}

fn watch() {
    if WATCHING.with(|w| w.replace(true)) {
        return;
    }
    let last = std::cell::Cell::new(crate::i18n::is_rtl());
    crate::i18n::on_change(move |_| {
        let now = crate::i18n::is_rtl();
        if last.replace(now) != now {
            relayout_all();
        }
    })
    .forever();
}
