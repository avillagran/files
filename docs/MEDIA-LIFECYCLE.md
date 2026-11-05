# On-demand media: identity, invalidation and cache — 0.0.4

Explicit user priority, 2026-10-26. After closing Help, previews and
visible thumbnails come before more peripheral features. The existing image
helper/IDCT, FFprobe/FFmpeg video, binary pipe and latest-wins controllers are reused.
The resource/startup freeze continues: the real budgets of this unit are NOT_MEASURED.
This document distinguishes the target contract from units not yet implemented.

## Identity and publication

Key: the **raw** absolute path (no lossy UTF-8 conversion), the resource type
(reduced image/video frame and size/position), dev+ino identity and the
size+mtime_sec/nsec+ctime_sec/nsec revision. Atime does not invalidate because of our own reads. Only
regular files, FD O_NOFOLLOW/NONBLOCK/CLOEXEC. O_NOFOLLOW only protects the
final component: parent symlinks and mountpoints are followed; it is not
path confinement nor a sandbox. Other OSes need an adapter;
zero dev/ino values are never fabricated. Identity alone is not a hash nor a content snapshot.

The FD that is read/decoded must match the expected signature, stable before and
after reading/decode. Before publishing, the name is also revalidated without following the
symlink: replacing with the same name does not reuse the old object. A signature
taken earlier in another open is NOT enough to certify the decoder's real FD.
Any mismatch frees the RGB/frame and delivers a cancellation/error, not old media.
The checked image IPC sends expected_revision and Response::Image returns the source of the
real FD. The UI requires a match with expected and its generation before Ready;
video snapshot/stream applies the same contract in its worker with a pinned FD.
Paths stay raw in the model/probe. RAW-PATH-WIRE (2026-10-27) adds
compatible Unicode strings and Unix bytes to Request; a full image with an opaque root and
name was verified. Windows/macOS adapters and other surfaces are still
pending.
On systems with coarse timestamps or concurrent changes there is no guarantee of an
atomic snapshot; observed events can never let a stale result be
accepted by a later generation.

## Events and reconciliation

One non-recursive watcher per active directory, not per file. At most the
parents of the visible folder and the active preview; bounded events and overflow/error
conservatively invalidate the set, without accumulating paths. Attaching a watch and
probing FD/metadata happen outside the UI, through a reusable worker and a
single pending destination/request; a blocked syscall does not spawn another thread when navigating.

The monotonic epoch is never cleared when a probe finishes. Callbacks/results
include a destination token; the watcher is attached BEFORE opening the source and
Verified is only published if token/epoch stay stable. The UI checks the epoch again
before accepting and before drawing (only atomics/slots, no I/O). A visible reconciliation
every 1 s and re-attaching the parent before probing cover parent rename/replacement
and lost events; without an event, instant invalidation is not promised.

Event received: immediately discard the source's image/frame/UI, cancel the
active generation and drain/reap before another decode. Reconcile on open/reuse,
and in a bounded way for a visible source if events are lost. Deletion shows an
unavailable state, modification reloads on demand, replacement changes the signature.
Own operations invalidate after ACK and before refresh; rename/move invalidates
source/destination, without migrating by name alone. When navigating, the previous directory is
disconnected and the last destination wins. Embedded Markdown resources need to integrate
their parents/signature per resource; they are not considered covered by the document's watcher.

## Demand, LRU and single helper

Grid only requests visible indices (selection first), never a full readdir
to preload images. The inspector only requests the selection; Space uses the
large preview and video plays/seeks with the existing backend. The initial video thumbnail
is one frame, not a playback loop in every cell. Leaving
Grid/hiding the inspector/closing Space removes demand and cancels unused jobs.
Only one active helper/decoder across previews/du/treemap/thumbnails; the next request
waits for ACK/reap, and resource use is not hidden in parallel children.

