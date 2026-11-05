//! Automatic layout motion for the toolkit's `Flex` containers.
//!
//! Every row and column the toolkit creates or fits into ([`layout::row`],
//! [`layout::column`], [`FlexExt::fit`], the rows and columns of an
//! [`AppWindow`] at `end`) draws through [`draw`]: before its children are
//! drawn it lays out what changed (FLTK's lazy layout), compares the
//! children with what it drew last time and, when animations are on and
//! the container itself kept its place, moves them there over
//! [`anim::ms::LAYOUT`] (190 ms OutCubic, o3's `PinnedKeyboardPanel`).
//!
//! - A `fixed` size that changes (a sidebar's width, `refit`), children
//!   added, hidden (`hide` + `layout`) or shown: the others slide.
//! - A child that appears grows from its start edge and fades in, keeping
//!   its final size (nothing reflows mid-way), as o3's `T3Reveal`.
//! - [`set_visible`]`(w, false)` collapses `w` to nothing and fades it out
//!   (a picture of its last frame) while the others close the gap.
//! - Not animated: the first layout, a container that moved or changed
//!   size (window resizes, a parent's animation), a direction flip, a new
//!   theme or language.
//!
//! FLTK's own state is always a real layout: each frame sets the children
//! to the interpolated rectangles, and an interruption (a window resize, a
//! new change, animations turned off) first puts the children back to
//! their target sizes and lays out again, so `fixed` sizes are never lost.
//!
//! [`layout::row`]: crate::layout::row
//! [`layout::column`]: crate::layout::column
//! [`FlexExt::fit`]: crate::layout::FlexExt::fit
//! [`AppWindow`]: crate::AppWindow
//! [`anim::ms::LAYOUT`]: crate::anim::ms::LAYOUT

use crate::anim::{self, Ease, Tween};
use crate::paint;
use crate::theme::{self, Rgb};
use fltk::{enums::*, group::Flex, image::RgbImage, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

type Rect = (i32, i32, i32, i32);

#[derive(Clone)]
struct Kid {
    w: Widget,
    rect: Rect,
    visible: bool,
}

impl Kid {
    fn ptr(&self) -> usize {
        ptr(&self.w)
    }
}

/// The widget's address, 0 once deleted (`as_widget_ptr` panics then).
fn ptr(w: &Widget) -> usize {
    if w.was_deleted() {
        0
    } else {
        w.as_widget_ptr() as usize
    }
}

fn same(a: &[Kid], b: &[Kid]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.ptr() == y.ptr() && x.rect == y.rect && x.visible == y.visible)
}

struct Item {
    w: Widget,
    from: Rect,
    to: Rect,
    appear: bool,
}

/// A picture of a child hidden by [`set_visible`], drawn while it collapses.
struct Ghost {
    img: RefCell<RgbImage>,
    rect: Rect,
}

struct Run {
    tween: Tween,
    items: Vec<Item>,
    ghosts: Vec<Ghost>,
}

#[derive(Default)]
struct Track {
    seen: bool,
    rect: Rect,
    rtl: bool,
    epoch: u64,
    shown: Vec<Kid>,
    run: Option<Run>,
    ghosts: Vec<Ghost>,
    // Inside `draw`'s own layout: the resize hook only mirrors.
    in_draw: bool,
}

type Tracks = HashMap<usize, (Flex, Rc<RefCell<Track>>)>;

thread_local! {
    static TRACKS: RefCell<Tracks> = RefCell::new(HashMap::new());
    static PRUNE_AT: Cell<usize> = const { Cell::new(256) };
    // Containers whose subtree does not animate (deletion-tracked).
    static OFF: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
    static LOCALE_EPOCH: Cell<u64> = const { Cell::new(0) };
    static WATCHING: Cell<bool> = const { Cell::new(false) };
}

extern "C" {
    // cfltk: the user data of a Flex's custom draw callback (null if none).
    fn Fl_Flex_draw_data(f: *const std::ffi::c_void) -> *mut std::ffi::c_void;
}

fn epoch() -> u64 {
    theme::generation()
        .wrapping_mul(1_000_003)
        .wrapping_add(LOCALE_EPOCH.with(|e| e.get()))
}

