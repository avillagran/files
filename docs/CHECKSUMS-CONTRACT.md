# SHA-256 observations and explicit comparison (C01)

Status: core/helper/UI integrated; scoped NODE X11/Wayland functional acceptance
passed; installed on NODE with five verified backups/hashes. C01 remains partial: this is selected-file hashing, not a
recursive duplicate finder, reclamation estimate or authority to delete.
The separate folder mode is documented in [DUPLICATES-CONTRACT.md](DUPLICATES-CONTRACT.md).

## User flow

Mark regular files or focus one, then open Acciones → SHA-256 or search `checksums` in Ctrl+Shift+P.
The panel captures the exact directory and selected names without filesystem I/O
on the UI thread. Opening the review does not hash files. F5/Calcular explicitly
starts one batch; F6/Captura replaces an idle capture with the current selection.
An explicit Clear with no marks/focus remains empty; otherwise an unmarked focused
file follows the existing browser operation policy. Directories, final symlinks and
special files are explicitly rejected. Browser selection/query are retained.

A running batch owns its capture regardless of later background owner/path or
listing changes. Browser actions are blocked while the panel is open. Up/Down,
Home/End and PageUp/PageDown navigate its virtual rows. The selected SHA-256 is
shown in two32-character lines so the whole value fits440px. Matching digest and
byte count observations form explicit comparison groups. Hardlinks/aliases are
not a promise of reclaimable duplicate storage.

Cancelar retains only fully decoded observations after the helper's reap ACK.
Cerrar/Escape cancels and closes only after that ACK; the root-window close also
waits. No second batch is admitted before the first ACK. Calcular can explicitly
retry the same captured names after a terminal result. Copy/Ctrl+C requests a
bounded human-readable report with exact escaped directory/names, digests,
byte counts and errors; it is not a sha256sum manifest or native clipboard proof.

## Identity and meaning

`files_core::checksum::calculate` uses the existing relative-source root+file FD
binding. It opens the actual regular file, captures device/inode/length/mtime/
ctime, reads exactly that length in64KiB chunks, rejects extra bytes and checks
both descriptors and current names again before publishing SHA-256. Cancellation
is checked before opening, each chunk and final publication. A change/truncation/
replacement fails instead of publishing a digest. Root identity must agree among
successful observations in a batch. Final file/root components are NOFOLLOW;
root parents may contain symlinks and mountpoints are not confined.

These are individual observations at helper-open/read time. They do not bind the
inode that an earlier listing displayed, create an atomic snapshot of all files,
watch changes after a completed row, or authorize a later mutation. No file is
written, renamed, trashed or removed by this tool. The existing SHA-256 dependency
is reused; there is no new dependency or external shell command.

## Bounds and scheduling

-1–128 direct regular filenames, total captured names at most8KiB; root path at
most4096 bytes. Names must be single normal components, unique, without NUL.
Unix path bytes survive the existing path wire. Actual request must fit64KiB.
- One ephemeral `files-helper hash_files` per batch, files sequentially, one
supervisor/result channel. No worker/helper per file and no filesystem read in
panel paint/event handling. Helpers use the existing96MiB address-space limit;
this is not a measured PSS guarantee.
- At most384 progress frames (128 initial +256 throttled updates),128 item frames
and one terminal Done. UI decoder caps each line at2048 bytes, validates IDs,
index order, monotone progress/lengths, root agreement and the exact final count.
EOF/extra frames/malformed or incomplete protocol do not become a completed batch.
- Shared supervised nonblocking pipe loop has a120s whole-batch deadline and its
existing4MiB output bound. Cancellation kills/reaps before releasing the slot;
uninterruptible kernel I/O is not promised to disappear within that deadline.
- Retained item errors are256 bytes each. Report is at most128KiB, with escaped
names so control characters/newlines cannot masquerade as extra report rows.
- Pending size admission is temporarily paused when Calcular starts; an already
active size helper runs to its normal ACK. The original pending-pause state is
restored on terminal result/cancel. Hash waits for preview/search/map/thumbnail/DU
readers to drain. The modal prevents new browser read tools from being launched.
This is a local scheduling/buffer contract, not a whole-process-tree budget.

## Required evidence

Core tests: known SHA-256 empty/abc vectors;64KiB+1 chunking; pre-open and final-
chunk cancellation; file/root replacement; symlink/FIFO/directory rejection;
traversal/duplicate/name budgets. Decoder tests reject missing terminal ACK,
wrong index, decreasing progress, changed total and oversized lines. UI tests
cover explicit empty selection versus focused-file policy, frozen capture/stale actions, original size-pause restore,
root/panel close waiting for a worker result, and minimum-size targets/raw report.

Functional acceptance passed on NODE with independent Python hashlib wire
reference, opaque root/name bytes and a newline filename, empty selection and
directory refusal with no helper, focused-without-marks capture, review with no
file read,4-file batch/one helper, matching group and full selected digest,
copy-request handler, actual sparse-file Cancel and root close after reader-open,
3logged helper PID/start identities gone/reused per run (Z not accepted),5painted
wide/minimum framebuffers and byte-identical small fixtures. The sparse cancel
fixture retained inode/length/mtime/ctime and was removed only by harness cleanup.
This proves those observed paths, not all process lifetimes or syscall fault cases.

Final receipts:
- `docs/evidence/checksums/node-functional-1793517471420967043/receipt.json` (X11)
- `docs/evidence/checksums/node-functional-1793517472873839910/receipt.json` (Wayland)

1188Rust PASS (416core/95helper/677UI),3existingignored; final8checksum UI/decoder
tests, workspace all-target Clippy-Dwarnings, release and fmt PASS. Earlier test
fixture/signature/style failures are preserved in `evidence/checksums/validation`.
At that checkpoint the Actions menu contained22 entries; palette58 entries, with Search still
first and recovery last. No code was serialized to hide test contention.

Reproduce with: `scripts/checksums-ui-check.py [--wayland] [--installed]`.
Native OS input/clipboard, AT-SPI,2× sharpness, lifetime coverage, other platforms,
resource/startup gates, recursive duplicate enumeration, hardlink accounting and
reviewed duplicate mutation flows remain pending. New previews stay paused;
PSS/Swap/CPU/startup experiments remain frozen.

## Installed NODE checkpoint

Source5da9265, installation record1793518010519338180: app4279419032a9,
helper48e017b4b4b2, existing native HTML04aa7953202d. All five prior artifacts
were backed up and independently SHA-256 verified against the install record;
all five installed artifacts equal the validated release files. No source push/tag.

The installed path passed the same Wayland flow (5PNG/exit0drain/3logged hash
helpers gone/reused), then an extended toolkit-popup check (7PNG/2exit0drain/
4logged hash helpers gone/reused). Existing HTML regression passed5PNG and
exit0/drain with the installed helper set. Receipts:
- `evidence/checksums/install-verification.json`
- `evidence/checksums/node-functional-1793518010567122297/receipt.json`
- `evidence/checksums/node-functional-1793518408960367323/receipt.json`
- `evidence/0.0.4/html-ui-1793518012909603951.json`

Next C01 work: directory duplicate discovery, hardlink/alias accounting and safe
explicit candidate review. Current SHA-256 groups remain observations, with no
automatic mutation or reclaimed-space promise. The full roadmap remains active.
