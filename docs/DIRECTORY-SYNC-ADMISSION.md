# Whole-batch staging admission

SOURCE ONLY, NODE, 2026-11-02. `Physical::staging_admission` checks the
complete sealed batch without filesystem access. The report supplies geometry
and a defined retained-FD forecast; it grants no approval, reservation, workspace
provenance, current source binding or execution authority.

Each transaction gets a staging-only shape and the union of `new` plus `old`
under its private workspace root. This union bounds preservation, publication
and reverse transitions by containment. The directory-promoted union of both
original inputs bounds every destination intermediate: current physical rows
always preserve their original logical target names. A future mapping that
changes that invariant is rejected until it has a different proof.

The shared capture limits remain 16,385 entries, 2 MiB retained relative names,
16,760,832 wire bytes, relative path 4,096 bytes, depth 33 and 4 MiB pending
name work per scope. Input limits remain unchanged. Wire forecasts include a
worst-case 255-byte workspace basename. Byte counts use raw names, including
non-UTF-8 bytes. New joined names are charged before allocation. Duplicate
paths and missing/non-directory parents reject. Union conflicts promote a
leaf to a directory solely for the envelope, never as an actual tree result.

Pending-name work includes active ancestor child lists: bare child name bytes,
`(2 * children + 32) * sizeof(OsString)` for each nonempty directory and the
largest child's recursive peak. This upper bound covers the capture walker's
minimum-32/doubling quota policy; it is not a resident-memory measurement.
The existing entry/name/depth limits keep even this conservative global bound
below 4 MiB on NODE. Capture and preflight share the namespace quota constants.

A valid input can still fail staging admission. In particular, replacing a
short logical root with `new` or `old` can exceed 4,096 bytes, and holding both
subtrees can exceed the retained-name budget. These are explicit refusals
before partial staging; no capture limit was widened. Workspace placement and
an alternative layout that admits such cases remain coordinator work.

The retained-FD policy holds two descriptors per workspace during creation,
plus current receipt and hardlink leaders until their last cohort member.
It drops other receipts promptly. Metadata with any steps reserves three
per workspace for original root, shadow root and distinct parent; the report
takes the larger phase total. `require_fd_slots` compares a supplied staging
allowance only. Prepared sources, Store/Journal, unrelated process FDs,
transient operations and safety headroom are excluded. There is no OS slot
reservation or automatic descriptor release. A future coordinator must follow
this retention policy and perform actual host/headroom admission.

Schema caches the report or rejection. New Journal creation rejects before
opening/creating its file. Owned creation, metadata, staging barrier and
namespace effects recheck this admission before their effect intent. Existing
journal headers remain readable and renewable as History even if their geometry
is no longer admissible. Manual event declarations remain data only. No journal
wire version, event cap, original capture counters or approval meaning changes.

Validation includes raw paths, exact entry/name budgets and first excess,
remapping/depth, parent graphs, directory promotion, ancestor pending work,
replace/add/retire and cross-workspace hardlink receipt slots. A real FD-walked
4,096-byte source path is accepted as input but rejected when remapped into
`new`: no journal artifact appears; a legacy empty header remains readable.
Full validation and failure history are in
[evidence](evidence/directory-sync-admission/assessment.json).

Workspace-root factory/placement/durability, actual live FD admission,
SharedDir AfterOwnChildren, native queue/approval/helper and absolute bindings,
recovery/undo and UI Apply remain open. Repeated full walks and journal readback
still lack bulk-scaling acceptance. No release, installation, native input,
resource/startup experiment or global acceptance follows from this checkpoint.

NODE validation: 1,572 workspace tests passed (665 core, 112 helper, 795 UI;
three historical ignored), including eight new tests. Initial test compilation
caught wrong test-only enum spelling and a missing mutable binding; corrected
before the focused/full passes. Five installed hashes remain unchanged.
Clippy workspace/all-targets with warnings denied and workspace format check
passed. All Rust commands used the original idle Cargo gate and normal harness.

Checkpoint 2026-11-02: [owned workspace factory](DIRECTORY-SYNC-WORKSPACES.md)
now creates complete batches under pre-existing private disjoint same-mount
containers, with durable intent/identity/materialization and a point-in-time FD
headroom check. Container selection/creation, integrated coordinator/FD lifetimes
and native/source/helper/absolute approval bindings remain open.

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
