# Directory synchronization effect declarations

Source foundation, not an enabled Apply command. The installed sync UI remains a
read-only dry run. This module does not create staging directories, copy user
objects, rename destinations, change their metadata, or undo operations.

`directory_sync_journal` derives a bounded schema from the saved record, logical
and physical plans and indexed actual metadata values. Events declare approval,
workspace creation, node identities and verification, staging synchronization,
old-object preservation, publication, directory metadata and undo. A declared
approval ticket is data: it does not prove human approval or grant authority.
Observations and scope hashes are also declarations, not filesystem verification.

The state machine requires all transaction workspaces and staged nodes to be
recorded and verified before any destination effect. Old objects require a
preservation and backup synchronization declaration before publication. Effects
are sequential, followed by shared-directory metadata. Undo reverses committed
transactions and then restores directory metadata. An unresolved effect must be
reconciled explicitly before undo can begin. There is no automatic retry or
recovery executor. Completion and undo retain private workspaces/backups for
explicit later inspection; neither state promises artifact cleanup.

The stream has a preparation-proof-bound header and sequence-numbered,
SHA-256 chained event frames with distinct hash domains. Strict streaming replay
rejects invalid transitions, wrong identities/intents, sequence gaps, corrupt
frames, trailing bytes and torn records. It never repairs or truncates files.
Hashes detect changes; they do not authenticate an untrusted writer.

On Linux, `directory_sync_store::Journal` holds the metadata Store borrow and its
own exclusive file lock. It reuses private-directory, no-follow ancestor walks,
mount, owner, regular-file, single-link, namespace-binding and creator-PID guards.
Creation is exclusive; existing names are never reused. Each append checks the
transition before writing, then rereads the complete chain against the expected
new sequence, length and tail digest, synchronizes the file and directory
ancestry and checks bindings again before acknowledging the new state. The
post-write stamp is captured before readback, so an intervening rewrite cannot
be accepted merely as the writer's own timestamp change. Complete readback on
each append is deliberately part of this foundation; it is not a throughput
claim. A failed write or ACK
latches Uncertain until drop. Reopen starts History; explicit renewal verifies
the complete stream and synchronizes it again. Neither renewal nor a durable
journal establishes source freshness or authorizes an effect. An external caller
must retain/check original journal identity when deciding whether a reopened
file is the same artifact; replay alone cannot establish that provenance.

Structural bounds: 192 bytes per event body, 589,848 events and 256 MiB per file.
These are format budgets, not measured PSS, CPU, startup or latency evidence.
Full physical execution, own-change revision accounting, hostile namespace
reconciliation, crash recovery UI and native approval remain pending.

Validation: 1,502 workspace tests pass (595 core, 112 helper, 795 UI), with three
historical ignored tests; workspace Clippy all-targets with warnings denied and
format check pass. Nine new tests cover five-mode staged/publish/metadata/undo
replay, invalid ordering/cohorts/identities, corrupt/torn/reordered streams,
exclusive ownership, cancellation, empty plans, failed write/readback/fsync/ACK,
prior-record tampering during append and actual process exit at three barriers.
These exercise owned fixtures, not native input, power-loss guarantees or user
file execution. Evidence and historical compilation/style failures are retained
in `evidence/directory-sync-journal/assessment.json`.

## Explicit own-namespace primitive (later source checkpoint)

DIRECTORY-SYNC-OWN-RENAME.md adds an opt-in trusted-coordinator route on the owned
Journal: strict physical role/scope binding, durable NamespaceIntent(event27),
exactly one FD-relative NOREPLACE syscall, full postchecks/data+parent fsync and
journal confirmation before publishing recorded inventories. It returns opaque
positive-call data. Reopening remains History and does not reconstruct source
freshness or native approval. Earlier event-only checkpoints above remain
historical; global staging, native approval/helper wiring, complete execution,
recovery UI and bulk scaling are still pending.

Later source checkpoint: `Journal::confirm_staging` now supplies an actual
all-workspace verification/fsync/reverification/ACK route before StagingComplete;
see [staging barrier](DIRECTORY-SYNC-STAGING-BARRIER.md). Manual append is still
a declaration API. This does not implement object creation/metadata reproduction,
visible approval/helper/absolute bindings, complete recovery, or installed Apply.

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
