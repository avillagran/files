# Complete private staging coordination

SOURCE ONLY, NODE, 2026-11-02. `Journal::stage_batch`, controlled and observed
variants now connect the existing owned primitives for the complete batch:
read-only parent admission, fresh sources, durable workspace factory, ordered
creation, full captures, durable metadata, node observations, fresh sources
again, and the global staging barrier. They never publish/preserve/delete user
destination entries. Native approval, helper/absolute bindings for execution,
container selection/creation and the destination/recovery coordinator remain
unfinished. Tickets, reports and returned receipts are not approval.

## Admission and reads

A batch must be new/unstarted in the Journal's staging phase. Physical and
Prepared's original record must match the saved complete intent exactly.
`StageBatchReport::inspect` uses existing geometry/placement metadata admission,
then predicts regular **tree payload** reads with checked u64 arithmetic:

`required = 2*S + C + V + (3 + 2*M)*W`

- S sums unique regular dev/inode bytes separately inside each original root;
  both source snapshots run once before creation and once after node recording.
- C counts copied leaders/independent files. V counts verification of every
  staged regular name, including hardlink aliases read back by the creator.
- W sums unique created regular objects separately inside each workspace.
  Hardlink cohorts deduplicate within a walk, not across workspace walks.
- M is the exact planned staged-metadata order. Each step walks all scopes twice;
  initial capture adds one walk and the global barrier adds two.

A single six-byte file requires54 payload bytes. Independent six-byte x/y names
require120. A retire-only old three-byte file requires6 and creates no new leaf.
The total caller budget is cumulative, not silently reset per primitive. It may
exceed the existing1TiB per-call limit: no new global1TiB restriction is imposed.
Each source root must fit its single-pass cap and2*W must fit the existing shared
two-pass metadata/barrier cap, admitted before artifacts. Overflow rejects.

A substep reserves its known payload allowance before attempting it; successful
receipts/capture counters must match. Failure reports **reserved** bytes, not an
invented count of partially completed reads. EOF probes, symlink literals,
attributes, metadata/journal/registry files and filesystem bookkeeping are not
this payload metric. No process memory, throughput or wall-clock acceptance is
implied. Repeated global metadata walks and journal readback remain potentially
quadratic and bulk scaling remains unaccepted.

## Ownership and order

Actual private/disjoint/same-mount parent and raw-name absence checks precede
original-source hashing. They are read-only. The factory repeats its own fresh
checks and keeps its point-in-time FD admission. The registry and tag34 remain
durable before any workspace mkdir. Controlled original snapshots now pass
Control through their full data walks, including pause/resume/cancel.

Creation follows Physical node order, using live receipts for cohort leaders.
Only leaders whose last member is still pending are retained; aliases and
independent receipts drop immediately, and a leader drops at its last alias.
This implements the earlier FD-retention forecast instead of retaining one
receipt per node. Every creator checkpoint verifies the selected absolute
container/name/root object. Root relocation therefore fails before a later
private write even when a held root FD would still be valid elsewhere.

Once all creation finishes, initial source snapshots and creation receipts drop.
Each obsolete empty workspace inventory drops BEFORE the fresh full capture,
avoiding an extra retained root per transaction. Shared parent handles survive
without dup. Metadata follows the sealed leader/non-directory/deepest-directory
order and supplies ALL mutable workspace owners to each existing durable
metadata primitive. All absolute workspace locations are checked at each
mutating metadata boundary. Metadata's all-owner commit/error guarantees remain
unchanged; there is no atomic transaction spanning the entire coordinator.

Node observations are read from complete final captures and checked by the
existing ordered Journal declarations. Each declaration checks its selected
absolute location; global checks remain before/after the barrier without adding
an N*T sweep at every declaration. Both original roots are then freshly rebound
and fully verified again, preserving canonical symlink access evidence. The
global staging primitive performs its verify/sync/reverify/readback/fsync/ACK.
Locations and Journal/Registry ownership are checked again before return.
These sequential checks are not a filesystem snapshot or lasting source/path
freshness: the future destination coordinator must revalidate before effects.

The returned StagedBatch owns complete inventories, shared absolute locations,
both final original-root NamespaceSnapshots and the historical barrier receipt.
`into_parts` transfers these to the next trusted phase. Public observed callbacks
report coarse admission/source/node/metadata/barrier checkpoints, can return an
error, and supply no native approval or persisted resume authority.

## Failure and history

Before private effects, admission/source failures leave the Journal's durable
owner usable. After factory/private effects, errors poison the owner as Uncertain
and preserve directories, artifacts, the last completed inventories/locations
and the original rich underlying error. Failure may have dropped live root/leader
FDs; the registry and recorded object data remain inspection aids, not recovery
permission. Partial metadata can already exist; later failure can follow a final
StagingComplete ACK. No rollback, delete, truncate or automatic retry occurs.
A newly reopened/renewed history cannot restart this batch. This API does not
establish native approval or replace explicit recovery/undo policy.

## Validation and remaining work

Eight new fixtures cover exact budget+explicit subsequent owned publication,
raw trees/empty directories and regular/symlink cohorts within/across workspaces,
budget/private-parent rejection before hashing/artifacts, eleven coordinator
fault checkpoints, pause/resume/cancel, late source changes, absolute root rename,
IndependentNames, retire-only staging and three real process exits107 at node,
metadata and global barrier boundaries. Destinations remain unchanged until the
separate publication call. Exits reopen History and registry locations; they do
not simulate power loss.

Failure chronology: initial checks exposed unused diagnostic imports/fields and
then an obsolete namespace wrapper, corrected by public coarse progress and the
controlled snapshot path. Initial tests needed a Debug implementation for the
returned batch. First4 had3PASS/1FAIL because the read forecast initially counted
hardlink names instead of walk-local inode deduplication; corrected to S/V/W/C
and added both same-workspace and cross-workspace aliases→4PASS. Expanded7PASS/
1FAIL retirement fixture chose row0 (root metadata); selecting the actual x row
and resolving pending root metadata corrected it→8PASS. Full results and raw
failure logs are in [evidence](evidence/directory-sync-staging-coordinator/assessment.json).

Container selection/creation, production queue and fresh native/source/helper/
absolute approval bindings, visible SharedDir AfterOwnChildren, destination
coordination/recovery/undo and UI Apply remain open. No new preview format,
release, installation, GUI/native/global or resource/startup acceptance belongs
to this checkpoint. Full goal and resource/startup freeze remain active.

NODE workspace validation:1,607 passed (700 core,112 helper,795 UI; three
historical ignored), eight new fixtures. Full creator→staging coordinator→owned
publication regression passed. Five installed hashes remain unchanged. Normal
Rust harness and original idle Cargo gate; no competitor control or release/
install/GUI/resource/startup experiment.
Final Clippy workspace/all-targets with warnings denied and workspace format
check passed on the final source.

Cross-workspace publication now has an opt-in
[whole-batch namespace observer prerequisite](DIRECTORY-SYNC-NAMESPACE-BATCH.md).
This does not yet implement the destination batch coordinator.
