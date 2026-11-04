# Shared-container batch preparation

SOURCE ONLY, NODE, 2026-11-02. The owned factory prepares a complete layout
before opening container pins or writing artifacts. This advances coordinator
integration; it does not supply native approval, automatic container selection,
source/helper/absolute bindings, recovery or installed UI Apply.

## Grouping and admission

`WorkspaceBatchReport::inspect` is pure layout data over the sealed Physical
and caller parent paths. Exact normalized raw bytes identify repeated parents.
The grouping map uses byte slices, not Path comparison that could normalize dot
components. Invalid dot/dotdot/repeated/trailing separators, NUL, relative paths,
oversized components, absolute paths over4,096 bytes and depth over128 reject.
Different raw paths remain separate even if they might resolve to an alias or
the same inode. Each absolute binding must retain its own proof.

Before component/path clones, container descriptors or artifacts, preparation
charges a16MiB requested-metadata budget:

- One transaction slot and pre-reserved returned receipt header, the canonical
  46-byte basename, group index and borrowed unique-path slot per transaction.
- One Directory/Arc header, raw path clone, exact requested component payloads
  and OsString headers, ancestor Node array and parent-vector Arc slot per
  distinct raw path.

Directory component/chain arrays and layout/receipt vectors request exact
capacities. Root receipt reservation uses a fallible allocation after admission.
This is an explicit structural subtotal: it excludes allocator overhead,
lookup/exclusion sets and transient verification buffers, kernel FD storage,
caller-owned paths, existing Schema/Prepared/Store/Journal/Registry data and
inventory entry payloads governed by their capture limits. It is not a resident
memory measurement or process memory acceptance. Valid sync inputs may require
a different container layout to satisfy this budget; sharing containers admits
large batches without duplicating each parent's metadata.

The pure maximum-count fixture accepts16,384 transactions sharing one4,096-byte,
128-component parent without opening that path. A set of distinct deep parents
exceeds the aggregate budget before any container is opened. All original
capture, record, registry and journal limits remain unchanged.

## Pins, exclusion checks and FD policy

Each unique parent opens once into an Arc<Directory>, with existing own0700,
NOFOLLOW and device/inode/mount chain verification. Every transaction still gets
its own canonical name and intent seal. Source directory/ancestor exclusion and
root-identity sets are built once from the complete immutable record; membership
and each candidate chain replace repeated full source-manifest scans. Bind-alias
exclusion by device/inode and the Store ancestor checks preserve earlier refusals. Store overlap now also
compares underlying device/inode across mount IDs, rejecting bind aliases of
the same protected Store directory. Planned target mount matching stays separate. Live parent/name guards before effects remain in place.

Let T be transactions, U distinct raw parent paths and R peak retained creation
receipts under the documented cohort-leader/drop policy. Creation needs T+U+R
retained descriptor slots; metadata with shadow roots needs2T+U (T+U without
metadata steps). The forecast is the larger phase total, equivalent to subtracting
T-U from the previous separate-parent forecast. The old conservative getter
remains available; invalid U values reject. The factory uses the grouped forecast
plus its existing164 transient headroom in the real RLIMIT/proc snapshot.
There is no FD reservation or exclusion of later consumers, and subsequent phase
admission must count all other already-held descriptors too.

`CreatedWorkspace::parent()` still borrows a File. `parent_handle()` and
`into_parts()` now provide a cloneable `WorkspaceParent`, which dereferences to
File and shares the container pin without a dup syscall. `verify()` checks its
saved absolute chain and private ownership. `into_parts` transfers the historical
inventory/name with this shared handle and has no fallible FD allocation. The
caller must drop obsolete empty inventories and creation receipts according to
the forecast before retaining fresh full captures and metadata shadows. Shared
handles are pins, not approval or lasting freshness.

The registry still serializes every transaction's complete path and chain in
the same FLSPLC v1 format. Sharing in memory never removes a row or aliases
independent raw bindings. The existing complete creator/metadata/barrier/rename
pipeline uses the shared handles through File borrowing.

## Validation and limits

Nine new fixtures cover maximum-count shared layout, exact metadata budget and
one-byte rejection, raw syntax/identity grouping, aggregate deep-layout refusal,
actual parent FD sharing and clone/into_parts/drop lifetime, distinct case/raw
bindings, cached exclusions with genuinely private input/ancestor/Store paths,
late replacement, a synthetic cross-mount Store-identity predicate (not an
actual bind-mount acceptance), and a child soft-FD limit that admits the grouped forecast
while refusing the old one. Existing Physical fixtures check grouped FD counts
and invalid U values. Functional FD counts are not a resource performance study.

Initial core check caught a missing public reexport (E0432), fixed. First focused
run had7PASS/1FAIL: the last close-count expectation omitted the Journal directory
FD. The fixture now compares to its pre-owner snapshot; eight passed. Initial
workspace pass covered1,598 tests. A final accounting audit added exact borrowed
reference/parent-vector slots before the final validation, then the Store-alias predicate was strengthened. Full results and
failure logs are in [evidence](evidence/directory-sync-batch-preparation/assessment.json).

Container selection/creation, the integrated production coordinator and fresh
native/source/helper/absolute bindings, SharedDir AfterOwnChildren,
recovery/undo/UIApply remain open. Full-scope walks and journal readback still
lack bulk-scaling acceptance. Resource/startup freeze and the pause on new
preview formats remain active. No release, installation, GUI/native/global
acceptance follows from this checkpoint.

NODE final validation:1,599 workspace tests passed (692 core,112 helper,795 UI;
three historical ignored), nine new fixtures. The original and accounting-audit
runs each passed1,598 before the extra Store-alias predicate fixture. Final
Clippy workspace/all-targets with warnings denied passed. Five installed hashes
remain equal to the accepted receipt. All Cargo used the original idle gate and
normal harness; competitors were untouched. No release/install/GUI/resource or
startup experiment ran.
Workspace format check also passed on the final source.

Checkpoint 2026-11-02: [private staging coordinator](DIRECTORY-SYNC-STAGING-COORDINATOR.md)
now connects complete owned creation, metadata and global barrier with cumulative
payload admission, bounded leader receipt lifetimes, shared parents and fresh
source checks. Destination execution, native approval and recovery are separate
unfinished phases; no installed Apply or bulk/resource acceptance follows.
