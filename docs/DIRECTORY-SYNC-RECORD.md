# Canonical synchronization evidence record

The source-only `directory_sync_record` format preserves the freshly captured
manifest evidence for review and future journal comparison. It does not approve
a plan, execute changes, write journals, create backups, publish names or recover
files. The installed dry-run remains unchanged. Full C02 and explorer goals remain
active; DIRECTORY-SYNC-PLAN.md records the subsequent source-only logical plan.
Explicit approval and recoverable effects still follow.

## Encoding and seal

Version1 uses fixed little-endian integers and raw path bytes, with no native
struct padding, JSON escaping, lossy Unicode conversion or filesystem I/O during
reading. The first eight bytes are `FLSYNC\0\x01`. The header binds request ID,
mode (Update A→B/B→A, Mirror A→B/B→A or Merge), read limit, charged preparation
payload and optional captured root device/inode identities.

Each root carries its original absolute path, resolved parent path, parent mount,
bounded ancestry and complete root-relative entries. The root itself has an empty
relative name. Each entry has its raw name, kind/device/inode, full revision,
access time, content digest, metadata digest and mount ID. The EXDEV internal
root-name prefix is not duplicated into canonical descendant names. Internal and
external hardlink counts are derived from these records, not trusted from a
second independent table.

A trailing32-byte SHA256 is computed over the domain
`Files:sync-evidence:v1\0` followed by every preceding byte. It detects unresealed
corruption and distinguishes intent/policy/evidence changes. It is not a MAC,
signature, provenance proof or operator approval: someone can recompute a seal for
well-formed fabricated history. A content/metadata digest claim can be structurally
valid without proving that any file was read. Live binding checks are separate.

`Prepared::record()` preflights the exact encoded size and reserves that size,
then validates its own output. `Record::decode` checks the total byte limit,
seal and complete borrowed structure before retaining an owned copy. `view()`
returns borrowed names and bounded numeric/index tables. Parsing can succeed
after all original directories have been removed; it never reopens paths.

## Structure and bounds

The decoder rejects unknown versions/modes/flags, trailing or truncated bytes,
invalid raw absolute/relative components, NUL/traversal/duplicates/order errors,
missing/non-directory parents, invalid identity/kind/time/mode/nlink/mount fields,
incoherent root identities, parent/ancestor cycles and physical directory aliases.
Shared canonical parents must agree on their mount and full ancestry and cannot
name two distinct roots with the same basename. The ancestry chains share a
namespace root; optional captured identities must match their root entries.

Every directory's child-name digest is recomputed from the complete canonical
namespace, including empty directories. Repeated regular-file identities must
agree in revision/content/access/metadata. Symlink access canonicalization is
consistent within each side; cross-side access observations can differ as in the
underlying inventory. Nlink bounds apply across both roots. The charged payload
must equal twice the sum of unique regular-file lengths per side (initial capture
and final rehash), within the requested limit≤1TiB. Hashes are observations, not
byte-for-byte equality or permission to mutate shared/external hardlinks.

Bounds are8192descendants plus root per side,1MiB relative-name bytes per side,
4096-byte paths,33 descendant components and128 ancestors per root. Absolute
root/parent paths use their byte bound and normalized components; they do not
consume the depth budget of the content below a selected root. A fixed entry
costs155bytes plus its name. The complete byte bound is:

`2*1048576 + 2*8193*155 + 4*4096 + 2*128*16 + 256 = 4657718`.

This supports both complete inventories beyond4MiB without truncation. The256-byte
allowance covers fixed header, optional root IDs, root envelopes and seal. Name
and numeric/hash/index buffers are bounded logical allocations, not measured PSS
or a whole-process resource gate. No helper response allowance changes here; a
future transport must explicitly support this full record instead of applying
an ordinary4MiB cap. Existing FLMOVE/EXDEV journal formats remain separate.

## Live binding and journal comparison key

`Prepared::bind_record` first requires exact canonical-byte equality with its
own captured record, not merely matching seals. It then revalidates both roots,
parents, namespaces, revisions, content, metadata and mount evidence using the
existing shared per-call read budget. The returned `BoundRecord` borrows both
live Prepared pins and the Record; there is no decoder for that runtime type.
It is not a guarantee against changes after verification, an immutable snapshot,
operator approval or a fresh durability barrier. Effects must recheck their
objects and reconcile known own changes through an explicit operation ledger.

A bound record can produce a56-byte `EvidenceKey`: eight-byte `FLSBND\0\x01`,
nonzero16-byte job ID and32-byte record seal. `StoredKey` reads comparison data
only and can compare it with a live-produced key. A matching stored key can remain
matching even after the filesystem changed, while a new `bind_record` correctly
fails. Neither key type authorizes writes or reconstructs live descriptors.
The new journal must also bind the resolved action-plan/decision seal, fresh UI
approval identity, owned journal/stage identities and durable barriers. This key
alone is deliberately insufficient.

## Remaining implementation

Build a canonical action plan for all five modes from fresh evidence, including
additions, merged directories, replacements, metadata and recoverable retirement.
Bind the actual reviewed choices and options; do not convert old dry-run decisions
or decoded history into approval. Transport and the new explicit review must own
pins/ACK lifetimes and invalidate stale requests. Then add recoverable no-clobber
effects and journal intent/backup/publication/fsync states, bounded queue pause/
cancel/close and partial/uncertain outcomes, crash/reopen/undo without overwriting
foreign replacements or automatically resuming. A metadata digest cannot restore
old ACL/xattr values: preserve old values or the complete old inode.

This is an internal evidence codec, not a new content preview format/decoder.
No release/helper/UI build, installation, PSS/Swap/CPU/startup experiment or global
acceptance is implied. All freezes and native input/clipboard/AT-SPI/2x/otherOS/
global lifetime/offline syscall gates remain pending.

## NODE verification

1425 workspace tests pass (518core/112helper/795UI) plus3 historical ignored,
including16 new codec/live-binding tests. Workspace all-target Clippy-Dwarnings
and fmt pass under normal Rust concurrency. The tests exercise all modes and
policy changes, raw paths/complete proof roundtrip, both full8192-child inventories
beyond4MiB with exact-size retained buffers, and real roots on different filesystems.
They reject truncation, unresealed corruption, impossible resealed fields, invalid
namespace/parents/ancestors/mounts/counts, broken directory digests and incoherent
hardlinks. Cross-side symlink atime canonicalization survives roundtrip.

Tests also deliberately accept well-formed resealed content/metadata claims as
history while rejecting their binding to the held capture. Matching StoredKey
can stay matching after a file changes while a new live binding fails. Offline
record decoding succeeds with the original paths removed. This explicitly proves
the separation between data integrity/coherence and live filesystem authority.

An initial check failed type inference for the borrowed parent-index map; its
index type is now explicit. A1424-test run passed before one collapsible-if lint,
fixed without suppression. A deeper audit found absolute root paths were wrongly
using the descendant depth predicate. A real40-level selected-root fixture failed
red; absolute normalization/byte bounds are now separate and the final1425 suite
passes green. The16-test corpus includes the deeper shared-parent ancestry check.
All source/failure/green logs and scopes are in
`evidence/directory-sync-record/assessment.json`.

No release/helper/UI build or installation occurred. All five installed dry-run
artifact hashes remain at71eb40f/24b44a7, installation1793574522897173033,
app289db327/helper170f995/HTML04aa7953202d. No resource experiment, preview decoder
or global acceptance is inferred from codec tests.