fn track_of(p: usize) -> Option<Rc<RefCell<Track>>> {
    TRACKS.with(|t| {
        t.borrow()
            .get(&p)
            .filter(|(f, _)| !f.was_deleted())
            .map(|(_, t)| t.clone())
    })
}

/// True when `f` draws through the motion hook.
pub(crate) fn installed(f: &Flex) -> bool {
    track_of(f.as_widget_ptr() as usize).is_some()
}

/// Installs the hook on `f` unless the app gave it its own draw callback.
pub(crate) fn install_if_free(f: &Flex) {
    if installed(f) {
        return;
    }
    // SAFETY: `f` is a live Fl_Flex created by fltk-rs (cfltk's derived class).
    if !unsafe { Fl_Flex_draw_data(f.as_widget_ptr() as *const _) }.is_null() {
        return;
    }
    install(f);
}

/// Makes `f` draw through the motion hook (its draw and resize callbacks).
pub(crate) fn install(f: &Flex) {
    if installed(f) {
        return;
    }
    watch_locale();
    let tr = Rc::new(RefCell::new(Track::default()));
    TRACKS.with(|t| {
        let mut t = t.borrow_mut();
        let limit = PRUNE_AT.with(|c| c.get());
        if t.len() > limit {
            t.retain(|_, (f, _)| !f.was_deleted());
            PRUNE_AT.with(|c| c.set((t.len() * 2).max(256)));
        }
        t.insert(f.as_widget_ptr() as usize, (f.clone(), tr.clone()));
    });
    let mut f2 = f.clone();
    // SAFETY: a Flex created through fltk-rs is cfltk's derived class.
    unsafe { f2.assume_derived() };
    let t2 = tr.clone();
    f2.draw(move |f| draw(f, &t2));
    f2.super_draw(false);
    let t3 = tr;
    f2.resize_callback(move |f, _, _, _, _| resized(f, &t3));
}

fn watch_locale() {
    if WATCHING.with(|w| w.replace(true)) {
        return;
    }
    crate::i18n::on_change(|_| LOCALE_EPOCH.with(|e| e.set(e.get() + 1))).forever();
}

/// Turns automatic layout motion off (`false`) or on again for `w` and
/// everything inside it.
pub fn set_animated<W: WidgetExt>(w: &W, on: bool) {
    let p = w.as_widget_ptr();
    OFF.with(|o| {
        let mut o = o.borrow_mut();
        o.retain(|x| !x.was_deleted() && x.as_widget_ptr() != p);
        if !on {
            // SAFETY: a live widget; the copy only tracks its deletion.
            o.push(unsafe { Widget::from_widget_ptr(p) });
        }
    });
}

/// False when `w` or a parent turned layout motion off ([`set_animated`]).
pub fn animated<W: WidgetExt>(w: &W) -> bool {
    OFF.with(|o| {
        let o = o.borrow();
        if o.is_empty() {
            return true;
        }
        // SAFETY: a live widget, only read.
        let mut cur = Some(unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) });
        while let Some(c) = cur {
            if o.iter().any(|x| ptr(x) == c.as_widget_ptr() as usize) {
                return false;
            }
            // SAFETY: as above.
            cur = c
                .parent()
                .map(|g| unsafe { Widget::from_widget_ptr(g.as_widget_ptr()) });
        }
        true
    })
}

/// Shows or hides `w` and lays out its `Flex` parent. When the parent is a
/// toolkit container and animations are on, hiding collapses `w` and fades
/// it out while the others close the gap, and showing grows it in. FLTK's
/// state is final at once (hidden is hidden; focus leaves it); only the
/// picture moves. Without a toolkit parent it is `hide()`/`show()` + layout.
pub fn set_visible<W: WidgetExt>(w: &W, on: bool) {
    // SAFETY: a live widget; show/hide go through FLTK as usual.
    let mut w2 = unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) };
    if on == w.visible() {
        return;
    }
    let parent = w.parent().and_then(|p| Flex::from_dyn_widget(&p));
    if !on {
        if let Some(p) = parent.as_ref() {
            if let Some(tr) = track_of(p.as_widget_ptr() as usize) {
                let ready =
                    tr.borrow().seen && anim::enabled() && animated(p) && w.w() > 0 && w.h() > 0;
                if ready {
                    if let Some(img) = capture(w) {
                        tr.borrow_mut().ghosts.push(Ghost {
                            img: RefCell::new(img),
                            rect: (w.x(), w.y(), w.w(), w.h()),
                        });
                    }
                }
            }
        }
        w2.hide();
    } else {
        w2.show();
    }
    if let Some(mut p) = parent {
        p.layout();
        p.redraw();
    }
}

