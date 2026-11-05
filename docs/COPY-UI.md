# Local copy with UI (0.0.4 in validation)

F5 in listings or the inspector's «Copy to…» opens its own form with an absolute destination,
a lossless selection and an option to start paused. Enqueue admits eight pending
jobs and one active; a toast with byte/entry progress, pause/resume,
cancel active and clear pending. No conflict overwrites a destination.
Errors keep the counters of partial copies, rescue paths and
cleanup visible before the usual detail. A success does not force closing a modal.

Borrowed preflight before cloning names or building paths. The controller
charges the retained capacities of requests, up to 4 MiB including a reservation of
2 MiB derived from the current report producer (paths 4097 B, errors 1024 B, sets 128).
It is not a claim about total PSS: the engine's auxiliary record is limited
separately to 16 MiB by default/32 MiB hard, plus the 256 KiB buffer and UI/OS memory.
The interface keeps a single complete result and frees it before dispatching
the next copy; there is no unbounded history of reports.

Visual report at most 64 KiB and explicitly truncated; the complete operation is kept.
«Copy full report» formats it on demand, at most 8 MiB, and rejects a
truncated version. That action may coexist with the temporary String, FLTK's CString
and the clipboard; it is not included in the controller's 4 MiB nor in a scene that
does not exercise it. Real clipboard publication requires Wayland focus and is still
pending along with the Space case; invoking a handler does not prove the OS clipboard.

Every result is accepted with an ID and ACK. The queue does not dispatch in the same
pump that delivers a result, so it can be stopped for rescue. Closing
cancels the active and pending jobs and keeps pumping until the worker is reaped;
Drop is not evidence of a safe close. A blocked FS syscall is not worked around by
creating new workers. Cancel does not unpublish roots that already finished: their
counters/paths are shown. The source is never deleted.

When roots finish in the visible folder it reloads asynchronously, keeping
filter, scroll, layout and a lossless snapshot of selection/focus. Two completions
before the reload reuse the snapshot. While it is pending, selection by
click/range/keyboard is blocked and the bar says so. The snapshot admits
100k names and an 8 MiB budget before cloning: if exceeded, it keeps the listing and
selection, shows an error and lets you reduce the selection and retry with Ctrl+R.

This implements local copy, not moving nor cut/paste of files, DnD,
durable resumption after a failure, a crash journal, replace/merge resolution,
ACL/xattrs nor the whole O01/O02/O03 scope. rwx permissions, timestamps and symlinks are
part of the engine; see its tests and the isolated comparison in COPY-BENCH.

## Reproducible checks

- `cargo test --workspace`, Clippy all-targets with warnings as errors and a release
  build: they must match the current sources, not the earlier suite.
- `python3 scripts/copy-ui-check.py --capture`: real framebuffer and internal
  handlers on its own fixture; data, SHA256/mode/mtime/symlink, no-clobber,
  queue/pending cancel, reloaded destination and exit 0 after an exact ACK.
- `python3 scripts/copy-ui-check.py`: the same scene without captures or clipboard;
  observed PSS+SwapPss tree, steady after copy with time limits, CPU,
  FD/threads/processes and PID/starttime cleanup. Goal 50 MB decimal, ceiling 150 MB.
  It does not replace the 10k/1080p baseline nor represent copies of 100k-entry trees.
- Throughput against cp in a separate run: COPY-BENCH.md.

171 Rust tests (73 core/8 helper/90 UI), Clippy all-targets and release build PASS.
Functional capture PASS in copy-ui-1793025091272455448.json: a real conflict between
different folders, visible controls, cancellation after 3120 files in staging and
exit 0 after an exact ACK. The first capture is kept: the toast covered the controls and the
conflict was a copy onto itself; it is not equivalent to the corrected evidence.

copy-resources.json PASS **sampled with an exit gap**: peak 34671 KiB =
35.503104 MB = 33.8584 MiB; steady 32542 KiB = 33.323008 MB; observed Swap 0. At most 19 FDs,
6 threads and 1 readable process at the same time; one child identity from startup exited during
smaps with no memory available. It is not counted as zero and lifetime coverage is not
promised. 125 samples, maximum interval 104.188 ms (10 ms requested). Post-copy CPU
median 0%, maximum 118.82% of one core: it is not settled idle. Observed PID/starttime
with no survivors, normal close after cancelling a paused job and ACK.
In this run, active cancellation after 3280 files/57930 logical bytes cleaned
the unpublished root; it may use reflink, userspace transfer is not claimed.

Fixture: 16 MiB + a directory with 10000 small files + a symlink; three roots and a visible
listing of 3/4 entries, not 10000 UI rows. Geometry 1000×650 logical/2000×1300 physical.
Hash files d8f2934638a56d069428e55a2209948db1df1a562c1eef8a012dd010e4059c40;
helper 9a4719bba08f63e5b7e2918cd896a8ee4a74684de75d638ce9c2a4e8a152ec2d.
New 10k/1080p baseline FAIL 56.743936 MB and startup FAIL 151.978267 ms maximum;
no global PASS nor 0.0.4 tag while Space/other gates are pending.
The initial suite/Clippy failures are kept in evidence/0.0.4/copy-initial.

Post-checkpoint UI correction: measured Nerd text width now determines the Cancel
button width (short label only if narrow); reports say «Bytes lógicos copiados»
and translate clean cancellation/destination-exists causes. Updated framebuffer
copy-ui-1793026558872068261.json PASS; no repeat of the already approved resource
scenario for these label-only changes. Its hash differs from the earlier measured
candidate; the 35.503104 MB figure remains attributed to the exact earlier hash.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Options in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925 Rust 319/85/521 + 3 existing ignored, Clippy workspace/all targets -Dwarnings/release/fmt
PASS. Native 1793457917645506527 UI/copy/mount PASS 16 framebuffers incl. 440² and
1000×560; 4 copied roots (raw 0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expected FAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves 57 partial
files and original 3000 files intact. 24 logged PID/start identities gone/0 errors/survivors,
not lifetimes; 3 roots exit 0 after normal callback. Native default copy/overall FAIL
exit 1 retained. Final copy-names UI 1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visual FAIL preserved + sidecar;
explicit bounded lines fixed and final framebuffer inspected. No source delete,
automatic removal of visible partials, power loss/native input/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.

Whether another review is available while the queue is active, and the progress geometry with MC, are detailed in QUEUE-REVIEW.md. F5 in Space recalculates; the palette and the inspector keep Copy as an explicit action.
