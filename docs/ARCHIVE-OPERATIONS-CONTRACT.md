# Archive operations contract

Status: ZIP32 review/Apply and queue installed e0dd847; full Z01 remains partial.
Z01 remains open. This does not expand media previews or lift resource/startup freeze.

## Intended product flow

Create and Extract are separate Actions/context/command-catalogue tools, usable
by mouse and keyboard. Each captures source, destination and an immutable reviewed
plan off the UI thread, then requires explicit Apply. Selection/tab changes never
reinterpret an accepted job. Work uses the existing global queue/owner/ACK model,
with bounded retained metadata/FD reservations, pause/cancel and close-drain.

Creation streams regular files/directories and explicitly supported literal links
from captured sources into one exclusive staged archive, then syncs and publishes
without overwriting an existing name. It never includes its own output or removes
sources. Metadata/source changes fail conservatively. No shell command expansion.

Extraction defaults to a **new exclusive child directory**, never merges with an
existing tree or overwrites occupants. The complete name/type/quota/local-record
plan must be checked before destination creation. During execution, source FD/name
proof and target parent proof remain pinned/revalidated; paths are traversed by
O_DIRECTORY/NOFOLLOW/openat relative to private owned staging. No member decides
permissions outside that root. Devices/FIFOs/sockets/absolute/traversal paths,
namespace collisions, unsupported links/encryption/methods fail explicitly, not
silently skipped. Files get ordinary restricted permissions, no setuid/setgid or
privileged ownership. Publication follows all payload CRC/integrity and sync checks.
A failure preserves source and reports retained rescue paths/phase if exact owned
cleanup cannot be proved. Output can contain partial unverified bytes in staging;
a decoder's successful write count is not publication authority.

ZIP creation/extraction starts with ZIP32 stored/DEFLATE support and exact raw Unix
names (UTF8-declared names must actually be UTF8). ZIP64, TAR variants, symlink/
hardlink extraction and additional formats remain in the overall scope and require
explicit format-specific contracts before being advertised. No external tool is
installed globally or used as an implicit shell fallback.

## Shared ZIP foundation

The existing central-directory lister is observational, not extraction authority.
A common member reader must compare local headers/names/flags/method/sizes/CRC with
central metadata, validate bounded extras/descriptors and exact payload ranges,
stream chunks and require real DEFLATE StreamEnd plus exact compressed consumption,
expected output length and CRC. Unterminated empty streams or trailing compressed
bytes are errors. Cancellation is checked around reads/decode/writes; blocked kernel
I/O has no guaranteed deadline. No full member allocation in the streaming API.

The Office adapter keeps its old1MiB output/8MiB compressed limits and uses that
same decoder; its Vec remains private until success. This is a refactor for archive
operations, not a new preview feature. flate2/crc32fast versions/backends already
exist in the workspace lock, with existing permissive dependency provenance; only
a direct core dependency edge is allowed, not a new package/backend/native codec.

The complete extraction plan must reject duplicate canonical names, file-vs-child
and file-vs-directory aliases, dot/empty/backslash/absolute/traversal components,
unsupported types/flags/encryption, overlapping local records and ranges touching
central data. Directory entries must be empty; declared output totals, member count,
path bytes/depth and per-file sizes are checked before any output. Runtime counters
also enforce actual emitted bytes; declarations alone never authorize disk usage.

Initial proposed defaults:10000 central entries,256KiB raw names (existing limits),
4096B path/64components,1GiB total declared+actual output,1GiB per file,32KiB output
and8KiB input chunks. Directory/index/FD/staging/allocator metadata are separate
budgets, **not** a PSS claim. Progress/queue/FD reservations must be settled before
UI admission; no new thread per job or unbounded per-entry retained descriptor.

## Validation and references

Original implementation follows existing files-core/archive_list and the helper
zip_member contract. Primary API/format references, consulted without copying their
text/source into the product:

