# Durable intents for staged metadata

Source-only Linux trusted-coordinator step. Journal.apply_stage_metadata and its
controlled form bind one metadata action to ALL private workspace inventories.
The installed sync UI remains read-only. Workspace creation/placement, native
approval, source/helper/session/absolute bindings and full recovery remain separate
preconditions, not authority supplied by journal declarations or these receipts.

Schema derives the exact Physical.stage_metadata_order: one non-directory cohort
leader, files before directories and directories deepest first. The step requires
all nodes created, no pending creation, no verified nodes yet and the next exact
metadata index. Every workspace root/name/private0700/effective owner/identity/
mount and every planned new name/object must match. Missing/extra/duplicate scopes
or external aliases reject. All content/size/link/owner claims must match the
sealed nodes, and already-confirmed metadata observations must match EXACTLY,
including revision/ctime. Capturing a fresh but changed object cannot silently
rebase an earlier confirmation.

The implementation obtains the exact indexed Target::Stage values from the owned
Store proof; AfterOwnChildren records are refused. It fully verifies and forks
all scope snapshots before writing intent. One shared regular-payload allowance
(max1TiB) covers pre and post walks across every workspace; known insufficient
space for both read passes rejects before an intent or metadata write. These
counters exclude bounded metadata/name IO and are not resource measurements.

Tag30 StageMetadataIntent carries node, exact before-observation and SHA seal
(body178B). The seal binds Store proof, desired digest, node, ordered workspace
roots/raw paths and every original observation. Full journal readback/file and
ancestor fsync/ACK precede metadata application. Tag31 StageMetadataApplied carries
the exact checked after-observation (146B). Existing192B/event256MiB/journal caps
remain unchanged. Older readers reject unknown tags; earlier declaration-only
streams remain history. Replay validates data/order, never human approval or
filesystem truth.

The existing pinned descriptor metadata applier checks the live journal owner
and exact pending guard at each checkpoint and before EVERY mutating syscall.
It reproduces raw values/ACL/mode/times and checks exact value readback. Candidate
inventories patch every known alias of that inode with the observed exact new
revision, metadata digest and desired access time; unrelated entries remain
strict. All candidates are fully rewalked, including payload hashes, names and
mounts, before the selected inode (except symlink) and containing directory are
synced. Symlink literals are never followed; canonical captured access policy is
retained across steps. Requested symlink attribute mutations remain unsupported.

Only after those checks does the after-observation receive durable ACK. Root and
all changed alias path/revision/mount guards run again, then every borrowed
inventory receives its candidate entries/mounts through a non-fallible commit.
All original inventories and capture counters remain unchanged on ANY error,
including an error after the applied record is acknowledged. The returned private
OwnStageMetadata is a historical receipt, not lasting freshness, a filesystem
snapshot or a reusable permission token. Sequential checks cannot lock out writers.

Failures separate acknowledged intent, attempted metadata mutation and acknowledged
applied record. False ACK flags may still leave complete bytes. Once intent writing
starts, any failure poisons the owner as Uncertain; partial metadata and objects
remain, with no rollback/delete/truncate/retry. A pending metadata guard blocks
another metadata action/verification even after History renewal. Once a complete
applied record exists, a later error can leave the guard clear; replay still grants
no recovery authority and cannot reconstruct the live receipt or owner snapshots.
Once tracked metadata begins, all planned metadata steps must finish before node
verification. Legacy manual append remains a declaration API, not positive proof.

The full owned sync fixtures now use creator→durable metadata→global staging
barrier→owned preserve/publish. Native queue/approval/helper/absolute root binding,
workspace factory/output shape-FD admission, visible shared-directory
AfterOwnChildren metadata and explicit recovery/undo/UI Apply remain unfinished.
Repeated all-scope payload verification and full journal readbacks can be
quadratic: bulk scaling remains unaccepted. No new decoder/production worker/timer,
resource/startup experiment, release or installation belongs to this checkpoint.

NODE validation (2026-11-02):1564 workspace tests passed (657 core,112 helper,795
UI; three historical ignored), eight new fixtures. Initial four→expanded eight
PASS: receipt/counters, sixteen fault boundaries, strict codec/pending History,
preflight quotas/missing scopes/fresh wrong payload, exact prior confirmations,
file/symlink cohorts in two workspaces, late failure/after-ACK atomic rejection,
Control, journal rewrite before mutation and three real process exits97 at intent,
value readback and applied ACK. Exits do not simulate power loss. Initial core
check passed with an unused diagnostic payload warning; flattened explicit points
removed the unused field. Final Clippy workspace/all-targets-Dwarnings passed.
All five installed hashes remain equal. No release/install/native/global or
resource acceptance is inferred.

Final audit moved the last fallible journal-owner check inside the batch's
pre-commit callback. No returned-error path now follows the all-owner commit.
A sixteenth injected boundary and journal rewrite just before owner publication
prove even a post-ACK failure leaves every original capture unchanged. The
corrected source again passed1564 workspace tests; original capture counters and
profiles remain intact.

Checkpoint 2026-11-02: [whole-batch staging admission](DIRECTORY-SYNC-ADMISSION.md)
now checks every `new`+`old` workspace and the destination-name envelope before
new journal creation and owned effects. Retained FD slots are a policy forecast;
workspace placement/factory and actual host/transient FD admission remain open.
