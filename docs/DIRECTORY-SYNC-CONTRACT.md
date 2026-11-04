# Directory synchronization review and execution contract

C02 synchronization remains in development. `directory_sync` prepares a fresh,
read-only review in the core. Source now includes versioned helper transport and
a supervised background reader plus a visible read-only dry-run, documented in
DIRECTORY-SYNC-UI.md. Executor, queue submission and recoverable Apply remain
pending; the dry-run is not complete synchronization. The installed comparison
is documented separately in DIRECTORY-COMPARE-UI.md. A resolved core review is
not an execution token. The full explorer goal remains active.

## Review semantics implemented

`prepare` takes two absolute raw paths, optional captured root identities, a mode,
comparison limits, cancellation and a progress sink. It runs a new bounded
inventory/hash comparison instead of accepting an earlier comparison report.
Review fields, observations and decisions are private; there is no public
constructor accepting an arbitrary old Report. The validated transport collector
derives a read-only review from checked observations. This proves stream/intent
coherence, not filesystem freshness against later writers or Apply authority.
Read-only accessors expose roots, mode, observations, proposals and decisions.

| Mode | Source-only names | Destination-only names | Shared differences |
| --- | --- | --- | --- |
| Update A to B / B to A | Copy proposal from chosen source | Retain | Explicit conflict |
| Mirror A to B / B to A | Copy proposal from chosen source | Explicit recoverable-removal conflict | Explicit conflict |
| Bidirectional merge | Copy proposal from whichever side has the name | Same rule on both sides | Explicit conflict; neither side wins automatically |

Bidirectional merge has no last-sync baseline. It cannot infer an intentional
deletion or choose a newer winner from a timestamp. Mirror is directional and
retirement of destination-only names always requires an explicit decision.

Content, literal symlink-target, type and basic metadata conflicts are distinct.
Shared directories whose children differ are containers, not whole-tree replace
proposals. Their own differing mode can require metadata review independently of
child decisions. A source-only directory, destination-only mirror directory or
type-conflict directory covers its descendants once; covered rows retain their
observations but cannot be independently resolved. Selecting Skip for a covering
conflict preserves that subtree. Retained destination-only Update subtrees also
have no child operations. Empty directories and non-UTF8/control names survive.

Every conflict starts Pending. Allowed decisions are Pending, Skip, preserve the
old destination and replace it from a chosen side, preserve and set basic metadata,
or preserve and remove a mirror destination-only object. Choices are checked
against conflict type and direction, applied to one row, and can be reset. There
is no overwrite-all, newer-wins or implicit conflict acceptance. These names
specify required recovery behavior for the future executor, not implemented
backup creation. No decision performs filesystem mutation.

Any incomplete/changed/unsupported/unreadable/aliased comparison blocks **every**
proposal, even otherwise known missing names. `blocked()` is separate from the
number of unresolved conflicts: zero conflicts does not imply a usable plan.
Read/data/path/item/depth limits are inherited from comparison without silently
truncating a complete plan. A failed consumer or cancellation returns an error.
The root paths must not contain `..`, coincide or canonically contain each other;
final root symlinks are rejected. Root identities are checked after comparison,
including after its terminal progress callback, and against any captured intent.
Canonical containment and unequal device/inode do not prove independence from
bind mounts or nested mount aliases. No atomic snapshot or blocking-syscall
cancellation deadline is claimed. Basic metadata is not ACL/xattr/owner equality;
root metadata and hardlink topology are not replicated by this review.

The core retains the bounded comparison rows, one proposal/choice per row, two
root paths and a subtree scope stack. It reuses raw paths from the comparison
rather than duplicating names into operation requests. There is no new dependency
or resource benchmark. This is an internal preparation milestone, not full sync.

## Existing engines inspected and reuse boundaries