- https://docs.rs/flate2/1.1.10/flate2/struct.Decompress.html
- https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT

Pure tests must cover CRC, stored/DEFLATE, signed+unsigned descriptors, premature
end, trailing data, quota-before-write, short/interrupted IO, sink failure,
cancellation, long/raw paths, prefix/type/name collisions and overlap. Filesystem
execution then needs byte/identity/no-clobber/metadata/staging cleanup/late ACK
fixtures before visible acceptance. Real UI screenshots come only after wiring;
no claim of an extraction feature based on a codec/plan test or previous ZIP preview.
Physical powerloss, hostile concurrent mount/topology, OS interoperability and resource
budget/startup acceptance are separate pending proof, never inferred from unit caps.

## 2026-10-31 — Validated source foundation, not an installed archive operation

`archive_member` now provides bounded inspect/copy_to and cancellable exact reads.
It handles stored/DEFLATE and signed/unsigned descriptors (including CRC equal to
the descriptor signature), validates local needed-version/name/flags/method/size,
checks extras/ranges, exact StreamEnd/input/output/CRC, and rejects output beyond
quota before passing those bytes to the sink.8KiB encoded/32KiB decoded chunks,
no full member Vec in the shared primitive. The Office adapter retains its own
1MiB Vec only after metadata validation; it uses the same reader. Its test fixture
central needed-version0 was corrected to match its existing local version20.
Version10/20 only and declared UTF8 validity are now explicit supported-input checks.

`archive_extract::prepare` reads the catalogue, checks strict raw components
(255B each/4096B path/64depth), canonical byte-name/type/prefix collisions,
unsupported types/flags/encryption, total/per-file declared output quotas and local
span overlaps before destination creation. Implicit directory keys borrow names;
20000-node metadata cap is checked before insertion. It returns immutable listing
and spans/counts, not a source FD proof or mutation permission. Actual payload CRC
and runtime totals remain execution duties. Casefold/normalizing-destination aliases
are not proven by this byte plan; execution must fail exclusive creates safely.

1068Rust367core/91helper/610UI+3existingignored PASS;18new pure core tests.
Clippyworkspace/alltargets-Dwarnings/release/fmt PASS.30optimized actual-helper
Office wire cases (10stored/10DEFLATE/10DEFLATE+descriptor) independently generated
with Python zipfile preserve expected source/text/error responses. These are IO
content-contract tests, not a resource/startup experiment or a new preview feature.
No actual extraction, destination files, queue/UI, archive creation or new install.
Installed SFTP report checkpoint remains appcfaa8f3ac1ac, all5installed hashes reread.

Preserved failures: missing u32 helper in moved tests (test-only restored); initial
comparison wrongly expected legacy DeflateDecoder to accept empty incomplete data.
Actual legacy decoder already rejected it, so comparison is corrected and no old
vulnerability claim is made. Shared reader also rejects it. Quotas/cancel/sink/CRC/
trailing-data/prefix/overlap tests pass; no runtime gate was weakened to get PASS.

Lock identity set (package/version/source/checksum) unchanged; only core direct
edges to existing flate2=1.1.10 rust_backend and crc32fast=1.5.3. Registry metadata
MIT OR Apache-2.0 verified, no upstream format/source text vendored. This expands
the core dependency graph and is not a code-size/PSS savings claim. Provenance and
logs: `evidence/archives/foundation-validation/`. No source UI screenshot exists
for extraction because it is not wired; old ZIP-preview images are not new evidence.

Next required work: descriptor-bound source/destination preparation, exact owned
staging and non-clobber publication/sync/cleanup-rescue, shared-queue FD/metadata
admission and owner ACK, explicit visible review/Apply, functional filesystem and
GUI evidence before installation. Then creation/ZIP64/TAR/links/other formats;
Z01/full ROADMAP remain open. Freeze resources/startup and new-preview pause remain.


## 2026-10-31 — FD-bound extraction execution, source-only

