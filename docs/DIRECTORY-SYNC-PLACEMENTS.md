# Durable placement registry

SOURCE ONLY, NODE, 2026-11-02. The owned factory now writes a complete placement
registry and acknowledges it before any workspace mkdir. This makes its saved
locations inspectable after process exit. It does not implement recovery/undo,
native approval, fresh source/helper/absolute binding or installed UI Apply.

## Format and bounds

One exclusive private0600 file per Store job is named
`sync-placements-<jobhex>.registry` under the private Store directory. It holds:

- Header256B: `FLSPLC\0\x01`, exact212B Store proof, physical seal32B, count32-bit.
- One row per physical transaction in order: path length16-bit and absolute raw
  parent bytes, ancestor count16-bit and every device/inode/mount triple24B,
  then the transaction's existing workspace intent seal32B.
- SHA256 trailer32B over header and rows. The exact workspace basename derives
  canonically from the header's job and row index: `sync-<jobhex>-<indexhex8>`.

Paths are normalized absolute raw names with no NUL, empty/dot/dotdot or oversized
components; maximum4,096 bytes and128 components. The root adds one ancestor,
so at most129 triples. Every inode and mount ID must be nonzero. A row is at
most7,228 bytes; at most16,384 rows fit below the128MiB artifact cap, including
header/trailer. Length/count checks bound each row allocation; the aggregate byte limit is
checked before artifact creation. Original sync capture and journal limits stay unchanged.

Writing and complete readback stream one row at a time. The only retained index
has offset/length/original-row SHA per row; it is rebuilt from a validated whole
stream, never deserialized from persisted offsets. `entry(index)` preads at most
one bounded row, checks its original SHA and owner/name/revision binding before
and after reading. This is a structural storage/IO bound, not a PSS or timing
measurement. All parents are admitted by the factory before this file is opened.

## Ownership and barriers

The file uses O_EXCL/NOFOLLOW/NONBLOCK/NOATIME/CLOEXEC and a positive returned
regular-file FD, normalized to0600; nlink1, effective owner, file/mount identity
and exact held/named revision are required. It shares the Store's pinned
ancestor chain and uses the existing creator-PID/flock ownership discipline.
Neither existing files nor incomplete streams are overwritten or truncated.

The writer remembers its expected digest and captures its write revision BEFORE
readback/callbacks. Complete canonical readback checks the Store proof, physical
seal, exact transaction count, each parent device/mount, canonical basename and
transaction seal, total length, digest and EOF. File then Store-ancestor fsync
and an ACK precede Durable state. Same-byte rewrites after the captured revision
reject too. Any failure preserves complete or partial bytes for inspection.

Reopen validates the file into History without opening a single saved parent
or workspace path. It can describe missing/moved containers. `renew` performs a
new complete readback against the previously observed seal/index and checks all
file/Store barriers. Failed renewal becomes Uncertain and requires reopening;
renewal cannot recover native approval or prove a saved location is current.
`Placement::open_parent` explicitly reopens the normalized absolute chain with
NOFOLLOW, exact ancestor IDs/mounts and own0700 checks. Its returned pin is a
fresh sequential observation, not a lasting binding or permission to execute.

## Journal integration

After registry ACK, tag34 `PlacementRegistered` (body33B) durably anchors its
whole-file seal before tag32 `WorkspaceCreateIntent`. Newly written tag32 declarations require that anchor. Older unregistered tag32
streams remain readable as historical data; the writer refuses to add new
unregistered intents even after renewal. A registered batch cannot be silently restarted. Legacy
declaration-only histories remain data; older readers reject the new tag.

The live factory keeps the registry owner. Registered journals' `ensure_current`
requires that live current owner with exact proof and original registered seal.
All existing owned effect guards therefore also notice registry owner/revision
changes. On reopening, the Journal remains inspectable without a registry;
Journal renewal alone does not satisfy execution guards. The caller must open
and explicitly renew a matching registry, then `attach_placements`. This binds
artifact barriers only. Native approval and current source/helper/absolute
workspace checks remain the trusted coordinator's responsibility. SHA and replay
are data integrity observations, never signatures or human authorization.

A well-formed resealed historical registry can be fabricated as data. It fails
attachment to an unchanged journal anchored to a different seal. Rewriting all
historical data is not an alternate route to approval; no decoder grants it.

Factory error flags still describe workspace mkdir/tag32/identity/materialization.
They do not claim that no registry file or tag34 exists: registry writes can
precede those flags. Once registry creation is attempted the Journal is poisoned
on failure; partial/full artifacts and completed workspaces remain. No cleanup,
implicit retry/resume or rollback is introduced. The registry adds one live FD
to the Store/Journal artifact group; factory admission's transient headroom covers
its creation. Future phase admission must count it and obey receipt lifetimes.

## Validation and remaining work

Eight new fixtures cover raw-history lookup, explicit renewed attachment,
container relocation with continued history readability, exact maximum row and
first excess, malformed/truncated data, sparse oversized-file refusal, eight
write/readback/fsync/ACK fault boundaries, uncertain renewal, same-byte rewrite,
registry tamper before mkdir forged data versus the original journal anchor, and legacy unregistered tag32
readability versus strict new writing.
The ten factory fixtures also pass with registration; the three real exit103
cases inspect saved locations after reopening. These exits do not prove power
loss. Complete factory→creator→durable metadata→barrier→owned publication is
included in workspace regression coverage.

Initial check passed. The first new-test run had43PASS/1FAIL: the maximum-path
fixture counted the leading slash twice and built4,097 bytes. Corrected to4,096,
the44-test filter passed, including the first seven new fixtures. A compatibility audit then preserved
old tag32 history parsing while tightening the new writer, adding an eighth
fixture; Clippy requested the equivalent fixed-array chunk API. Full results and
preserved logs are in [evidence](evidence/directory-sync-placements/assessment.json).

Container selection/creation, integrated coordinator with actual descriptor
lifetime management and fresh native/source/helper/absolute bindings,
SharedDir AfterOwnChildren, recovery/undo and UI Apply remain unfinished.
Repeated full-scope walks and journal readback still lack bulk-scaling acceptance.
Registry streaming bounds do not establish a process memory budget: the factory
still retains a Directory per transaction. Aggregate metadata admission and
parent sharing belong to the production coordinator; no bulk/resource acceptance
is inferred. Resource/startup freeze and new-preview pause remain active. No release,
installation, GUI/native/global acceptance follows from this checkpoint.

NODE final workspace results: 1,590 passed (683 core, 112 helper, 795 UI;
three historical ignored), eight new fixtures. Initial1,589 passed before the
compatibility audit; the final45-test filter and1,590 workspace pass include
legacy-read/new-write separation. Five installed hashes remain unchanged.
Final Clippy workspace/all-targets with warnings denied and workspace format
check passed. All Cargo ran through the original idle gate and normal harness;
competitors were untouched. No release/install/GUI/resource experiment ran.

Checkpoint 2026-11-02: [shared-container batch preparation](DIRECTORY-SYNC-BATCH-PREPARATION.md)
now groups exact raw parent paths, bounds requested placement metadata before
pinning, shares parent handles and uses the grouped FD forecast. Registry rows
and per-effect absolute/name checks remain intact; no process-resource or native
execution acceptance is inferred.