| Existing component | Useful contract | Missing synchronization behavior |
| --- | --- | --- |
| `copy_engine` | Bounded worker, literal links, staged/exclusive no-clobber publication, cancellation/reporting | No destination overwrite, merged-tree transaction or crash-durable sync journal |
| `multi_origin` | Raw path admission, pinned source groups, parent ancestry and mount checks, effective-engine proof checks | Not an arbitrary recursive synchronization manifest or permission to replace/remove |
| `copy_review` and transfer queue | Explicit review, captured settings, bounded job queue, active pause/cancel and retained ACK | No sync choices, replacement recovery artifact or whole sync lifetime |
| `rename_transaction` | Same-directory no-replace staging, identity checks and process-failure undo | Not a crash-durable cross-root sync transaction; never blind rollback over replacements |
| `trash` | Managed recoverable retirement and identity-aware restore | Not a general old-destination backup/publication journal |
| `move_manifest` / `move_journal` / `exdev_move` | Durable per-root intents/barriers, content verification, replay/recovery, no deletion on uncertain ACK | Existing journal is a single-root move with source deletion semantics, not a sync batch or its undo |

The next implementation must reuse these contracts where they actually apply.
Routing CopyFrom proposals alone into the copy queue would only add missing names;
it would not implement the promised synchronization modes or resolve conflicts.

## Required next stages before Apply

The source-only `directory_sync_manifest` layer now captures pinned parent/root
and per-object revision/content/metadata/mount/link-group evidence, reusing EXDEV
inventory contracts. See DIRECTORY-SYNC-MANIFEST.md for verification and bounds.
DIRECTORY-SYNC-RECORD.md now records its canonical evidence format/seal and
revalidated live comparison binding. DIRECTORY-SYNC-PLAN.md now supplies canonical
logical decisions and their seal, and DIRECTORY-SYNC-PHYSICAL.md derives complete
selected staging/preservation scopes, cohorts and metadata-value requirements.
DIRECTORY-SYNC-METADATA.md now streams/revalidates actual values and checks
cohort compatibility. DIRECTORY-SYNC-STORE.md now persists complete intent and
values with owned-file readback and fresh artifact barriers. New operator
approval, the effect journal, helper/UI integration and execution effects remain pending; existing dry-run choices never become Apply tokens by inference.

1. Transport and visible dry-run: supervised fresh preparation, strict bounded
   rows/summary/choice transport, captured root/mode/budget intent and one-reader
   ACK admission. Expose all directions and modes, Pending/conflict/blocked state,
   exact raw paths, subtree coverage, filters and source/destination effects.
   Root/mode edits invalidate decisions; stale paste, action and job receipts
   cannot affect a replacement review. Native input/clipboard is a separate gate.
2. Capture an execution manifest with pinned roots/parents and per-object source
   and destination revisions, content/link/metadata requirements and checked
   absence. Validate mount topology, bind/ancestor aliases and cross-root hardlink
   effects. Recheck before every effect and after publication. Existing hashes or
   a Review alone never authorize overwriting, metadata changes or retirement.
3. Implement recoverable replacement and removal, not plain overwrite/unlink.
   Persist operation ID, reviewed intent, destination backup/stage identities and
   explicit state before moving an old destination. Stage new content separately;
   no-clobber publish and verify it. Sync files, relevant parents and journal
   barriers before calling an effect committed. A foreign object at any name is
   preserved. Metadata updates need the old metadata and checked object identity.
4. Integrate the existing bounded queue/owner lifecycle, with cancel/close ACK,
   per-effect progress and partial results. Cancellation does not erase committed
   work or lose backups. A failed/uncertain journal ACK prevents further effects.
   Do not claim a whole-tree atomic transaction or rollback where none exists.
5. Recovery and undo: list bounded operation journals, explicit inspect/retry/
   restore/discard, identity-checked recovery without overwriting later user data,
   and retained manual-recovery locations when an object cannot be identified.
   No automatic resume on startup. The recovery catalog must distinguish crash,
   cancellation, committed effect, published-unsynced and uncertain effect.
6. Owned-fixture acceptance: all modes/directions, content/link/type/metadata,
   hidden/raw paths, empty/nested directories, mounts/aliases, changed source and
   destination, no-clobber races, cancellation at each durable barrier, injected
   I/O failures, crash/reopen/undo and normal close. Then UI on each backend and
   installed hashes. Never run unattended Apply against personal folders.

The current read-only review does not satisfy these stages by inference.
PSS/Swap/CPU/startup experiments and new preview formats remain frozen. Native
input, clipboard, AT-SPI, 2x, other OS, global lifetime and offline syscall gates
remain pending.

## Earlier core-only verification (aa38963)