Goal of the first cache: shared RAM, global retained RGB 4 MiB, at most 32
entries and 256 KiB of keys/metadata; thumbnails ≤160 px, current large preview ≤1024 px.
It counts the Vec's capacity (including the drained wire), not just len. One owner with
borrows shares Grid/inspector in M2, without cloned RGB nor external pins;
a resource pinned by the UI is still charged even if it is removed from the LRU. Before the helper
the maximum allowed transient is reserved and unpinned entries are evicted; if it does not fit,
it waits for/frees another resource; it does not decode first and reject afterwards.
The viewport scratch and the decoder/helper bytes are recorded separately: these limits
do NOT prove the tree's 50 MB nor 150 MB. Do not raise the ceiling nor cut features to
placeholders to turn an ideal goal into a PASS.

## Disk, temporaries and close

First new unit: RAM/pipes only, with no persistent cache nor files
next to the original. The current image/video use buffers/pipes; PDF uses a process/pipe
with an open FD. Evidence captures belong to the harness, not a product cache.
Without its own disk cache, no collector of other apps' files is invented and
other apps' ~/.cache is not deleted. The test must verify that no product
files are created after success/error/cancel/close. Close asks the readonly worker to stop without a join on the UI: a syscall on a mount
may be impossible to interrupt. Whether the ACK arrived is recorded; if it did not,
process termination asks to recover its thread/FD and that is NOT a drained
probe. A task in an uninterruptible kernel state can delay even
process termination: no offline close deadline is guaranteed and no
replacement worker is created to work around it. This
does not relax the ACK/reap of external helpers before exiting.

A crash frees RAM/FDs; no new thumbnails are left
on disk. If disk is added later: a private 0700 subdirectory,
a manifest by identity/signature, exclusive temporaries, atomic publication and quota/TTL,
cleanup of orphans at startup and staging recovery after a crash. Instant deletion
while files is off is never promised.

## Deliveries and observable acceptance

1. **M1 implemented/verified on Linux, selected scope**: FD signatures and a cancellable
   watcher/reconciliation for the selected image/video preview; hide old media on delete/modify/
   replace/rename, including own changes, without a new persistent cache.
2. **M2 implemented/verified on Linux in Grid/inspector**: visible Grid + inspector thumbnails, global LRU/tickets
   and prior reservation, existing engines with reduced output and a cache shared with
   Space. MEDIA-THUMBNAILS documents the real scope: RGB shared by Grid/inspector,
   LRU 4 MiB/tickets and exclusion when Space opens; a cache across documents and all
   global buffers/formats are still pending. Do not count icons as thumbnails.
3. **M3 pending**: tests of quotas/pinned entries/lost events/fast
   navigation/close/error and orphans according to the storage actually used;
   stale images/frames are never accepted by a new generation.
4. **Measurement pending because of the freeze**: the tree's PSS+Swap, peaks/steady/CPU/latency/
   release and video without simultaneous captures; GPU separately if applicable. The historical FAIL and
   UNVALIDATED results remain intact, with no global PASS nor 0.0.4 tag.

Own fixtures: external deletion and from files, the same path replaced keeping
mtime/size, in-place modification, rename/move, cancel A→B→C, active close, a revision
lost by the watcher/overflow, LRU by capacity and pinned references. Framebuffers
with identifiable colors before/after, clipboard/OS input separate, install
only a validated functional checkpoint with backup and consistent app/helper/HTML SHAs.

## M1 evidence — 2026-10-26

410 Rust tests (132 core, 45 helper, 233 UI), Clippy workspace/all-targets with
-D warnings and release clean. Deterministic tests: Verified(A)→replace(B) before
the helper's open rejects A; a late Ready(A) after dirty/Verified(B) is not published;
callbacks/results from old destinations do not change the new epoch. A short mutex
serializes token/epoch against the callback; it never contains I/O. Real inotify tests
replacement, deletion and a change of the parent's identity; probing/reattach happen outside the UI.

`evidence/media-lifecycle/media-lifecycle-ui-1793064138443049803.json`: bounded
functional PASS, four real framebuffers 1000×650. Red→blue with the same name/size/mtime,
green modification, deletion, rename away/back, a replaced parent, A→B→C, video
snapshot/stream and deletion during playback. Hiding the preview launched no other
helper during the reconciliation interval; Trash hid the media before
mutating and confirmed ACK success/Undo. Root exit 0 after ACK; 13 registered helper
PID/starttime identities no longer alive. The readonly probe's ACK was true in this run;
it does not test offline/stuck kernel nor full temporal process coverage.