/// A picture of `w` as it is now (offscreen, over the color under it).
fn capture<W: WidgetExt>(w: &W) -> Option<RgbImage> {
    use fltk::surface::ImageSurface;
    let (ww, hh) = (w.w(), w.h());
    let surf = ImageSurface::new(ww, hh, false);
    ImageSurface::push_current(&surf);
    paint::fill(0, 0, ww, hh, 0, under(w));
    surf.draw(w, 0, 0);
    let img = surf.image();
    ImageSurface::pop_current();
    img
}

/// The color under `w`: the nearest parent with a box, else the window's.
fn under<W: WidgetExt>(w: &W) -> Rgb {
    let mut cur = w.parent();
    while let Some(g) = cur {
        if g.frame() != FrameType::NoBox || g.as_window().is_some() {
            let (r, gg, b) = g.color().to_rgb();
            return Rgb(((r as u32) << 16) | ((gg as u32) << 8) | b as u32);
        }
        cur = g.parent();
    }
    theme::with(|t| t.palette.background)
}

fn is_column(f: &Flex) -> bool {
    f.get_type::<fltk::group::FlexType>() == fltk::group::FlexType::Column
}

fn kids(f: &Flex) -> Vec<Kid> {
    (0..f.children())
        .filter_map(|i| f.child(i))
        .map(|c| Kid {
            rect: (c.x(), c.y(), c.w(), c.h()),
            visible: c.visible(),
            w: c,
        })
        .collect()
}

fn along(r: Rect, column: bool) -> i32 {
    if column {
        r.3
    } else {
        r.2
    }
}

/// Before FLTK lays out again mid-animation: children whose size along the
/// axis is still the one this hook set get their target size back, so a
/// `fixed` child is laid out at its real size, not at a frame of the motion.
fn restore(run: &Run, shown: &[Kid], column: bool) {
    for it in &run.items {
        let mut c = it.w.clone();
        if c.was_deleted() {
            continue;
        }
        let p = ptr(&c);
        let untouched = shown.iter().any(|k| {
            k.ptr() == p && along(k.rect, column) == along((c.x(), c.y(), c.w(), c.h()), column)
        });
        if untouched && it.rect_differs() {
            let (x, y) = (c.x(), c.y());
            if column {
                c.resize(x, y, c.w(), it.to.3)
            } else {
                c.resize(x, y, it.to.2, c.h())
            }
        }
    }
}

impl Item {
    fn rect_differs(&self) -> bool {
        self.from != self.to
    }
}

/// The resize hook: FLTK has just laid `f` out. Mid-animation that layout
/// used a frame's sizes: the children get their target sizes back and `f`
/// is laid out again. If that gives the same targets (a parent laid out
/// again, a snapshot) the current frame is put back and the motion goes
/// on; otherwise it stops there (the container moved, or the app changed
/// something: the next draw starts from what is on screen). Rows then
/// mirror in a right-to-left direction.
fn resized(f: &mut Flex, tr: &Rc<RefCell<Track>>) {
    let column = is_column(f);
    let rect = (f.x(), f.y(), f.w(), f.h());
    let same_rect = {
        let Ok(t) = tr.try_borrow() else {
            return crate::layout::flip_row(f);
        };
        if t.in_draw || t.run.is_none() {
            drop(t);
            return crate::layout::flip_row(f);
        }
        restore(t.run.as_ref().unwrap(), &t.shown, column);
        t.rect == rect
    };
    // Lays out again; this hook runs once more (in_draw) and mirrors.
    relayout(f, tr);
    let mut t = tr.borrow_mut();
    let keep = same_rect
        && t.run
            .as_ref()
            .is_some_and(|run| same_targets(run, &kids(f)));
    if keep {
        for k in &t.shown {
            let mut c = k.w.clone();
            if !c.was_deleted() && (c.x(), c.y(), c.w(), c.h()) != k.rect {
                c.resize(k.rect.0, k.rect.1, k.rect.2, k.rect.3);
            }
        }
    } else {
        t.run = None;
    }
}

