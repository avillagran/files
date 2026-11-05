// Pointer input for a PRIVATE nested compositor used by UI checks.
//
//   vpointer wayland-N W H x0 y0 x1 y1 [hoverMS | cN]
//
// Absolute press at (x0,y0), 20 motion steps to (x1,y1), release (a drag;
// a click when both points match). `cN` clicks N times at (x0,y0) instead;
// a number hovers that many milliseconds before pressing. It connects only
// to the explicit socket and refuses wayland-0/wayland-1, so it can never
// reach the user's own session.
#include "wlr-virtual-pointer-unstable-v1-client.h"
#include <linux/input-event-codes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>

static struct zwlr_virtual_pointer_manager_v1 *manager;

static void global(void *data, struct wl_registry *registry, uint32_t id, const char *name, uint32_t version) {
  (void)data, (void)version;
  if (!strcmp(name, zwlr_virtual_pointer_manager_v1_interface.name))
    manager = wl_registry_bind(registry, id, &zwlr_virtual_pointer_manager_v1_interface, 1);
}
static void global_remove(void *data, struct wl_registry *registry, uint32_t id) { (void)data, (void)registry, (void)id; }
static const struct wl_registry_listener listener = {global, global_remove};

static uint32_t now(void) {
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return t.tv_sec * 1000 + t.tv_nsec / 1000000;
}
static void nap(int ms) {
  struct timespec t = {ms / 1000, (ms % 1000) * 1000000L};
  nanosleep(&t, 0);
}

int main(int argc, char **argv) {
  if ((argc != 8 && argc != 9) || strncmp(argv[1], "wayland-", 8) || !strcmp(argv[1], "wayland-0") ||
      !strcmp(argv[1], "wayland-1")) {
    fprintf(stderr, "usage: vpointer wayland-N W H x0 y0 x1 y1 [hoverMS|cN] (private nested socket only)\n");
    return 2;
  }
  struct wl_display *display = wl_display_connect(argv[1]);
  if (!display) return 1;
  struct wl_registry *registry = wl_display_get_registry(display);
  wl_registry_add_listener(registry, &listener, 0);
  wl_display_roundtrip(display);
  if (!manager) return 3;
  int w = atoi(argv[2]), h = atoi(argv[3]), x0 = atoi(argv[4]), y0 = atoi(argv[5]), x1 = atoi(argv[6]), y1 = atoi(argv[7]);
  struct zwlr_virtual_pointer_v1 *p = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, 0);
#define FLUSH(ms) (zwlr_virtual_pointer_v1_frame(p), wl_display_flush(display), nap(ms))
  zwlr_virtual_pointer_v1_motion_absolute(p, now(), x0, y0, w, h);
  FLUSH(120);
  if (argc == 9 && argv[8][0] == 'c') {
    for (int k = 0; k < atoi(argv[8] + 1); k++) {
      zwlr_virtual_pointer_v1_button(p, now(), BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
      FLUSH(40);
      zwlr_virtual_pointer_v1_button(p, now(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
      FLUSH(60);
    }
  } else {
    for (int i = 0; argc == 9 && i < atoi(argv[8]) / 100; i++) {
      zwlr_virtual_pointer_v1_motion_absolute(p, now(), x0 + (i & 1), y0, w, h);
      FLUSH(100);
    }
    zwlr_virtual_pointer_v1_button(p, now(), BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
    FLUSH(80);
    for (int i = 1; i <= 20; i++) {
      zwlr_virtual_pointer_v1_motion_absolute(p, now(), x0 + (x1 - x0) * i / 20, y0 + (y1 - y0) * i / 20, w, h);
      FLUSH(25);
    }
    zwlr_virtual_pointer_v1_button(p, now(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
    FLUSH(50);
  }
  wl_display_roundtrip(display);
  zwlr_virtual_pointer_v1_destroy(p);
  wl_display_roundtrip(display);
  return 0;
}
