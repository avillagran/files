//! Native window gestures on Wayland: ask the compositor to move or resize
//! a window interactively (`xdg_toplevel.move` / `.resize` through FLTK's
//! bundled libdecor), with the serial of the real button press. Works on
//! any Wayland compositor; no compositor IPC.
//!
//! Call [`begin_move`] or [`begin_resize`] from the handler of the
//! `Event::Push` that starts the gesture. They return false (do nothing)
//! when the gesture is not possible: not Wayland (X11 backend), the window
//! is not shown, it is not a top-level window with a libdecor frame, or
//! the current event is not a button press. `AppWindow` uses them for its
//! header drag and corner grips, and falls back to Hyprland IPC.
//!
//! This reads FLTK internals of the pinned fltk-sys 1.5.23 (FLTK 1.4):
//! the `wld_window` record and the Wayland screen driver's seat. The
//! layout is checked against that version only.

use fltk::{app, enums::Event, prelude::*};
use std::ffi::{c_int, c_void};
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

#[repr(C)]
struct WlList {
    prev: *mut c_void,
    next: *mut c_void,
}

/// Prefix of FLTK 1.4's `struct wld_window` (Fl_Wayland_Window_Driver.H).
#[repr(C)]
struct WldWindow {
    fl_win: *mut c_void,
    outputs: WlList,
    wl_surface: *mut c_void,
    frame_cb: *mut c_void,
    buffer: *mut c_void,
    xdg_surface: *mut c_void,
    kind: c_int,
    /// Union: `libdecor_frame*` when `kind == DECORATED`.
    frame: *mut c_void,
}

const DECORATED: c_int = 0;

extern "C" {
    #[link_name = "_Z13fl_wl_displayv"]
    fn fl_wl_display() -> *mut c_void;
    #[link_name = "_Z9fl_wl_xidPK9Fl_Window"]
    fn fl_wl_xid(win: *const c_void) -> *mut WldWindow;
    #[link_name = "_ZN2Fl13screen_driverEv"]
    fn fl_screen_driver() -> *mut c_void;
    #[link_name = "_ZN24Fl_Wayland_Screen_Driver10get_serialEv"]
    fn wayland_get_serial(this: *mut c_void) -> u32;
    #[link_name = "_ZN24Fl_Wayland_Screen_Driver11get_wl_seatEv"]
    fn wayland_get_wl_seat(this: *mut c_void) -> *mut c_void;
    fn fl_libdecor_frame_move(frame: *mut c_void, seat: *mut c_void, serial: u32);
    fn fl_libdecor_frame_resize(frame: *mut c_void, seat: *mut c_void, serial: u32, edge: c_int);
}

/// The libdecor frame, seat and press serial, when a gesture is possible now.
fn target<W: WidgetExt>(w: &W) -> Option<(*mut c_void, *mut c_void, u32)> {
    if std::env::var("OMARCHY_FLTK_NATIVE_GESTURES").is_ok_and(|v| v == "0") {
        return None; // diagnostics: force the fallback path
    }
    if app::event() != Event::Push {
        return None; // the serial must be the one of this button press
    }
    let win = w.top_window()?;
    if !win.shown() {
        return None;
    }
    // SAFETY: plain FLTK calls on the UI thread. `fl_wl_display` is null
    // unless the Wayland backend is active, so the screen driver is the
    // Wayland one below; during a button press its seat exists.
    unsafe {
        if fl_wl_display().is_null() {
            return None;
        }
        let xid = fl_wl_xid(win.as_widget_ptr() as *const c_void);
        if xid.is_null() || (*xid).kind != DECORATED || (*xid).frame.is_null() {
            return None;
        }
        let drv = fl_screen_driver();
        let seat = wayland_get_wl_seat(drv);
        let serial = wayland_get_serial(drv);
        (!seat.is_null() && serial != 0).then_some(((*xid).frame, seat, serial))
    }
}

/// True when [`begin_move`]/[`begin_resize`] would work for `w` now.
pub fn available<W: WidgetExt>(w: &W) -> bool {
    target(w).is_some()
}

/// Starts an interactive move of `w`'s window (from an `Event::Push` handler).
pub fn begin_move<W: WidgetExt>(w: &W) -> bool {
    let Some((frame, seat, serial)) = target(w) else { return false };
    // SAFETY: valid frame/seat of a shown window, real press serial.
    unsafe { fl_libdecor_frame_move(frame, seat, serial) };
    MOVES.fetch_add(1, Ordering::Relaxed);
    true
}

/// Starts an interactive resize from `edge` (from an `Event::Push` handler).
pub fn begin_resize<W: WidgetExt>(w: &W, edge: Edge) -> bool {
    let Some((frame, seat, serial)) = target(w) else { return false };
    // SAFETY: as above.
    unsafe { fl_libdecor_frame_resize(frame, seat, serial, edge as c_int) };
    RESIZES.fetch_add(1, Ordering::Relaxed);
    true
}