/// True when the visible children are where `run` takes them.
fn same_targets(run: &Run, now: &[Kid]) -> bool {
    let vis: Vec<&Kid> = now.iter().filter(|k| k.visible).collect();
    vis.len() == run.items.len()
        && vis
            .iter()
            .zip(&run.items)
            .all(|(k, it)| k.ptr() == ptr(&it.w) && k.rect == it.to)
}

fn relayout(f: &mut Flex, tr: &Rc<RefCell<Track>>) {
    tr.borrow_mut().in_draw = true;
    let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
    f.resize(x, y, w, h);
    tr.borrow_mut().in_draw = false;
}

/// The draw callback of a toolkit `Flex` (FLTK's own draw is off).
fn draw(f: &mut Flex, tr: &Rc<RefCell<Track>>) {
    let column = is_column(f);
    let rect = (f.x(), f.y(), f.w(), f.h());
    let rtl = !column && crate::layout::is_rtl(f);
    let ep = epoch();
    let full = f.damage_type().bits() & !Damage::Child.bits() != 0;
    let now = kids(f);

    let reset = {
        let t = tr.borrow();
        !t.seen || t.rect != rect || t.rtl != rtl || t.epoch != ep
    };
    if reset {
        // First layout, a resize, a direction flip, a new theme or language:
        // FLTK's layout as it is, no motion.
        let old = {
            let mut t = tr.borrow_mut();
            t.ghosts.clear();
            t.run.take().map(|r| (r, t.shown.clone()))
        };
        if let Some((run, shown)) = old {
            restore(&run, &shown, column);
        }
        relayout(f, tr);
        let mut t = tr.borrow_mut();
        t.seen = true;
        t.rect = rect;
        t.rtl = rtl;
        t.epoch = ep;
        t.shown = kids(f);
    } else {
        let (changed, running) = {
            let t = tr.borrow();
            (
                !same(&now, &t.shown) || !t.ghosts.is_empty(),
                t.run.is_some(),
            )
        };
        if changed || (full && !running) {
            let from = {
                let t = tr.borrow();
                if let Some(run) = t.run.as_ref() {
                    restore(run, &t.shown, column);
                }
                t.shown.clone()
            };
            relayout(f, tr);
            let targets = kids(f);
            let mut t = tr.borrow_mut();
            let ghosts = std::mem::take(&mut t.ghosts);
            // Same targets (a relayout with nothing new): the motion goes on.
            let same = ghosts.is_empty()
                && t.run
                    .as_ref()
                    .is_some_and(|run| same_targets(run, &targets));
            if same {
                // Back to this frame; the placing below sets it anyway.
            } else if let Some(old) = t.run.take() {
                // Ghosts still collapsing keep going from where they are.
                let _ = old;
            }
            if !same && anim::enabled() && animated(f) {
                let mut items = Vec::new();
                let mut moves = !ghosts.is_empty();
                for k in targets.iter().filter(|k| k.visible) {
                    match from.iter().find(|o| o.ptr() == k.ptr() && o.visible) {
                        Some(o) => {
                            moves |= o.rect != k.rect;
                            items.push(Item {
                                w: k.w.clone(),
                                from: o.rect,
                                to: k.rect,
                                appear: false,
                            });
                        }
                        None => {
                            moves = true;
                            items.push(Item {
                                w: k.w.clone(),
                                from: k.rect,
                                to: k.rect,
                                appear: true,
                            });
                        }
                    }
                }
                if moves {
                    t.run = Some(Run {
                        tween: Tween::start(anim::ms::LAYOUT, Ease::OutCubic),
                        items,
                        ghosts,
                    });
                }
            }
        }
    }

    // Place the children for this frame.
    let mut t = tr.borrow_mut();
    let mut done = false;
    if let Some(run) = t.run.as_mut() {
        done = !anim::enabled() || run.tween.done();
        let k = if done { 1.0 } else { run.tween.value() };
        for it in run.items.iter_mut().filter(|i| !i.appear) {
            if it.w.was_deleted() {
                continue;
            }
            let r = if done {
                it.to
            } else {
                lerp_rect(it.from, it.to, k)
            };
            if (it.w.x(), it.w.y(), it.w.w(), it.w.h()) != r {
                it.w.resize(r.0, r.1, r.2, r.3);
            }
        }
    }
    if done {
        t.run = None;
    }
    t.shown = kids(f);
    let frame = t.run.as_ref().map(|r| r.tween.value());
    drop(t);

    if full && f.frame() != FrameType::NoBox {
        fltk::draw::draw_box(f.frame(), f.x(), f.y(), f.w(), f.h(), f.color());
    }
    let Some(k) = frame else {
        f.draw_children();
        return;
    };
    anim::request_redraw(f);
    let bg = under(&*f);
    let t = tr.borrow();
    let Some(run) = t.run.as_ref() else { return };
    let opacity = (k * 1.8).min(1.0);
    for i in 0..f.children() {
        let Some(mut c) = f.child(i) else { continue };
        if !c.visible() {
            continue;
        }
        let appear = run
            .items
            .iter()
            .find(|it| it.appear && ptr(&it.w) == c.as_widget_ptr() as usize);
        match appear {
            Some(it) => {
                let span = (along(it.to, column) as f64 * k).round().max(0.0) as i32;
                let clip = span_rect(it.to, span, column, rtl);
                if clip.2 > 0 && clip.3 > 0 {
                    fltk::draw::push_clip(clip.0, clip.1, clip.2, clip.3);
                    f.draw_child(&mut c);
                    fltk::draw::pop_clip();
                    if opacity < 1.0 {
                        paint::alpha_fill(clip.0, clip.1, clip.2, clip.3, bg, 1.0 - opacity);
                    }
                }
            }
            None => {
                f.draw_child(&mut c);
                f.draw_outside_label(&mut c);
            }
        }
    }
    for g in &run.ghosts {
        let span = (along(g.rect, column) as f64 * (1.0 - k)).round() as i32;
        let clip = span_rect(g.rect, span, column, rtl);
        if clip.2 <= 0 || clip.3 <= 0 {
            continue;
        }
        fltk::draw::push_clip(clip.0, clip.1, clip.2, clip.3);
        g.img
            .borrow_mut()
            .draw(g.rect.0, g.rect.1, g.rect.2, g.rect.3);
        fltk::draw::pop_clip();
        paint::alpha_fill(clip.0, clip.1, clip.2, clip.3, bg, k.min(1.0));
    }
}

