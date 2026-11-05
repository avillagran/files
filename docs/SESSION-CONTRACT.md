# Durable session — M23 contract

Status: M23 functionally accepted and integrated; versioned tab session.
527 Rust (141 core/67 helper/319 UI) and Clippy workspace/all-targets -D warnings
PASS, release/fmt PASS. Scene `tabs-session-ui-1793111266679001110.json` PASS
with eight app processes and six real captures; installation recorded separately.
The installed M22 is kept until this block is accepted. Resource/startup freeze
in effect: NOT_MEASURED. It does not resume operations nor media automatically.

## Startup and ownership

- No path in argv: restore a compatible session if one exists; otherwise Home. An
  explicit path, even after `--`, opens only that path and neither applies nor autosaves
  the session. Only the visible Save session action enables an ownership attempt
  in that instance, without restoring the existing file.
  `--no-session` disables session reading and writing for that instance;
  smoke scenes stay isolated unless their fixture explicitly enables it.
- The first instance that acquires a non-blocking inter-process lock on the
  `session.lock` file owns writing **until it exits**. The descriptor stays
  open in AuxIo and the lock is held during validation, temp/fsync/rename and the
  directory sync. CONFIG_UPDATE is not used as inter-process exclusion.
- Another window can read the last complete session but not save it; it shows a
  persistent session read-only notice. It does not claim ownership automatically
  when the first one goes away: this prevents its old snapshot from replacing a new one.
  Its global preferences keep their current scope, with no new CAS promise.
- The lock is not deleted on close; it is released through the FD. Verify the identity of the
  lock's pathname before saving. An external replacement or a conflict => block
  saving with a recoverable error, never silently overwrite.

AuxIo's initial response carries a startup token/generation. If the user
navigated, switched tabs/edited a destination or started closing, the ownership/diagnostic
result can be kept but the late document is **not applied** nor are listings/helpers
started because of it. There is no synchronous I/O on the UI nor input blocking.
Deterministic regression: a late response after navigation and after Closing does not change
path/generation nor revive previews.

## Format and limits

Private `session.bin` file, versioned schema and a platform tag. Raw paths and
names, without lossy conversion nor I/O in draw. Unix raw bytes; Windows/macOS
support is distinguished from the Linux evidence. At most eight active and eight closed,
the same selection/history/view/sort/descending/query/scroll/grid_zoom/pan and
anchor/focus as M22. Show hidden stays GLOBAL and is not serialized per tab.
New IDs when rebuilding; hits, indices, workers, bitmaps and helpers are not restored.

At most 4 MiB of file before parsing; lengths/counts are validated against the remaining bytes
and the M22 limits before reserving/copying. Output size preflight
before building the buffer. No silent truncation. Restoration first
validates everything and the aggregate budgets, then publishes the coherent set;
the raw selection is intersected with hidden/query only after the final generation's Done.

An invalid, truncated, platform-incompatible or future-version file is preserved
byte for byte: a visible error, and the session is disabled for writing in that instance.
It is not transformed automatically on save. To recover, you can close without
saving and explicitly rename/restore that file outside this unit.

## Saving and closing

Reuse the existing bounded AuxIo/channel; one outstanding session request, ACK with
the exact revision. Snapshot/buffer retained until ACK, Full keeps it pending,
a disconnect/error is never equivalent to success. Saved on normal close and through the explicit
save session action; state is not captured/cloned on every draw/gesture.
Switching tabs does no synchronous I/O. Saving does not interrupt the confirmed global queue.

Close drains operations/previews/du/layout/config and the session. An error stays
visible with Retry, Keep working or Close without saving pending items. If
rename already published but the directory sync fails, the result distinguishes
**published, durability not confirmed**; a retry under the same lock validates the
effective state and does not assume an earlier baseline. No atomicity across
config/layout/session nor task recovery after kill/power loss is claimed.

## Acceptance for this block

Serialized suite/Clippy/release/fmt, then two consecutive processes with the
same private profile save→restore: history/view/filter/raw selection and scroll;
explicit argv ignores restore; invalid/future stays intact with a visible error;
a second window does not overwrite the owner's new state. Captures of the restored instance
and of the recoverable error. Handler/framebuffer is not equivalent to OS events; no benchmarks.
Only after functional evidence, install a consistent app/helper/HTML with backup/hashes.


## Implementation and local reservations