`archive_extract_fs::Prepared::capture` pins source parent/file and destination
parent, validates regular source Revision and statx mount identity, absent new
basename and the full ZIP plan. It creates no destination. Execution uses openat2
BENEATH/NO_SYMLINKS/NO_MAGICLINKS/NO_XDEV and a random exclusive child stage.
No fallback weakens these requirements. Modern Linux x86_64/aarch64 is the current
implementation; supported() is an architecture indication, not a kernel/FS gate.
The host must also provide descriptor-mode changes and directory sync support.

Files stream through the common CRC/length/quota reader. Files are0600 (0700 if
owner execute was requested), directories0700; group/other/special bits are stripped.
Modes apply to pinned FDs without changing the application umask. DOS timestamps,
owner/group, ACL and xattr restoration are NOT implemented. Source bytes are never
removed. Files and directories are synced before renameat2 NOREPLACE, then the
publication binding is checked and its parent synced. Paused/cancelled work uses
existing Control; blocked kernel I/O still has no deadline.

The registry borrows plan paths, holds identities and tracks each actual unlink.
Cleanup first verifies the known tree; missing/replaced/foreign nodes retain it
with phase and verified rescue path when available. A discovery failure gives only
an unverified candidate, never permission to delete it. Explicit cleanup retries
use the removal ledger; a reused staging name after completed removal is preserved.
PublishedUnsynced supports sync-only retry, never decoder replay or deleting the
visible tree. Uncertain publication is separate from confirmed success.

This is not an atomic snapshot or inode-conditional namespace transaction. A
same-UID actor can race name checks and unlink/rename syscalls. The rename-gap test
observes a foreign root relocated by rename, diagnoses Uncertain and never rolls
it back or declares success. Mount topology/inode reuse/full hostile concurrency
are not globally solved. Failure does not imply all stages are absent. No durable
archive journal/powerloss recovery exists yet.

Admission API separates retained_bytes (including capacities), execution auxiliary
reserve (max6MiB), three prepared FDs plus12 transient FDs, and outcome_reserve.
Retained failures can own the plan/registry and two FDs until ACK; the queue MUST
reserve this outcome before accepting a job and account for an external last
result. The fixed2MiB report slot is usable only with an explicit upper-bound
check and the prior result dropped before next dispatch, never by assuming a
ZIP error has the copy-report layout. Do not increase the shared budget silently. Capacity limits are not process-memory measurements.

Validation:1088Rust387core/91helper/610UI, three existing ignored; workspace
Clippy all-targets -Dwarnings/release/fmt PASS.20 filesystem tests cover source/
destination replacement, CRC, cancel, owner modes, symlink replacement, unknown
occupants, partial cleanup ledger, moved parent, publication-gap uncertainty,
sync-only retry and completed-cleanup sync failure. Fault hooks are synthetic
before a sync syscall, not an actual failed fsync or powerloss.
The optimized inert-by-default example and independent Python zipfile contract
exercise12 owned filesystem cases, including descriptors, raw Unix names,
umask0777 in ONLY the probe child, CRC cleanup and cancel after positive progress.
Evidence: `evidence/archives/execution-validation/`. No UI scene, queue ownership,
late UI ACK, native input or installed extraction is claimed. Next: existing
AuxIo preparation, shared-queue admission and a keyboard/mouse review/Apply flow.
ZIP creation/ZIP64/TAR/links/other OS and complete Z01 remain pending.


## 2026-10-31 — Review and global-queue integration

Extract ZIP is available in Actions, selection context menu and the command
catalogue (`Ctrl+Shift+P`, search extraer/zip/extract). One selected source per
operation; the default exclusive output name is stem-extraido, in the opposite
panel when applicable. The destination is an editable new-child absolute path;
opaque defaults retain raw PathBuf bytes. The existing modal/editor handles
keyboard/selection/undo/paste admission; copy renaming/merge/skip options cannot
alter the extraction policy. First Enter/button prepares; a second explicit
Apply enqueues the immutable token. Editing destination invalidates it. Apply
buttons carry editor epoch and prepareID; stale buttons never apply a newer plan.

