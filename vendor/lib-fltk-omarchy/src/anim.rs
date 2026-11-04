//! Omarchy's short transitions for control states (Button 120 ms color,
//! PanelActionButton and rows 60 ms, Toggle 100 ms, ToggleSwitch 120 ms
//! color and knob, PanelSlider 140 ms OutCubic), drawn by the components.
//!
//! One shared fltk timer redraws only the widgets that are mid-transition
//! (60 fps) and stops when none is. Theme changes are not animated.
//! `anim::set_enabled(false)` (or `OMARCHY_FLTK_ANIMATIONS=0`) turns
//! transitions off; components then jump to the final state.

use crate::theme::Rgb;
use fltk::{app, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::time::Instant;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static PENDING: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
    static TIMER: Cell<bool> = const { Cell::new(false) };
}

/// Are transitions on? (default: yes, unless `OMARCHY_FLTK_ANIMATIONS=0`)
pub fn enabled() -> bool {
    ENABLED.with(|e| {
        e.get().unwrap_or_else(|| {
            let on = std::env::var("OMARCHY_FLTK_ANIMATIONS").map_or(true, |v| v != "0" && v.to_ascii_lowercase() != "off");
            e.set(Some(on));
            on
        })
    })
}

pub fn set_enabled(on: bool) {
    ENABLED.with(|e| e.set(Some(on)));
}

/// Easing curves used by Omarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    /// Qt's ColorAnimation default.
    Linear,
    /// Easing.OutCubic (positions, sizes).
    OutCubic,
}

impl Ease {
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
        }
    }
}

/// Values that can be interpolated.
pub trait Lerp: Copy + PartialEq {
    fn lerp(a: Self, b: Self, t: f64) -> Self;
}

impl Lerp for f64 {
    fn lerp(a: f64, b: f64, t: f64) -> f64 {
        a + (b - a) * t
    }
}

impl Lerp for Rgb {
    fn lerp(a: Rgb, b: Rgb, t: f64) -> Rgb {
        b.over(a, t)
    }
}

/// What a control background looks like: fill, border color and width.
/// "No fill" is the color under the control; "no border" is width 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Rgb,
    pub border: Rgb,
    pub border_width: i32,
}

impl Lerp for Look {
    fn lerp(a: Look, b: Look, t: f64) -> Look {
        // A border that appears or disappears fades from/to the fill color.
        let ab = if a.border_width > 0 { a.border } else { a.fill };
        let bb = if b.border_width > 0 { b.border } else { b.fill };
        Look {
            fill: Rgb::lerp(a.fill, b.fill, t),
            border: Rgb::lerp(ab, bb, t),
            border_width: if t >= 1.0 { b.border_width } else { a.border_width.max(b.border_width) },
        }
    }
}

/// One animated value of a widget. Keep it next to the widget's draw
/// state; call [`Anim::get`] with the target every draw.
#[derive(Debug, Clone)]
pub struct Anim<T: Lerp> {
    from: Option<T>,
    to: Option<T>,
    start: Option<Instant>,
    theme: u64,
}

impl<T: Lerp> Default for Anim<T> {
    fn default() -> Self {
        Anim { from: None, to: None, start: None, theme: 0 }
    }
}

impl<T: Lerp> Anim<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current value moving toward `target` over `ms` milliseconds. While a
    /// transition runs, `widget` is redrawn on the next frame.
    pub fn get<W: WidgetExt>(&mut self, target: T, ms: u32, ease: Ease, widget: &W) -> T {
        let gen = crate::theme::generation();
        let (Some(to), true, true) = (self.to, enabled(), self.theme == gen) else {
            // First draw, transitions off or a new theme: jump.
            self.from = Some(target);
            self.to = Some(target);
            self.start = None;
            self.theme = gen;
            return target;
        };
        let now = Instant::now();
        let current = self.current(now, ms, ease).unwrap_or(to);
        if target != to {
            self.from = Some(current);
            self.to = Some(target);
            self.start = Some(now);
        }
        let v = self.current(now, ms, ease).unwrap_or(target);
        if self.start.is_some() {
            request_redraw(widget);
        }
        v
    }

    fn current(&mut self, now: Instant, ms: u32, ease: Ease) -> Option<T> {
        let (from, to, start) = (self.from?, self.to?, self.start?);
        let t = now.duration_since(start).as_secs_f64() * 1000.0 / ms.max(1) as f64;
        if t >= 1.0 {
            self.start = None;
            self.from = Some(to);
            return Some(to);
        }
        Some(T::lerp(from, to, ease.apply(t)))
    }

    /// Sets the value with no transition (e.g. while dragging).
    pub fn jump(&mut self, v: T) -> T {
        self.from = Some(v);
        self.to = Some(v);
        self.start = None;
        self.theme = crate::theme::generation();
        v
    }

    /// True while a transition runs.
    pub fn running(&self) -> bool {
        self.start.is_some()
    }
}

/// Redraws `w` on the next animation frame (one shared timer).
pub fn request_redraw<W: WidgetExt>(w: &W) {
    // SAFETY: `w` is a live widget; the copy only tracks its deletion.
    let w = unsafe { <Widget as WidgetBase>::from_widget_ptr(w.as_widget_ptr()) };
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        // Widgets deleted mid-transition (a closed window) leave the list first.
        p.retain(|x| !x.was_deleted());
        if !p.iter().any(|x| x.as_widget_ptr() == w.as_widget_ptr()) {
            p.push(w);
        }
    });
    if TIMER.with(|t| t.replace(true)) {
        return;
    }
    app::add_timeout3(1.0 / 60.0, |_| {
        TIMER.with(|t| t.set(false));
        let list = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
        for mut w in list {
            if !w.was_deleted() {
                w.redraw();
            }
        }
    });
}

/// Widgets waiting for an animation frame (tests).
pub fn pending() -> usize {
    PENDING.with(|p| p.borrow().iter().filter(|w| !w.was_deleted()).count())
}
