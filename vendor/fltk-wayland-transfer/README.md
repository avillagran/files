# FLTK 1.5.23 own-window DnD hook

This is a narrow modification of the FLTK Wayland clipboard/DnD translation
unit shipped in the checksummed fltk-sys1.5.23 crate. `SOURCE.json` identifies
archive/original/modified/patch/license hashes. `upstream/` and the zero-fuzz
patch reconstruct the modified file. All FLTK changes retain its LGPL with the
static-link exception (`COPYING`); this file is not relicensed as application MIT.

The C ABI `fl_files_set_dnd_delegate_v1(predicate)` opts in only windows accepted
by a lifetime-checked application predicate. At enter it captures delegation
before upstream set_actions. The delegated sequence skips upstream motion,
roundtrip, drop/read/finish, while still releasing its FLTK-owned offer on
leave/drop/unregister. Unregister preserves sequence suppression until a NEW
enter, including late motion/drop. Clipboard/default windows follow upstream.
The app-owned data_device has different proxies and retains its own lifetimes;
this patch never replaces those listeners or gives the app a FLTK-owned offer.

`native/fltk/inject-v1.cmake` defers until target fltk exists and replaces exactly
ONE source, checking the original SHA. It copies into the build directory,
sets its original header include in TARGET_DIRECTORY fltk, and preserves other
source entries (including generated protocols). Registry files are READ ONLY.
The workspace CMAKE wrapper forwards other configure/build/version argv intact;
only pinned cfltk1.5.23 gets CMAKE_PROJECT_INCLUDE. It does not change compilers,
global installs or other projects. The companion toolchain covers clean trees;
a newly supplied toolchain alone was experimentally ignored by an existing
CMake tree, hence the configure wrapper.

The build-input contract covers original/modified source, patch, license,
SOURCE.json, injection/guard/toolchain/wrapper. CMake appends a strong native
symbol named by that full digest to the generated source. ui/build.rs watches
repo inputs, checks generated contents and references that exact strong symbol
from the bridge. An old OTHER build directory cannot silently validate the
library eventually linked: final app linkage must resolve the precise symbol.
A changed input without fltk-sys rerun fails closed. Bump BOTH versioned wrapper
and CFLTK_TOOLCHAIN paths in .cargo/config.toml when updating inputs (and update
the paths in contract-v1.cmake/build.rs); do not bypass the guard or edit registry.
The runtime weak API by itself is not content/version proof.

Specific lifecycle regression: tests/delegate.cxx includes the actual modified
TU and wraps only libwayland version/marshal for opaque private pointers. It
checks single destruction and late-event suppression plus default negotiation.
It uses NO display/window/clipboard/input/protocol handshake. The compiled static
object guard and this fixture do NOT prove native drag/drop, app runtime or
interoperability. External DnD remains pending application receive/review/queue
wiring and genuine Wayland events; don't mark O06 complete for this hook.
