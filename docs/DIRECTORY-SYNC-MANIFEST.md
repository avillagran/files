# Fresh synchronization manifest evidence

`directory_sync_manifest` is a source-only, read-only preparation layer. It does
not execute the installed dry-run choices, create stages/backups, publish names,
change metadata, retire objects or write a journal. It is not an Apply token.
The full C02 synchronization and explorer goals remain active.

## Captured evidence and binding

A new versioned Intent supplies raw roots, optional root identities, mode and
regular-file hash budget. The constructor accepts no previous comparison/report
or choices. It normalizes lexical separators/dots, rejects parent traversal,
identical roots, final-root links and physical root containment, then pins both
parent/root pairs. Ancestor symlinks are supported through checked parent
canonicalization. The original selected paths remain in Intent; redirecting an
ancestor alias invalidates the preparation.

Root binding checks reopen the current parent pathname and compare identities,
`statx` mount IDs, ancestors and root revision with the held Source. Checking only
a held descriptor would miss a bind mount over an otherwise identical parent.
Both roots retain one parent Source handle and one Inventory root handle: four
retained FDs structurally, not a measured whole-process FD peak. Ancestry, fresh
name checks and tree walks use additional bounded temporary handles. Leaves are
opened no-follow during walks, not pinned for the whole lifetime. Future effects
must reacquire checked parent/object handles and validate them before and after
each effect; no atomic snapshot or hostile-writer isolation is claimed.

The inventory reuses `exdev_source`: raw paths, kind, device/inode, size, mode,
uid/gid, mtime/ctime/nlink revisions, original access time, regular SHA256, literal
link-target digest, directory child-name digest and complete bounded metadata
hash (including xattrs/ACL/security attributes). Access/link observations inherit
its no-atime behavior. Nothing traverses a symlink target or opens a FIFO/device
for data. A special/unsupported/unreadable/changed object fails the preparation;
there is no truncated or partial executable manifest.

Opt-in sync capture additionally records a mount ID for every entry, checking
both the held object and its current name before and after observation, and again
when finishing a directory. Revalidation rewalks the exact namespace and compares
all revisions/content/metadata and mount IDs. Cross-root directory identities are
checked as a complete set, catching physical subtree aliases. Independent roots
on different filesystems are allowed; differing mount/device numbers alone are
not treated as overlap or as proof of independence.

Every non-directory inode with nlink>1 has a group of `(side,index)` occurrences
and an external-link count. Links are deduplicated for hashing within each side;
cross-side reads are counted separately. The external count does not locate those
names or authorize mutating a shared inode. Symlink access-time canonicalization
remains per captured inventory; these sequential observations do not establish a
global instant. Link groups and metadata evidence are for fresh review/planning,
not implicit hardlink propagation or permission to overwrite outside either root.

## Bounds and unchanged EXDEV contract

- Per side:8192 descendant entries plus the root itself (8193), at most1MiB of
  retained root-relative names, paths4096bytes below the root and33 descendant
  components. The EXDEV internal root-name prefix is accounted separately, so
  adding the root does not discard the8192nd child or consume the relative budget.
- Regular payload budget is shared across **initial capture and final rehash** of
  both roots, bounded by the Intent limit≤1TiB. A file is rejected before reading
  its body if its snapshot length exceeds remaining budget. Hardlink cache reuse
  is charged only once per side/pass. EOF probes, link reads and bounded metadata
  syscalls are additional; this is a regular payload budget, not all filesystem
  I/O. `payload_read` reports initial preparation work; a later `verify` has its
  own shared per-call budget and returns its charged regular payload.
- Metadata limits remain64attributes/128KiB attribute bytes per object and a64KiB
  data scratch buffer. Sync uses bounded2MiB directory-name work and8MiB inventory
  wire-accounting ceiling per side, with at most8193 topology keys. Temporary
  topology keys duplicate bounded prefixed names; the retained sorted mount vector
  stores only u64 IDs aligned with entries. These are logical bounds, not PSS.
- The inherited full-metadata backend currently requires owned sources and genuine
  O_NOATIME support. Foreign-owner/unsupported-attribute backends need additional
  work. There is no silent weakening to basic metadata or permission fallback.

The ordinary EXDEV `capture`, controlled capture, verification, Entry fields and
FLMOVE encoding/version remain unchanged. Legacy inventory capture has no mount
proof option, no added statx dependency and its previous entry/path/name/read
semantics. New bounds and mount checks are explicitly opted into by the fresh
sync-manifest path. The shared Source adds only a borrowed worker-only parent
handle accessor; no ownership or lock is transferred.

## Remaining execution stages

This is pinned evidence, not the final durable operation manifest. Canonical
evidence encoding, seal and revalidated live byte binding are now implemented in
DIRECTORY-SYNC-RECORD.md. They are not the new operator approval, resolved action
plan or durability barriers; old dry-run choices lack object revisions and cannot
serve as approval. Derive all additions, replacements, merged directory effects, recoverable
retirements and metadata decisions from the new evidence. Missing-only copying is
not complete synchronization.

Before effects, add no-clobber staging and recoverable preservation of each old
destination, durable intent/backup/publication/barrier states, queue pause/cancel/
close ACK and per-effect partial/uncertain results. Metadata rollback needs actual
old values or preservation of the whole old inode; a metadata digest cannot
reconstruct an ACL/xattr. Account for internal/external hardlinks and owned rename
revision changes through an explicit ledger, not a general rebase exception.
Then implement inspect/retry/restore/discard and crash/reopen/undo without touching
foreign replacements or auto-resuming on startup. Owned fixtures only; no unattended
agent Apply against personal directories.

No helper command, visible manifest review, new installation, resource experiment,
new decoder or global acceptance follows from this core preparation. PSS/Swap/CPU/
startup experiments and new preview formats remain frozen. Native input/clipboard/
AT-SPI/2x/other OS/global lifetime/offline syscall deadlines remain pending.

## NODE verification

1409 workspace tests pass (502core/112helper/795UI) plus3 historical ignored,
including12 new manifest tests and1 opt-in/legacy inventory test. All-target
Clippy-Dwarnings and fmt pass with normal Rust test concurrency. An earlier
1408-test workspace passed before one collapsible-if lint; the condition was
collapsed without suppression. A further parent-path audit added explicit fresh
name/mount binding and the ancestor-alias test; the final full suite passes.

Owned fixtures cover raw files/link targets, empty directories, content and
metadata digests including real xattrs, both complete8192-child inventories with
both roots retained, shared read caps through final rehash, hardlinks inside,
across and outside roots, changed same-length content/permissions/namespace/links,
changed roots/parents, captured-identity mismatch, containment/final-link/FIFO
rejection, cancellation, one-shot consumer error and terminal root replacement.
A real independent `/dev/shm` root exercises a different filesystem/mount. Ancestor
symlink capture succeeds and retargeting invalidates it. A numeric mount-proof
mismatch exercises the rejection path; no privileged live bind/unmount experiment
was run. Counts, source hashes and logs are in
`evidence/directory-sync-manifest/assessment.json`.

No release/helper/UI build or installation occurred for this layer. The five
installed dry-run artifacts remain at checkpoint71eb40f/24b44a7,
installation1793574522897173033 (app289db327/helper170f995/HTML04aa7953202d).
No resource experiments or global acceptance are inferred from these core tests.
