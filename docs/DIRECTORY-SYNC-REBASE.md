# Reconcile a sync inventory after a root rename

Source-only Linux executor primitive. The sync UI remains read-only. This code
performs a fresh read-only traversal after a caller has positively identified its
own root rename; it does not perform the rename, establish its provenance,
update the complete two-root mutation ledger, or authorize any operation.

`Inventory::rebase_for_sync` requires an opt-in sync inventory with captured
mount IDs. It walks the new name relative to a supplied parent FD, using the
existing no-follow, NOATIME, full content/metadata, topology, hardlink and bounded
name/count/depth rules. It compares each raw relative suffix with the original
inventory. The original pinned root identity and every mount must match. Only
the root's ctime may differ; root mode, size, links, owner, mtime, content and
metadata remain exact. All descendant revisions remain exact. Regular file
atime stays exact; symlink atime uses the existing captured-value policy because
readlink can itself alter it. That policy does not detect later atime-only
changes to symlinks.

All replacement paths are prepared before traversal. After complete comparison,
namespace cardinality, root FD/name/revision/mount and cancellation are checked
again. Only then are path prefixes and the root revision updated, with no
fallible operation remaining. Failure leaves the owner's recorded inventory,
mounts, prefix and original capture read accounting unchanged. A failed
reconciliation is not fresh evidence and grants no permission to retry effects.

The explicit API returns regular-file payload bytes consumed by this pass, with
a caller-supplied shared read allowance. Hardlinked payload is read once per
inode. Default own-root-rename reconciliation uses the original per-pass capture
allowance and preserves pause/cancel Control behavior. It supports directories,
symlinks and multiply linked leaves only for opt-in sync inventories; the legacy
EXDEV policy still permits only a single regular file with one link and does not
acquire a new mount dependency.

The supplied parent FD is an anchor, not proof of its current absolute pathname.
The future executor must verify parent/path bindings and durable intent, identify
its own rename, reconcile affected parent namespaces and every alias inventory,
and manage uncertain effects. Reconciliation of one hardlink name deliberately
does not refresh other snapshots of the inode: their old ctime verification
still fails. Prepared root/source pins and the complete sync journal/approval
remain separate; this is not a recovered Apply token or a filesystem snapshot.

Tests use owned temporary fixtures and real renames. They cover trees with
internal hardlinks, symlinks and empty directories, raw non-UTF8 root prefixes,
rename to another parent and back, hardlink and symlink roots, unchanged child
revisions, strict unrelated-snapshot verification, root replacement, child/root
permissions, child-only ctime, changed content/names/nlink, cancellation, read
budget exhaustion, a controlled mount mismatch and a full 8,193-entry inventory.
Limits and byte counters are structural checks, not performance measurements.

Validation: 1,507 workspace tests pass (600 core, 112 helper, 795 UI), with three
historical ignored tests. Workspace Clippy all-targets with warnings denied and
format check pass. Initial four focused tests also passed; the full-inventory
case and additional revision/alias checks are included in the final workspace.
No release/helper/UI build or installation; five installed hashes unchanged.
See `evidence/directory-sync-rebase/assessment.json`.
