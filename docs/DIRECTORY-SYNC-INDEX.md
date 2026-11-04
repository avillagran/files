# Indexed synchronization metadata reads

The SOURCE ONLY preparation store now builds a bounded in-memory record index
while validating complete saved intent and metadata. It can retrieve one original
Stage, Directory or UndoDirectory record without reparsing the whole artifact.
The full goal remains active; effect journal, approval, executor and helper/UI
integration are still pending and the installed dry-run is unchanged.

## Trusted construction, untrusted history

The existing metadata inspector shares one semantic parser. Its ordinary public
inspection still checks the same bytes/rules and does not compute unused record
checksums. The internal indexed visitor additionally receives each original
validated record's stream offset, length and domain-separated checksum. It hashes
`Files:sync-index-record:v1\0`, the little-endian u64 stream-relative offset, the
exact10-byte typed header and every original value byte.

The index is never loaded from an offset table on disk. All slots remain local
until the full metadata stream passes intent/schema/recipe/cohort/seal/EOF checks,
its total length matches the artifact, and the store passes its existing proof
and file/namespace binding checks. Creation also compares with actual acquisition
and obtains the existing durability ACK. Reopen rebuilds the index and remains
History; indexing creates no fresh durability or source authority.

At most32770 slots retain offset,32-byte checksum, encoded span length and mtime
policy flag. A slot is48B on NODE; its maximum vector storage is1,572,960B,
excluding normal Vec/Index bookkeeping. Capacity is reserved for exactly the
derived count; attribute payloads are not retained. This is a structural memory
bound, not measured PSS, a process budget or a performance benchmark. No format,
serialized offset, helper stdout allowance or content-preview decoder is added.

## One bounded lookup

`Store::value(expected_proof, target)` requires the exact expected212B artifact
identity/intent proof and checks its current creator-PID, private file/name,
revision, directory and mount bindings. Target mapping is constant-time numeric
indexing: Stage nodes first, then each directory's Undo/Desired pair. Out-of-range
Stage/Directory/Undo indices are rejected before any payload read.

The getter uses positional I/O over exactly the indexed span, at most131594B
(header10 plus existing131584B value allowance). It does not move the shared file
cursor or read the saved intent, other objects or trailer. Partial system reads
and EINTR can require multiple underlying syscalls; the requested byte range
remains the one record. It checks expected tag/index/flag/length and the original
record checksum, decodes the canonical bounded values and checks file binding
again after the read. Record offset is included in the checksum, so relocation
cannot substitute for a valid slot. Read-only calls can run concurrently against
one owner without sharing a seek cursor.

`ValueRecord` retains target and explicit AfterOwnChildren policy with the values.
The flag for a shared directory's desired Basic metadata is distinct from its
literal undo record. The stored audit mtime is not an instruction to overwrite
natural child-publication mtime when that flag is set.

These are original-snapshot data checks, not a new whole-artifact checksum on
every access. Other records and the trailer are not reread by a selective lookup.
Normal file revision checks revoke a modified owner, while the independent slot
checksum protects the requested record even when a timestamp detector is treated
as unavailable/coarse. This does not establish an atomic filesystem snapshot,
provenance, current source contents or freedom from all concurrent writers.

History and Uncertain owners may read recognized historical values; neither can
produce a durable barrier by doing so. A caller must still obtain a fresh store
barrier, current source proofs and explicit approval before future effects.
Dropping the owner or original source paths does not make parsed data an Apply
capability. Visitors/readers remain bounded by their own retention choices.

## Final metadata order for private staging

`Physical::stage_metadata_order()` returns each non-directory cohort leader once,
then staged directories with children before parents. All selected IndependentNames
inodes remain in the order. It computes stage-parent depths once and sorts numeric
keys; it does not repeatedly parse names during sorting. All creation/content and
hardlink steps must already be complete before this final metadata pass. Shared
existing-directory metadata retains its separate physical postorder/root-last
list, after publication of child effects.

The order can retrieve each selected object's values directly from Store instead
of retaining all attrs in RAM or replaying the whole artifact for every object.
It describes a future executor requirement, not implemented metadata application,
staging, fsync, no-clobber publication or recovery.

## NODE acceptance and remaining work

Tests compare reverse/random indexed reads against full inspection, preserve the
shared cursor, exercise concurrent readers, distinguish desired/undo policies,
check proof mismatch and post-read binding revocation, corrupt header/body/offset
and bypass a controlled timestamp stand-in, retain History/Uncertain semantics,
reopen without original inputs, check empty preparation and cohort/IndependentNames
postorder, and query beginning/middle/end of the full16384-record artifact>8MiB.
A read-span counter verifies one bounded record request; it is not a latency,
CPU, startup or memory measurement. Full regression results are recorded in
`evidence/directory-sync-index/assessment.json`.

Next is the owned sequenced effect journal referencing the complete preparation
proof, new visible approval/live helper/pins/ACK, own namespace/revision/nlink/ctime
ledger, global source staging and verified metadata reproduction, recoverable old
preservation/no-clobber publication/barriers, bounded queue/pause/cancel/close ACK,
partial/uncertain results and explicit crash/reopen/recovery/undo. Untouched
hardlink topology still needs explicit review. Owned fixtures only.

No new release, installation, native UI or global acceptance follows. Absolute
PSS/Swap/CPU/startup freeze and new-preview pause remain in force; original X11,
other-OS, syscall-deadline and native/global gates remain open.

NODE final validation:1493 workspace tests (586core/112helper/795UI), three
historical ignored,11 new indexed-read/order tests. All-target Clippy-Dwarnings
and fmt PASS with normal harness concurrency. Initial27 focused Store tests
passed; final suite adds the empty-index case and IndependentNames order check.
No failed tests or lints in this change. Existing saved-intent/crash/fork tests
also pass with index construction enabled.
