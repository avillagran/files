# Owned workspace factory

SOURCE ONLY, NODE, 2026-11-02. `Journal::create_workspaces` and its controlled
variant create all transaction workspaces in physical order under caller-selected,
already existing private containers. This is an explicit trusted-coordinator API;
it does not supply native approval, helper acknowledgement, fresh source binding,
automatic recovery or installed UI Apply.

## Admission before any creation

The physical seal must match the Journal Schema's cached whole-batch geometry
admission. Exactly one parent per physical transaction is required; the batch
must not already have begun. Every absolute parent is opened component by
component with NOFOLLOW, pinned by device/inode/mount and checked for ownership
and exact0700. Parent/child overlap with either original tree or its ancestors is
rejected, including bind aliases detected by identity. Parent/child overlap with
the Store is rejected too. Each parent must match the transaction's device AND
mount ID. Parents may be shared between workspaces.

Names are deterministic raw components: `sync-<jobhex>-<transactionhex8>`.
All names must be absent without following the final symlink. Every parent and
absence is rechecked before the first intent, then again before its mkdir.
Thus a bad later placement refuses the entire batch before creating the first
workspace. No container directory is created implicitly.

A point-in-time RLIMIT_NOFILE and `/proc/self/fd` check counts occupied descriptor
numbers below the current soft limit. It compares free slots to the preceding
retained-FD forecast plus `4 * MAX_SCOPE_DEPTH + 32` transient slots. No process
limit is changed and no slot is reserved. Other threads can consume slots after
this observation; actual open failures still fail closed and preserve partials.
This is functional admission, not a memory/CPU/startup measurement or a guarantee
of descriptor availability throughout later execution. Existing Prepared,
Store/Journal and unrelated FDs are counted as occupied. Future execution still
needs current admission and the documented receipt-retention policy.

## Durable sequence and failure

Tag32 `WorkspaceCreateIntent` has transaction+seal, body37B. Its seal binds the
Store proof, physical seal, parent identity/mount and exact name. Full journal
readback/file+ancestor sync/ACK precede one FD-relative `mkdirat(0700)` with no
fallback or retry. A nonzero guard also records the pending workspace. The
journal owner and parent binding are checked just before the syscall.

The named postimage must be an empty directory owned by the effective UID with
exact0700, on the planned mount. An immutable inventory pins its object and
revision. Existing `WorkspaceCreated` records/ACKs that observed identity. The
inventory's verified synchronization opens real directory descriptors, syncs the
root and parent, then the factory syncs the complete container ancestor chain.
It rechecks the empty inventory and parent before tag33 `WorkspaceMaterialized`
(body5B) readback/fsync/ACK. Named/root bindings are checked after ACK and for the
whole returned batch. Original capture counters and wire caps remain unchanged.

The pending guard allows only identity recording and materialization for that
transaction. It blocks legacy declarations and other effects even if History is
renewed. A final materialization marker can clear the guard before a later
returned error; that still grants no recovery or retry authority. The batch API
rejects any already-started/completed workspace batch. Old declaration-only
streams remain data; older readers reject the new tags.

Once any intent write starts, errors poison the owner as Uncertain. The error
reports the current path, separately records intent ACK/syscall attempted/
succeeded/identity ACK/materialization ACK, and retains completed workspace
receipts. Files/directories are preserved; no delete, truncate, rollback or
implicit resume happens. False ACK flags can still leave complete journal bytes.
A live batch receipt holds two FDs per workspace (empty inventory root and
container pin). `into_parts` transfers these to the caller. Its observation is
historical: later creation changes the empty inventory, so the staging capture
must be rebuilt and validated by the owned creation/metadata/barrier pipeline.
Drop the obsolete empty inventory/root before retaining the full staging
capture and its metadata shadow; keeping both would exceed the forecast policy.

`mkdirat` returns no inode FD. An equivalent empty replacement between mkdir and
first capture cannot be distinguished. Evidence is a **named postimage**, not
atomic positive inode provenance. A later replacement or changed ancestor is
rejected by the pinned revision/name checks; partial objects stay in place.
A restrictive umask can produce a non0700 directory: reject and preserve it,
without changing process umask or chmod of an unproven named inode.

## Validation and remaining work

Fixtures cover complete factory→creator→durable metadata→staging barrier→owned
publication, multiple roots/raw parent names, strict codec and pending guard,
all-placement rejection (input ancestor/Store/public/symlink/relative/foreign
mount), twelve fault boundaries, second-workspace failures/collisions, same-byte
journal rewrite before mkdir and root replacement after final ACK. Child tests
lower the actual FD soft limit or set a restrictive umask without affecting the
parent process; cooperative pause/resume/cancel and three real exits103 test
History reopening. Ordinary exits do not simulate power loss.

Detailed results and preserved failure logs are in
[evidence](evidence/directory-sync-workspaces/assessment.json).

The intent seal does not encode a recoverable absolute placement path. A durable
placement registry must precede production use; these History records alone
cannot locate or authorize recovery of the directories.

Choosing/creating persistent containers and that registry, final original-source/helper/absolute
bindings and native approval, one integrated queue/coordinator with actual FD
lifetime management, SharedDir AfterOwnChildren, recovery/undo and UI Apply remain
open. Full walks and journal readbacks still have no bulk-scaling acceptance.
Resource/startup freeze and the pause on new preview formats remain in force.
No release, installation, GUI/native/global acceptance follows from this source
checkpoint.

NODE results: 1,582 workspace tests passed (675 core, 112 helper, 795 UI;
three historical ignored), ten new fixtures. Initial core check warned about
missing outer reexports, corrected. First four factory tests had three failures:
fsync on the O_PATH inventory root returned EBADF. Reusing the existing verified
`sync_staging` path corrected it, then four passed. Expanded test compilation
caught a wrong Store::renew call signature; corrected, then nine and ten passed.
After the full pass, Clippy requested collapsing a nested guard condition; this
style change was followed by another ten-fixture pass. Raw failure/pass logs
remain part of the evidence. Five installed hashes remain unchanged.
Final Clippy workspace/all-targets with warnings denied and workspace format
check passed. Cargo used the original idle gate and normal harness; competitors
were untouched. No release/install/GUI/resource experiment was run.

Checkpoint 2026-11-02: [durable placement registry](DIRECTORY-SYNC-PLACEMENTS.md)
now preserves complete raw parent paths/chains and canonical workspace names
before mkdir, anchors the registry seal with tag34 and requires a matching live
renewed registry for registered journals. History inspection remains independent
of saved path availability; artifact attachment supplies no native approval.

Checkpoint 2026-11-02: [shared-container batch preparation](DIRECTORY-SYNC-BATCH-PREPARATION.md)
now groups exact raw parent paths, bounds requested placement metadata before
pinning, shares parent handles and uses the grouped FD forecast. Registry rows
and per-effect absolute/name checks remain intact; no process-resource or native
execution acceptance is inferred.

Checkpoint 2026-11-02: [private staging coordinator](DIRECTORY-SYNC-STAGING-COORDINATOR.md)
now connects complete owned creation, metadata and global barrier with cumulative
payload admission, bounded leader receipt lifetimes, shared parents and fresh
source checks. Destination execution, native approval and recovery are separate
unfinished phases; no installed Apply or bulk/resource acceptance follows.
