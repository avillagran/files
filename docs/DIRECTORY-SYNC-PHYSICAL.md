# Physical synchronization blueprint

`directory_sync_physical` derives bounded transactions and staging nodes from a
resolved canonical logical Plan. This is SOURCE ONLY. It performs no filesystem
I/O, grants no approval and implements no executor, backup, journal or recovery.
The full explorer goal remains active and the installed dry-run is unchanged.

## Complete selected effects

All five modes retain their logical choices. Each Add, Replace, Retire or
IsolatedMetadata transaction identifies its logical row, destination side,
captured parent, containing mount, optional old root and complete old scope,
and range of new staging nodes. Old trees have complete root/descendant references
for preservation, verification and durable flushing. A raw sibling `a-` is never
included in the `a` tree. Covered logical descendants are expanded exactly once;
retained/skipped subtrees produce no transaction. Empty directories survive.

Every staging node references captured source evidence and its stage parent.
Directories precede their children. Creation steps distinguish a new file,
directory, literal symlink or a hardlink to an earlier **staged** inode. No new
object is linked directly to a live source or old destination. New stage identities
and revisions must be captured rather than copied from source proofs. Create all
cohort links before final metadata/content verification and syncing; their own
nlink/ctime transitions must be recorded. Shared-directory
metadata remains separate and deepest first, with root metadata last.

The phase contract requires acquiring values and staging **all** selected sources
before preserving or publishing **any** destination. This matters for Merge and
cross-root inode aliases: early publication can otherwise destroy a source needed
later. The future executor must enforce this contract, verify staged content and
metadata, sync stages, persist the full intent, preserve old objects with durable
ACK, then publish without clobber and flush relevant parents. No barrier is
implemented by a blueprint or its hash. Publication across transactions is not
whole-batch atomic, including names in one hardlink group.

## Hardlinks and old aliases

PreserveInternal groups selected non-directory materializations by source
physical inode and destination side. A staged group may span transactions and
subtrees; later members link to its staged leader. The policy preserves links
**among selected materializations**. It does not silently rewrite a skipped,
retained or unchanged name. Each cohort exposes selected members, complete source
name count, omitted source count and outside-root link count for future review.
Full topology reconciliation involving untouched equal/retained names needs a
separate explicit review before complete topology replication can be claimed.
IndependentNames explicitly requests a distinct staged inode for each name.

A cohort crossing destination mounts is rejected rather than silently split.
A destination scope containing a mount different from its parent is rejected
before any mutation: recoverable mounted-tree retirement is not implemented.
Numeric mount-tamper fixtures test these branches; they are not privileged live
bind-mount/unmount acceptance. Fresh live mount checks remain the executor's duty.

Backup renames of old linked leaves may change their inode ctime, including at
captured source/retained aliases and outside roots. AliasImpact records all
captured names and the number of external links, without locating outside names.
Old data and applied attributes must remain intact; there is no promise that
kernel-maintained ctime is invisible outside the roots. The future own-effect
ledger must update only identities and namespace changes it positively proves,
not relax general source revision checks or reuse a stale Prepared.verify result.

## Metadata recipes and unresolved values

For a new object, or replacement by a different kind, the recipe requires full
source metadata. No attempt is made to reinterpret old attributes across kinds.
For replacement of the same kind, Ignore retains full old-destination metadata;
Basic retains destination owner/access/attributes and overlays source permission
bits and non-directory mtime; ExactObserved requires full source metadata.
Shared-directory Basic overlays only permissions, preserving mtime after own
child effects. ExactObserved directory metadata applies after children.

Even a metadata-only leaf change is a staged isolated replacement with the old
inode preserved. It must not change data/attributes through external or unselected
hardlinks. Shared directories need durable actual old metadata, checked identity
and a reversible update after child effects, rather than whole-tree replacement.

A hash is not the attribute values. Every recipe still requires reacquiring
bounded actual values and proving captured revisions/digests. Group recipes known
to yield equal metadata can share a staged inode. If equality cannot be proved
from evidence, the cohort retains an explicit needs-metadata-values constraint.
Different old hashes can become equal after a Basic overlay, so such recipes are
not falsely rejected as incompatible and are never silently split. The next
stage must resolve actual complete values: unequal final recipes require a new
explicit policy/choice review before approval; equal values must be sealed with
the prepared execution identity. Zero cohort constraints is **not** readiness.

## Bounds, canonical bytes and remaining work

At most16,384 transactions,16,384 staging nodes and16,384 preserved entries are
retained, with bounded numeric references rather than duplicated paths or xattr
payloads. A separate sealed staging-payload limit (default/maximum1TiB) charges
each new file or literal symlink target once per selected staged inode. Explicit
IndependentNames charges every name, so hardlink multiplication cannot bypass
the limit. Zero permits only zero-payload preparations. This is distinct from
the evidence read budget, disk availability, allocated bytes and xattr payloads. The maximum physical wire allowance is3,145,856 bytes. FLSPHYv1 includes
both evidence and logical-plan seals, all transaction/node/cohort/recipe/old-scope/
parent/alias fields and a domain-separated SHA256 trailer. Re-derivation is
canonical. There is no decoder that reconstructs live objects or approval.
Acquired metadata values and actual stage/backup identities are not yet present.

Before Apply: resolve and seal metadata values, physical staging identities and
hardlink constraints; fresh live proof binding; explicit review/approval tied to
job/evidence/complete plan/journal; durable journal and own mutation ledger;
no-clobber backup/publication, failure/uncertain barriers, bounded queue and
pause/cancel/close ACK; crash/reopen/recovery/undo and helper/UI integration.
Owned fixtures only; no unattended Apply against personal directories.

Workspace/Clippy/fmt acceptance is recorded in
`evidence/directory-sync-physical/assessment.json`. Tests cover all modes,
complete trees, raw/hidden names, hardlink and metadata policies, preserved
scopes, canonicality, mount proof rejection and full inventories. Planning tests
do not prove actual rename/link/fsync/crash behavior. No release or installation
occurs. PSS/Swap/CPU/startup remain frozen, new previews paused, native/global
gates open; byte and count limits are not resource measurements.

NODE final validation:1450 workspace tests (543core/112helper/795UI), three
historical ignored,14 new physical tests, workspace all-target Clippy-Dwarnings
and fmt check PASS. Initial12 focused and1448 full tests passed before adding the
separate payload limit; budget tests raised the suite to1450, then the nested
mount-proof fixture was strengthened and full validation rerun. All Rust harness
concurrency remained normal.

The subsequent SOURCE ONLY DIRECTORY-SYNC-METADATA.md acquires actual values
through pinned paths, resolves cohort equality, emits undo/desired records and
revalidates after the last sink callback. It still does not supply an owned
durable metadata store, operator approval or execution.
