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

use crate::theme;
use fltk::{group::Flex, prelude::*};

/// Vertical stack with the panel gap (14 px at base 12).
pub fn column() -> Flex {
    let mut f = Flex::default().column();
    f.set_spacing(theme::with(|t| t.spacing.panel_gap));
    f
}

/// Vertical stack with the small section gap (6 px), for a header + rows.
pub fn section() -> Flex {
    let mut f = Flex::default().column();
    f.set_spacing(theme::with(|t| t.spacing.md));
    f
}

/// Horizontal row of controls (gap 8, control height).
pub fn row() -> Flex {
    let (gap, h) = theme::with(|t| (t.spacing.control_gap, t.spacing.control_height));
    let mut f = Flex::default().row().with_size(100, h);
    f.set_spacing(gap);
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
    }

    fn close_fitted(&mut self) {
        self.end();
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
