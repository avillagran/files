// MIT. Pinned FLTK 1.5.23 Wayland clipboard adapter. No foreign listener
// replaced.
#include "files_fltk_dnd_build.h"
#include "src/drivers/Wayland/Fl_Wayland_Window_Driver.H"
#include <FL/Fl.H>
#include <FL/Fl_Window.H>
#include <FL/platform.H>
#include <array>
#include <cerrno>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <fcntl.h>
#include <memory>
#include <new>
#include <unistd.h>
#include <wayland-client.h>
static_assert(FL_API_VERSION == 10405 && FL_ABI_VERSION == 10400,
              "Review pinned transfer adapter");
extern "C" int fl_files_set_dnd_delegate_v1(int (*)(Fl_Window *))
    __attribute__((weak));
extern "C" int FILES_FLTK_DND_INPUT_SYMBOL();
namespace {
constexpr size_t WIRE = 1024 * 1024;
constexpr const char *URI = "text/uri-list",
                     *GNOME = "x-special/gnome-copied-files",
                     *KDE = "application/x-kde-cutselection";
using Clock = std::chrono::steady_clock;
struct Offer {
  wl_data_offer *proxy = nullptr;
  uint64_t id = 0;
  unsigned flags = 0, mimes = 0, actions = 0, action = 0;
  bool invalid = false;
};
struct Source {
  wl_data_source *proxy = nullptr;
  std::unique_ptr<unsigned char[]> bytes;
  size_t size = 0, offset = 0;
  bool cut = false, cancelled = false, drag = false;
  uint64_t id = 0;
  Clock::time_point deadline;
};
struct Send {
  int fd = -1;
  Source *source = nullptr;
  const unsigned char *bytes = nullptr;
  size_t size = 0, offset = 0;
  Clock::time_point deadline, progress;
};
struct Permit {
  uint64_t id = 0, epoch = 0;
  wl_seat *seat = nullptr;
  wl_surface *surface = nullptr;
  std::unique_ptr<Fl_Widget_Tracker> tracker;
  uint32_t serial = 0;
  bool mouse = false;
  Clock::time_point deadline;
};
struct DragEvent {
  uint64_t sequence = 0, offer = 0, pane = 0, tab = 0, generation = 0;
  int kind = 0, x = 0, y = 0;
};
struct State {
  wl_registry *registry = nullptr;
  wl_callback *sync = nullptr;
  wl_data_device_manager *manager = nullptr;
  wl_data_device *device = nullptr;
  wl_seat *seat = nullptr;
  uint32_t manager_name = 0;
  std::array<uint32_t, 16> seat_names{};
  uint64_t next = 0, epoch = 1, selection = 0, drag = 0, reading = 0,
           publication = 0;
  uint64_t reading_epoch = 0, drop_offer = 0, drop_epoch = 0;
  uint64_t drag_source = 0;
  bool reading_drag = false, drop_pending = false;
  uint32_t drag_serial = 0;
  DragEvent context, hover, dropped;
  Clock::time_point drop_deadline;
  int status = 0;
  bool ready = false, closing = false, seat_lost = false;
  Permit permit;
  Permit drag_permit;
  std::unique_ptr<Fl_Widget_Tracker> owned_window;
  std::array<Offer, 4> offers;
  std::array<std::unique_ptr<Source>, 2> sources;
  std::array<Send, 4> sends;
} state;
int delegates_window(Fl_Window *window) {
  return !state.closing && state.owned_window &&
         !state.owned_window->deleted() &&
         state.owned_window->widget() == window;
}
void unregister_dnd() {
  if (fl_files_set_dnd_delegate_v1)
    fl_files_set_dnd_delegate_v1(nullptr);
}
uint64_t fresh() {
  if (state.next == UINT64_MAX) {
    state.status = -9;
    return 0;
  }
  return ++state.next;
}
void close_send(Send &s) {
  if (s.fd >= 0)
    close(s.fd);
  s = Send{};
}
void clear_offer(Offer &o) {
  if (o.proxy)
    wl_data_offer_destroy(o.proxy);
  o = Offer{};
}
Offer *offer(uint64_t id) {
  for (auto &o : state.offers)
    if (o.proxy && o.id == id)
      return &o;
  return nullptr;
}
void collect_offers() {
  for (auto &o : state.offers)
    if (o.id != state.selection && o.id != state.reading &&
        o.id != state.drag && o.id != state.drop_offer)
      clear_offer(o);
}
void disable(int reason) {
  unregister_dnd(); // Releases only the FLTK-owned delegated offer, if any.
  state.owned_window.reset();
  state.drop_offer = 0;
  state.drop_pending = false;
  state.hover = {};
  state.permit = {};
  state.drag_permit = {};
  state.selection = state.drag = state.reading = state.publication = 0;
  state.drag_source = 0;
  if (state.device) {
    if (wl_data_device_get_version(state.device) >= 2)
      wl_data_device_release(state.device);
    else
      wl_data_device_destroy(state.device);
    state.device = nullptr;
  }
  for (auto &o : state.offers)
    clear_offer(o);
  for (auto &s : state.sends)
    close_send(s);
  for (auto &s : state.sources) {
    if (s && s->proxy)
      wl_data_source_destroy(s->proxy);
    s.reset();
  }
  state.seat = nullptr;
  state.status = reason;
  ++state.epoch;
}
void mime(void *data, wl_data_offer *, const char *name) {
  auto &o = *static_cast<Offer *>(data);
  if (++o.mimes > 32 || !name || strnlen(name, 257) > 256) {
    o.invalid = true;
    return;
  }
  if (!strcmp(name, URI))
    o.flags |= 1;
  else if (!strcmp(name, GNOME))
    o.flags |= 2;
  else if (!strcmp(name, KDE))
    o.flags |= 4;
}
void negotiate_drag();
void source_actions(void *data, wl_data_offer *, uint32_t actions) {
  auto &o = *static_cast<Offer *>(data);
  o.actions = actions;
  if (o.id == state.drag)
    negotiate_drag();
}
void offer_action(void *data, wl_data_offer *, uint32_t action) {
  static_cast<Offer *>(data)->action = action;
}
const wl_data_offer_listener offer_listener = {mime, source_actions,
                                               offer_action};
void data_offer(void *, wl_data_device *, wl_data_offer *p) {
  for (auto &o : state.offers)
    if (!o.proxy) {
      o.proxy = p;
      o.id = fresh();
      if (!o.id) {
        wl_data_offer_destroy(p);
        o = {};
        return;
      }
      wl_data_offer_add_listener(p, &offer_listener, &o);
      return;
    }
  // Abort only OUR device on overflow; never orphan an untracked
  // proxy/listener.
  wl_data_offer_destroy(p);
  disable(-4);
}
bool copy_offer(Offer *o) {
  return o && !o->invalid && (o->flags & 1) &&
         (wl_data_offer_get_version(o->proxy) < 3 ||
          (o->actions & WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY));
}
void negotiate_drag() {
  auto *o = offer(state.drag);
  if (!o)
    return;
  bool copy = !state.reading && !state.drop_pending && !state.closing &&
              !state.seat_lost && copy_offer(o);
  wl_data_offer_accept(o->proxy, state.drag_serial, copy ? URI : nullptr);
  if (wl_data_offer_get_version(o->proxy) >= 3)
    wl_data_offer_set_actions(
        o->proxy, copy ? WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY : 0,
        copy ? WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY : 0);
}
void hover_at(int kind, wl_fixed_t x, wl_fixed_t y) {
  state.hover = state.context;
  state.hover.sequence = fresh();
  state.hover.offer = state.drag;
  state.hover.kind = kind;
  auto *window = state.owned_window && !state.owned_window->deleted()
                     ? static_cast<Fl_Window *>(state.owned_window->widget())
                     : nullptr;
  float scale = window ? Fl::screen_scale(window->screen_num()) : 1.f;
  state.hover.x = wl_fixed_to_int(x) / scale;
  state.hover.y = wl_fixed_to_int(y) / scale;
}
void enter(void *, wl_data_device *, uint32_t serial, wl_surface *surface,
           wl_fixed_t x, wl_fixed_t y, wl_data_offer *p) {
  state.drag = 0;
  bool own = state.owned_window && !state.owned_window->deleted() &&
             fl_files_set_dnd_delegate_v1 && !state.closing && !state.seat_lost;
  auto *window =
      own ? static_cast<Fl_Window *>(state.owned_window->widget()) : nullptr;
  auto *native = window && window->shown() ? fl_wl_xid(window) : nullptr;
  own = native && fl_wl_surface(native) == surface;
  for (auto &o : state.offers)
    if (own && o.proxy == p)
      state.drag = o.id;
  state.drag_serial = serial;
  if (state.drag && !state.drop_pending && !state.reading) {
    negotiate_drag();
    hover_at(1, x, y);
  } else if (p) {
    wl_data_offer_accept(p, serial, nullptr);
    if (wl_data_offer_get_version(p) >= 3)
      wl_data_offer_set_actions(p, 0, 0);
    state.drag = 0; // Late source_actions cannot revive a busy/rejected enter.
  }
}
void leave(void *, wl_data_device *) {
  state.drag = 0;
  state.hover.kind = 3;
  state.hover.sequence = fresh();
  collect_offers(); // A dropped/reading offer survives until explicit ACK.
}
void motion(void *, wl_data_device *, uint32_t, wl_fixed_t x, wl_fixed_t y) {
  if (!state.drag || state.drop_pending || state.reading)
    return;
  negotiate_drag();
  hover_at(1, x, y);
}
void drop(void *, wl_data_device *) {
  auto *o = offer(state.drag);
  if (!copy_offer(o) || state.reading || state.drop_pending) {
    state.drag = 0;
    state.hover.kind = 3;
    state.hover.sequence = fresh();
    collect_offers();
    return;
  }
  state.dropped = state.hover;
  state.dropped.sequence = fresh();
  state.dropped.kind = 2;
  state.drop_offer = o->id;
  state.drop_epoch = state.epoch;
  state.drop_pending = true;
  state.drop_deadline = Clock::now() + std::chrono::seconds(3);
  // No further negotiation is legal after drop. The drop_offer pin retains
  // our proxy independently of any later leave callback.
  state.drag = 0;
}
void selection(void *, wl_data_device *, wl_data_offer *p) {
  state.selection = 0;
  for (auto &o : state.offers)
    if (o.proxy == p)
      state.selection = o.id;
  collect_offers(); // A reading old offer survives only until its explicit ACK.
}
const wl_data_device_listener device_listener = {data_offer, enter, leave,
                                                 motion,     drop,  selection};
void target(void *, wl_data_source *, const char *) {}
void send(void *data, wl_data_source *, const char *name, int fd) {
  auto *source = static_cast<Source *>(data);
  if (fd < 0)
    return;
  if (!name || source->cancelled) {
    close(fd);
    return;
  }
  const unsigned char *bytes = nullptr;
  size_t size = 0;
  if (!strcmp(name, URI)) {
    bytes = source->bytes.get() + source->offset;
    size = source->size - source->offset;
  } else if (!strcmp(name, GNOME)) {
    bytes = source->bytes.get();
    size = source->size;
  } else if (!strcmp(name, KDE)) {
    static const unsigned char cut = '1', copy = '0';
    bytes = source->cut ? &cut : &copy;
    size = 1;
  }
  if (!bytes) {
    close(fd);
    return;
  }
  int flags = fcntl(fd, F_GETFL);
  int descriptor_flags = fcntl(fd, F_GETFD);
  if (flags < 0 || descriptor_flags < 0 ||
      fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0 ||
      fcntl(fd, F_SETFD, descriptor_flags | FD_CLOEXEC) < 0) {
    close(fd);
    if (source->id == state.publication)
      state.status = -5;
    return;
  }
  for (auto &s : state.sends)
    if (s.fd < 0) {
      s = {fd,          source, bytes,
           size,        0,      Clock::now() + std::chrono::seconds(3),
           Clock::now()};
      return;
    }
  close(fd);
  if (source->id == state.publication)
    state.status = -4;
}
void cancelled(void *data, wl_data_source *) {
  auto *source = static_cast<Source *>(data);
  source->cancelled = true;
  for (auto &s : state.sends)
    if (s.source == source)
      close_send(s);
  if (source->id == state.publication)
    state.status =
        3; // Ownership lost; never delete/cancel/consume filesystem roots.
}
void dropped(void *, wl_data_source *) {}
void finished(void *data, wl_data_source *) {
  auto *source = static_cast<Source *>(data);
  if (source->drag) {
    for (auto &send : state.sends)
      if (send.source == source)
        close_send(send);
    source->cancelled = true; // Deferred cleanup only, never delete files.
  }
}
void action(void *, wl_data_source *, uint32_t) {}
const wl_data_source_listener source_listener = {target,  send,     cancelled,
                                                 dropped, finished, action};
void global(void *, wl_registry *registry, uint32_t name, const char *interface,
            uint32_t version) {
  if (!strcmp(interface, wl_seat_interface.name)) {
    for (auto &seat_name : state.seat_names)
      if (!seat_name) {
        seat_name = name;
        return;
      }
    state.seat_lost = true;
    disable(-4);
    return;
  }
  if (!strcmp(interface, wl_data_device_manager_interface.name) &&
      !state.manager && !state.seat_lost) {
    state.manager_name = name;
    state.manager = static_cast<wl_data_device_manager *>(
        wl_registry_bind(registry, name, &wl_data_device_manager_interface,
                         version < 3 ? version : 3));
  }
}
void removed(void *, wl_registry *, uint32_t name) {
  bool seat = false;
  for (auto &seat_name : state.seat_names)
    if (seat_name == name) {
      seat_name = 0;
      seat = true;
    }
  if (!seat && name != state.manager_name)
    return; // Monitor/other global changes are unrelated.
  state.seat_lost = true;
  disable(-6); // Conservative for ANY known seat; no pointer reuse.
  if (name == state.manager_name && state.manager) {
    wl_data_device_manager_destroy(state.manager);
    state.manager = nullptr;
  }
}
const wl_registry_listener registry_listener = {global, removed};
void synced(void *, wl_callback *callback, uint32_t) {
  wl_callback_destroy(callback);
  state.sync = nullptr;
  state.ready = true;
}
const wl_callback_listener sync_listener = {synced};
Fl_Wayland_Screen_Driver *screen() {
  return static_cast<Fl_Wayland_Screen_Driver *>(Fl::screen_driver());
}
bool device() {
  if (!state.ready || !state.manager || state.seat_lost || state.closing)
    return false;
  auto *driver = screen();
  if (!driver->seat || !driver->seat->wl_seat)
    return false;
  if (state.device && state.seat != driver->seat->wl_seat) {
    state.seat_lost = true;
    disable(-6);
    return false;
  }
  if (!state.device) {
    state.seat = driver->seat->wl_seat;
    state.device =
        wl_data_device_manager_get_data_device(state.manager, state.seat);
    if (!state.device) {
      state.status = -7;
      return false;
    }
    wl_data_device_add_listener(state.device, &device_listener, nullptr);
    state.status = 1;
  }
  return true;
}
bool current_permit(const Permit &p, uint64_t id) {
  if (!id || p.id != id || p.epoch != state.epoch ||
      Clock::now() > p.deadline || !device() || p.seat != state.seat ||
      !p.tracker || p.tracker->deleted())
    return false;
  auto *window = static_cast<Fl_Window *>(p.tracker->widget());
  if (!window->shown())
    return false;
  auto *native = fl_wl_xid(window);
  auto *driver = screen();
  return native && fl_wl_surface(native) == p.surface && driver->seat &&
         (p.mouse ? driver->seat->pointer_focus
                  : driver->seat->keyboard_surface) == p.surface;
}
bool current(uint64_t id) { return current_permit(state.permit, id); }
} // namespace
extern "C" void files_transfer_init() {
  if (!fl_wl_display() || state.registry || state.closing)
    return;
  state.registry = wl_display_get_registry(fl_wl_display());
  if (!state.registry) {
    state.status = -7;
    return;
  }
  wl_registry_add_listener(state.registry, &registry_listener, nullptr);
  state.sync = wl_display_sync(fl_wl_display());
  if (!state.sync) {
    state.status = -7;
    return;
  }
  wl_callback_add_listener(state.sync, &sync_listener, nullptr);
}
extern "C" int files_transfer_register_window(Fl_Window *window) {
  if (FILES_FLTK_DND_INPUT_SYMBOL() != 1)
    return 0;
  if (!window || state.closing)
    return 0;
  auto tracker = std::unique_ptr<Fl_Widget_Tracker>(
      new (std::nothrow) Fl_Widget_Tracker(window));
  if (!tracker)
    return 0;
  unregister_dnd();
  state.owned_window = std::move(tracker);
  if (!fl_files_set_dnd_delegate_v1)
    return 0; // Custom/unpatched build: no external DnD capability.
  return fl_files_set_dnd_delegate_v1(delegates_window);
}
extern "C" int files_transfer_dnd_hook_ready() {
  return fl_files_set_dnd_delegate_v1 && state.owned_window &&
         !state.owned_window->deleted() && !state.closing && !state.seat_lost;
}
extern "C" uint64_t files_transfer_gesture(Fl_Window *window, int event) {
  state.permit =
      {}; // New real gesture invalidates any pending publication permit.
  state.drag_permit = {}; // A later gesture cannot reuse an earlier press.
  files_transfer_init();
  if (!fl_wl_display() || !window || !window->shown() || Fl::event() != event ||
      !device())
    return 0;
  bool mouse = event == FL_PUSH && Fl::event_button() == FL_LEFT_MOUSE &&
               (Fl::event_buttons() & FL_BUTTON1);
  bool keyboard = event == FL_KEYDOWN || event == FL_SHORTCUT;
  if (!mouse && !keyboard)
    return 0;
  auto *native = fl_wl_xid(window);
  auto *driver = screen();
  if (!native || !driver->seat || !driver->seat->serial)
    return 0;
  auto *surface = fl_wl_surface(native);
  if ((mouse ? driver->seat->pointer_focus : driver->seat->keyboard_surface) !=
      surface)
    return 0;
  auto id = fresh();
  if (!id)
    return 0;
  auto tracker = std::unique_ptr<Fl_Widget_Tracker>(
      new (std::nothrow) Fl_Widget_Tracker(window));
  if (!tracker)
    return 0;
  state.permit = {id,
                  state.epoch,
                  state.seat,
                  surface,
                  std::move(tracker),
                  driver->seat->serial,
                  mouse,
                  Clock::now() + std::chrono::seconds(2)};
  return id;
}
extern "C" void files_transfer_invalidate() { state.permit = {}; }
// Move the permit captured on FL_PUSH into a distinct, one-shot drag slot.
// It must be invalidated on release/cancel, even if the proof is still pending.
extern "C" int files_transfer_hold_drag(uint64_t id) {
  state.drag_permit = {};
  if (!current(id) || !state.permit.mouse)
    return 0;
  state.drag_permit = std::move(state.permit);
  state.permit = {};
  return 1;
}
extern "C" void files_transfer_cancel_drag() { state.drag_permit = {}; }
// Caller must validate the original press before entering this helper.
static int start_drag_admitted(uint32_t serial, wl_surface *surface,
                               const unsigned char *bytes, size_t size) {
  if (!bytes || size <= 5 || size > WIRE || memcmp(bytes, "copy\n", 5))
    return -2;
  if (wl_data_device_manager_get_version(state.manager) < 3)
    return -3;
  auto free = state.sources.end();
  for (auto i = state.sources.begin(); i != state.sources.end(); ++i)
    if (!*i) { free = i; break; }
  if (free == state.sources.end()) return -4;
  auto source = std::unique_ptr<Source>(new (std::nothrow) Source);
  if (!source) return -7;
  source->bytes = std::unique_ptr<unsigned char[]>(new (std::nothrow) unsigned char[size]);
  if (!source->bytes) return -7;
  memcpy(source->bytes.get(), bytes, size);
  source->size = size;
  source->offset = 5;
  source->drag = true;
  source->deadline = Clock::now() + std::chrono::seconds(30);
  source->id = fresh();
  if (!source->id) return -9;
  source->proxy = wl_data_device_manager_create_data_source(state.manager);
  if (!source->proxy) return -7;
  wl_data_source_add_listener(source->proxy, &source_listener, source.get());
  wl_data_source_offer(source->proxy, URI);
  wl_data_source_set_actions(source->proxy, WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  wl_data_device_start_drag(state.device, source->proxy, surface, nullptr, serial);
  state.drag_source = source->id;
  *free = std::move(source);
  return 1; // Request only: not proof of compositor acceptance or delivery.
}
extern "C" int files_transfer_start_drag(uint64_t id,
                                          const unsigned char *bytes,
                                          size_t size) {
  // The only accepted payload is our COPY URI encoding. Never export a Cut
  // or convert a dnd_finished callback into permission to remove a source.
  if (!current_permit(state.drag_permit, id) || !state.drag_permit.mouse ||
      !(Fl::event_buttons() & FL_BUTTON1))
    return -1;
  int result = start_drag_admitted(state.drag_permit.serial,
                                   state.drag_permit.surface, bytes, size);
  state.drag_permit = {}; // Consume even on failure; a retry needs a new press.
  return result;
}
extern "C" int files_transfer_outgoing_active() {
  for (const auto &source : state.sources)
    if (source && source->id == state.drag_source && !source->cancelled)
      return 1;
  return 0;
}
extern "C" int files_transfer_publish(uint64_t permit,
                                      const unsigned char *bytes, size_t size) {
  if (!current(permit))
    return -1;
  if (!bytes || size > WIRE || size < 5)
    return -2;
  bool cut = size >= 4 && !memcmp(bytes, "cut\n", 4),
       copy = size >= 5 && !memcmp(bytes, "copy\n", 5);
  if (!cut && !copy)
    return -2;
  auto free = state.sources.end();
  for (auto i = state.sources.begin(); i != state.sources.end(); ++i)
    if (!*i) {
      free = i;
      break;
    }
  if (free == state.sources.end())
    return -4;
  auto source = std::unique_ptr<Source>(new (std::nothrow) Source);
  if (!source)
    return -7;
  source->bytes =
      std::unique_ptr<unsigned char[]>(new (std::nothrow) unsigned char[size]);
  if (!source->bytes)
    return -7;
  memcpy(source->bytes.get(), bytes, size);
  source->size = size;
  source->offset = cut ? 4 : 5;
  source->cut = cut;
  source->id = fresh();
  if (!source->id)
    return -9;
  source->proxy = wl_data_device_manager_create_data_source(state.manager);
  if (!source->proxy)
    return -7;
  wl_data_source_add_listener(source->proxy, &source_listener, source.get());
  wl_data_source_offer(source->proxy, URI);
  wl_data_source_offer(source->proxy, GNOME);
  wl_data_source_offer(source->proxy, KDE);
  // Fresh one-shot source per publication. Never reuse source for start_drag.
  wl_data_device_set_selection(state.device, source->proxy,
                               state.permit.serial);
  state.permit = {};
  state.publication = source->id;
  *free = std::move(source);
  state.status = 2;
  return 1; // Request sent, NOT an ownership/consumer acknowledgement.
}
extern "C" uint64_t files_transfer_offer(unsigned *flags) {
  if (flags)
    *flags = 0;
  if (!fl_wl_display() || !device())
    return 0;
  auto *o = offer(state.selection);
  if (!o || o->invalid)
    return 0;
  if (flags)
    *flags = o->flags;
  return o->id;
}
extern "C" int files_transfer_receive(uint64_t permit, uint64_t id,
                                      unsigned format) {
  if (!current(permit) || state.reading || id != state.selection)
    return -1;
  auto *o = offer(id);
  if (!o || o->invalid || !(o->flags & format) ||
      (format != 1 && format != 2 && format != 4))
    return -2;
  int fds[2];
  if (pipe2(fds, O_CLOEXEC) < 0)
    return -3;
  // Only OUR reader is nonblocking. The external emitter keeps normal pipe
  // backpressure instead of unexpected EAGAIN/truncated writes.
  int flags = fcntl(fds[0], F_GETFL);
  if (flags < 0 || fcntl(fds[0], F_SETFL, flags | O_NONBLOCK) < 0) {
    close(fds[0]);
    close(fds[1]);
    return -3;
  }
  state.reading = id;
  state.reading_drag = false;
  state.reading_epoch = state.epoch;
  state.permit = {};
  wl_data_offer_receive(o->proxy,
                        format == 2   ? GNOME
                        : format == 4 ? KDE
                                      : URI,
                        fds[1]);
  close(fds[1]);
  return fds[0]; // Caller owns exactly once, must read bounded and ACK after
                 // drain.
}
// Request URI and KDE metadata from the very same offer and epoch. Both read
// ends are returned together; the worker must drain them concurrently.
static int receive_kde_admitted(Offer *o, uint64_t id, int *metadata_fd) {
  int uri[2] = {-1, -1}, metadata[2] = {-1, -1};
  if (pipe2(uri, O_CLOEXEC) < 0 || pipe2(metadata, O_CLOEXEC) < 0) {
    for (int fd : uri) if (fd >= 0) close(fd);
    for (int fd : metadata) if (fd >= 0) close(fd);
    return -3;
  }
  int uri_flags = fcntl(uri[0], F_GETFL), meta_flags = fcntl(metadata[0], F_GETFL);
  if (uri_flags < 0 || meta_flags < 0 ||
      fcntl(uri[0], F_SETFL, uri_flags | O_NONBLOCK) < 0 ||
      fcntl(metadata[0], F_SETFL, meta_flags | O_NONBLOCK) < 0) {
    for (int fd : uri) close(fd);
    for (int fd : metadata) close(fd);
    return -3;
  }
  state.reading = id;
  state.reading_drag = false;
  state.reading_epoch = state.epoch;
  state.permit = {};
  wl_data_offer_receive(o->proxy, URI, uri[1]);
  wl_data_offer_receive(o->proxy, KDE, metadata[1]);
  close(uri[1]); close(metadata[1]);
  *metadata_fd = metadata[0];
  return uri[0];
}
extern "C" int files_transfer_receive_kde(uint64_t permit, uint64_t id,
                                          int *metadata_fd) {
  if (metadata_fd) *metadata_fd = -1;
  if (!metadata_fd || !current(permit) || state.reading || id != state.selection)
    return -1;
  auto *o = offer(id);
  if (!o || o->invalid || (o->flags & 5) != 5)
    return -2;
  return receive_kde_admitted(o, id, metadata_fd);
}
extern "C" int files_transfer_read_ack(uint64_t id, int accepted) {
  auto *o = offer(id);
  bool valid = state.reading == id && state.reading_epoch == state.epoch && o &&
               !o->invalid && !state.closing &&
               (state.reading_drag
                    ? (state.drop_offer == id && copy_offer(o) &&
                       (wl_data_offer_get_version(o->proxy) < 3 ||
                        o->action == WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY))
                    : state.selection == id);
  if (state.reading == id) {
    if (state.reading_drag) {
      if (valid && accepted && wl_data_offer_get_version(o->proxy) >= 3)
        wl_data_offer_finish(
            o->proxy); // COPY wire accepted; NEVER delete source.
      state.drop_offer = 0;
      state.drop_pending = false;
    }
    state.reading = 0;
    state.reading_drag = false;
  }
  collect_offers();
  return valid ? 1 : 0;
}
extern "C" void files_transfer_drag_context(uint64_t pane, uint64_t tab,
                                            uint64_t generation) {
  state.context.pane = pane;
  state.context.tab = tab;
  state.context.generation = generation;
}
extern "C" int files_transfer_drag_event(DragEvent *out) {
  if (!out || state.closing || !state.registry)
    return 0;
  *out = state.drop_pending ? state.dropped : state.hover;
  return out->sequence && out->kind;
}
extern "C" void files_transfer_reject_drop(uint64_t id) {
  if (state.drop_offer == id && state.reading != id) {
    state.drop_offer = 0;
    state.drop_pending = false;
    state.hover.kind = 3;
    state.hover.sequence = fresh();
    collect_offers();
  }
}
extern "C" int files_transfer_receive_drop(uint64_t id) {
  auto *o = offer(id);
  if (!state.drop_pending || state.drop_offer != id || state.reading ||
      state.drop_epoch != state.epoch || Clock::now() >= state.drop_deadline ||
      !copy_offer(o))
    return -1;
  int fds[2];
  if (pipe2(fds, O_CLOEXEC) < 0)
    return -3;
  int flags = fcntl(fds[0], F_GETFL);
  if (flags < 0 || fcntl(fds[0], F_SETFL, flags | O_NONBLOCK) < 0) {
    close(fds[0]);
    close(fds[1]);
    return -3;
  }
  state.reading = id;
  state.reading_epoch = state.epoch;
  state.reading_drag = true;
  state.drop_pending = false;
  state.hover.kind = 3;
  state.hover.sequence = fresh();
  wl_data_offer_receive(o->proxy, URI, fds[1]);
  close(fds[1]);
  return fds[0];
}
extern "C" int files_transfer_pump() {
  if (!fl_wl_display() || state.closing)
    return state.status;
  files_transfer_init();
  device();
  if (state.drop_pending && Clock::now() >= state.drop_deadline) {
    state.drop_pending = false;
    state.drop_offer = 0;
    state.hover.kind = 3;
    state.hover.sequence = fresh();
    collect_offers();
  }
  size_t quota = 64 * 1024;
  for (auto &s : state.sends) {
    if (s.fd < 0)
      continue;
    if (Clock::now() >= s.deadline ||
        Clock::now() - s.progress >= std::chrono::seconds(1)) {
      if (s.source->id == state.publication)
        state.status = -8;
      close_send(s);
      continue;
    }
    if (!quota)
      continue;
    size_t size = s.size - s.offset;
    if (size > quota)
      size = quota;
    ssize_t n = write(s.fd, s.bytes + s.offset, size);
    if (n > 0) {
      s.progress = Clock::now();
      s.offset += static_cast<size_t>(n);
      quota -= static_cast<size_t>(n);
      if (s.offset == s.size)
        close_send(s);
    } else if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK &&
               errno != EINTR) {
      if (s.source->id == state.publication)
        state.status = -5;
      close_send(s);
    }
  }
  for (auto &source : state.sources)
    if (source && source->drag && !source->cancelled && Clock::now() >= source->deadline) {
      for (auto &send : state.sends)
        if (send.source == source.get()) close_send(send);
      source->cancelled = true;
    }
  for (auto &source : state.sources)
    if (source && source->cancelled) {
      wl_data_source_destroy(source->proxy);
      source.reset();
    }
  return state.status;
}
extern "C" void files_transfer_shutdown() {
  state.closing = true;
  disable(0);
  if (state.sync) {
    wl_callback_destroy(state.sync);
    state.sync = nullptr;
  }
  if (state.manager) {
    wl_data_device_manager_destroy(state.manager);
    state.manager = nullptr;
  }
  if (state.registry) {
    wl_registry_destroy(state.registry);
    state.registry = nullptr;
  }
}

