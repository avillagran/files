// Specific lifecycle regression for the actual modified FLTK translation unit.
// Fake offer pointers reach ONLY wrapped libwayland marshal/version functions.
// No display, window, clipboard, protocol handshake or OS input is used.
#include "../fl_wayland_clipboard_dnd.cxx"
#include <cassert>
#include <cstdio>
#include <cstdint>
static int destroyed = 0, negotiated = 0;
extern "C" uint32_t __wrap_wl_proxy_get_version(wl_proxy *) { return 3; }
extern "C" wl_proxy *__wrap_wl_proxy_marshal_flags(wl_proxy *, uint32_t opcode,
    const wl_interface *, uint32_t, uint32_t flags, ...) {
  if (flags & WL_MARSHAL_FLAG_DESTROY) {
    assert(opcode == WL_DATA_OFFER_DESTROY);
    ++destroyed;
  } else {
    assert(opcode == WL_DATA_OFFER_SET_ACTIONS);
    ++negotiated;
  }
  return nullptr;
}
static int accepts(Fl_Window *) { return 1; }
static wl_data_offer *opaque(uintptr_t n) {
  return reinterpret_cast<wl_data_offer *>(n);
}
int main() {
  fl_files_set_dnd_delegate_v1(accepts);
  files_delegated_sequence = true;
  files_delegated_offer = opaque(1);
  fl_files_set_dnd_delegate_v1(nullptr);
  assert(destroyed == 1 && files_delegated_sequence && !files_delegated_offer);
  data_device_handle_motion(nullptr, nullptr, 0, 0, 0);
  data_device_handle_drop(nullptr, nullptr);
  data_device_handle_leave(nullptr, nullptr);
  fl_files_set_dnd_delegate_v1(nullptr);
  assert(destroyed == 1 && negotiated == 0 && files_delegated_sequence);
  // A new enter resets sequence suppression. Unknown/default surface follows
  // upstream negotiation, without calling the own-window predicate.
  fl_files_set_dnd_delegate_v1(accepts);
  data_device_handle_enter(nullptr, nullptr, 1, nullptr, 0, 0, opaque(2));
  assert(!files_delegated_sequence && negotiated == 1 && destroyed == 1);
  files_delegated_sequence = true;
  files_delegated_offer = opaque(3);
  data_device_handle_leave(nullptr, nullptr);
  data_device_handle_drop(nullptr, nullptr);
  data_device_handle_motion(nullptr, nullptr, 0, 0, 0);
  assert(destroyed == 2 && negotiated == 1 && files_delegated_sequence);
  files_delegated_offer = opaque(4);
  data_device_handle_drop(nullptr, nullptr);
  data_device_handle_leave(nullptr, nullptr);
  fl_files_set_dnd_delegate_v1(nullptr);
  assert(destroyed == 3 && negotiated == 1 && !files_delegated_offer);
  std::puts("{\"lifecycle\":\"PASS\",\"destroyed_once\":3,\"default_negotiations\":1,\"late_events_suppressed\":true,\"native_protocol\":\"UNVALIDATED\"}");
}
