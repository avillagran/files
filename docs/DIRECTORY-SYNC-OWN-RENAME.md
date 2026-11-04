# Journal-bound own no-replace namespace operations

Source-only trusted-coordinator primitive. The installed sync UI remains a
read-only dry run. This adds actual Linux rename syscalls for preservation and
publication; it is not a complete synchronization/staging/recovery executor.
No personal file operations are run by development validation.

`Journal::execute_namespace` consumes a prepared namespace plan for one selected
transaction/action. The journal must be currently Durable, owned by this process,
and in the appropriate Effects step after the declared global staging barrier.
The caller still must establish ACTUAL complete staging and metadata verification
for all selected sources, visible human approval, live source/helper/session pins
and current absolute root/ancestor bindings. Journal declarations do not prove
these requirements; this API does not supply or bypass the application gate.

Before writing intent, the adapter validates the namespace plan against the exact
saved physical intent. Preservation moves the recorded destination-relative name
to `old` under the recorded private workspace identity. Publication moves `new`
from that workspace to the recorded destination name/parent. Workspace root mode
must be0700; root/parent IDs and mounts must match. Preservation checks the entire
old scope fingerprint against the saved record. Publication compares every staged
relative name/object/revision/content/metadata with its verified declarations;
unknown or missing names reject. Existing ctime-only allowances remain scoped to
rename/alias semantics, not a general freshness or approval rule.

A canonical SHA256 seal binds scope count, entry counts, all captured scope rows,
raw root/source/destination names and numeric endpoints. Event27 NamespaceIntent
records transaction, action and seal and receives full journal readback/file and
namespace fsync/ACK BEFORE any rename syscall. The action byte is strict0/1;
body38B remains below the existing192B event cap. Current replay accepts earlier
streams without this event as history; older readers reject unknown27 rather than
skip it. Neither variant reconstructs execution authority from disk.

After intent acknowledgement, the operation fully rechecks the original scope
snapshots under its remaining read allowance and acquires no-follow source and
parent descriptors. It rechecks the journal guard, actor PID, source identity and
revision and issues exactly one descriptor-relative renameat2(RENAME_NOREPLACE).
There is no overwrite, copy/unlink fallback or retry on errno/EINTR. Destination
collisions remain intact. Linux has no conditional rename-this-inode primitive:
a writer can change the name between the last check and syscall. A mismatched
postimage is detected, retained and reported uncertain; no automatic rollback or
cleanup risks overwriting a newer name.

After a successful kernel return, complete namespace/metadata/content/mount
rewalks must pass. The moved regular files and directories are fsynced in reverse
path order (children before ancestors); symlinks are never followed or fsynced as
O_PATH files, and their containing directory is synchronized. Both affected
parents are synchronized. Current bindings and the journal guard are checked
again before OldPreserved+BackupSynced or Published+EffectSynced receive durable
journal ACKs. Final namespace root checks precede all recorded-owner commits.
No root inventory advances if any callback, verification, fsync or ACK fails.

Success returns a private-field, non-decodable OwnNamespaceRename with exact
intent seal, action, transaction, acknowledged sequence and RenameChange. This
is a positive historical fact of this call's syscall return and checked ACK;
it is not present source freshness, a filesystem snapshot or human approval for
another effect. The same change can revalidate remaining known aliases.

Failures distinguish an acknowledged intent, a syscall attempted, and a kernel
success return. False intent_recorded does NOT imply no journal bytes: an intent
write/ACK can fail after visible complete bytes. Attempted with a negative errno
does NOT prove no remote-filesystem effect. Every failure after intent/write or
attempt poisons that journal owner. A late source-name substitution can cause
the syscall to move an unexpected object; a failing postcheck preserves both
that object and the actor-relocated original instead of claiming a receipt. No current retry, rollback, delete, truncate
or resume is attempted. Reopen is History; a pending NamespaceIntent blocks the
same operation even after artifact renewal. Explicit reconciliation/recovery is
separate work. Effects may remain preserved at the intended backup name or at
actor-changed names; recovery must inspect identities rather than trust paths.

Controlled execution uses existing cooperative Control checkpoints. Pause/cancel
before intent leaves the journal usable with no syscall; cancellation after a
recorded intent becomes uncertain. Blocking kernel calls still have no bounded
deadline. No production worker/thread/timer is added.

`Prepared::namespace_snapshot` forks verified original entries and held root/
parent handles for mutable execution data. It preserves captured symlink atime
evidence instead of silently replacing it with a later readlink-induced value.
The original sealed intent remains immutable. A snapshot is not approval or a
lasting filesystem view; explicit byte allowances and later guards still apply.

Global staging/metadata reproduction, live approval/helper integration, absolute
root bindings, explicit undo/recovery UI, and bulk scaling remain pending. Full
namespace walks and full journal readbacks can be quadratic across large batches;
this mechanism has no mass-operation performance acceptance. Kernel power-loss,
other filesystems, native input/clipboard/AT-SPI/2x/lifetime/deadline and global
acceptance remain open. Resource experiment freeze and preview pause stay intact.

Validation on NODE (2026-11-02): 1,533 workspace tests passed (626 core, 112
helper, 795 UI), with three historical ignored tests. Nine new owned fixtures
cover preserve/publish/add/retire, ten fault boundaries, no-clobber collisions,
source substitution on both sides of the final guard, wrong role/scope, strict
intent codec/reopen, cooperative pause/cancel, and three real process exits83
after syscall/data sync/confirmation. These exits do not simulate power loss.
Clippy across all workspace targets with warnings denied and workspace format
check passed. Initial E0432
export syntax and E0373 scoped test closure compilation failures are retained
with their corrected passing runs in the evidence directory. No release, install,
GUI acceptance, resource experiment or performance claim belongs to this step.

Later source checkpoint: `Journal::confirm_staging` now supplies an actual
all-workspace verification/fsync/reverification/ACK route before StagingComplete;
see [staging barrier](DIRECTORY-SYNC-STAGING-BARRIER.md). Manual append is still
a declaration API. This does not implement object creation/metadata reproduction,
visible approval/helper/absolute bindings, complete recovery, or installed Apply.

Later source checkpoint: [stage metadata reproduction](DIRECTORY-SYNC-STAGE-METADATA.md)
now applies exact desired raw values to pinned staged objects and checks readback;
existing owned rename fixtures use it before the global staging barrier. This
adds no staged tree creator/native approval/installed Apply and explicitly rejects
symlink xattr changes when no safe descriptor mutation route is available.
