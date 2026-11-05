//! Native window gestures: ask the compositor to move or resize a window
//! interactively (`xdg_toplevel.move` / `.resize` through FLTK's bundled
//! libdecor on Wayland), with the serial of the real button press. Works on
//! any Wayland compositor; no compositor IPC.
//!
//! Call [`begin_move`] or [`begin_resize`] from the handler of the
//! `Event::Push` that starts the gesture. They return false (do nothing)
//! when the gesture is not possible: not Wayland (X11 backend, macOS,
//! Windows), the window is not shown, it is not a top-level window with a
//! libdecor frame, or the current event is not a button press. `AppWindow`
//! uses them for its header drag and corner grips, and falls back to
//! Hyprland IPC, then to moving/resizing the FLTK window itself.
//!
//! The OS-specific part lives in [`crate::platform`].

use fltk::{app, enums::Event, prelude::*};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

static MOVES: AtomicU32 = AtomicU32::new(0);
static RESIZES: AtomicU32 = AtomicU32::new(0);

/// Native gestures started so far: (moves, resizes). Diagnostics.
pub fn gesture_counts() -> (u32, u32) {
    (MOVES.load(Ordering::Relaxed), RESIZES.load(Ordering::Relaxed))
}

/// Window edge or corner for [`begin_resize`] (libdecor's order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum Edge {
    Top = 1,
    Bottom = 2,
    Left = 3,
    TopLeft = 4,
    BottomLeft = 5,
    Right = 6,
    TopRight = 7,
    BottomRight = 8,
}

/// Starts (or with `dry_run` only checks) a gesture on `w`'s top window.
fn gesture<W: WidgetExt>(w: &W, edge: Option<Edge>, dry_run: bool) -> bool {
    if std::env::var("OMARCHY_FLTK_NATIVE_GESTURES").is_ok_and(|v| v == "0") {
        return false; // diagnostics: force the fallback path
    }
    if app::event() != Event::Push {
        return false; // the serial must be the one of this button press
    }
    let Some(win) = w.top_window() else { return false };
    if !win.shown() {
        return false;
    }
    crate::platform::native_gesture(win.as_widget_ptr() as *const c_void, edge.map(|e| e as i32), dry_run)
}

/// True when [`begin_move`]/[`begin_resize`] would work for `w` now.
pub fn available<W: WidgetExt>(w: &W) -> bool {
    gesture(w, None, true)
}

/// Starts an interactive move of `w`'s window (from an `Event::Push` handler).
pub fn begin_move<W: WidgetExt>(w: &W) -> bool {
    let ok = gesture(w, None, false);
    if ok {
        MOVES.fetch_add(1, Ordering::Relaxed);
    }
    ok
}

/// Starts an interactive resize from `edge` (from an `Event::Push` handler).
pub fn begin_resize<W: WidgetExt>(w: &W, edge: Edge) -> bool {
    let ok = gesture(w, Some(edge), false);
    if ok {
        RESIZES.fetch_add(1, Ordering::Relaxed);
    }
    ok
}

/// Device pixels per FLTK unit of `w`'s window: FLTK's own scaling times the
/// Wayland output scale (2 on a HiDPI/Retina output). fltk-rs'
/// `Window::pixels_per_unit` only returns FLTK's scaling on Linux, so images
/// meant to keep every device pixel (video, photos) would be sized for half
/// the real resolution on a 2× output. X11 and unshown windows: FLTK's scaling.
/// macOS and Windows: FLTK's own `pixels_per_unit` (it includes Retina).
pub fn pixels_per_unit<W: WidgetExt>(w: &W) -> f32 {
    let Some(win) = w.top_window() else { return 1.0 };
    let ppu = match crate::platform::output_scale(win.as_widget_ptr() as *const c_void, win.shown()) {
        Some(output) => app::screen_scale(win.screen_num()) * output as f32,
        None => {
            // SAFETY: `win` is a live top-level window; the wrapper only reads it.
            let w = unsafe { fltk::window::Window::from_widget_ptr(win.as_widget_ptr()) };
            w.pixels_per_unit()
        }
    };
    if ppu.is_finite() && ppu > 0.0 { ppu } else { 1.0 }
}
