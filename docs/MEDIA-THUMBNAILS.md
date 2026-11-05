# Visible thumbnails — 0.0.4 in development

Grid and inspector show PNG/JPEG/WebP thumbnails and an initial video frame.
Only the selection and up to 32 visible sources are requested, never the whole folder.
A non-UTF-8 Unix name travels as bytes, without lossy conversion. The inspector's
thumbnail is the same entry that Grid draws; clicking it opens Space. Video
plays with controls in Space, not in each cell. There is no new disk cache.

## Pipeline and local memory

One reusable worker with one active request/result: open FD/revision outside the UI,
reuse the bitmap if it matches the earlier revision, or invoke the existing engine
with expected_revision and check the effective source. Images use the shared helper;
baseline 12/24 MP JPEG reduces the IDCT first and samples to 160 px afterwards. Progressive images
that exceed 32 MiB of coefficients are still rejected: P02 stays partial.
Video shares the existing snapshot's FFprobe→FFmpeg and scales to 160 px; no shell,
no network, no nested processes. No new decoder nor new dependency/license.

The non-recursive parent watch reuses the MEDIA-LIFECYCLE worker: source mode
for Space and directory mode for thumbnails. Attach before authorizing demand,
and the job's epoch/token and ACK prevent accepting stale results. Any event
conservatively invalidates and empties the RGBs; one FD probe per visible resource is
scheduled when its 1 s proof expires, to cover lost events. Since the flicker fix of 2026-11-04, during a periodic
renewal the last confirmed image is kept as a visual snapshot; it is
neither a fresh proof nor operational authority. Real invalidations
and errors still remove the bitmap. A blocked syscall
does not imply a guaranteed deadline. No symlink_metadata/stat/readdir from draw/UI.
The final O_NOFOLLOW does not confine parent symlinks. Linux verified; the remaining adapters
are pending. A final FIFO/directory/symlink is not read as an image.

Single-owner LRU, accessed by borrowing during draw: no pinned Arcs outside
without charging, and no RGB cloning per occurrence. RGB Vec capacity (including the drained
wire header) + the active ticket of at most 80896 B share 4 MiB; at most 32 entries.
Reserved BEFORE decode and held until ACK/insert or discard. Cache keys
64 KiB counted by capacity+structure; demand 64 KiB checked by references
before clone/join. The sampling viewport is single and reused, at most 76800 B,
explicitly separate from the RGB ledger. Thumbnails are rounded 8 px even with
square corners: a CPU mask with edge coverage applied to the real image,
including letterbox. Painter is still software and creates no widgets per cell.

The cache is freed when demand is hidden/on navigation/when Space opens. Preview, du and Space
wait for the thumbnails to drain before another helper; du is deferred if needed.
Full Space does not reuse this LRU yet: it excludes it to avoid duplicating the
cache, keeps its earlier limits and separate pipeline/scratch buffers.
This does NOT yet unify every format/frame into a global budget, nor prove the tree's
PSS+Swap 50 MB/150 MB. P09 is still partial. Footer numbers
(main PSS) are not used for approval. Resources/startup NOT_MEASURED because of the explicit freeze.

Codec failures stay tied to the revision: revalidating an unchanged file
does not relaunch the decoder every second. Deletion/failure removes its bitmap; a replacement
with the same name/mtime/size needs the new identity. A lost event on
an entry outside demand is detected on reuse; instant deletion
without an event is not promised. RAM goes away on exit; no disk TTL/collector nor sweeping of other
caches. Saving originals in the Trash is a user operation, not a media cache.

The visibility gate uses FLTK's shown/visible_r: it cancels media and changes the token,
makes no new hidden decodes; when coming back it probes again and video returns
paused at the earlier position. Handler tests, not a native minimization test.
A readonly worker that cannot be interrupted may prevent the process from ending in the kernel:
no join from the UI nor new threads to work around it. Before allowing exit in the readonly
phase, a gate forbids any future spawn; the helper phase waits for cancel/reap.
The probe's ACK and the permission to close are kept distinct, with no fake full drain.

## Functional evidence

423 Rust (132 core/46 helper/245 UI), Clippy workspace/all-targets -Dwarnings and release
clean. The cache requires a ticket, counts capacity, limit 32/LRU, oversize rejection,
a late response after dirty discarded, error memoized per revision, preflight of
large input, an expired proof is not Ready, reused worker/ACK and the readonly vs
helper gate. A rounded letterboxed image and a reused scratch are tested with the recorder.

