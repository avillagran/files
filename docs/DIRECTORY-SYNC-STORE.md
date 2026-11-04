# Owned synchronization preparation store

`directory_sync_store` persists the complete review intent and actual metadata
values in an owned, exclusively created file. This SOURCE ONLY layer implements
artifact readback, ownership and Linux durability barriers. It is not an effect
journal, operator approval, source freshness lease, executor or automatic resume.
The installed read-only dry-run is unchanged and the full goal remains active.

## Complete saved intent

FLSSAVv1 starts with an eight-byte magic, little-endian u32 evidence/plan lengths
and u64 staging-payload limit. It stores the complete canonical FLSYNC evidence,
FLSPLN choices/options and then the FLSMET actual value stream. SavedIntent can
reconstruct the logical plan and physical blueprint without original paths or
Prepared pins. Stage limit is saved explicitly; using a default after reopen
would change the physical seal. Pending/illegal choices and malformed evidence
are rejected by the existing validators and physical derivation.

Each evidence and plan has its own canonical seal; the nested value stream binds
both seals and the rederived physical seal including budget. Exact framing and
nested EOF reject extra bytes. No alternative unverified proposal/operation table
is accepted. Record.from_owned uses the same borrowed full scanner before keeping
the already bounded owned buffer, avoiding another complete evidence byte copy.
It changes neither FLSYNC nor legacy EXDEV formats.

Only bounded intent bytes and current metadata records are retained during
readback. Metadata values still stream one object at a time. Maximum artifact
size is8,594,608,798B:24B framing plus the existing4,657,718B evidence allowance,
16,464B plan allowance and8GiB metadata stream allowance. This is an on-disk
artifact bound, not a helper stdout allowance or process memory claim. Actual
functional fixtures store both full inventories with metadata beyond8MiB and
reopen all16,384 records without truncation.

## Ownership and namespace

The caller supplies an existing, owned0700 directory outside both input trees.
The store performs no directory provisioning. Absolute raw paths are bounded to
4096B/128 components, reject traversal/empty components and follow no symlink
components. Each prefix is opened as a directory and its device/inode/mount
binding is captured. Original path bindings are rechecked before and after
barriers. Selected root ancestry and complete captured directory identities reject
store/input containment and direct aliases before creating an artifact.

Names are deterministic `sync-values-<nonzero16B-job-hex>.record`. Creation uses
O_EXCL/NOFOLLOW; existing/partial files are never reused, replaced or truncated.
Only the newly exclusive file receives0600 permissions. Input limits and mismatch
against Prepared are checked before creation. Later failures preserve any visible
partial/complete artifact for explicit inspection; Drop never deletes it.

A record must remain an owned0600 regular file with nlink1, on the store mount.
Opening history first pins O_PATH/NOFOLLOW and checks type/ownership, then opens
that exact validated proc FD for data with NOATIME/NONBLOCK/CLOEXEC. A raceable
name is never opened as a device/FIFO for data. File/name identity, length,
mtime/ctime and current directory/mount binding are checked around readback and
barriers. Locks are advisory, not a filesystem snapshot or exclusion of an
uncooperative writer. Current positive checks do not establish an ABA-free global
namespace or lasting freshness.

Each open owner takes an exclusive nonblocking flock. The lock's creating PID is
retained: Drop explicitly unlocks only in that process, retrying EINTR. A forked
copy cannot unlock the parent's lease; the parent unlocks even if a child still
holds a copied descriptor. File inspection and renewal reject use from another PID. There is no implicit takeover of an active owner.

## Readback, barriers and comparison keys

Creation acquires actual values, reads back the complete saved intent and value
stream, compares it with the live acquisition, and checks file binding. It then
syncs the file and namespace directories from the containing directory upward to
`/`, reopening/checking one prefix at a time instead of retaining an FD for every
ancestor. After the last callback and ACK it checks bindings/cancellation, and
creation revalidates the full Prepared capture once more before returning.
This extra verification is separate from the metadata acquisition's final pass;
each inherits Prepared's per-call read budget. No atomic snapshot is claimed.

