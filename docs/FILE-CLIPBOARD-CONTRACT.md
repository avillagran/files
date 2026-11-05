# Cut/copy/paste — M28 (2026-10-27)

Features/UI priority after the installed M27. New previews paused and the
resource/startup freeze intact. This unit reuses the queue, reviews, ACK and engines;
no other copy/move engine. The earlier contract and verifiable progress are at the end.

## Two different capabilities

First delivery: **a files file clipboard, internal to this window**,
with captured raw names/paths and identity. Ctrl+C/Ctrl+X, Actions/context menu
Copy/Cut and Paste open a visible review with source/destination; F5/F6 keep
their direct access to the opposite pane. The UI identifies the buffer as «Portapapeles de files» (files clipboard).
It does not publish a list of paths as a supposedly interoperable OS clipboard, nor
modify the text clipboard when capturing files. O06 stays partial until an
external MIME backend and real tests exist. Closing the window frees this buffer; a Cut is
not persisted nor resumed when restoring a session.

FLTK 1.5.23 Wayland copies plain text even when given a type; the focus serial 0 is still
unresolved. For external input, get_clipboard_or_dragged_text transforms all of
text/uri-list, decodes URIs, strips file:// and delivers plain text; %0A/%00/hostnames
are not a lossless transport and the upstream read is not bounded by a later Rust
cap. Do not treat app::event_text as MIME/raw file bytes,
do not decode again, do not split lines as an authorization to move, do not incidentally edit the
registry/vendor. OS interoperability/external DnD need a bounded native
MIME/bytes adapter BEFORE that transformation; it is not marked
complete with the private buffer nor with a URI written in text/plain. A later internal DnD
can reuse the same raw token without inferring files from text.

## Capture and authority

Copy/Cut capture only the current visible selection (including the focus only if there is
no batch). OsString names, PathBuf path and owner (pane ID, tab ID), with no lossy
operational conversion; later focus/navigation/hidden changes do not change the batch.
Cut does NOT delete or rename on capture. Borrowed preflight before cloning: 128
roots, unique components <=4096 B, parents/paths <=4096 B, <=1 MiB of names
plus paths in total; a visible rejection keeps the earlier buffer. The common 128 limit allows
the same existing Move review, without faking unlimited clipboard support.

A capture/revalidation job on the existing AuxIo gets the identity of the
parent by FD and the names' signatures (device/inode/kind/size/mtime/ctime/nlink,
without atime). No stat/open/revalidate in draw/click/the UI thread. Publication requires
a current ID/generation; an old response never replaces a new buffer nor starts an
operation on close. Not one thread per gesture: at most one capture in flight, the latest
desired intent bounded, cooperative cancellation/drain of the ACK. Kernel D-state
can still block the syscall; no substitute worker is created.

Review/execution revalidates the source against the capture before accepting a move;
PreparedMove already pins objects by FD and revalidates before rename. A replacement of the
same name or a different linked parent requires recapture, not moving the substitute.
The signatures detect changes; they are not a content snapshot nor an atomic defense against
a hostile writer; Move's inode-conditional Linux limits are kept. Copy does not
delete the source and keeps the current engine's stability contract. Source and
result are shown without reducing identity to a textual path or CopyReport.completed.

## Paste, destinations and text

Ctrl+V is routed BEFORE the canvas interceptor that today always calls paste_text.
- Omnibar/rename/editable destination keep pasting OS TEXT, with the existing
  limits and semantics; text is never turned into a file operation.
- The browser without inputs/modals opens Paste from the private buffer and captures NOW
  the raw destination, owner and intent token. The dialog does not resolve the destination against
  the pane/tab that happens to be active when the asynchronous response arrives. Nothing starts
  automatically when FL_PASTE is received.
