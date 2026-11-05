# Native MIME and drag/drop — M29 (2026-10-27)

Contract BEFORE wiring. The installed M28 1a8e48a/1c95bf1 stays stable. O06
is still partial: the private buffer is not equivalent to OS MIME. Features/UI
priority, new previews paused and the resource/startup freeze intact. No new
measurements, daemons, global configuration, incompatible dependencies/licenses
nor fabricated input/focus. A contract is not a feature nor an acceptance.

## Capabilities and gesture

Linux/Wayland product: a platform bridge with wl_data_device_manager,
wl_data_device and wl_data_source; the FLTK backend's real display/seat are used.
Publishing the clipboard or starting a drag requires a permission captured during an
explicit gesture on this window, with the event's real serial and the associated focus/surface.
The asynchronous ACK never queries get_serial() to fabricate that association. A
fixture callback does not generate that permission. Without gesture/focus/capability: a visible error and the private M28
buffer kept; no silent global takeover nor publishing when restoring a
session. Receiving happens only after an explicit Paste/accepted drop on our window.

Do NOT use ext/wlr-data-control to get around fixtures' focus/serial, read the
personal clipboard in the background or replace a selection without an action. A local
ext-data-control inventory is not adoption. No wl-copy as a fake fix for the bridge.
Codecs/transport in private fixtures are distinguished from the handshake and OS events.
CUA 0/native input is still pending; an internal test is not turned into OS input.

Native publication will have its own state (not published/owner/ownership lost/
error); the UI distinguishes Paste from files and Paste from the system, without an ambiguous fallback
when both differ. Ctrl+C/X can keep the private buffer and publish only if
the backend receives the valid gesture; never describe global success if it is only private.

## Gesture permission versus asynchronous capture

In the real handler a GesturePermit {id,intent,kind,serial,seat,seat_epoch,
surface,window_lifetime,event_epoch,deadline} is pinned. The bridge gets the serial from the
**checked current event** and validates its focus on that same window; the serial is not
serialized into the clipboard nor sent to another display. The opaque C++ token is consumed
once: Rust keeps the ID, not borrowed pointers reusable as authority.
Any new relevant gesture, a different Copy/Cut/drag intent, cancellation,
Closing, a seat/surface change/loss or the 2 s expiry invalidates the pending permission.
A Source ACK carries intent+permit ID; only the bridge that captured the permission can
revalidate lifetime/epoch/focus and publish using THE saved serial. Never get
the latest serial when the ACK is received nor do a synchronous stat/open in the handler.
If the proof is slow/the permission expires, the private buffer may be ready but the UI says
«No publicado al sistema; vuelve a copiar» (not published to the system; copy again), keeping the earlier global clipboard.
The server/compositor decides acceptance; a sent request is not ownership.

The clipboard allows deferred publication under that explicit gesture, even if the key
is no longer pressed, only if permission/focus are still valid. DragPermit is different:
the serial of a real button press + a button/implicit grab still held over its own
surface; neither keyboard_enter_serial nor a keyboard gesture authorizes start_drag.
If the Source is not ready yet on release, cancel the drag before starting it, without UI I/O
nor inventing a press. Motion/threshold does not replace the original press's serial.
Do not publish URI metadata before the Source proof in this delivery; MIME ownership
does not by itself constitute authority to move/delete anyway.

## Coexistence with FLTK, ownership and offers

Do not replace listeners nor destroy wl_data_device/wl_data_source/offer proxies
that belong to FLTK. Create **our own** data_device from the borrowed manager/seat; a listener
only on that proxy and on the offers/sources created by/delivered to that listener. Display,
seat and manager remain owned by FLTK. If two data_devices cannot coexist
on the host, declare the capability unavailable; never hijack another listener.
No new Wayland connection to reuse a serial from another connection.

Callbacks on the UI thread only record bounded metadata/FDs; no blocking read,
stat nor decode. One own device per process, with an explicit lifetime and without
recreating it on each repaint. MAX 4 retained offers, 32 MIME types per offer, 256 B per MIME type;
overflow invalidates the capability/offer and frees only own proxies. Clipboard
selection and drag-enter are distinguished; monotonic IDs are never recycled nor are they
pointers. An offer epoch/token (window, pane, tab, intent) accompanies each read.
A replaced/cancelled offer, a Closing window, a different generation or a closed
destination discard the response even if it contains valid bytes. The last intent
wins, but an asynchronous slot is not reassigned without ACK/correlation.

A published source keeps bounded immutable payloads until cancelled/loss and the
close of pending sends. Its lifetime is controlled against reentrant send/cancel;
the FD received in send is ours and is closed exactly once. Cancel neither deletes
files nor consumes a Cut. On close: invalidate the intent, cancel/drain the read and
sends, destroy own proxies without touching the FLTK connection/listeners. No join of a
blocked syscall on the UI nor a substitute worker. Transport state/error visible.