fn lerp_rect(a: Rect, b: Rect, k: f64) -> Rect {
    use crate::anim::Lerp;
    (
        i32::lerp(a.0, b.0, k),
        i32::lerp(a.1, b.1, k),
        i32::lerp(a.2, b.2, k),
        i32::lerp(a.3, b.3, k),
    )
}

/// The first `span` pixels of `r` along the axis, from the start edge
/// (the right one in a right-to-left row).
fn span_rect(r: Rect, span: i32, column: bool, rtl: bool) -> Rect {
    let span = span.clamp(0, along(r, column));
    if column {
        (r.0, r.1, r.2, span)
    } else if rtl {
        (r.0 + r.2 - span, r.1, span, r.3)
    } else {
        (r.0, r.1, span, r.3)
    }
}

/// True while some toolkit container is moving its children (tests).
pub fn animating() -> bool {
    TRACKS.with(|t| {
        t.borrow()
            .values()
            .any(|(f, t)| !f.was_deleted() && t.try_borrow().map_or(true, |t| t.run.is_some()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_grow_from_the_start_edge() {
        assert_eq!(
            span_rect((10, 20, 100, 30), 40, false, false),
            (10, 20, 40, 30)
        );
        assert_eq!(
            span_rect((10, 20, 100, 30), 40, false, true),
            (70, 20, 40, 30)
        );
        assert_eq!(
            span_rect((10, 20, 100, 30), 12, true, false),
            (10, 20, 100, 12)
        );
        assert_eq!(
            span_rect((10, 20, 100, 30), 500, false, false),
            (10, 20, 100, 30)
        );
    }

    #[test]
    fn rects_interpolate_and_end_exactly() {
        assert_eq!(
            lerp_rect((0, 0, 56, 100), (0, 0, 240, 100), 0.0),
            (0, 0, 56, 100)
        );
        assert_eq!(
            lerp_rect((0, 0, 56, 100), (0, 0, 240, 100), 0.5),
            (0, 0, 148, 100)
        );
        assert_eq!(
            lerp_rect((0, 0, 56, 100), (0, 0, 240, 100), 1.0),
            (0, 0, 240, 100)
        );
    }
}