`evidence/thumbnails/thumbnails-ui-1793069315038209135.json`: scenario PASS,
three serialized instances and five real captures 1000×650. First: 7 visible
resources, JPEG 12 MP→160×120, raw filename, RGB shared with the inspector, a video frame,
Space→back, no new decoder in the stable interval, replace/modify/delete/Trash ACK,
folder change and callback with a live helper→exit 0. Second: notify callbacks
deliberately suppressed, FD reconciliation changes red→blue→callback exit 0.
Third: deleting its own C++ widget with a live helper→guard and drain→expected exit 1,
without a panic. 45/2/10 registered helper identities no longer exist after exit;
the observed concurrency 1/0/1 is not full lifetime coverage nor a budget.
Internal handlers and framebuffer, NOT OS input nor a real Super+W.

The earlier FAILs and sidecars are preserved: a Close marker missing from the instrument,
a legitimate ESRCH when the helper exited, a command before the first paint. In that last case
there was also a WidgetTracker panic, removed without a backtrace. Its original cause
is still open; a 20 s idle run with a backtrace did not reproduce it. The new guards do not
use a deleted window/canvas and cancel/drain through the failure path; the fixture
verifies that protection, not the cause nor the compositor. Do not claim that Super+W was
validated. An unexpected backend failure is not durable recovery of prefs/tasks
after a crash: X02 is still pending; operation errors/rescue are printed.

Pending: cache shared between documents/Space, embedded Markdown/HTML,
PDF/audio, other views/tabs/panes, native HiDPI,
minimization/OS input/offline and measuring the tree without concurrent captures/builds.
The earlier global FAIL/UNVALIDATED results remain; no 0.0.4 tag nor global PASS.


Checkpoint 2a4c95e installed by the user with backup and verified hashes:
files 644d3ed6a9dc18a463af9ddb5dbb1fbabae78ac4bfa7a6fb7549c234ba5b60c6;
helper 76d279c3ac70bdbfdae4c8e82b12a27f09ef28808ab73c946dc52002f143b5ff;
HTML a5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
`evidence/0.0.4/installed-checkpoint-1793069978470793539.json`. Known open items
include the original cause of the WidgetTracker panic; the verified protection does not replace its
investigation. No publication nor new tag. Next: extend identity/cache to
embedded resources and solve the raw transport of full images, keeping
the media priority and the rest of the ROADMAP open.


2026-10-27: RAW-PATH-WIRE closes raw Unix full-image transport and CLI
args_os, keeping compatible strings. 427 Rust/Clippy/release and scene
1793071898429792393 PASS with a 0xFE directory + 0xFF file. The embedded and global cache
pending items remain, as does the original cause of the WidgetTracker panic.

## Large visible PNG/JPEG — 2026-10-27

`python3 scripts/thumbnails-ui-check.py --large-codecs --normal-only`: scene
1793082861888227321 PASS with a 24 MP PNG decoded by rows and a 24 MP progressive JPEG with DC 1/8,
reduced to 160 px. It uses the existing engines/FD proof/cache; no duplicated
codec/cache implementation. The snapshot includes source_width/height and format to require
6000×4000/PNG,JPEG before capturing. 4 framebuffers 1000×650, Grid/inspector
share the RGB, replacement/delete/raw/Trash/navigate/Space/active close.
44 registered helper PID/start identities absent/reused, root exit 0; not full lifecycle
coverage. This command skips repeating the earlier notify-suppressed/widget-loss
fixtures; it does not replace them. 454 Rust/Clippy/release PASS.
Local caps unchanged, no new disk cache; resources/startup NOT_MEASURED freeze.
APNG/ICC/large WebP/global P09/other OSes/native 2x sharpness remain pending.


M16 GIF-PREVIEW reuses the first video frame/160 px and the existing LRU. Scene
1793098465429205562 PASS red 160×90 inspector, source 640×360/FD signature/real pixel and a
visible capture after closing Space (the last Space frame was blue). It is legitimate visible
demand, not a hidden loop; a GIF in the grid was not captured, so do not claim visual verification
of that surface yet. 485 Rust/Clippy/release, no resources/startup because of the freeze.
