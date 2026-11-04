# Exact rename transition across known hardlink aliases

Source-only Linux reconciliation. This code performs read-only checks and
updates recorded inventory revisions; it does not rename objects, identify who
performed a rename, approve operations or enable synchronization in the UI.

A successful `Inventory::reconcile_root_rename` now returns a private-field
`RenameChange`: exact old/new revision, inode identity, mount, original access,
content/metadata digests and bytes read. The existing internal rebase API still
returns bytes and retains its legacy policy. The transition has no decoder or
constructor from public observations. Its origin is a successful full rewalk,
not proof that the caller performed a particular syscall.

`RenameChange::reconcile_aliases` accepts parent-FD/name anchors for all supplied
inventories. Every inventory must be an opt-in complete sync capture and contain
at least one matching name. Affected entries must match the exact original
revision, values and mount from the transition. Current observations must match
the exact new revision, including ctime; other entries remain entirely strict.
There is no general ctime exception and no nlink change allowance.

Before content rewalks, the batch bounds combined entries and rejects duplicate
physical names, using parent directory device/inode plus raw basename (captured
parent entries for descendants, the supplied parent FD for scope roots). This
catches overlapping inventory scopes while permitting different hardlink names
with the same inode. The number of supplied names cannot exceed observed nlink.
It does not prove discovery or verification of unknown/external names.

Every scope is fully reread using existing FD/NOFOLLOW/NOATIME content, metadata,
namespace, hardlink and mount checks under a shared regular-payload allowance.
Payload is deduplicated within an inventory; separate scopes are rehashed and
charged separately. Root FD/name/revision/mount bindings and cancellation are
checked again after all walks. Only then are ALL recorded affected revisions
updated, with no remaining fallible operation. A failure leaves every supplied
inventory recorded as before; it does not undo the earlier root rebase or the
caller's actual rename. Original capture accounting stays unchanged. The report
returns alias occurrences and bytes consumed by this batch, separately from the
root rebase's bytes.

The combined structural cap is 32,772 inspected entries, including unrelated
objects. It is not a memory or CPU measurement. Reconciliation is sequential and
not a simultaneous filesystem snapshot; a future effect must revalidate current
objects and parent absolute-path/ancestor bindings. Parent FD anchors alone do
not prove those bindings. Symlink atime retains the existing captured-value
policy because readlink can change it; later symlink-atime-only changes are not
detected by that policy.

Directory transitions cannot be used as hardlink transitions. The complete
parent-namespace/nlink mutation ledger, positive own-effect provenance, live
source/helper pins, human approval, durable effects and crash-recovery executor
remain pending. A readable transition or successfully reconciled inventory
supplies no authority to mutate user files.

The controlled API uses the existing worker `Control` checkpoints for cooperative
pause/resume/cancel, including full inventory walks and the final commit boundary.
It adds no thread, timer or queue worker. Tests spawn a worker only to verify that
pause waits and cancellation wakes it without recorded-state changes; this is
not a bounded deadline for a blocked filesystem syscall or native UI acceptance.

Validation: 1,515 workspace tests pass (608 core, 112 helper, 795 UI), with three
historical ignored tests. Workspace Clippy all-targets with warnings denied and
format check pass. Eight new tests cover exact multi-scope reconciliation,
all-or-nothing failure/cancellation, known-name overlap, distinct same-inode
names, raw FF versus literal escape bytes, symlink aliases, pause/resume/cancel,
full combined 32,772 entries and one-over rejection. Initial five focused tests
and a pre-control 1,514-test workspace pass are retained separately. The final
overlap/root-name tests use real pre-rename captures and a second real rename,
replacing the initial test's recorded-revision stand-in. No release, installation
or GUI acceptance; five installed hashes unchanged. Evidence is in
`evidence/directory-sync-aliases/assessment.json`.
