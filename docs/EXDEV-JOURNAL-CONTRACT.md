# EXDEV and recovery — M25 contract

2026-10-27. Implementation in progress; the installed M24 still rejects EXDEV.
O01/O03 remain partial. Resource/startup freeze in effect. Do not authorize
deletion through CopyReport.completed nor equate fsync with atomicity across FSs.

## Unit and limits

Reuse M24's FD review, queue, pause/cancel/ACK and results. The first
executable integration may limit types/batch explicitly; it will not degrade
trees, hardlinks, ACLs or xattrs into an incomplete copy followed by a delete.
Each limitation keeps the source and has a diagnostic. The final contract requires
a per-entry manifest, raw names, FS identity and revision/content/metadata.
Initial maximum 8192 entries, relative paths 4096 B/64 components, manifest 4 MiB
wire, total journal 8 MiB. Reject BEFORE reserving/copying data that exceeds them.
Account for retained capacity, descriptors and coexistence with the review/queue;
these are local limits, NOT proof of PSS nor authorization to lift the freeze.

## Batch roots and partial publication

A journal corresponds to EXACTLY one root (file, symlink or tree). The
inventory codec can represent 128 roots for the batch preflight, but
create, State and replay/open reject a multi-root journal even if its CRC is valid.
The batch controller will have stable root IDs and independent journals;
do not split by cloning 128 manifests outside the budget. The maximum of 8192 entries/
4 MiB of manifest is an aggregate for the admitted batch, not 128 times that limit.
Before touching each root its durable Prepared must exist. A crash between roots
shows which ones completed and which were interrupted; neither success
nor a global restore is inferred from one root's state. Resume/restore is per root with its
identities, without undoing completed ones nor claiming others' names. The
batch record groups results; it never replaces each journal's barriers.
For a tree, the staging/publication of its root is a single rename, and the inner
deletion is per entry; that distinction removes the batch's fake atomic
rename. UI recovery and the multi-root controller are still a pending part of M25.

## Publication and deletion

The executor keeps parents/objects by FD. It uses a private 0700 staging, exclusive
and no-clobber names; never remove_dir_all on a rebuilt path. The source
can be isolated by a rename into staging on the SAME FS, with the intent persisted
before and an inode check afterwards; that is not yet deletion of data. A
replaced entry or an unconfirmed publication keeps everything and requires review.