Twelve new core tests pass within the full workspace:1371 passed
(482core/109helper/780UI), plus3 historical ignored. Workspace all-target Clippy
with warnings denied and fmt pass under normal test concurrency. Evidence lives
in `evidence/directory-sync-review/assessment.json` and the retained full log.
The first focused run passed8/9: its read-limit fixture used unequal file lengths,
so comparison correctly needed no hash reads and stayed complete. Equal-length
unequal content exercises the intended read-limit branch; the corrected focused
run passed11/11, and the final root-progress-sink test passes in the full12-test
suite. The initial unused-method warning was removed without suppression.

Owned fixtures also verify fresh observations after an older equal comparison,
all modes/directions, typed choices and reset, literal links, raw names, empty and
covered subtrees, no writes on choices, read/item partial inventories, changed
files during hashing, replaced roots, terminal-callback root replacement,
cancellation, consumer failure, captured-identity mismatch and root containment.
These are core tests, not native UI, helper supervision or execution acceptance.
No new installation occurred; all five installed-file hashes still match the
comparison checkpoint1793571784648344250. Source builds use the existing idle
Cargo snapshot gate with one build job, without serializing the Rust harness or
controlling competing Cargo processes. No resource experiments were run.


## Earlier helper transport and supervised reader (e963a2a)

`prepare_directory_sync` accepts a nested version1 Intent: request ID, two raw
absolute roots, optional captured device/inode identities, explicit mode and a
hashed-data budget. Nested intent and frame schemas reject unknown fields. Path,
version, mode and budget admission occurs before starting the background reader;
canonical topology and filesystem operations remain in fresh helper preparation.
CLI command and Request variant must match. The helper keeps its existing96MiB
address-space limit; that limit is not a measured memory or resource gate.

Each JSONL frame carries version and mode around a strict comparison observation
frame with the intent ID. The collector requires exactly the requested version,
mode, ID, optional root identities and read budget, ordered namespace/pages,
valid kind/difference/ancestor/summary shapes and monotonic counters/reasons.
Errors poison the collector; a prior terminal does not permit trailing frames.
Captured or returned identical roots are rejected. Partial reports are retained
as blocked reviews, never silently converted into missing-name actions.

Wire rows contain observations, not replacement/removal commands or trusted
proposals. After complete validation, the receiving worker derives proposals,
subtree coverage and fresh Pending choices locally using the same core semantics.
`into_observation` explicitly discards decisions and consumes a review into
read-only history for helper emission. This is not a reverse Apply conversion.

Pages remain128rows/4096raw path bytes; encoded frames remain64KiB. Version/mode
wrappers have a128-byte allowance. At most16384 singleton pages,512 progress
frames and one terminal yield the derived whole-stream cap:
`21237760 + 128 * (16384 + 512 + 1) = 23400576 bytes`.
A control-name maximum test exercises both full retained inventories beyond4MiB
without truncation. Only this command receives the new allowance; comparison
retains21237760B, ordinary commands4MiB and duplicates its previous exception.

The new background Job streams bounded arbitrary byte fragments, publishes live
progress, and returns a Review only after EOF, successful child exit and reap.
A terminal frame by itself, EOF while the process lives, nonzero exit, malformed
bytes or cancellation cannot publish a review. Cancellation/Drop request the
existing supervisor to kill and reap; no filesystem walk, hash or join runs on
UI. The owner must retain the Job through its result ACK before another intent
or window close. At e963a2a Model/admission/close integration was still pending; the later
DIRECTORY-SYNC-UI.md records the visible owner, private job module, controls and
its separate runtime acceptance. Worker tests do not prove global helper
lifetimes or offline syscall interruptibility.

NODE verification passes1388 workspace tests489core/112helper/787UI plus3
historical ignored, all-target Clippy-Dwarnings, fmt and a debug helper build.
Seventeen new tests cover all modes, derived choices, strict intent/envelope,
poisoning/version/mode/ID/root errors, raw Request roundtrip, full retained wire
budget, real helper page/sink behavior, fragmented/oversized/trailing bytes and
real fake-helper process exit0/exit9/EOF-without-exit/cancel-after-terminal reap.
The existing scoped supervisor budget regression also exercises this command
without widening ordinary responses. Evidence and actual CLI receipt scope are
recorded in `evidence/directory-sync-wire/assessment.json`.