## Bounded FD transport before allocating

One active reception and the latest desired one, using the existing AuxIo; not a thread per
offer. A CLOEXEC pipe on both ends, O_NONBLOCK only on our own reader; read/poll with a 3 s deadline and cancellation per
chunk. MAX_WIRE 1 MiB, check cap+1 BEFORE extending the buffer, exact EOF, no
unbounded read_to_end. On exceed/truncation/error/deadline/cancel: close the FD,
respond with an error without a partial document nor an operation. The offer/epoch identity is
revalidated when returning the result, not only when requesting the receive. A read state
without FD/channel/ACK is not confused with an empty success.

Sending is also non-blocking: an immutable payload preflighted before serializing,
MAX 4 simultaneous send FDs, a per-pump quota <=64 KiB; EAGAIN keeps the offset and
a bounded record, EPIPE/cancel closes without a busy loop. No process/daemon nor
unbounded buffer per reader. The joint reservation includes the MIME payload, the received
buffer, Source proofs, review/queue/un-ACKed result and FD scratch within the 256
gate; local caps do NOT prove PSS. No global 60 fps timer nor renders while waiting.

## Formats and paths

- text/uri-list: COPY by default. Do not infer Cut from plain text nor from an isolated URI.
- x-special/gnome-copied-files: exact first line copy/cut, then URIs; an explicit
  format of the same offer. KDE cutselection is only accepted as exact metadata
  of that same offer/epoch, not by combining clipboard from different moments.
- A versioned own MIME type can carry the operation/window identity to avoid a
  double internal dispatch; never trust a foreign process's job ID as proof.

Do not use FL_PASTE URI→text: the pinned FLTK decodes globally and strips file://,
losing boundaries/%0A/%00/hostnames. The adapter receives MIME bytes BEFORE that.
A per-URI codec, percent-decode EXACTLY once; %0A inside a filename is not a
separator and %25 keeps %. Unix raw OsString, no lossy operational UTF-8. file:///
and file://localhost/ only local; a remote host/different scheme/NUL/unescaped query/fragment
/relative path/traversal/overflow rejected before I/O. A '#' at the start
of a line is only a uri-list comment, not permission for fragments. CRLF/LF separate
URIs BEFORE decoding. A URI is never opened nor navigated while parsing it.

MAX 128 roots, 4096 B per decoded path, 1 MiB wire and explicit joint metadata.
A borrowed preflight of count/lengths/checked sums before Vec<PathBuf>. Raw parents
grouped with Source proofs, at most 32 parents per delivery; if exceeded, a visible
rejection to reduce the batch, without truncation nor a silent recapture. The limit does not
reduce the future goal of multi-source operations. Each effective Copy/Move FD
is validated against its signature group, not verify(path)→a substitute capture.

The destination PathBuf/pane ID/tab ID/intent is pinned at Paste/drop; a late callback does not use
the pane that is active now. The Source is captured in AuxIo before the review; the paths of an offer
are untrusted input, not an authorization to overwrite nor delete. The existing M28 dialog and queue,
conservative conflict/cancel/ACK, a refresh of both folders through the
captured paths, without duplicating engines nor a Model per offer.

## Visible DnD and move agreement

A drag over a row/batch captures the raw visible selection; a movement threshold avoids
stealing click/range/double click/header grab. Pane/folder/background targets by
ID+generation, clear highlight/reveal and a non-mutating Escape/leave/stale drop.
Drop opens a review (destination at the gesture) before enqueuing. No move on hover.

Sending bytes or receiving dnd_finished does NOT allow unlinking the source. files as the source
never runs a delete because of that event nor because of a MOVE action a third party sends.
Avoid a double delete if the destination already renamed; Cut only consumes own publications
confirmed by the engine/IntentProof/ACK, not the absence of the name. The first external
capability is announced as COPY only; external MOVE stays explicitly pending
until a verifiable agreement that does not duplicate the deletion. Do not announce MOVE and then
simulate it as a successful Copy. Internal DnD between our panes can offer
Copy/Move through the review and the global queue/proofs, without source-side auto-delete.
If an external source requires only MOVE without COPY, reject the drop with an explanation.
This does NOT turn a GNOME Cut imported through the clipboard into a Copy: an explicit cut from
that offer keeps the Move intent and opens review/proof/queue; if it is not admitted,
it is visibly rejected. DnD negotiation and the Cut clipboard are different channels.
Publishing a Cut does not authorize files as the source to delete when serving bytes, cancelled
or dnd_finished either: another manager may have done a rename; no second delete. Without its own
verifiable ACK, the private buffer does not infer consumption from the absence of names.

## Layered acceptance, without metrics