Preparation uses the existing AuxIo worker and cloned Control. Cancellation and
close invalidate intent; only the exact pendingID can release its reservation or
publish a review result. Preparation pins eight transient-FD slots; review tokens
reserve their handles with other selection/native-transfer work. No FS reads or
writes run in the UI. Execution uses the ONE global copy/move worker, the same
8pending+1active queue, latest progress, owner(panel,tab), pause/cancel and ACK.
No extra archive thread, helper, shell tool or dependency was added.

Queue accounting retains preparation+execution+possible failure allocations in
addition to the existing fixed copy-report headroom, within the SAME4MiB controller
limit. This intentionally conservative current accounting can reject large ZIP
plans (7000-member fixture rejects without mutation). Larger bounded/paged plans
and tighter lifetime accounting remain work, not a silent budget increase.
Archive failures retain their complete core token. Admission explicitly requires
outcome_reserve <= the existing2MiB report slot, and the existing24FD last-result
allowance covers its two handles. Input/scratch charge releases only at exact ACK;
Model drops the old result before dispatching another job. Thus a dismissed error
cannot strand the whole queue behind an unreleasable input reservation. Recovery
paths/publication/cleanup are formatted in the existing scrollable/copyable
result panel; no archive paths are source-delete authority. Core sync-only and
cleanup retries remain retained capabilities; dedicated UI retry buttons and a
durable archive recovery catalogue are not implemented in this first UI unit.

Success refreshes captured destination directories, including an inactive source
pane/tab scenario, without redirecting the output or stealing current selection.
The existing queue bar reports extraction and reuses its controls. Close cancels
paused work and requires the worker ACK; an actual blocked syscall is not covered
by that paused fixture. No instant close/kernel-D guarantee.

1093Rust387core/91helper/615UI+3existingignored PASS, workspace Clippy/release/fmt.
Five new UI/queue tests verify success bytes, readonly preparation, paused source
replacement, outcome charge held until release, over-budget rejection, stale
button epochs, edited destination and cancelled/foreign replies. The existing
copy-ui-check.py --capture --archives extends the same fixture, no metrics.
First scene1793491193816772458 passed filesystem contracts but visual review found
the missing ZIP basename at440; its positive report/7PNGs and FAIL sidecar remain.
Final scene/visual acceptance is recorded separately before installation.


Final UI scene1793491842976117966 PASS scoped:7PNG1000x650/440x440 checked by
IHDR and visual review; filename now first, parent on next row. Review/cancel and
paused Apply create no output; success bytes/raw member/0700-0600-ownerexec modes,
unchanged source hashes and other-tab selection verified. Occupied/traversal
reject; CRC failure reports unpublished/cleanup complete, no stage left. Callback
with paused active request -> exact workerACK -> root0, not active-syscall proof.
Final reservation tests also prove a copy runs after the failed archive ACK;
no held-input budget deadlock.1093Rust and finalClippy/release/fmt PASS. Evidence:
`evidence/archives/ui-validation/checks.json`; installer uses only the final scene.
Native input/clipboard bytes/kernelD/powerloss/large-plan completeness and global
resource/startup gates remain pending. No new preview or dependency.


Installed sourcee0dd847: app7e264dd0c3c927cecd5116e3547fd477e7e96acab94c73944a8865e356570170,
helper63a05ffd3fcd12b364fc5d980fa32eeb2b38924bd62cfa4448e0b4ad822ee439,
HTML85ea1b8009ef unchanged. Record1793492160790962056 and independent five installed
plus five backup hashes in ui-validation/install-verification.json. No running
user windows controlled, no source push/tag, no global acceptance claim.

## Explicit retained-result recovery (installed61f93c6; scoped UI validated)

