//! Window chrome is platform UI, separate from file hit targets and corner style.
use crate::{
    model::Model,
    painter::{Painter, Rect},
};
use files_core::theme::WindowMode;
use fltk::{enums::Cursor, prelude::*, window::Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Move,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
impl Zone {
    fn edge(self) -> i32 {
        match self {
            Self::Move => 0,
            Self::TopLeft => 1,
            Self::TopRight => 2,
            Self::BottomLeft => 3,
            Self::BottomRight => 4,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "Mover ventana",
            Self::TopLeft | Self::BottomRight => "Redimensionar NW/SE",
            _ => "Redimensionar NE/SW",
        }
    }
}
/// 24px interior targets; file/header controls always win. No control is
/// synthesized or dispatched by testing this geometry.
pub fn zone(
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    mode: WindowMode,
    blocked: bool,
    controls: impl Iterator<Item = Rect>,
) -> Option<Zone> {
    if blocked
        || mode != WindowMode::Normal
        || x < 0
        || y < 0
        || x >= w
        || y >= h
        || controls.into_iter().any(|r| r.contains(x, y))
    {
        return None;
    }
    for (r, z) in [
        (Rect::new(0, 0, 24, 24), Zone::TopLeft),
        (Rect::new(w - 24, 0, 24, 24), Zone::TopRight),
        (Rect::new(0, h - 24, 24, 24), Zone::BottomLeft),
        (Rect::new(w - 24, h - 24, 24, 24), Zone::BottomRight),
    ] {
        if r.contains(x, y) {
            return Some(z);
        }
    }
    (y < 58).then_some(Zone::Move)
}
pub fn blocked(m: &Model) -> bool {
    m.closing
        || m.search.visible
        || m.checksums.is_some()
        || m.comparison.is_some()
        || m.bookmarks.panel.is_some()
        || m.commands.is_some()
        || m.size_panel.is_some()
        || m.sftp.form.is_some()
        || m.help.is_some()
        || m.context_menu.is_some()
        || m.preview.visible()
        || m.rename_dialog.is_some()
        || m.properties_dialog.is_some()
        || m.notice_open
        || m.copy_form.is_some()
        || m.mutation_applied
        || m.drag.is_some()
        || m.settings
        || m.actions_menu
}
pub fn at(m: &Model, w: i32, h: i32, x: i32, y: i32) -> Option<Zone> {
    zone(
        w,
        h,
        x,
        y,
        m.window_mode,
        blocked(m),
        m.hits
            .iter()
            .filter(|h| h.generation == m.generation)
            .map(|h| h.rect),
    )
}
pub fn paint_grips(p: &mut dyn Painter, m: &Model, w: i32, h: i32) {
    if m.window_mode != WindowMode::Normal || blocked(m) {
        return;
    }
    for (x, y, sx, sy) in [
        (5, 5, 1, 1),
        (w - 6, 5, -1, 1),
        (5, h - 6, 1, -1),
        (w - 6, h - 6, -1, -1),
    ] {
        for offset in [0, 4, 8] {
            p.rect(Rect::new(x + sx * offset, y, 2, 2), m.palette.muted);
            p.rect(Rect::new(x, y + sy * offset, 2, 2), m.palette.muted);
        }
    }
}
pub fn apply(win: &mut Window, mode: WindowMode) {
    // Decorations are standard compositor-managed decorations, never an overlay
    // or a floating rule. Mode changes use FLTK's native platform adapter.
    if win.fullscreen_active() {
        win.fullscreen(false);
    }
    match mode {
        WindowMode::Normal => {}
        WindowMode::Fullscreen => win.fullscreen(true),
    }
}
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn files_window_gesture(window: *mut std::ffi::c_void, edge: i32) -> i32;
    fn files_window_pointer_focus(window: *mut std::ffi::c_void) -> i32;
    fn files_window_grab_cursor(window: *mut std::ffi::c_void, closed: i32) -> i32;
}
/// Called synchronously from genuine FL_PUSH only. Native bridge independently
/// checks event/button/window/seat focus and serial; no test command calls it.
pub fn begin(win: &mut Window, zone: Zone) -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe { files_window_gesture(win.as_widget_ptr().cast(), zone.edge()) == 1 }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (win, zone);
        false
    }
}
pub fn cursor(win: &mut Window, zone: Option<Zone>, grabbing: bool) {
    #[cfg(target_os = "linux")]
    if unsafe { files_window_pointer_focus(win.as_widget_ptr().cast()) } == 0 {
        return;
    }
    if zone == Some(Zone::Move) {
        #[cfg(target_os = "linux")]
        if unsafe { files_window_grab_cursor(win.as_widget_ptr().cast(), i32::from(grabbing)) } == 1
        {
            return;
        }
    }
    win.set_cursor(match zone {
        Some(Zone::Move) => {
            if grabbing {
                Cursor::Move
            } else {
                Cursor::Hand
            }
        }
        Some(Zone::TopLeft | Zone::BottomRight) => Cursor::NWSE,
        Some(_) => Cursor::NESW,
        None => Cursor::Default,
    });
}
/// Conservative animation gate: inactive/hidden/minimized windows never need
/// marquee redraws. Wayland's cached seat focus also pauses when pointer leaves;
/// the backend has no portable compositor-minimized state query.
pub fn animation_visible(win: &Window) -> bool {
    if win.was_deleted() || !win.shown() || !win.visible() {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        let focus = unsafe { files_window_pointer_focus(win.as_widget_ptr().cast()) };
        if focus >= 0 {
            return focus == 1;
        }
    }
    fltk::app::focus()
        .filter(|f| !f.was_deleted())
        .and_then(|f| f.window())
        .is_some_and(|w| !w.was_deleted() && w.as_widget_ptr() == win.as_widget_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hit(x: i32, y: i32) -> Option<Zone> {
        zone(
            1000,
            650,
            x,
            y,
            WindowMode::Normal,
            false,
            [Rect::new(30, 10, 446, 38), Rect::new(500, 12, 470, 34)].into_iter(),
        )
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_bridge_rejects_null_and_non_event_requests() {
        assert_eq!(unsafe { files_window_gesture(std::ptr::null_mut(), 0) }, 0);
        assert_eq!(unsafe { files_window_gesture(std::ptr::null_mut(), 4) }, 0);
    }
    #[test]
    fn corners_and_free_header_exclude_controls() {
        assert_eq!(hit(5, 5), Some(Zone::TopLeft));
        assert_eq!(hit(995, 5), Some(Zone::TopRight));
        assert_eq!(hit(5, 645), Some(Zone::BottomLeft));
        assert_eq!(hit(995, 645), Some(Zone::BottomRight));
        assert_eq!(hit(480, 25), Some(Zone::Move));
        assert_eq!(hit(100, 53), Some(Zone::Move));
        for point in [
            (35, 20),
            (100, 20),
            (600, 25),
            (900, 25),
            (950, 25),
            (500, 100),
        ] {
            assert_eq!(hit(point.0, point.1), None);
        }
    }
    #[test]
    fn overlays_and_fixed_modes_never_take_gestures() {
        assert_eq!(
            zone(
                700,
                440,
                2,
                2,
                WindowMode::Fullscreen,
                false,
                std::iter::empty()
            ),
            None
        );
        assert_eq!(
            zone(700, 440, 2, 2, WindowMode::Normal, true, std::iter::empty()),
            None
        );
        assert_eq!(hit(-1, 0), None);
        assert_eq!(hit(1000, 650), None);
        // Even a control placed in a grip wins.
        assert_eq!(
            zone(
                700,
                440,
                2,
                2,
                WindowMode::Normal,
                false,
                [Rect::new(0, 0, 24, 24)].into_iter()
            ),
            None
        );
    }
}