1. Pure codec: LF/%0A/%25/non-UTF-8/localhost/remote host/NUL/invalid URI, the
   count/path/wire boundary, the explicit Cut format and cap before alloc. No clipboard read.
2. Private transport with pipes: chunks/EOF/EAGAIN/overflow/cancel/deadline, offer
   change/epoch/close/ACK/reentrancy; no claim about the compositor nor OS input.
3. Product bridge: own data_device/listeners, real permissions/serial/focus,
   exact published/received MIME with another authorized private consumer, without
   reading the earlier clipboard. If the native driver is unavailable, keep it unvalidated.
4. UI extending the existing scene: raw source/destination, focus/tab changed before the
   callback, DnD between panes and folder/leave/cancel, non-mutating review/paused,
   conflict/retry and root 0 ACK, menu 800×440. Injected handlers ≠ OS event.

Real captures and auto-installation only for a validated checkpoint. O06 is not complete because of a
codec/pipe or a private buffer; external evidence is never invented from fixtures.
Other OSes will have explicit adapters; Linux-only is not a Windows/
macOS build. No more previews nor resources/startup while the freeze is in effect.


## M29a — implementation and partial validation (2026-10-27)

Integrated Rust codec without new dependencies: two passes before materializing,
raw Unix and a single percent-decode per URI. A C++ bridge over FLTK 1.5.23 with its own manager,
data_device, sources/offers and listeners on the borrowed display/seat. One-shot sources
and Fl_Widget_Tracker; no foreign listener, new connection nor data-control.
The «solicitud enviada» (request sent) status does not prove ownership/consumption. The permission keeps the real
serial and focus of the gesture; it is never rebuilt at ACK time. Losing the window invalidates the
native epoch BEFORE the Model borrow; reading_epoch prevents a reentrant acceptance.

The existing AuxIo performs reception/capture/encode. A slot keeps a tombstone
until ACK; a queued reader expires and closes its FD from the pump even if AuxIo is busy.
The reader worker uses a 20 ms poll, a total 3 s deadline/cancel/cap+1 before extend. An external sender
receives a blocking write FD; own sends NONBLOCK, quota 64 KiB/pump, deadline 3 s
/inactivity 1 s, four slots. No MIME read through FL_PASTE nor read_to_end.
Destination PathBuf/pane-tab/generation captured; a changed offer/close discard.
An imported Source enters the existing review labeled System with the FD proof;
it does not consume a private Cut with the same ID. The budget rejection notice is kept.

LOCAL reservations, not a PSS gate: two C++ payloads of up to 1 MiB (sends share
views, they do not duplicate the payload); one Rust flight with a wire up to 1 MiB, encode and import
mutually exclusive. Before the Source capture the wire is freed; decoded paths
<=128×4096 B plus containers, freed before retaining the Source. One native
source/review <=2 MiB and an earlier system-recovery <=2 MiB are admitted jointly <=4 MiB;
outside the Request they are a separate reservation, NOT part of the private 2 MiB cap nor of the supposed
4 MiB queue. When enqueuing, the Request already charges lease.retained_bytes() and its FDs. 8 additional
external FDs are reserved: up to 4 sends, the reader, up to 2 Source scratch and a margin for
a transient pipe writer. The existing global cap of 256 FDs still counts review/active/
queue/ACK; limits are not multiplied when keeping another result. These caps do not prove
the tree's memory nor latency/CPU. No new transfer helpers/threads.

Explicit limits: the first import admits ONE common parent and rejects EVERY
multi-source batch before capture (the 32-parent goal is still pending); KDE without GNOME is
rejected for lack of a correlated Cut read; other OSes pending. External DnD
NOT implemented: the pinned FLTK backend calls set_actions and does a roundtrip even with the
handler returning false, besides its unbounded global URI→text when accepting a drop. Returning
false avoids our FL_PASTE path but does NOT prove a coexisting negotiation.
Next adaptation: an opt-in hook for own windows BEFORE the backend's enter/motion/drop,
with cleanup/leave/offer ownership and the default intact for other windows;
a reproducible source/patch in the repo, never editing the global registry.

637 Rust tests (208 core/67 helper/362 UI), 3 ignored; Clippy all-targets -Dwarnings and
fmt --check exit 0. They include codec 5, reader 3 and state 5: offer/destination/owner/close/
cancel, queued expiry EPIPE before take, a visible budget rejection, system Cut
without private consumption and focus loss. State uses injected callbacks, not
Wayland/C++ events; that distinction is kept. The first C++ reading_epoch FAIL is preserved.
Release exit 0 and scene clipboard-ui 1793133638367792338 scoped PASS 9 PNG/2 root 0 ACK:
button/rejection without a gesture and the private M28 regression; isolate-input does NOT enable the bridge.
Validation/hashes in native-transfer-checkpoint-validation.json. It does NOT claim working OS MIME,
complete DnD/KDE/multi-source nor PSS/startup. Previews/freeze intact.


