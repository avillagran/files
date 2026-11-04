# Per-folder view preferences (V12)

Status: core storage and UI/controller integrated; scoped functional acceptance
passed on NODE with Xvfb/X11 and Hyprland/Wayland. Installed on NODE after
completing and validating the native five-artifact runtime set.

## User behavior to integrate

“Guardar vista de esta carpeta” explicitly captures the focused folder's view,
Name/Size/Modified order and direction, grid zoom, and vertical scroll. “Olvidar
vista de esta carpeta” removes that saved default. Normal navigation back into a
folder restores its saved default. Saving is explicit: two tabs on the same path
can experiment independently without silently overwriting one another.

Hidden files remain GLOBAL. Query, selection, focus, session history and task
state are not folder preferences. A restored tab/session snapshot takes priority
over directory defaults. A saved default must not disable dual-pane mode.
Columns' complete retained chain/horizontal positions are not included in this
first format. Scroll is applied after loading and then clamped to actual content.
A late initial read must not override a newer navigation, tab/session restoration,
manual view/order/zoom/scroll change, or close intention.

## Core API and bounds

`files_core::folder_views` exports `Settings`, `Entry`, `Document`, `Op`,
`Updated`, `encode`, `decode`, and `apply(config_directory, &op)`.

- `Op::Read`: read only; missing config does not create it.
- `Op::Save(Entry)`: replace the exact path's settings or add a new path, merging
  the freshest stored document under a cooperative exclusive nonblocking lock.
- `Op::Forget(Entry)`: remove only if the freshest entry still equals the expected
  settings; stale removal fails. This is value comparison, not an ABA-proof token.
- `Op::Sync`: sync current file and parent; never rewrite an earlier snapshot.
- `Updated` separates a durable publication from a published-but-unsynced result.
  Retry sync against the current file; do not replay an old document.

Limits: 256 rows; 256 KiB encoded file; 4096 bytes per absolute path; zoom65–180%;
scroll0–100,000,000 pixels. At capacity, updating an existing entry is permitted;
adding another fails explicitly without eviction. These are local bounds, not
measured whole-app PSS/CPU guarantees.

Stable view tags are Groups0, List1, Columns2, Grid3, SpaceMap4. Sort tags are
Name0, Size1, Modified2. No implicit reuse of the tab/session wire format.

## Persistence and interpretation

Only `folder-views.bin` and `folder-views.lock` inside the application config
directory are used. Saved folder paths are keys only: storing them performs no
filesystem lookup, mkdir or writes in those directories. On Unix keys preserve
exact bytes, including invalid UTF-8 and lexical repeated separators; no
canonicalization or URL decoding occurs. This intentionally distinguishes aliases.
Other platforms require UTF-8 keys and coordinated writes remain unsupported
until the store lock has an implementation there.

Wire: `FILESVW1`, u16LE version1, u8 platform1Unix/2other, u16LE count; each sorted
raw-byte key consists of u16LE length, path bytes, view/sort/descending/zoom bytes,
and u32LE scroll. Strict ascending raw keys, no duplicates/trailing bytes. The
whole wire is validated with borrowed paths before materializing any entries.
Encode preflights all rows and total size before allocating the output.

Read requires a regular no-follow/nonblocking file on Unix, enforces metadata and
actual prefix byte limits, and revalidates FD/path revision after read. Writes use
same-directory exclusive temporary files, sync the temporary, recheck target and
lock identity, then atomic replacement and parent sync. An invalid/future file,
symlink, busy lock, or changed target is preserved with an error.

This is cooperative multi-window coordination, not a claim of protection against
all hostile noncooperating writers, configuration-parent swaps, power loss, or
network filesystem behavior. No kernel fault injection or power-loss proof has
been performed for this unit.

## UI/controller integration and remaining acceptance

Implemented on NODE: existing bounded AuxIo worker, one retained folder-view
operation through exact ACK, capped error text, retry/discard and close drain.
A published-but-unsynced ACK replaces the retry with Sync, never an earlier Save.
The retained catalog obeys the core format limits; retained write intents reject
paths/settings beyond that format before queueing. No extra navigation worker.

