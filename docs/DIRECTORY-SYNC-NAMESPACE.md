# Planned rename reconciliation for parent namespaces

Source-only Linux read-only reconciliation. This does not perform a rename,
identify who performed it, authorize an effect or enable Apply in the sync UI.
The caller must derive the expected operation from its reviewed intent, hold
current parent/path bindings and execute its own no-clobber syscall separately.

`plan_namespace_move` borrows one or two complete, disjoint sync directory
inventories and parent-FD/root-name anchors. Raw source/destination paths are
relative descendants of those roots. Both parents must exist and be directories;
the destination must be absent, the source cannot be a root, directory cycles
are rejected, and moved root/source parent/destination parent mount IDs must
match. Existing aliases of the moved inode must have coherent original values.
Directory identity overlap between scopes is rejected.

Preparation builds the exact expected namespace, including every descendant of
a moved directory and every unaffected entry. Each resulting entry's count,
relative name bytes, path length, depth and wire allowance are charged before
cloning its candidate path. Each execution result permits 16,385 entries (two bounded inputs plus one
root), 2 MiB retained relative-name bytes and the separate 16 MiB-minus-16 KiB
wire allowance. Original input capture still permits only 8,193 entries and
1 MiB retained relative-name bytes. The execution profile is adopted only after
complete reconciliation; failed plans do not widen the original owners.
Read-only `capture_namespace` and its controlled counterpart can reopen these
execution snapshots, require directory roots before content hashing, and do not
change the original source-capture/record formats or limits. It sorts
entry/mount pairs together, preserving their association. These are
structural bounds, not PSS or CPU measurements.

Preparation fully verifies every original scope, acquires the original affected
parent metadata, then fully verifies every original scope again before returning
the borrowed plan. It retains at most two compact digest templates instead of
raw attribute payloads. `preparation_read` reports regular payload bytes from
both passes; attribute/stat reads are not regular payload. The caller's shared
allowance applies across both passes and all scopes.

After its own rename, the caller consumes the plan through `reconcile` (or the
controlled equivalent). A complete new walk of each scope must match the exact
planned paths, identities and mounts. Every unrelated entry stays exact. Only
the moved inode may change ctime, with one coherent new revision across all its
known names; its content, metadata, mtime, nlink, ownership and permissions stay
exact. Descendants of moved directories retain their complete revisions.

Affected parent directories must have the exact planned direct-child-name
digest, unchanged identity/mount/mode/uid/gid/access/attributes and predicted
nlink. Directory size, mtime and ctime are acquired from the successful new walk
only for those affected parents. The metadata digest is checked using the
original attribute template at the observed new mtime. This cannot attribute
mtime/ctime-only changes on those parents to a particular writer.

For cross-parent directory moves, nlink is predicted from either an originally
observed constant value of one or the original value `2 + direct directory
children`. The former stays exactly one; the latter changes by minus/plus one.
Other observed models are rejected before the caller performs its rename.
Same-parent moves and non-directory moves keep parent nlink exact. This is an
explicit prediction, not a general nlink exception or filesystem compatibility
claim.

Only after complete walks, cardinality/topology checks, final root FD/name/
revision/mount checks and a control checkpoint does reconciliation replace ALL
recorded owners together. Every failure preserves the original recorded entries
and mounts. It does not undo the caller's actual rename; the consumed plan must
not be treated as an implicit retry/recovery token. Preparation and post-check
allowances are separate; operation-wide failure/retry accounting remains the
caller's responsibility. Original capture counters and default per-scope read
allowances stay unchanged; use explicit allowances when checking a scope that
has gained payload.

The result is the existing exact `RenameChange`, whose payload counter now covers
the complete post-check walks. It can reconcile additional unchanged-namespace
hardlink scopes using DIRECTORY-SYNC-ALIASES.md. No decoder, durable barrier or
approval is created. Controlled preparation and post-check use the existing
worker Control; no production thread or timer is added.

This is sequential evidence, not a simultaneous filesystem snapshot. Parent FD
anchors do not prove their current absolute paths. Symlink access time keeps the
existing captured-value policy because readlink can change atime; later changes
only to symlink atime remain outside that policy. Sealed original intent,
Prepared pins, live helper/session acknowledgement and visible human approval
remain separate. Creation/removal/link-count transitions beyond this rename,
actual staged metadata reproduction, durable preservation/publication and crash
recovery execution remain pending. Journal scaling is still unaccepted. This correctness foundation also performs
two complete before-walks and a complete after-walk per rename. Repeating it for
a large batch can make total inspection work quadratic; bulk namespace execution
scaling is unaccepted and needs more selective, positively bound guards before
shipping a mass-operation executor. This is a static algorithmic limitation, not
a resource benchmark.

Validation: 1,524 workspace tests pass (617 core, 112 helper, 795 UI), with three
historical ignored tests. Workspace Clippy all-targets with warnings denied and
format check pass. Nine new tests cover same-parent moves, cross-parent and
cross-scope file/directory moves, exact descendant revisions, raw names, full
parent metadata reacquisition and composition with external alias receipts.
Failures cover extra/missing/replaced names, unrelated data/ctime, parent
permissions/xattrs, nlink, roots, allowances and cancellation without publishing
partial owners or execution profiles. Models of directory nlink and a mount
mismatch use controlled records, not proof of other filesystems. A real full
union/reopen reaches 16,385 entries, original source capture still rejects it,
and one-over/name/depth limits reject before rename. Pause/resume/cancel are
worker functional checks, not native input or blocked-syscall deadline evidence.
Historical focused and pre-boundary-audit passes remain distinct from the final
acceptance in `evidence/directory-sync-namespace/assessment.json`. No release,
installation or GUI acceptance; the five installed hashes remain unchanged.

The later journal-gated own syscall route is documented in
DIRECTORY-SYNC-OWN-RENAME.md. Default plan reconciliation remains read-only;
explicit Journal execution uses the same guards, preserves every object on
uncertain results and has no installed UI route or automatic recovery.

The opt-in whole-batch observer profile is documented in
[DIRECTORY-SYNC-NAMESPACE-BATCH.md](DIRECTORY-SYNC-NAMESPACE-BATCH.md).
