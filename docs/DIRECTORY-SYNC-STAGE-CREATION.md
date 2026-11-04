# Journal-bound staged node creation

Source-only Linux trusted-coordinator primitive. Journal.create_stage_node creates
one selected physical node under an already established private workspace. The
controlled form uses existing pause/resume/cancel checkpoints. It never publishes,
replaces, deletes, truncates, rolls back or automatically resumes a name. Installed
sync remains read-only. This is not the complete staging/native-approval/recovery
coordinator.

The caller must establish the workspace's owned provenance, durable ancestor
bindings and correct same-mount placement, explicit current approval/helper and
fresh original scopes first. Workspace declarations are not proof of these. This
API checks the supplied held private0700/effective-owner root against its exact
recorded identity/mount and walks planned parent names with NOFOLLOW, checking
all declared parent identities/mounts and private modes. Source snapshot root,
index, normalized raw relative path, object/revision/content/metadata must match
the sealed source node. Actual metadata/path pins are freshly checked. Metadata
readers retain the existing canonical symlink-atime policy. Whole source-scope
freshness and absolute path bindings remain coordinator responsibilities.

Two strict journal events extend v1 with tags28/29: NodeCreateIntent(node,seal),
37 body bytes, and NodeMaterialized(node),5 bytes. The seal binds Store proof,
node index, source/root observation, destination kind/device/mount, workspace,
raw planned ancestor/name paths and optional existing cohort identity. Existing
192B event and256MiB journal caps remain unchanged. Older readers reject unknown
events; old declaration-only streams remain readable as history. Replay checks
order/data, never filesystem truth or approval.

A pending create guard blocks another creation, verification or advancement.
After full intent readback/fsync/ACK, the implementation rechecks source, parent,
owner, control and guard before attempting exactly one name-creation syscall:

- A regular leader uses O_CREAT|O_EXCL|O_RDWR|NOFOLLOW/NONBLOCK/NOATIME. Its returned
  descriptor identifies the inode actually created. After identity ACK its mode
  is normalized through that FD to0600, without changing process umask.
- A directory uses mkdirat0700, then a no-follow descriptor and empty/private
  postimage checks. Masked owner permissions/incompatible inherited ACLs may fail
  rather than silently broadening an unknown named directory.
- A symlink uses the exact bounded raw literal read from the held source link
  descriptor; no target is followed. Literal hash and later readlink must match.
- An additional cohort alias requires a live non-decodable leader receipt from
  this same Store/Journal identity and process, actual held inode/mount and exact
  currently known link count. linkat through that held descriptor's proc magic
  link is followed to its inode, not a mutable old raw filename. No source input
  inode is linked into staging. NODE fixtures cover regular AND symlink cohorts
  across two workspaces and prove the symlink target's link count/data stay intact.

Creation evidence is deliberately typed. ExclusiveFile identifies an atomic
O_EXCL-created inode; LinkedHeldDescriptor identifies the held leader inode.
NamedPostimage for mkdirat/symlinkat means a successful syscall followed by an
observed current name, not proof that no same-user actor replaced that name in
the gap before descriptor acquisition. The syscall has no returned inode FD.
Wrong kind/mount/owner, nonempty directory, changed literal or unexpected links
reject, but an equivalent empty named replacement can be indistinguishable.
This limitation is not native provenance acceptance and is not hidden behind
an unconditional ownership claim. Native coordinator policy remains unfinished.

NodeCreated receives durable ACK immediately after binding the observed identity
and before regular payload writes. A regular leader then streams64KiB chunks,
checks exact length/EOF and SHA against the original captured source, and reads
back the actual staged FD under the same expected-size bound plus one EOF byte.
Each regular copy and readback is separately capped by the caller's read argument
(at most1TiB per pass); returned copied/verified counters are separate payload
counts, not resource measurements. An alias skips copying but verifies its data.
Directories are empty scaffolds at this point; later selected children establish
the final content digest. Symlink literals are reread without following targets.
File/directory data and the containing parent are synced, source/path/owner checks
run again, then NodeMaterialized receives durable ACK before a private live receipt.
The final exact metadata pass and all-workspace staging barrier remain mandatory.

Failures distinguish acknowledged intent, attempted creation syscall, kernel
success and acknowledged identity. A false ACK flag can still leave complete
journal bytes. Errors after intent writing/attempt poison the owner as Uncertain;
partial data and all observed names remain. A pending intent blocks replay/retry
even after artifact History renewal. Open/renew alone never resumes execution or
reconstructs a live leader receipt. After a complete materialization ACK the guard
may be clear despite a later failed callback; that is history, not automatic
recovery authority. No power-loss or remote-filesystem proof is claimed.

Metadata reproduction still needs its own durable write-intent integration.
Workspace creation/placement, complete batch quotas and FD admission, source /
helper/native approval lifetimes, SharedDir AfterOwnChildren, explicit recovery /
undo and UI Apply remain unfinished. Repeated source/namespace checks and journal
readbacks have no bulk performance acceptance; existing quadratic scaling gate
remains open. No new preview decoder/production worker/timer, resource/startup
experiment or installation belongs to this checkpoint.

NODE validation (2026-11-02):1556 workspace tests passed (649 core,112 helper,795
UI; three historical ignored), with eight new creator fixtures. Initial four
then seven then eight new fixtures passed, plus one existing EXDEV test selected
by the filter. Coverage includes empty and131073-byte files, rawFF/newline tree
children through the complete pipeline, file/symlink cohorts across workspaces,
codec cuts/order, ten injected fault boundaries, collision/source/budget checks,
substitution/no-overwrite, Control and three actual process exits89 at creation,
identity ACK and materialization ACK. These exits do not simulate power loss.
Initial E0433/E0599 missing imports were corrected and retained. The original
idling gate was preserved; only the ignored waiting wrapper's deadline increased
from180 to900 seconds to observe exact competing PID/start identities longer.
Competitors were never controlled; an idle snapshot is not exclusion of later
Cargo/cache-lock competition. No new release or installation occurred.

Final source audit corrected the symlink branch's syscall_attempted flag before
symlinkat and added a raced symlink collision assertion. The corrected source
again passed1556 workspace tests. Readback length is bounded before each read,
not only checked after an oversized chunk. Whole-batch output shape/name/wire
preflight and descriptor admission still belong to the pending coordinator; a
per-node receipt alone does not prove a complete tree fits later capture limits.

Final all-target workspace Clippy with warnings denied passed on the corrected
source. Five installed hashes remain unchanged. No power-loss, UI/native input,
resource or global acceptance is inferred from these scoped fixtures.

Later source checkpoint: [durable staged metadata](DIRECTORY-SYNC-METADATA-JOURNAL.md)
now binds exact ordered metadata intents/after ACKs, revalidates all workspaces
and commits all alias owners together after durable confirmation. Complete owned
fixtures use this route; workspace factory, native approval/helper/absolute
bindings and recovery/UI remain pending. Replay/declarations never grant authority.

Checkpoint 2026-11-02: [whole-batch staging admission](DIRECTORY-SYNC-ADMISSION.md)
now checks every `new`+`old` workspace and the destination-name envelope before
new journal creation and owned effects. Retained FD slots are a policy forecast;
workspace placement/factory and actual host/transient FD admission remain open.

Checkpoint 2026-11-02: [owned workspace factory](DIRECTORY-SYNC-WORKSPACES.md)
now creates complete batches under pre-existing private disjoint same-mount
containers, with durable intent/identity/materialization and a point-in-time FD
headroom check. Container selection/creation, integrated coordinator/FD lifetimes
and native/source/helper/absolute approval bindings remain open.
