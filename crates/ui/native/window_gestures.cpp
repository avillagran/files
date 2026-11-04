// MIT. Narrow adapter for the pinned FLTK 1.5.23 Wayland backend.
// Compiled against its actual headers; never guesses native struct layouts.
#include <FL/Fl.H>
#include <FL/Fl_Window.H>
#include <FL/platform.H>
#include "src/drivers/Wayland/Fl_Wayland_Window_Driver.H"
#include "libdecor/build/fl_libdecor.h"
#include <wayland-cursor.h>
static_assert(FL_API_VERSION == 10405 && FL_ABI_VERSION == 10400, "Review the adapter when updating FLTK");
#ifndef FLTK_USE_WAYLAND
#error "This adapter requires the pinned Wayland backend"
#endif

extern "C" int files_window_gesture(Fl_Window *window, int edge) {
  // Never manufacture a seat serial. This entry point is only valid in the
  // synchronous left-button FL_PUSH handler of this very window.
  if (!fl_wl_display() || !window || !window->shown() ||
      Fl::event() != FL_PUSH || Fl::event_button() != FL_LEFT_MOUSE ||
      (!Fl::belowmouse() || Fl::belowmouse()->window() != window) || Fl::event_buttons() != FL_BUTTON1)
    return 0;
  auto *native = fl_wl_xid(window);
  auto *screen = static_cast<Fl_Wayland_Screen_Driver *>(Fl::screen_driver());
  if (!native || native->kind != Fl_Wayland_Window_Driver::DECORATED ||
      !native->frame || !screen->seat || !screen->get_serial() ||
      !screen->get_wl_seat() ||
      screen->seat->pointer_focus != fl_wl_surface(native))
    return 0;
  if (edge == 0) {
    libdecor_frame_move(native->frame, screen->get_wl_seat(), screen->get_serial());
  } else {
    libdecor_resize_edge e;
    switch (edge) {
    case 1: e = LIBDECOR_RESIZE_EDGE_TOP_LEFT; break;
    case 2: e = LIBDECOR_RESIZE_EDGE_TOP_RIGHT; break;
    case 3: e = LIBDECOR_RESIZE_EDGE_BOTTOM_LEFT; break;
    case 4: e = LIBDECOR_RESIZE_EDGE_BOTTOM_RIGHT; break;
    default: return 0;
    }
    libdecor_frame_resize(native->frame, screen->get_wl_seat(), screen->get_serial(), e);
  }
  Fl::pushed(nullptr);
  Fl::e_state = 0;
  return 1; // Request sent; only the compositor decides whether to move/resize.
}

extern "C" int files_window_pointer_focus(Fl_Window *window) {
  if (!fl_wl_display()) return -1; // Other platform: let FLTK handle its cursor.
  if (!window || !window->shown()) return 0;
  auto *native = fl_wl_xid(window);
  auto *screen = static_cast<Fl_Wayland_Screen_Driver *>(Fl::screen_driver());
  return native && screen->seat && screen->seat->wl_pointer &&
      screen->seat->pointer_enter_serial && screen->seat->pointer_focus == fl_wl_surface(native);
}

extern "C" int files_window_grab_cursor(Fl_Window *window, int closed) {
  if (!fl_wl_display() || files_window_pointer_focus(window) != 1) return 0;
  auto *screen = static_cast<Fl_Wayland_Screen_Driver *>(Fl::screen_driver());
  if (!screen->seat || !screen->seat->cursor_theme) return 0;
  auto *cursor = wl_cursor_theme_get_cursor(screen->seat->cursor_theme,
                                            closed ? "grabbing" : "grab");
  if (!cursor) return 0;
  Fl_Wayland_Screen_Driver::do_set_cursor(screen->seat, cursor);
  return 1;
}

// Pinned Wayland clipboard_contains compares MIME pointers, not string bytes.
// fltk-rs 1.5.23 passes a newly allocated CString and therefore reports false
// even for FLTK's own text selection. Pass FLTK's canonical public constant.
// This only queries compatibility: it never publishes, requests focus or reads.
extern "C" int files_clipboard_has_text() {
  return Fl::clipboard_contains(Fl::clipboard_plain_text);
}
