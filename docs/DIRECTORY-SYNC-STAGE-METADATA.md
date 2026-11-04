# Reproducing metadata on staged objects

Source-only explicit trusted-coordinator primitive. Inventory.apply_staged_metadata
(and the controlled form) reproduces one selected object's captured desired
Values through pinned descriptors, checks exact readback and returns a private
historical receipt. It does not create the staging tree, establish native
approval/stage membership or journal authority, fsync/publish, or advance the
original inventory. Installed sync remains read-only.

The coordinator must establish its private staged workspace and owned-created
node identities and exact expected cohort link counts (including other workspaces), appropriate journal intent/approval, current source/absolute
bindings and compatible sealed recipes first. This API is not a caller gate for
those preconditions. Values decoded from history alone are not authorization.
Apply once per cohort leader and process files before directories, deepest first,
using Physical.stage_metadata_order. Recapture and the actual all-workspace
staging barrier remain mandatory before effects.

Before writing, complete desired values are bounded/validated, the selected
original path is freshly reopened and its complete old metadata reacquired.
Identity/revision/mount and all ancestors are checked using existing no-follow
pins; owned effective uid and compatible type are required. Data descriptors use
NOATIME/NONBLOCK/NOFOLLOW. Changed/unowned/oversized objects fail before mutation.

Application preserves the held object's identity/mount/size/link count:

1. Change group with descriptor-relative fchownat only when needed; desired uid
   must remain the effective owner. Filesystem permission errors are retained.
2. Put regular files/directories temporarily into0600/0700, remove attributes not
   in the desired complete catalog, and set desired raw values. Access ACL is set
   last because it can remove owner write permission. Raw names/empty/binary
   values, ACL/default ACL and security/capability names are not silently dropped.
3. Set final mode (including ACL mask semantics), then exact access/modified times
   using the held descriptor and AT_EMPTY_PATH/NOFOLLOW. No path-based mutation
   fallback exists. Kernel normalization or privilege restrictions can reject a
   recipe; they never become a partial-success receipt.
4. Read every actual attribute and scalar again, require complete Values equality,
   and recheck the selected full path with only its exact observed post-operation
   revision substituted. All ancestor identities/revisions/mounts remain strict.

Symlinks keep their literal target and immutable0777 mode. Owner/group/time calls
use their held O_PATH descriptor without following a target. Attribute reading
uses the existing guarded no-follow reader. Requested changes to symlink
attributes are rejected before any metadata mutation: current NODE's xattrat
AT_EMPTY_PATH on O_PATH returns EBADF, and mutating through a replaceable symlink
name could touch an unrelated link. This limitation is explicit, not silent
attribute loss. Already-identical attributes are preserved and strictly reread;
nonempty symlink attribute profiles have no positive NODE fixture acceptance.
Unsupported kernels/filesystems/permissions may also reject
AT_EMPTY_PATH times or exact scalar/attribute readback. Symlink attribute mutation remains pending.

An attempted syscall sets mutation_attempted before invocation, even on error.
After a failure the object can retain partial metadata; no cleanup, undo or
implicit retry runs. The coordinator must mark its effect uncertain and retain
it for explicit reconciliation. Failure never changes any recorded inventory.
Success returns identity/mount/exact revision/full metadata digest as historical
readback data, not durable state or fresh source content. A subsequent global
capture/barrier must hash payloads: metadata time writes can mask the timestamp
of a concurrent content write. This method does not claim to detect all payload
changes itself. Multiple checks are sequential, not a filesystem snapshot.

Name substitution before mutation rejects. A later substitution affects only the
original held inode; final name/ancestor checks reject the receipt and preserve
both the original and replacement. Symlink attribute mutation is refused rather
than using that name. Concurrent same-inode writers can still race verification.
Cooperative Control pauses/cancels between metadata operations; kernel calls have
no bounded deadline. No new production worker/timer or preview decoder is added.

Full staged tree creation, source binding, approval/helper integration, ownership
ledger/recovery/undo, broader exact-metadata filesystem acceptance and UI Apply
remain pending. Bulk namespace/journal scaling is unaccepted. PSS/Swap/CPU/startup
experiments remain frozen, and native/global gates remain open.

Linux API cross-check: [kernel xattr implementations](https://github.com/torvalds/linux/blob/master/fs/xattr.c),
local linux/xattr.h and asm/unistd_64.h. The owned O_PATH capability probe is
recorded in the evidence directory; it is a functional syscall probe, not a
performance or power-loss experiment.

Use this route for Target::Stage values. Shared visible-directory Desired values
with AfterOwnChildren policy require their separate own-effect/mtime ledger; this
primitive does not implement that policy and must not restore its audit baseline
mtime onto a visible directory. Complete staged object creation and its positive
provenance remain future work.

NODE validation:1548 workspace tests passed (641 core,112 helper,795 UI; three
historical ignored), with eight new metadata fixtures. Raw/binary/empty attributes,
extra removal, actual access/default ACLs and final modes, symlink times/target
preservation, stale/kind/owner/unsupported preflight, seven fault boundaries,
early/late name substitution and cooperative pause/resume/cancel are covered.
The existing owned rename fixture now tests Basic retaining old attributes and
named ACL entries while overlaying source mode, and Full on a type-changing tree
with raw attributes on hardlinked leaves. Manual fixture creation is not a durable
creation/metadata-intent ledger or native approval acceptance. The first workspace
admission was deferred(exit75), not a failed test or a running Files build.

Final Clippy workspace/all-targets with warnings denied passed. No compile, test
or lint failure occurred in this checkpoint; gate admission deferral and package
cache waits are recorded without touching competing processes. No release or
installation was performed; all five installed hashes match the accepted dry-run.

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