Store state is History, Durable or Uncertain. Every reopen starts History even
for an earlier fully synced file. `renew` performs complete intent/value readback
and new file/namespace syncs. Once renewal begins, any failed check/I/O/cancel/ACK
leaves that owner Uncertain; it cannot renew again. Explicit Drop and reopen are
required. A failed creation can likewise leave a fully parseable artifact without
a successful owner/ACK. No automatic resume or deletion follows.

The212B FLSMSTv1 comparison proof includes directory/file identities, both mount
IDs, job, evidence/logical/physical/value seals, total artifact length and record
count. StoredProof decoding creates comparison data, never a durability barrier.
An identical-byte replacement file has a different identity and cannot satisfy
an old proof. A borrowed BarrierReceipt checks a Durable owner, expected proof
and current file/namespace binding; it cannot be decoded and can be reverified.
It proves only the artifact contract at that point. It is not operator approval
or proof that current input files match the saved intent.

`open_saved` reads/reconstructs history without accessing original input paths.
`open` additionally requires exact expected evidence/choices/stage budget.
`inspect` visitors receive provisional historical records until the whole call
and post-read binding checks succeed. Source mutation can coexist with a freshly
resynced artifact: a new Plan.bind still rejects changed inputs. The future
worker must require both storage/intent identity and fresh source proofs.

## NODE acceptance and next stage

Tests exercise exclusive creation, readback, complete namespace sync and reopen;
partial writes and failures after each barrier; poisoned renewal and explicit
reopen; same-byte replacement, extra hardlink, truncation, namespace replacement,
symlink/FIFO and corrupt files; Source/ACK cancellation; durable history without
fresh source authority; deep/raw paths; complete large inventories; and original
paths/pins gone with decisions/custom stage budget recovered from the file.

Owned child processes exit abruptly before a full record, after values are written
and after namespace sync/ACK. Reopen yields partial rejection or History with no
fresh barrier; normal retry requires new syncs. These are process-exit tests, not
power-loss or hardware durability experiments. Fork tests verify both child Drop
and parent Drop while an inherited descriptor remains, with normal Rust harness
concurrency. Source data and foreign replacement files are never applied over.

The initial metadata-only prototype passed16 focused tests; recovery review then
identified the need to persist evidence/choices as well. Final tests add complete
saved-intent recovery. Counts and final workspace/Clippy/fmt results are recorded
in `evidence/directory-sync-store/assessment.json`. No release, new helper/UI
integration or installation occurs; five installed hashes are checked unchanged.

Next: an owned effect journal referencing this exact proof, explicit visible
approval tied to job/full intent and live helper/pins/ACK, own namespace/revision/
nlink/ctime ledger, global source staging, verified no-clobber old preservation
and publication, durable effect barriers, bounded queue/pause/cancel/close ACK,
partial/uncertain outcomes and crash/reopen/recovery/undo. Untouched topology still
needs explicit review. No unattended Apply against personal folders.

PSS/Swap/CPU/startup experiments remain frozen and new previews paused. Structural
byte/FD bounds are not resource measurements. Native/global acceptance, other OS,
privileged live mount scenarios and blocking-syscall deadlines remain open.

NODE final validation:1482 workspace tests (575core/112helper/795UI), three
historical ignored,17 new store tests, all-target Clippy-Dwarnings and fmt PASS.
Initial16 metadata-only focused tests passed; the complete saved-intent version
passed17. First1482 full passed with one Clippy range-style finding, fixed using
RangeInclusive.contains without suppression. Final1482/Clippy/fmt passed again
after strengthening process-exit recovery to open_saved with inputs removed,
and fork liveness/EINTR handling with a positive waitpid WNOHANG check.

Subsequent SOURCE ONLY indexed access in DIRECTORY-SYNC-INDEX.md derives bounded
checksummed slots during full readback and verifies individual original records
with positional reads. Stage metadata ordering avoids retaining all attributes
or replaying this artifact for every object. It does not add Apply authority.