The first report 1793064084657403710 stays FAIL intact: the instrument omitted the variable
that enables the ACK marker and expected the original name in Trash (it uses a unique name).
The sidecar explains the fix; the emergency cleanup of that FAIL is not a lifecycle
PASS. The RGBs were checked in State against the pattern and the effective revision, in addition
to capturing the framebuffer. Internal handlers, not OS input. The main PSS footer does not
approve a budget; tree/Swap/CPU/startup NOT_MEASURED because of the freeze. No product
cache created in the fixture's source/config/data; Trash keeps the original because of
Trash semantics, not as a cache. No audit of the whole filesystem.

Pending: Grid/inspector thumbnails, global LRU cache/tickets, embedded Markdown,
raw-path image IPC, other OSes, minimization/OS input and offline recovery with a
blocked syscall. P02/P07/P09 not complete, no global PASS/0.0.4 tag.


M2: see MEDIA-THUMBNAILS and scene 1793069315038209135: real thumbnails, raw IPC,
reconciliation with notify suppressed, 45/2/10 helpers gone, closes 0/0/1 (expected
backend loss). The earlier WidgetTracker panic has an open cause; guards against a
removed surface verified with own destruction, not the compositor. 423 Rust/Clippy/release,
no new metrics; global P09 and native input remain open.

2026-10-27: RAW-PATH-WIRE verifies CLI args_os/IPC/full raw Unix image,
427 Rust and scene 1793071898429792393 PASS. This fix replaces the pending
raw image IPC item from the M2 delivery; not the FD signature of embedded images.

2026-10-27: RELATIVE-SOURCE adds the effective root/text signature and a checked probe→decoder
for embedded Markdown. A replacement between probe and open is rejected before
reading; root/name/file are revalidated after decode. It does not yet cover changes after
publishing nor all of their parents: watcher/reconcile for embedded resources is still pending.

M4 follow-up 2026-10-27 DOCUMENT-MEDIA: the existing worker accepts the MD manifest,
watch before the final probe and a hidden candidate until token/epoch are verified.
Absent intents without RGB and a 1 s reconciliation recover recreation/root; 442 Rust
and scene 1793076116463397564 PASS 2 instances with/without callbacks, 5 captures/26 helpers
registered per instance. Readonly stop false/true kept separate from the helper ACK; NO UI join,
no fake drain. Resources/startup NOT_MEASURED; global P09/other formats pending.

## Selected audio — 2026-10-27
AudioJob adopts the same watcher/token/epoch/reconcile, expected FD Revision
and the effective signature on Ready. It rejects probe A/open B and a late publication of another
original; audio is not added to thumbnails nor a playback engine/cache.
AUDIO-PREVIEW records 458 Rust/Clippy/release and audio-ui 1793083931390323451 PASS
with callbacks suppressed, replace/delete/rename/parent and 11 logged helpers gone.
The ReadOnly/Helper close gate separates exit permission from ACK; no UI join nor
future spawns after closing readonly. Real kernel D-state/offline not yet tested.
No parent confinement nor atomic snapshot; resources/startup NOT_MEASURED.


M12 (2026-10-27): the selected PDF adopts a mandatory expected Revision, the real FD
shared by pdfinfo/pdftoppm, intermediate/final revalidation and Bitmap.source. Watch
before probe/epochs/reconcile are reused; a typed source flag forces a new token.
473 Rust/Clippy/release and pdf-ui 1793092913810767063 functional PASS with callbacks
suppressed/replace/in-place/delete/parent recovery/23 logged helpers gone/close 0 ACK.
readonly_gate shared by audio/PDF: readonly exit permission is not equivalent to a drain; no
join/UI nor new workers for a stuck syscall, no kernel D-state test. PDF-PREVIEW details
the limits; HTML and the other previews do not yet inherit this contract automatically.


M13 (2026-10-27): the selected HTML uses expected/effective Revision, an FD inherited
by the adapter instead of reopening the path, and the shared readonly_gate. Ready requires
generation/signature, dirty/error removes the display list. 476 Rust/Clippy/release,
fd-contract 1793093345381869451 and html-ui 1793093483630467339 scoped PASS: notify
suppressed/replace-in-place-delete-parent recovery/14 logged helpers gone/close 0 ACK.
HTML-ENGINE-PROBE details the limits; text approximation/resources/links/parser
are not complete with this unit. Freeze NOT_MEASURED intact, no disk cache.