extern "C" void files_transfer_cancel_io() {
  state.permit = {};
  state.drag_permit = {};
  state.drop_pending = false;
  if (!state.reading_drag)
    state.drop_offer = 0;
  state.drag = 0;
  state.hover.kind = 3;
  state.hover.sequence = fresh();
  collect_offers();
  // Closing can be cancelled in Rust. Cancel the current delegated offer while
  // keeping future own-window drags suppressed until final shutdown/seat loss.
  unregister_dnd();
  if (fl_files_set_dnd_delegate_v1 && state.owned_window &&
      !state.owned_window->deleted() && !state.closing && !state.seat_lost)
    fl_files_set_dnd_delegate_v1(delegates_window);
  for (auto &send : state.sends)
    close_send(send);
  for (auto &source : state.sources)
    if (source && source->drag)
      source->cancelled = true; // Pump destroys only our own proxy.
  // Rust retains its reader/tombstone until ACK; no new request may reuse it.
}

extern "C" int files_transfer_window_event(Fl_Window *window, int event) {
  if (!fl_wl_display() || !state.registry || !window)
    return 0;
  if (event != FL_UNFOCUS && event != FL_HIDE)
    return 0;
  auto *native = fl_wl_xid(window);
  auto *driver = screen();
  bool lost = event == FL_HIDE && (!window->shown() || !window->visible_r());
  // An internal widget unfocus while keyboard_surface stays in this window is
  // NOT a loss of the Wayland window's focus. keyboard_leave clears it first.
  if (event == FL_UNFOCUS && native && driver->seat &&
      driver->seat->keyboard_surface != fl_wl_surface(native))
    lost = true;
  if (lost) {
    state.permit = {};
    state.drag_permit = {};
    ++state.epoch;
    return 1;
  }
  return 0;
}
