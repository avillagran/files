# Actual all-workspace staging confirmation

Source-only Linux foundation. `Journal::confirm_staging` checks and synchronizes
all selected transaction workspaces before appending WorkspaceSynced and
StagingComplete. This does not create staged objects, reproduce metadata, perform
user-data effects, grant native approval, or enable installed Apply.

The journal must be currently owned and Durable in Staging, with every workspace
and every node declared/verified and zero previous WorkspaceSynced declarations.
The caller supplies exactly one complete borrowed inventory/parent/name per
physical transaction, in transaction order. Each recorded private root must match
the declared device/inode/kind/mount, be owned by the effective user and mode0700.
Retire-only workspaces must be empty. Other workspaces must contain exactly the
planned `new` subtree: no missing names or extra names are accepted. Every staged
object/revision/content/metadata must exactly match its verified observation,
which replay already checks against the sealed desired values, size, link/cohort
constraints and protected identities. No ctime exception is used at this barrier.

After checking snapshot shape and semantics, the implementation:

1. Fully rewalks every scope with existing FD/no-follow/NOATIME/hash/metadata/
   mount/revision verification against its captured evidence.
2. Pins and fsyncs every regular file/directory in reverse raw path order, so
   children precede their containing directories, then synchronizes each external
   workspace parent. Symlinks are pinned but never followed/fsynced as O_PATH;
   their containing directory is synchronized later.
3. Fully rewalks every scope again, under the SAME shared regular-read allowance
   (at most1TiB across both passes and every workspace). Existing within-scope
   hardlink hashing deduplication applies; cross-scope reads are charged again.
4. Rechecks all workspace roots, current Store/Journal ownership/bindings and
   cancellation, then appends all WorkspaceSynced records and StagingComplete
   through existing full journal readback/file/ancestor fsync/ACK barriers.
5. Rechecks roots/owner and returns a private non-decodable StagingReceipt with
   canonical proof/transaction/path/exact-observation seal, journal sequence and
   separately counted regular payload bytes.

Before journal appends, failure never advances staging declarations; callers may
reinspect/reverify. Once journal appends start, any error poisons that owner as
Uncertain. Partial/all durable declarations may exist; no receipt is returned on
failure. There is no truncation, cleanup or automatic replay/resume. Reopen is
History, and renewing artifact durability does not reconstruct this receipt.
An already recorded global barrier cannot be repeated using this method.

The receipt is a historical fact of this call's verification, fsync and journal
ACK, not a filesystem snapshot or lasting source/staging freshness. Checks across
scopes are sequential and writers may mutate objects after their last check.
Later per-effect guards remain mandatory. This API does not verify current
absolute root ancestors, visible human approval, helper/session lifetime or
fresh original source bindings. The existing explicit trusted-coordinator rename
primitive still requires those preconditions; manual journal append remains a
declaration API and cannot establish this positive receipt.

Metadata reproduction and the complete staging executor, approval/helper
integration, recovery/undo and UI Apply remain unfinished. Full namespace and
journal rechecks can be cumulatively quadratic in a bulk run; no mass-operation
performance acceptance is claimed. No decoder, worker or timer is added. Resource
and startup experiments remain frozen; installed dry-run stays unchanged.

`confirm_staging_controlled` uses the existing cooperative Control for pause,
resume and cancellation during rewalks, between pinned sync operations and
between journal frames. Cancellation while paused wakes the worker. No new
production worker/timer is introduced; blocking kernel calls still have no
bounded deadline. Functional pause waits are not performance measurements.

NODE validation (2026-11-02): seven new fixtures cover stale/extra/unbound roots,
fresh snapshots with changed desired values, duplicate/missing scopes, two real
workspaces with a shared two-pass read allowance, mutation/cancel after fsync,
four pre/post ACK fault boundaries and partial first-workspace ACK followed by
History reopen. Control pause/resume/cancel and historical receipt/no-repeat
checks pass. The existing nine actual owned-rename fixtures now call this real
staging confirmation path, including retire-only and directory/hardlink/symlink
workspaces. Initial E0425 import failure and the corrected runs are retained.
Workspace passed1540 tests (633 core,112 helper,795 UI; three historical ignored).
A test-only indexed hook produced an unused-field warning in the first workspace
build; the unnecessary index was removed. Final Clippy all-targets with warnings
denied passes. No release/install/GUI/resource acceptance belongs to this step.

Later source checkpoint: [stage metadata reproduction](DIRECTORY-SYNC-STAGE-METADATA.md)
now applies exact desired raw values to pinned staged objects and checks readback;
existing owned rename fixtures use it before the global staging barrier. This
adds no staged tree creator/native approval/installed Apply and explicitly rejects
symlink xattr changes when no safe descriptor mutation route is available.

Later source checkpoint: [journal-bound node creation](DIRECTORY-SYNC-STAGE-CREATION.md)
now supplies actual exclusive regular/dir/symlink/cohort creation with a prior
durable intent, identity ACK before file payload writes and materialization ACK.
The complete owned sync fixtures use it before metadata/global staging/rename.
Workspace creation, durable metadata intents, native approval and recovery/UI
remain pending; directory/symlink named-postimage provenance is explicitly qualified.

Later source checkpoint: [durable staged metadata](DIRECTORY-SYNC-METADATA-JOURNAL.md)
now binds exact ordered metadata intents/after ACKs, revalidates all workspaces
and commits all alias owners together after durable confirmation. Complete owned
fixtures use this route; workspace factory, native approval/helper/absolute
bindings and recovery/UI remain pending. Replay/declarations never grant authority.

Checkpoint 2026-11-02: [private staging coordinator](DIRECTORY-SYNC-STAGING-COORDINATOR.md)
now connects complete owned creation, metadata and global barrier with cumulative
payload admission, bounded leader receipt lifetimes, shared parents and fresh
source checks. Destination execution, native approval and recovery are separate
unfinished phases; no installed Apply or bulk/resource acceptance follows.