`session_codec` makes one borrowed validation pass over the whole document before
copying paths/history/names; `validate` is used in Store without materializing
disposable sessions. Encode uses the same conservative charge and counts the whole
wire before the output Vec. Active <=1 MiB; inactive+closed <=3 MiB, selection
<=256 KiB of charge/100000 names and each field <=4096 B; up to 64 History paths
per record. NaN/infinity/zoom outside 0.65..1.8/scroll or pan out of range are rejected.
Save/retry does not trust the imported `Saved.charge`: it recomputes from the content.

During capture an active snapshot <=256 KiB can coexist with a wire <=4 MiB. AuxIo
can also retain an earlier read <=4 MiB (bounded capacity, regular-only through an
O_NOFOLLOW|O_NONBLOCK|CLOEXEC FD); validate does not build History/selections. Load
can have a wire <=4 MiB and a decoded state <=4 MiB of charge plus small bounded
containers coexisting, until it is applied and the file is dropped. These are reservations on top of the
listing and existing caches; they are not a PSS limit for the tree nor do they replace gates.
No new buffer per tick and no multiple session buffers in the queue.

A capture/encode failure blocks automatic close retries until Retry;
Full keeps the exact buffer and a stale ACK does not free it. A failed directory
sync after rename is tested: the new file is already visible, the result is
Unsynced and a retry verifies the effective file under the same lock.

Startup fixtures with FILES_TEST_SESSION use an observational-only smoke after
startup is resolved and painted; they do not call the mutating smoke_test. M22 keeps its standard
smoke. The existing scene tabs-ui-check.py --sessions checks save→restore,
argv, --no-session, another writer and invalid/future without instrumenting resources.
Windows raw/ownership are still Unsupported; the Unix codec does not imply macOS evidence.
The lock is advisory between cooperating instances; it does not confine config parents
nor stop an external editor that deliberately ignores/replaces the lock.


## M23 evidence, 2026-10-27

`session-final-test.log`: 527 Rust = 141 core/67 helper/319 UI.
`session-observational-clippy.log`, `session-release.log` and
`session-final-fmt-check.log` PASS; copies in session-checkpoint-validation.json.
The first test FAIL was a receiver dropped by the fixture (Disconnected),
fixed by keeping the binding; both Clippy FAILs were style. Logs intact.

`tabs-session-ui-1793111266679001110.json`: own 566 B file, save→restore in
consecutive processes, A Grid 125%/scroll 120/pan 48/descending order/query keep/2
selected/anchor/focus and B List/query same/independent back history; a closed
raw 0xFF tab restored when reopening. An explicit path opened only B and did not save on
close; --no-session refused to save. Another window stayed read-only and did not take
ownership when the owner exited nor overwrite its new bytes. Invalid and version 99
remained byte for byte. Eight normal exits 0 after drain/ACK.

Six logical framebuffers 1000×650, internal actions/handlers without OS input nor
clipboard. No operation/media resumed, and no promise of task recovery
after a crash. Resources/startup NOT_MEASURED; the footer's PSS does not prove a budget.
V10 stays partial because of cross-platform/OS input evidence and the frozen gates;
instance tabs/session are no longer listed as unimplemented.


M26 view compatibility: the legacy Spatial tag 3 is decoded as Columns, with
scroll/zoom/pan reset (incompatible icon geometry); path, history,
filter, raw selection/focus/anchor kept. The new Columns tag 5 keeps the
vertical scroll. VERSION does not change and old layouts are not deleted; old readers
that do not know tag 5 reject/preserve the file. The whole chain is not persisted.
Regression encode legacy→decode and tag 5 round trip PASS; no new resource gate.


M27 dual pane does not change the format nor promise to persist both halves: Save
session and close capture only the Tabs of the focused group. Preferences shows
«Guardar sólo este grupo» (save only this group) in dual mode. The secondary/hidden group's metadata stays
bounded jointly with the 3 MiB snapshots, but it is not serialized in M23. Operations
and media are never resumed through restore. Persisting both panes is pending.

## Descriptor ownership on normal release

OWNER-LOCK-RELEASE.md records the creator-PID explicit flock release added in
source ebfbd46. A child retaining an inherited descriptor no longer extends
normal owner lifetime; dropping a child copy cannot unlock the active creator.
Nonblocking acquisition, genuine busy/read-only/CAS behavior and formats remain
unchanged. Linux fork repros failed before and passed after; 1290 workspace tests
and corresponding source/installed UI checks passed, with scoped limitations.