Durable states, in order (each arrow requires the ACK of the journal's fsync):

1. **Prepared**: a complete bounded manifest, planned identifiers/parents/staging.
   Journal created exclusively, file and directory synced.
2. **StageIntent**: the identities of the already synced private staging are recorded
   before renaming the source. Recovery inspects both names by identity.
3. **SourceStaged**: source identity confirmed in staging; sync the staging
   and the source parent. No copy authorizes deleting this preserved source.
4. **DestinationVerified**: each destination entry has a registered identity,
   content and metadata compared, file fsync and staging directories fsync.
   Root inode/mtime does NOT prove the descendants. Verify permissions/uid/gid,
   timestamps, ACL/xattrs and hardlink topology; a failure keeps the source.
5. **PublishIntent**: write-ahead before publishing with rename NOREPLACE.
6. **DestinationPublished**: publication confirmed by identity. It may be
   visible but NOT durable. An error/crash here never enables deleting the source.
7. **DestinationSynced**: destination and parent synced after publication,
   identities confirmed and this state synced in the journal. A sync error
   after rename is reported as published, not confirmed/durable;
   do not assume the earlier state again nor overwrite a foreign entry.
8. For each entry, children before parents: **DeleteIntent(i)** only after
   revalidating the captured source and the durable destination per entry (content,
   revision, metadata and identity). Persist the intent, check again and
   unlink relative to the own staging's FD. **Removed(i)** only after
   syncing its directory. An intermediate failure stops the following deletions.
9. **Completed** only after every entry has been removed, cleanup of the empty
   staging whose identity matches and the final syncs confirmed.

A failed journal ACK may have written visible bytes. The instance is
marked uncertain and stops mutating; it does not update the confirmed state nor retry an
action assuming nothing happened. When reopening, parse complete records under the
lock; a cut/corrupt tail, a future version or a changed identity => keep the
bytes and review mode, without truncating/skipping the problem. CRC detects accidental
corruption, not authenticity against the same hostile user.

## Recovery and identities

No tasks are resumed at startup. Enumerate journals in a bounded way outside the UI and
show the interrupted ones; recovery only through an explicit action. A per-journal
lock held during load/update; never take control of a live job.
Validate name→FD, owner, regular type, limits, parents/staging/entries.
If a parent was renamed and cannot be resolved by identity, report an
unconfirmed location; do not operate on a replacement at the old path nor search the whole FS.

Recovery is idempotent: an intent is not equivalent to an executed syscall.
Compare source/staging/destination against the registered identities, telling absent
apart from inaccessible; if a foreign object matches, do not move/delete/claim it. An already
published, verified destination can be kept while the source is restored with
NOREPLACE: an explicit partial result, never a complete Move nor a double delete.
After DeleteIntent, an absent entry allows confirming its deletion
only with an intact durable destination and a sync of the parent; a new entry is NOT touched.
Every deletion check is repeated on recovery, even if the journal had
an earlier check. Even a readable DestinationSynced may come from
a failed fsync ACK: renew the verification and the fsync of destination, parents and journal
in this run before any new deletion, without assuming a reboot. Cleanup only of confirmed own files/staging.

Pause/cancel before publishing allows restoring the source without clobber; after
publishing it stops deletion and keeps/restores the source when verifiable.
Earlier completed roots have no global rollback. Close cancels ALL the
subsystems, drains the worker/ACK and shows rescue; a blocked syscall remains an
explicit limitation. Linux rename/unlink are not inode-conditional: there is an
interval against a hostile writer; private staging reduces exposure but does not
prove exclusion of open FDs nor confinement of every ancestor.

## Revisions changed by our own operations

The initial revision is not blindly compared after staging/deletion:
rename can change ctime; deleting hardlinks changes the nlink/ctime of the same inode;
removing children modifies the mtime/ctime/nlink of directories. The executor keeps a
per-inode ledger of internal links that still exist and of its own child names.
It only updates the observed revision immediately after an own syscall whose
identity/publication is confirmed; it does not predict timestamps nor accept an arbitrary
new revision. Check content/hash again, unmutated metadata,
the set of names and expected foreign links; a mismatch => keep and stop.
For directories, compare the remaining children through the manifest (not just mtime); for
hardlinks, validate topology and bytes once per inode and every entry by
name. Mandatory cases: two hardlinks of one inode and a folder with several children,
with and without an external writer. If that ledger is not implemented yet, those types are
rejected BEFORE staging; do not widen the comparator to ignore ctime/nlink globally.

## Acceptance required before enabling EXDEV

- SameFS stays intact. A real cross-FS transfer separate from fault injection;
  own fixtures, never personal data nor /tmp, no global mounting/configuration.
- Bytes, permissions/ACL/xattrs/timestamps/hardlinks/symlinks per supported type;
  unsupported or error => source kept and a clear partial result.
- Crash/failure at each transition before/after write, fsync, rename, unlink;
  reopen twice, without deleting replacing objects nor incompatible journals.
- Substituted source/dest/parents; a descendant edited during the copy; collision,
  destinations inside the source, raw paths, cancel before and after real progress.
- Real UI review/confirm/progress/result/interrupted, both folders and
  tabs; normal close after ACK, own helpers/FDs released. Internal scenes
  are not equivalent to OS input. Install only a validated, consistent visible checkpoint.
- Resources/startup NOT_MEASURED until the user lifts the freeze; do not declare
  the memory goal nor a global close with only the tests/caps of these modules.

## Effective inventory in progress

Linux FD walk: parents by components without symlinks, O_PATH first so as not to
open a FIFO/device, data O_NOFOLLOW|NONBLOCK|NOATIME (own sources for
now), fdopendir on a CLOEXEC dup of the NOATIME directory FD, RAII close. A single
64 KiB scratch, 8192 entries/64 levels/4096 B paths/1 MiB capacity+pending names,
xattrs 64/128 KiB in total (including ACL/security). SHA256 once per inode group,
attributes and sorted child names, revision before/after. Final raw order,
verify through a borrowed lookup without a second owned inventory. No atomic snapshot nor
interruption of a blocked syscall; cancel between syscalls/chunks.

A separate AccessTime keeps the first observation for the destination. NOATIME directories and
files do not change atime with these reads. A symlink readlink may change the
source's atime: it is not restored through a write and does not serve as a detector of an
external actor; verify reuses the initial access, also across hardlink aliases of the
symlink. Content, mtime/ctime, ownership/mode/xattrs and topology are checked.
Hardlinks across roots need a shared batch ledger; while it does not exist,
they are rejected before staging. Hardlinks within a tree do not prove global
support across roots nor a deletion ledger. None of these modules enables EXDEV
in the UI yet; running copy/publish/recovery is the next M25 work.

Copy to staging: the first backend keeps the FD of each created destination node for
runtime cleanup without inode reuse; before creating it limits to 128 nodes per
root. This is an explicit temporary limit of the backend, not a reduction of the final scope
of 8192-entry trees/batches/hardlinks/journal. The source is neither deleted nor published in this
phase. It must be accounted together with review/queue before wiring; do not multiply
128 FDs by 8 jobs nor assume the isolated limits prove global memory/FDs.
Larger support needs an ownership/recovery strategy through the manifest
without keeping one FD per node, still pending, without reinterpreting doubtful identities.

## M25: single-file executor and recovery (WIP, 2026-10-27)

`exdev_move` runs a regular file with nlink=1; it rejects trees, symlinks and
hardlinks BEFORE staging. NativeMove stays separate. The effective copy of
trees has staging tests, but this executor does not authorize publishing/deleting them.
There is no EXDEV UI support nor automatic recovery enabled yet.

Each own rename has a separate returned/identity-confirmed phase. The
new revision is kept only after comparing bytes/metadata/earlier revision (except
the ctime of that rename); the SourceStaged/DestinationPublished receipts persist the
effective revision. After unlink a pending sync is kept; a retry does not delete a new
occupant. A planned stage that was never created never authorizes cleanup; one created without an FD or a
confirmed binding remains a rescue candidate, not ownership inferred by name.

Recovery opens new FDs and requires the persisted parent IDs and stage IDs. Before
each action it checks the journal's health/binding: an uncertain ACK requires reopening, not a
mutating retry on the same instance. A publication with a PublishIntent without ACK
requires the registered destination identity + full hash/metadata. It renews the sync
of the published file, destination_parent, destination_stage and journal+its parent
before deleting the source. DestinationPublished does not record DestinationSynced if
the stage barrier fails. Absence is not turned into identity: only an
already unlinked source inside a known stage with DeleteIntent is recognized, after a
renewed verification/sync of the destination; a new occupant is kept with an error.
A reopened RestoreIntent verifies the source by inode/signature, restores without clobber and
syncs both directories. A published destination is never restore cleanup.
Partial copies without a registered identity and stages before StageIntent are
kept as candidates; recovery reports an error instead of claiming them.

Pause/cancel cooperate in phases, traversal and each 64 KiB hash/copy chunk. The
runtime restore drains despite Cancel to keep the source; it does not promise to
interrupt a syscall on an offline mount. After DeleteIntent a deletion already
started may finish despite a late Cancel. Initial parents are opened with a final
O_NOFOLLOW, but their ancestors may be symlinks: it is not absolute
confinement. The descendant walk is FD-relative/no-follow.

Core verification: 191 tests PASS, 3 ignored; Clippy core all-targets -Dwarnings
PASS. Two opt-in tests run separately: a real transfer st_dev 63→75; three
process pairs on those FSs (exit 73 without Drop after publish-rename, source-unlink
or restore-rename, then a new recovery process exit 0). It does not test power loss,
SIGKILL nor an fsync that really failed: sync/ACK failures are declared hooks.
The harness's first atime FAIL and the initial Clippy are kept in target; the fixture
read switched to O_NOATIME without relaxing the engine's check.

Missing: wiring of queue/AuxIo/UI and their joint FD/metadata budgets,
visible recovery, capture, installation; then the tree/hardlink ledger and
per-root batches. No new release nor installation for this foundation. M24 stays
installed. Resources/startup NOT_MEASURED: freeze intact and earlier FAILs active.


## M25 — validated visible integration, 2026-10-27

This delivery replaces the UI-less state of the earlier foundation. F6 keeps the
non-mutating review and the global queue; a single regular file with nlink 1 on a different FS
uses the verified executor. Actions → Interrupted moves lists names
and source→destination, with a readable status and copyable technical details. Finish or
Restore are explicit and depend on the phase; Completed/Restored offer no
recovery and RestoreIntent only allows Restore. No automatic resumption.

The catalog opens the same enumerated directory by FD. An action requires a proof
of identity of the directory/journal and the SHA256 of the reviewed manifest; replacing
one job with another does not silently change its paths. The retained owner/lock is
transferred to the request and only released inside the worker before reopen. An admission
rejection keeps the owner; a pending cancel leaves the journal rediscoverable.

586 Rust tests PASS (193 core/67 helper/326 UI), three opt-in entries ignored in the
ordinary suite, documented and run separately when applicable. Clippy
workspace/all-targets -Dwarnings and release clean. Scene
exdev-ui-1793122272433112203.json PASS: three real cases st_dev 63→75, review and
pause without mutation; transfer, a new process after a publish gap and restore,
normal exit 0 after ACK and stages absent. Ten framebuffers with checked real
dimensions. The INITIAL 440×440 window shows complete controls; long titles
and diagnostics are clipped; full readability and the OS clipboard are not proven.
Live resize is still UNVALIDATED: the 440↔1000 oscillation is preserved in the earlier FAIL,
not replaced by accepting any capture. Method handlers/Actions +
framebuffer, without OS input, SIGKILL, power loss nor a real fsync failure.

Pending: EXDEV for trees/batches/hardlinks, a shared batch ledger,
general conflicts and recovery of other tasks. O01/O03/X02 remain
partial. New previews/media paused by the user's instruction: the next
visible units are dual pane, cut/paste, DnD, operations and mounted SFTP.


## Joint budget of the Linux integration

Local accounting; it is NOT a process/PSS measurement nor an approved global gate.

| Property | Limit/admission | Coexistence considered |
|---|---|---|
| Queue | 4 MiB, 8 pending, 1 active/ACK | Request.metadata_bytes reserves 1 MiB for each EXDEV/Recovery; paths/capacities separately; not eight engines |
| FD tokens | 256 including a 24 reserve for the last external result | Native roots+2; EXDEV/Recovery reserves 24 (return+temporaries), active/ACK+pending add up; an additional AuxIo review up to 130 => 386 tokens; catalog up to 4 additional FDs => 390 conservative tokens (not the process total) |
| Result | one complete Outcome, 2 MiB reserve | Native CopyError OR ExdevFailure; not both. Model frees copy_last before the next dispatch. Recovery moves the earlier owner into the Request; the worker drops it right before reopen; a rejection returns the owner intact |
| Retained EXDEV | 1 MiB reserve/input; one file without hardlinks | Inventory capacity 32 Entries (~7 KiB)+name 255; manifest two parents 4096+root 255 (<12 KiB); journal/state/maps for one entry; 2 stage names; 1 Created pin; contexts/proof/path copies and a 1-root report/candidates. Xattrs/hash scratch are NOT kept in the result |
| Recovery/catalog wire | 64 KiB BEFORE read Vec; the generic WAL 8 MiB is not used in the UI | decode/scan/materialization bounded by that wire, trees rejected before retaining Recovery. intent_proof encodes a single-file manifest <12 KiB; fstat/encode only worker/AuxIo |
| EXDEV work | scratch 64 KiB + attrs 64/128 KiB in total per phase | transient hash/copy/verification/metadata work on top of what is retained; conservatively reserved 2 MiB in planning, not attributed as app use. Syscalls/kernel/allocator/FLTK separately |
| Catalog | 32 rows, 1024 entries examined, conservative charge 512 KiB | the preflight includes 2×parents/names (join growth), root+2048 B per row for structs/proof/diagnostic; old UI+new response+wire/transients need a separate 3 MiB reserve, not just 512 KiB |

The error result keeps the phase/intent proof/ID, the captured paths and the locations
resolved by FD, the candidate staging and the Transfer/Recovery owner. It is not reduced to
just a CopyReport nor operated by reading visual paths. Cancelling a pending Recovery
frees FDs but leaves the journal rediscoverable in the catalog (regression). The catalog
root is enumerated and opened by FD; the action requires directory+file identity and the
SHA256 of the reviewed manifest before acting. It does not authenticate against a hostile writer
with the same user nor replace the current barriers. Changes in bytes/identity
/intent give an error to review; the same job is never reinterpreted with other paths.

These caps/reservations include the new local objects; **the real global budget
is still NOT_MEASURED because of the freeze**. UI/FLTK/listing/installed previews/OS threads
add to the process and helpers; do not deduce the 50 MB goal from this table. The earlier baseline and
startup FAILs remain open.
