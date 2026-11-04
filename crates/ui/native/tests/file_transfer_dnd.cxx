// Private, deterministic test of the actual receiver TU. Opaque fake proxies
// touch only the wrapped marshal/version functions. No compositor or input.
#include "../file_transfer.cpp"
#include <cassert>
#include <cstdarg>
#include <cstdio>
static int accepts = 0, actions = 0, finishes = 0, destroys = 0;
static int kde_receives = 0;
static int sender_requests = 0;
static uint32_t sender_serial = 0, sender_actions = 0;
static wl_surface *sender_surface = nullptr;
static wl_proxy *const fake_manager = reinterpret_cast<wl_proxy *>(uintptr_t(0x111));
static wl_proxy *const fake_device = reinterpret_cast<wl_proxy *>(uintptr_t(0x222));
static wl_proxy *const fake_source = reinterpret_cast<wl_proxy *>(uintptr_t(0x333));
static const char *last_mime = nullptr;
static uint32_t last_actions = 999;
extern "C" uint32_t __wrap_wl_proxy_get_version(wl_proxy *) { return 3; }
extern "C" int __wrap_wl_proxy_add_listener(wl_proxy *, void (**)(void), void *) { return 0; }
extern "C" wl_proxy *__wrap_wl_proxy_marshal_flags(wl_proxy *proxy, uint32_t opcode,
                                                   const wl_interface *,
                                                   uint32_t, uint32_t flags,
                                                   ...) {
  va_list ap;
  va_start(ap, flags);
  if (proxy == fake_manager) {
    assert(opcode == WL_DATA_DEVICE_MANAGER_CREATE_DATA_SOURCE);
    va_end(ap);
    return fake_source;
  }
  if (proxy == fake_source) {
    if (opcode == WL_DATA_SOURCE_OFFER) {
      assert(!strcmp(va_arg(ap, const char *), URI));
    } else {
      assert(opcode == WL_DATA_SOURCE_SET_ACTIONS);
      sender_actions = va_arg(ap, uint32_t);
    }
    va_end(ap);
    return nullptr;
  }
  if (proxy == fake_device) {
    assert(opcode == WL_DATA_DEVICE_START_DRAG);
    assert(va_arg(ap, wl_data_source *) == reinterpret_cast<wl_data_source *>(fake_source));
    sender_surface = va_arg(ap, wl_surface *);
    assert(va_arg(ap, wl_surface *) == nullptr);
    sender_serial = va_arg(ap, uint32_t);
    ++sender_requests;
    va_end(ap);
    return nullptr;
  }
  if (flags & WL_MARSHAL_FLAG_DESTROY) {
    assert(opcode == WL_DATA_OFFER_DESTROY);
    ++destroys;
  } else if (opcode == WL_DATA_OFFER_ACCEPT) {
    (void)va_arg(ap, uint32_t);
    last_mime = va_arg(ap, const char *);
    ++accepts;
  } else if (opcode == WL_DATA_OFFER_RECEIVE) {
    const char *mime = va_arg(ap, const char *);
    int fd = va_arg(ap, int);
    const char *data = !strcmp(mime, URI) ? "file:///x\r\n" : !strcmp(mime, KDE) ? "1" : nullptr;
    assert(data && write(fd, data, strlen(data)) == static_cast<ssize_t>(strlen(data)));
    ++kde_receives;
  } else if (opcode == WL_DATA_OFFER_SET_ACTIONS) {
    last_actions = va_arg(ap, uint32_t);
    (void)va_arg(ap, uint32_t);
    ++actions;
  } else if (opcode == WL_DATA_OFFER_FINISH) {
    ++finishes;
  } else {
    assert(false && "unexpected protocol request");
  }
  va_end(ap);
  return nullptr;
}
static Offer &setup(uint64_t id) {
  auto &o = state.offers[0];
  o = {};
  o.proxy = reinterpret_cast<wl_data_offer *>(uintptr_t(id));
  o.id = id;
  o.flags = 1;
  o.actions = WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY;
  o.action = WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY;
  return o;
}
int main() {
  auto &first = setup(1);
  // Even an erroneously retained drag ID cannot renegotiate while busy.
  state.drag = 1;
  state.reading = 42;
  source_actions(&first, first.proxy, WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  assert(accepts == 1 && actions == 1 && !last_mime && last_actions == 0);
  state.drag = 0; // rejected enter makes a later source_actions inert
  source_actions(&first, first.proxy, WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  assert(accepts == 1 && actions == 1);
  state.reading = 0;
  state.drag = 1;
  state.drop_pending = true;
  source_actions(&first, first.proxy, WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  assert(accepts == 2 && actions == 2 && !last_mime && last_actions == 0);
  // Epoch changes reject the drop before allocating a pipe.
  state.drop_offer = 1;
  state.drop_epoch = 4;
  state.epoch = 5;
  state.drop_deadline = Clock::now() + std::chrono::seconds(1);
  assert(files_transfer_receive_drop(1) == -1 && !state.reading);
  // An admissible offer with accepted=false is destroyed without finish.
  state.drop_pending = false;
  state.drag = 0;
  state.reading = 1;
  state.reading_epoch = state.epoch;
  state.reading_drag = true;
  assert(files_transfer_read_ack(1, 0) == 1 && finishes == 0 && destroys == 1);
  auto &second = setup(2);
  state.drop_offer = 2;
  state.reading = 2;
  state.reading_epoch = state.epoch;
  state.reading_drag = true;
  assert(files_transfer_read_ack(2, 1) == 1 && finishes == 1 && destroys == 2);
  (void)second;
  auto &third = setup(3);
  state.drag = 3;
  state.hover.offer = 3;
  state.hover.kind = 1;
  drop(nullptr, nullptr);
  assert(state.drop_pending && state.drop_offer == 3 && state.drag == 0);
  int prior_accepts = accepts, prior_actions = actions;
  source_actions(&third, third.proxy, WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  assert(accepts == prior_accepts && actions == prior_actions);
  files_transfer_reject_drop(3);
  assert(!state.drop_pending && !state.drop_offer && state.hover.kind == 3);
  assert(finishes == 1 && destroys == 3);
  // Sender lifecycle: no press permit means no start. Completion only marks
  // our drag source for deferred cleanup; it never mutates any filesystem.
  static const unsigned char copy[] = "copy\nfile:///a\r\n";
  assert(files_transfer_start_drag(0, copy, sizeof(copy) - 1) == -1);
  assert(files_transfer_hold_drag(0) == 0);
  state.drag_permit.id = 42;
  state.drag_permit.epoch = state.epoch - 1;
  state.drag_permit.deadline = Clock::now() + std::chrono::seconds(1);
  assert(files_transfer_start_drag(42, copy, sizeof(copy) - 1) == -1);
  state.drag_permit.epoch = state.epoch;
  state.drag_permit.deadline = Clock::now() - std::chrono::seconds(1);
  assert(files_transfer_start_drag(42, copy, sizeof(copy) - 1) == -1);
  files_transfer_cancel_drag();
  assert(files_transfer_start_drag(42, copy, sizeof(copy) - 1) == -1 && sender_requests == 0);
  // Only this fixture invokes the internal already-admitted helper with fake
  // proxies; production reaches it exclusively after real permit validation.
  state.manager = reinterpret_cast<wl_data_device_manager *>(fake_manager);
  state.device = reinterpret_cast<wl_data_device *>(fake_device);
  auto *surface = reinterpret_cast<wl_surface *>(uintptr_t(0x444));
  static const unsigned char cut[] = "cut\nfile:///a\r\n";
  assert(start_drag_admitted(471, surface, cut, sizeof(cut)-1) == -2);
  assert(sender_requests == 0);
  assert(start_drag_admitted(471, surface, copy, sizeof(copy)-1) == 1);
  assert(sender_requests == 1 && sender_serial == 471 && sender_surface == surface);
  assert(sender_actions == WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  assert(files_transfer_outgoing_active() == 1);
  assert(state.sources[0] && state.sources[0]->drag && !state.sources[0]->cut);
  finished(state.sources[0].get(), nullptr);
  assert(state.sources[0]->cancelled && files_transfer_outgoing_active() == 0);
  state.sources[0].reset();
  // Same fake offer serves both MIME streams; changing selection between EOFs
  // rejects the unique ACK. This does not validate compositor behaviour.
  auto &kde = setup(4); kde.flags = 5;
  state.selection = 4; state.epoch = 20;
  int meta = -1;
  int uri = receive_kde_admitted(&kde, 4, &meta);
  assert(uri >= 0 && meta >= 0 && kde_receives == 2 && state.reading_epoch == 20);
  char bytes[32]; assert(read(uri, bytes, sizeof(bytes)) == 11); assert(read(uri, bytes, sizeof(bytes)) == 0);
  state.selection = 0; // New selection before metadata EOF.
  assert(read(meta, bytes, sizeof(bytes)) == 1 && bytes[0] == '1'); assert(read(meta, bytes, sizeof(bytes)) == 0);
  close(uri); close(meta);
  assert(files_transfer_read_ack(4, 1) == 0 && state.reading == 0);
  state.manager = nullptr;
  state.device = nullptr;
  auto sender = std::unique_ptr<Source>(new Source);
  sender->id = 99;
  sender->drag = true;
  state.drag_source = 99;
  state.sources[0] = std::move(sender);
  assert(files_transfer_outgoing_active() == 1);
  finished(state.sources[0].get(), nullptr);
  assert(state.sources[0]->cancelled && files_transfer_outgoing_active() == 0);
  state.sources[0].reset();
  std::puts("{\"busy_late_actions\":\"PASS\",\"stale_drop_epoch\":\"PASS\","
            "\"reject_without_finish\":\"PASS\",\"accepted_copy_finish\":"
            "\"PASS\",\"sender_copy_serial_and_cleanup\":\"PASS\",\"kde_offer_change_rejected\":\"PASS\",\"native_protocol\":\"UNVALIDATED\"}");
}