The visible fresh dry-run is now integrated with captured mode/root/budget
stamps, Pending/conflict/blocked/covered states, per-row choices and retained
single-reader ACK admission/cancel/close; see DIRECTORY-SYNC-UI.md for owned
acceptance. Apply still needs the pinned object manifest and recoverable journal/
execution/recovery stages above. Missing-only copies do not complete C02. No
resource measurement or global acceptance follows from transport tests.

Source-only logical planning now derives all five modes, typed conflicts, subtree
coverage, metadata policies and canonical choices from fresh sealed evidence.
See DIRECTORY-SYNC-PLAN.md. Resolved plans/live bindings are not approval; the
physical metadata resolution, helper/UI review and durable recoverable execution
remain open; DIRECTORY-SYNC-PHYSICAL.md records the source-only blueprint.

Actual metadata values and historical inspection now have source-only acceptance
in DIRECTORY-SYNC-METADATA.md; a hash or completed Write is not an owned/durable
store or approval. Live acquisition and journal/worker ownership must remain
distinct before Apply.

The SOURCE ONLY preparation store now retains complete evidence/choices/budget
and values across process exit, with identity/readback/fsync and no fresh barrier
on reopen; see DIRECTORY-SYNC-STORE.md. A durable artifact is still neither
operator approval nor an effect journal or current source proof.

DIRECTORY-SYNC-INDEX.md now records bounded per-object preparation-value lookup
and final staging metadata postorder. These data/order primitives remain
separate from the effect journal, approval and checked user-data mutations.

## Sequenced effect declarations (source foundation)

DIRECTORY-SYNC-JOURNAL.md describes a preparation-bound event state machine and
owned Linux persistence. It records staging/preservation/publication/directory
metadata/undo declarations with strict sequence and digest-chain validation.
The writer acknowledges only after file and namespace synchronization and fresh
bindings. A failed ACK poisons the owner; every reopen starts historical.
Approval tickets and observations remain caller declarations. This foundation
does not execute effects, authenticate a human action or recreate source pins.
The installed dry-run and all outstanding execution/native/global gates remain.

Source-only `DIRECTORY-SYNC-REBASE.md` adds full opt-in inventory reconciliation
following a caller-identified own root rename. It rereads content, metadata and
mounts, allows only root ctime change and updates paths only after full success.
Other hardlink snapshots remain strict. Parent namespace/alias mutation ledger,
positive rename provenance and actual execution are still pending.

Source-only `DIRECTORY-SYNC-ALIASES.md` adds exact rename-transition reconciliation
across supplied alias inventories. Full rewalks and final root checks precede an
all-or-nothing update of recorded revisions; unrelated entries remain strict.
Physical duplicate names, inconsistent old evidence and any revision other than
the exact observed new revision are rejected. This does not supply own-rename
provenance, parent namespace accounting or authority for an effect.

Source-only `DIRECTORY-SYNC-NAMESPACE.md` adds a borrowed expected-rename plan
across one/two disjoint inventories, full before/after checks, exact child-name
and parent attribute predictions, directory nlink models and an all-owner commit.
Execution scopes can hold the full union of two bounded inputs (16,385 entries)
without widening original input captures, formats or helper response caps.
Returned exact rename evidence composes with known-alias reconciliation. The
caller still owns positive syscall provenance, no-clobber execution, durable
intent/barriers, root absolute bindings and visible approval/helper lifetime.

DIRECTORY-SYNC-OWN-RENAME.md documents the explicit owned namespace primitive:
physical preservation/publication role and full scope checks, durable event27
intent before one FD-relative NOREPLACE syscall, postchecks and data/parent/journal
fsync before positive-call receipt. Current application does not invoke it.
Global actual staging, native approval/live helper roots, complete queue/undo/
recovery and scaling remain separate acceptance gates.

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

Checkpoint 2026-11-02: [whole-batch staging admission](DIRECTORY-SYNC-ADMISSION.md)
now checks every `new`+`old` workspace and the destination-name envelope before
new journal creation and owned effects. Retained FD slots are a policy forecast;
workspace placement/factory and actual host/transient FD admission remain open.

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

Checkpoint 2026-11-02: [private staging coordinator](DIRECTORY-SYNC-STAGING-COORDINATOR.md)
now connects complete owned creation, metadata and global barrier with cumulative
payload admission, bounded leader receipt lifetimes, shared parents and fresh
source checks. Destination execution, native approval and recovery are separate
unfinished phases; no installed Apply or bulk/resource acceptance follows.