- A pending TEXT callback carries the intent/editable context; when switching
  pane/tab, leaving editing, closing or opening another dialog it is discarded instead
  of pasting into the new one. At most one pending text request; a second one does not
  silently replace the token. A callback without a token does not authorize files.
  FL_PASTE has no request ID: invalidating leaves a tombstone and keeps the
  slot busy until that callback is drained; it is not reassigned by timeout (avoiding A→B ABA).
  paste_text is called after releasing RefMut<Model>, since FLTK may deliver
  the paste synchronously for its own selection. A invalidated→B rejected→A
  drained→new legitimate B has a regression independent of the OS product.
  A compatible MIME preflight before reserving the slot. Real FLTK 1.5.23 Wayland
  delivers both own and external synchronously and may return WITHOUT a callback
  (empty/incompatible type); only that verified backend allows releasing on the
  definitive NoCallback return. X11/macOS/Windows/unknown keep the tombstone
  until the callback. A receipt identifies our invocation/return, NOT FL_PASTE:
  an old return cannot clear a newer reentrant request either.

The dialog uses the captured destination even if the focus changes. Explicitly changing its
text replaces the raw PathBuf just like M27, without breaking destination 0xFE. Confirm
admits into the existing global queue, keeps the job ID/paths/proofs and can be paused,
cancelled or closed with ACK. Pending/reject keeps the buffer and ownership. Jobs are not
duplicated by repeated Ctrl+V while a dialog/review already exists.

## ACK, memory and visible state

Each admitted operation is tied to a clipboard generation and the exact captured
subset; replacing the buffer does NOT change the job nor allow its ACK to empty the new one.
Copy keeps the buffer. Cut consumes only the roots confirmed by the Move
result. A confirmed total success consumes that batch; error/cancel keeps the uncompleted ones.
In a partial result, only exact destinations that can be correlated with the requests and a confirmed
publication are consumed; ambiguity/a renamed path/pending recovery is kept and
reported for revalidation. Success is never inferred from the source's absence. A recovery
ACK keeps the same intent/identity, without reinterpreting a new buffer.

A current buffer plus captures/dialogs/queue/un-ACKed results share an aggregate
2 MiB limit of clipboard metadata, checked before allocating/retaining (Arc without
cloning names); capacities and the capture scratch are included separately. One parent
FD per effective capture counts in addition to the review/active/pending FDs and the
un-ACKed result in the existing local 256 gates; each job is not budgeted
as if it were alone. No PSS claim from these caps: resources NOT_MEASURED freeze.
Versions are discarded only when their last consumer is released, not on mere enqueue.

## Planned acceptance (extension of the existing tabs/dual scene)

A/B same name different bytes; capture A→switch pane/tab→Paste in B shows
explicit source A/destination B. Exact 0xFF names and 0xFE destination. A non-mutating Cut,
review+paused non-mutating, copy keeps, move consumes the confirmed ones; cancel/
partial collision does not remove pending ones. Replacing the same path before revalidation
rejects; a new clipboard during a job survives an old ACK. Text in the omnibar,
renamer/destination does not trigger an operation; a late response after a change/close,
a double pending Paste, close with ACK. Real captures/handlers, without claiming OS input/
external clipboard nor new metrics. No new instrument other than extending the
existing one and unit regressions that exercise these contracts.


## Historical M28 foundation — 1fa3d00, before the visible wiring

`selection_source.rs` captures by parent FD and raw names, signatures with ctime/mtime
without atime, private fields and read-only access. 1 FD retained; verify opens at most
1 additional temporary parent FD (current is released before last). O_NOFOLLOW only on the
final component of the parent and statat without following the object; no confinement of every
ancestor nor a recursive directory snapshot. The descriptors of the
PreparedMove/PreparedFileMove token are compared with the captured signature (effective parent and
root); verify(path)→capture(path) is not enough. Deterministic regressions
verify A→replace B→capture B reject B in both engines, without mutation. The EXDEV of this
regression uses capture_inner(false) only to test the token, not as new proof
of different st_dev values nor EXDEV transfer.

201 core PASS/3 ignored, Clippy core/all-targets -Dwarnings PASS. The first compile FAIL
for nlink u32 on aarch64 is kept. The per-path 4096 budget and the aggregate one are independent;
128 valid names with a sum >4096 and the exact per-path boundary tested.
`text_paste_gate.rs` still without wiring/lib.rs: 7 standalone tests PASS for the tombstone,
synchronous return/NoCallback, invocation receipt and released borrow; not a test of the
OS clipboard. The current scope is not the complete clipboard UI, no new installation.
AuxIo/Controller/dialog, joint quotas/FDs, partial queue/ACK payloads
and canvas routing still follow. The installed M27 intact, no new previews nor metrics.