Preferencias and Ctrl+Shift+P expose Guardar/Olvidar vista de esta carpeta,
Recargar/reintentar and Omitir intento. Save captures the current focused path
and settings explicitly. Forget uses the expected catalog entry; concurrent
changes fail instead of removing a newer default. Neither normal browsing nor
restoring a tab writes a folder preference. The initial catalog read is silent;
errors and explicit write ACKs remain visible in the status/close UI.

Navigation arms one restoration target with owner, exact path, generation and
baseline settings. Restoration waits for the listing and catalog. Explicit
view/sort/zoom/scroll/focus actions cancel it, including changes later reverted;
owner/path/generation or baseline mismatches also reject it. Tab/session
restoration cancels it after load_location, taking priority. Dual-pane mode is
retained (its List view wins); order/zoom/scroll still restore for the focused
folder. Scroll is applied after the listing and clamped after view geometry is
painted. Columns retain the existing listing data-budget admission check.

The catalog is local to the window after its initial read. Recargar reads fresh
cross-window defaults; successful Save/Forget merge with fresh disk state and
replace the local catalog. This is not live cross-window synchronization.

Meaningful core tests cover wire/raw/budget checks, update/merge/stale-forget,
read-only absence, busy lock and invalid/future/symlink preservation, and final
identity rechecks. Controller tests additionally cover captured save→navigation→restore, stale
Forget against a real temporary config store, late responses/manual intent,
tab/session priority, dual retention, global hidden/query preservation, post-load
scroll clamp, palette dispatch and ACK/retry/discard/close pressure. These are
model/handler tests, not native keyboard/mouse or framebuffer acceptance.
NODE acceptance now covers actual FLTK framebuffers at1000×650 and440×440,
raw-path explicit Save→navigate→restore with viewport scroll, independent live
tabs on the same path, global hidden state, painted Forget, short busy lock,
close blocked until resolution, retry preserving its original folder after
navigation, new-process restoration and the production toolkit popup Save/Forget.
The owned source trees remained byte-identical. Each backend had three normal
callback/drain exits. This is internal handler/painted-target acceptance, not
OS keyboard/mouse delivery, clipboard, AT-SPI,2× sharpness or lifetime coverage.

Reproduce with `python3 scripts/folder-views-ui-check.py` for isolated Xvfb and
`python3 scripts/folder-views-ui-check.py --wayland` for the verified NODE socket.
The latter opens only fixture windows and captures their own framebuffers; it
requires `/run/user/1000/wayland-1`. Final receipts:
- `docs/evidence/folder-views/node-functional-1793514702737086642/receipt.json`
- `docs/evidence/folder-views/node-functional-1793514706809565497/receipt.json`

Earlier harness FAILs are retained with assessments: f32 exact equality and TOML
whitespace comparisons were overly strict. No application failure is inferred
from those assertions. Search remains the first unfiltered palette command;
folder-view actions are grouped with view commands, with a regression test.
The existing command-palette script now expects57 commands.

NODE installation completed after native HTML-adapter build and release HTML
acceptance. `docs/evidence/node-install-native/receipt.json` verifies all five
installed hashes and all three executable ELF machines as x86_64. It records the
first-install prior state: no previous files existed, so the backup directory is
empty. M2 installation and ARM archives were left untouched.

The installed app was then accepted independently using
`python3 scripts/folder-views-ui-check.py --wayland --installed` (3normal exits,
9framebuffers, same source-tree fingerprints) and
`WAYLAND_DISPLAY=wayland-1 XDG_RUNTIME_DIR=/run/user/1000 python3 scripts/html-ui-check.py --installed`
(existing HTML renderer,5framebuffers, exit0/drain and logged helper cleanup).
The latter is a relocation/runtime check, not new preview development or a
resource measurement. Installation record:
`docs/evidence/0.0.4/installed-checkpoint-1793515028382395392.json`.
Do not stage imported ARM artifacts as NODE binaries.
Resource/startup experiments remain frozen; previews remain paused.

## Descriptor ownership on normal release

OWNER-LOCK-RELEASE.md records the creator-PID explicit flock release added in
source ebfbd46. A child retaining an inherited descriptor no longer extends
normal owner lifetime; dropping a child copy cannot unlock the active creator.
Nonblocking acquisition, genuine busy/read-only/CAS behavior and formats remain
unchanged. Linux fork repros failed before and passed after;1290workspace tests
and corresponding source/installed UI checks passed, with scoped limitations.