M14 PDF text uses the same FD/gate/watcher and mutually exclusive outputs on demand.
The stable PDF/HTML size capability NO longer emits the SourceUnavailable/reprobe flag:
a terminal error until a new revision/reopen, avoiding a worker/token loop.
480 Rust/Clippy/release and pdf-ui 1793094189643854652 scoped PASS. No new measurements;
PDF-PREVIEW details extracted text/selection and limits, not PDF geometry/OS clipboard.


M17 VIDEO-SOURCE-CLOSE: the selected snapshot/stream add the shared readonly_gate
like audio/PDF/HTML, mutex admission against closing and readonly permission ≠ worker ACK.
487 Rust/Clippy/release and simulated hold 1793099666110053045 PASS 2 cases/2 FB/exit 0 with
DRAINED false/no late admission, NOT real kernel D-state. GIF regression 1793099692852727198 PASS
6 FB/10 helpers gone/active close ACK 0. A Helper phase with a blocked revalidate is not resolved,
no promise of instant D-state termination, thumbnails keep the earlier supervisor.
Freeze NOT_MEASURED; the source doc details the scope, no global P09 nor native Super+W proof.

## M18 — video and GIF zoom/pan, 2026-10-27

VIDEO-ZOOM: the same Viewport 1–8×/Ctrl+wheel/+/-/0/drag, frames invalidate pixels without
resetting the transform; seek frees the RGB and keeps 3 floats, a new source/revision resets.
488 Rust 141/58/289 + Clippy workspace all targets -Dwarnings/release/fmt PASS. Scenes
video-play-ui 1793101116101351845 (GIF 11 FB/12 helpers) and 1793101152229152975 (MKV 9 FB/10)
PASS 1000×650/zoom 2 pan 48/36/kept across frames-paused-seek-resume EOF/reopen reset/
active close 0 ACK/logged PID/start gone Z false. Internal handlers, no OS input/clipboard/
2x sharpness/lifetimes, no photographic detail proven by uniform colors.
No 24-frame clip nor PSS/Swap/CPU/startup because of the freeze; P07/P09 still partial.

## M19 — visible preview navigation, 2026-10-27

PREVIEW-NAVIGATION: Nerd previous/next buttons + Alt↑/↓ share an Action,
visible order/hidden/filter; no UI I/O/preload. An end keeps state/revision;
a real change reuses selection/generation/cancel/drain and the last destination.
488 Rust 141/58/289 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-neighbor-ui 1793101665648168454 PASS 5 FB 1000×650/PNG-WebP-raw PNG 0xFF/
hidden excluded/FD source and pixels/real error + back/rapid last wins/callback 0 ACK/
7 logged helper IDs gone Z false. Hit target/accessible name verified; no OS input/
AT-SPI/clipboard/lifetime coverage/active helper close in this scene.
Resource/startup freeze NOT_MEASURED; no playlist/autoplay nor P09 completeness.

## M20 — non-destructive viewer rotation, 2026-10-27

PREVIEW-ROTATION: Nerd/R/Shift+R buttons only rotate the image/PDF/video/GIF view;
direct sampling of the original RGB, with no rotated copy nor modified files.
The numeric transform keeps the rotation across frames/seek and PDF pages/text-return;
a new source/revision/reset 0 removes it. F1 Help adds the real media shortcuts.
489 Rust 141/58/290 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-rotation-ui 1793102443334515413 PASS 14 FB 1000×650/asymmetric 4 quadrants with
checked framebuffer pixels/CW-CCW-reset rotation/video-GIF play/seek/PDF page 2 +
text return/original bytes intact/callback 0 ACK/21 logged helpers gone Z false.
No OS input/native 2x/AT-SPI/lifetimes/active helper close in this scene nor a test of
photographic color; 14 functional readbacks, NOT an isolated measurement of use.
Resources/startup NOT_MEASURED freeze; no global P02/P06/P07/P09 nor 0.0.4 tag.