## M28 wiring and text MIME query

AuxIo captures the Source and publishes by ID; the buffer, the dialog and the jobs
share a Lease through an Arc. The ACK consumes only the confirmed subset.
Retries of a partial Cut verify the original signatures only of the
remaining roots: they do not look up nor recapture the one already moved. Dead Weak refs are pruned
before the lease limit. Source/Prepared use a live Control between roots and
keep io::ErrorKind::Interrupted, with no AtomicBool snapshot nor string that would
turn cancellation into a generic failure. Recovery only inherits the Cut tag
if the reviewed job and IntentProof match; another journal does not consume that buffer.

The canvas intercepts Ctrl+V before taking RefMut. An FL_PASTE without a token neither modifies
text nor authorizes files; the fields keep their bounded preflight. On close
the capture is cancelled and its ACK awaited along with the existing drain; no blocking join
nor substitute worker. Keep working after cancelling the close keeps the buffer.

A reproducible local source finding: fltk-rs 1.5.23 clipboard_contains(Text)
creates a CString; the Wayland driver compares the pointer with fl_selection_type[1].
The local native adapter files_clipboard_has_text passes the public constant
Fl::clipboard_plain_text. The registry/vendor is not edited, no other clipboard
is read, no serial is fabricated and no focus is requested. Other backends use the
existing API and their behavior is still not validated on a host.

The own-text test in a single call only proves the synchronous callback,
borrowing and the bytes of that case. The earlier scene with copy→event loop→paste
failed because of a false-negative preflight: the FAIL is kept. Do not attribute that result
to a loss of selection/focus. Persistence across loop iterations needs to be observed
with the fixed query; the compositor and external exchange are still pending. Do not use data-control
to get around this limit or take over the global selection without an explicit gesture.

M29: a contract before wiring for native MIME/bounded URI bytes and real DnD,
with a gesture on its own window and a real serial through wl_data_device/the FLTK bridge;
private codecs/transport are tested separately from OS input. No data-control
focus fallback, and no promise of interoperability based on the M28 buffer.


## Verified visible delivery (M28)

624 Rust (203 core/67 helper/354 UI, 3 core ignored), Clippy all-targets -Dwarnings,
release without warnings and fmt --check exit 0. Five real-flow regressions AuxIo→
buffer→dialog→queue→ACK include a partial subset + 100 leases, replacement, an old ACK
against a new buffer and the text token ABA/close. Controlled cancellation keeps
Interrupted and is tested between roots, without claiming syscall interruption.

`clipboard-ui-1793130549985863820.json` scoped functional PASS: eight exact IHDR
framebuffers (six 1000×650, two 800×440), two root exit 0 after drain ACK; Copy 0xFF
to destination 0xFE keeps bytes/source/occupants. Cut 2 capture/review/paused without
mutating; cancelling the dialog or a paused job keeps sources/count/generation;
a collision after review publishes only cut-a and keeps cut-b, a retry of the subset
completes. A replacement is rejected without moving the actor. A new Copy survives an old ACK.
Immediate own text AND copy→event loop→paste exact; two preflights show
rust_wrapper=false/canonical=true. This fixes the pointer false negative; it does not
test an external serial/focus nor file MIME. Do not attribute the earlier FAIL to a loss
of selection. Narrow menus complete without a toast, message kept on close.

The earlier functional PASS 1793130287088020335 and the Actions visual sidecar are
kept; the context menu did not cover controls (only a side strip). The original FAILs
for key loaded, partial JSON and the MIME query remain intact. See
`clipboard-checkpoint-validation.json` for logs/exit codes/hashes and limits.
No OS input/live resize/resources/startup, no global PASS nor closing O06. The next
M29 needs a MIME/DnD contract before wiring, without data-control focus.