## M29b — receiver and partial DnD hook (2026-10-27)

An opt-in hook for own windows in FLTK 1.5.23 before enter/motion/drop: it avoids
the upstream negotiation/roundtrip/read in that sequence; it frees only FLTK's own
offer on leave/drop/unregister and suppresses late events until enter.
Other windows keep upstream behavior. `vendor/fltk-wayland-transfer/` contains the
original/modified source/patch/hash/COPYING under the FLTK LGPL license + exception.
CMake replaces one TU with a hash verified at build time, without writing the registry.
The guard validates the inputs and requires a strong symbol tied to the digest at the final link;
if the inputs are updated, the wrapper and toolchain must be versioned according to the README.

The own receiver announces COPY/URI, coalesces hover, pins pane/tab/generation and the
raw destination before reading, uses a reader bounded by AuxIo and the existing review/queue.
A borrowed target does not change focus nor selection; on ACK it revalidates pane/
column, revision, name and filter. Cancel/stale/budget open no dialog nor
finish. Drop marks the drag inactive and the offer stays pinned until ACK/cleanup;
a late source_actions does not renegotiate. An obsolete epoch rejects before the pipe. The ACK of a
COPY of accepted bytes never authorizes deleting the source; MOVE is not announced.

C++ wrapped-proxy receiver PASS and 14 targeted Rust tests + 642 workspace PASS;
Clippy/fmt/release exit 0. Scene clipboard-ui 1793135944759735086 scoped PASS
with a private pipe/fake ACK: hover, cancel, review, raw copy and 2 normal closes.
It does NOT prove the handshake, serial, offer, external DnD, memory nor OS input.
M29/O06 pending: proven sender and internal/external DnD, correlated KDE,
grouped multi-parent and OS adapters. Do not enable data-control to get around focus.


## M29c — partial DnD sender, 2026-10-27

A COPY sender on its own window: it keeps the permission of the original press, waits for an
8 px threshold, captures raw names on press and verifies the Source/encodes in
AuxIo. Before requesting `start_drag` it checks window/seat/epoch/original serial,
expiry and the left button; releasing or cancelling invalidates before
taking the Model. A multiple selection keeps its batch when dragging and a click
without a drag is resolved on release. The joint reservation includes the desired
clipboard, proof, payload and published source; it is held until ACK or cleanup
(also bounded by a deadline). The source announces only COPY; `finished` or
`cancelled` never delete files nor consume a Cut. The own drop reuses the
existing review and queue.

648 workspace tests (208 core, 67 helper, 373 UI; 3 ignored), Clippy/fmt/
release PASS. A C++ fixture uses simulated proxies: COPY/serial/surface in an
already admitted helper, permission/epoch/deadline rejections and `finished` marks cleanup; it does not
prove the compositor's grab/focus nor a complete teardown. Scene
`clipboard-ui-1793136958438850356.json` bounded PASS: 13 framebuffers, real Source
and AuxIo, a simulated final dispatch, Cut kept and two closes with ACK.
There is no OS input nor proven Wayland interop; O06, KDE, multi-parent and adapters
remain open. The first scene `clipboard-ui-1793136933634122245.json` FAIL:
pasting own text after the event loop did not arrive. The PASS repeat neither explains nor
erases the failure; a stable global clipboard is still not proven. Resources and
startup NOT_MEASURED (freeze); new previews paused.


## M29d — correlated KDE, 2026-10-27

The KDE import requests `text/uri-list` and `application/x-kde-cutselection`
from the same offer and epoch, with a single ACK after validating intent, Source and
destination. Two pipes are drained concurrently with a common 3 s deadline; the URI list is
limited to 1 MiB and the metadata to exactly one byte `0` or `1` with EOF. An error,
cancellation, limit or offer change rejects everything without turning Cut into Copy.
The bridge's per-phase maximum is 8 FDs: four sends plus four transient
ends; after opening, two readers remain, which are closed before capturing the
Source. Other workers keep independent reservations.

652 workspace tests (208 core, 67 helper, 377 UI; 3 ignored), Clippy/fmt/
release PASS. A C++ fixture with simulated proxies verifies two MIME types of one offer
and rejection when the selection changes between EOFs. Seven transport tests include
both orders, invalid metadata, the limit, the deadline and cancellation after the URI
EOF. Scene `clipboard-ui-1793137466717410666.json` bounded PASS: two private
pipes, a simulated ACK, Cut opens the Move review, cancel keeps the source, two
framebuffers and exit 0 after ACK. Real KDE input/interop are still UNVALIDATED;
multi-parent, real DnD and O06 remain open. The intermittent text FAIL
`clipboard-ui-1793136933634122245.json` remains unexplained. Resources and
startup NOT_MEASURED because of the freeze, new previews paused.