Only the exact retained failure offers cleanup or parent-directory sync. Neither
button reconstructs authority from a displayed path. Review does not mutate disk;
confirmation carries result ID and a fresh stamp, invalidated by Back, pause toggle
or clearing pending work. Keyboard: Ctrl+K cleanup review, Ctrl+S sync review,
Ctrl+P start paused, Ctrl+Enter confirm, Esc back. Native input remains unvalidated.

Recovery uses the same worker and unchanged controller/report/FD budgets. Admission
and submission are atomic, before the displayed count of pending jobs; no recovery
token can be stranded in the ordinary pending queue. Full/busy/closing admission
returns the intact token and original result. New recovery attempts have fresh IDs.
Cancel before work preserves capability; cleanup cancellation after an owned unlink
retains its removal ledger. Foreign entries/changed binding stop cleanup. A stage
already removed but awaiting parent sync never deletes a reused stage name.

Sync retry verifies the pinned published binding before and after actual parent
fsync. It neither opens the ZIP nor decodes/copies content. Cleanup success counts
zero newly published roots; sync success confirms one existing root with zero copied
bytes. Cancellation describes the new attempt, independently of the initial cause.

The private UI scene can pause after positive extraction progress or fail a bounded
publish notification before parent sync. This is NOT a failed fsync syscall, power
loss or kernel-D test. All injected actor files are in the scene's private directory.

## ZIP creation — sourcee5970ff installed, scoped UI validated

Create ZIP is a separate selection/context/Actions/command-catalogue operation.
The existing editable destination modal and AuxIo perform a read-only review;
Apply transfers its exact token into the shared operation worker/queue. Selection
or tab changes do not reinterpret the accepted roots. No new worker/decoder/helper.

Linux source and destination parent descriptors are pinned; dev/inode/statx mount ID are revalidated, including after publication sync. This is not proof of complete hostile mount topology. Child opens use
openat2 BENEATH/NO_SYMLINKS/NO_XDEV; final symlinks are O_PATH observations and
readlinkat reads their literal target, never its referent. Source entry revisions
include device/inode/type/size/mode/link count/mtime/ctime; checked at review,
before/after member reads and before publication. Directory revisions detect
observed membership changes. This is not an immutable snapshot against writers.
Destination ancestors are compared by descriptor identity against selected dirs;
self inclusion (including aliases) is rejected. Devices/FIFO/socket and ambiguous
ZIP traversal/backslash/drive-style names fail the review, not silently skipped.

ZIP32 DEFLATE fast streams in64KiB chunks, CRC per member, data descriptors and
bounded central metadata. Supports ordinary files, empty dirs/files, literal links
and exact Unix raw names; UTF8 flag only for valid UTF8. Dates are UTC DOS fields
at2second resolution, clamped to1980–2107. Stores file type and ordinary rwx only;
no setuid/setgid/sticky, ACL, xattrs or ownership. Archive file requested mode0600, subject to the existing process umask. Hardlinked
regular files are independent members, not preserved hardlink relationships.

Limits:128 roots,4096 entries,512KiB aggregate names,4095B member path, depth32
below a selected root,3GiB regular source bytes. Review rejects above limits before
staging. Prepared memory +1MiB execution reserve +256KiB outcome reserve are charged
to the existing4MiB controller; admission may reject a large plan.76FD reservation
covers pinned parents and bounded recursive inventory; it is not a PSS estimate.
Existing extraction/list limits differ; large created archives may exceed them.

Exclusive0600 stage, output fsync, source/binding recheck, RENAME_NOREPLACE,
parent fsync and post-publish binding check. Cancellation before publication removes
only the identified owned regular stage and syncs the parent. A substituted stage
name is preserved and reported; ambiguous publication is never cleaned as if
unpublished. Failure after stage unlink but before parent sync does not invent a
retained file. Published/unsynced/uncertain states are distinct from success.

Durable crash journal and explicit creation-failure recovery UI, ZIP64/TAR,
full metadata fidelity, other OS and process resource/startup gates remain pending.
