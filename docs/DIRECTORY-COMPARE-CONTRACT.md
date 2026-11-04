# Directory comparison and synchronization

C02 remains open. The comparison tool now captures two local roots recursively and lets keyboard and mouse users inspect differences and locate either side. Synchronization still requires a fresh plan, explicit conflict review and operation-specific recovery. A comparison result never authorizes mutation. See DIRECTORY-COMPARE-UI.md and its assessment for current runtime acceptance.

Implementation sequence: bounded Linux comparison core, strict helper transport/supervisor and visible root selection/results/diagnostics are integrated. Owned functional acceptance is recorded separately from the foundation histories below. Synchronization planning/execution/conflict recovery remains pending; a foundation-only checkpoint was never a complete explorer tool.

`directory_sync` adds fresh review preparation with directional
Update/Mirror and bidirectional Merge, typed per-row conflict decisions, subtree
coverage and whole-review blocking on partial observations. DIRECTORY-SYNC-UI.md
now records the visible dry-run integration. This is not an Apply token or a
complete synchronization executor. DIRECTORY-SYNC-CONTRACT.md records the
engine reuse audit, helper/transport verification and remaining pinned execution
manifest,
recoverable replacement/removal, durable journal, queue and recovery requirements.

Comparison includes hidden names, empty directories, regular-file SHA-256 content, literal symlink targets and basic mode/mtime differences. Non-UTF8 names remain raw. No symlink traversal, device/FIFO/socket reads, deletion, writes or shell commands. Hardlink names remain separate namespace entries. SHA equality is a cryptographic observation rather than a byte-for-byte proof or atomic snapshot. ACL/xattr/owner equivalence is not implied.

Both roots are captured by device/inode and pinned. Linux descriptor walks reject intermediate symlinks. Enumeration records revisions and directory stamps, checks them again after hashing and detects root replacement. Root ancestors may be symlinks, as with the existing checked-resource API. Mount boundaries are allowed; bind aliases are skipped explicitly. Cancellation checks and limits cannot interrupt an already-blocked filesystem syscall; helper supervision is still required before UI integration.

Each inventory has at most8192 entries/1MiB retained relative paths plus symlink targets,100000 enumeration attempts,32 levels. Comparisons have a shared default1GiB hashed-data budget (configurable within a1TiB hard bound; checked-resource EOF sentinel reads are additional),64KiB hash chunks and explicit partial reasons. Result namespace is bounded by both inventories. Unknown, unsupported, changed, unreadable and omitted results never imply equality or absence. If either inventory is incomplete, unmatched names are Unknown rather than OnlyLeft/OnlyRight. A later detected directory change also revokes absence classifications. Shared directory rows aggregate differing/unknown descendants; partial inventories cannot certify a directory subtree. Post-observation changes remain possible; synchronization must capture and validate fresh evidence.

No PSS/Swap/CPU/startup experiments, new preview formats, dependency changes, publication or source push. Full goal, native input/clipboard/AT-SPI/2x/other OS and global gates remain open.

## Foundation evidence on NODE

12 new core tests pass. Workspace1323=459core/105helper/759UI plus3 historical
ignored; Clippy workspace/alltargets-Dwarnings and fmt pass with normal test
concurrency. `evidence/directory-compare-foundation/assessment.json` records scope
and logs. A strengthened one-shot consumer-failure test reproduced an incorrectly
swallowed BrokenPipe as a partial report; the original error now propagates and
the red/green evidence is retained. Directory summaries propagate descendant
differences and uncertainty. That foundation checkpoint was source-only: installed app/helper then remained the
accepted CopyPath checkpoint. Later wire and UI integrations are recorded below.

## Helper transport and supervisor integration

The source adds `compare_directories` with two raw/Unicode roots, optional captured
root identities and an explicit hashed-data budget. It keeps the existing64KiB
request limit and96MiB helper address-space limit. Neither is a measured whole
process resource gate.

The new `directory_compare_wire` ledger validates canonical relative paths,
strictly increasing unique namespace order, per-side8192/1MiB retention bounds,
kind/difference shapes, ancestor types and descendant summaries, phase/counter
coherence, latched reasons, intent ID/root identities and the requested read
budget. An invalid frame poisons the complete ledger. A terminal error cannot
follow result pages; terminal reports are not published merely on receipt.

Pages contain at most128rows and4096raw path bytes. JSON frames are at most64KiB.
Strings can expand to six bytes per control byte, versus four for raw byte arrays.
The whole stream has a derived hard byte budget:
`6 * 2MiB + 512 * 16384rows + 512updates * 512B + 4096B = 21237760B`.
The row allowance covers page envelopes even for singleton pages. This admits
both complete retained inventories instead of silently dropping rows at4MiB.
Only this command receives that supervisor allowance; ordinary commands remain
at4MiB and the existing duplicates exception remains unchanged.

The background job rejects oversized request intents before starting a worker,
validates byte-fragmented JSONL, and releases a final report only after successful
process exit, EOF and reap. Cancelled, malformed, failed or timed-out readers
cannot publish a report. Drop requests cancellation; the owner must still retain
and drain the job's ACK before another intent. The job module is now private and integrated with directory_compare_ui. Model
retains its slot through ACK, including window close; modal/reader admission and
thumbnail/size/SpaceMap coordination have owned controller/runtime coverage.

Transport verification on NODE passes1346 workspace tests470core/109helper/767UI
plus3 historical ignored, all-target Clippy-Dwarnings, fmt and a debug helper
build. Seven owned real-helper CLI cases pass in
`evidence/directory-compare-wire/helper-wire-1793567017338791814.json`.
The23 new tests cover full retained control-name wire budgets, malformed typed and
fragmented JSON streams, per-command supervisor bounds, successful/failed exits,
EOF without exit ACK and cancellation/reap. Initial1346testsPASS followed by two
Clippy int_plus_one failures is retained; equivalent inequalities fixed the lint
and the final full checks pass. See `assessment.json` for source/helper hashes.

Cargo admission was deferred while specific external Cargo identities remained
live. Waiting only checked those identities and retried the original gate; no
competing process was controlled, no gate bypassed, no Rust harness serialized.
Admission establishes an idle snapshot, not exclusion of later external Cargo.

At transport49396f4 the background job was compiled/tested scaffolding without
Model/UI or a new installation. The later DIRECTORY-COMPARE-UI.md and
evidence/directory-compare-ui/assessment.json record visible integration, scoped
acceptance and the installed checkpoint. Synchronization remains pending.
