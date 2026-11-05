# Rename and create (0.0.3)

F2 or the inspector's Rename opens its own form with a before/after view.
Select several items with Ctrl/Shift; the batch also includes selected items
hidden by the filter and is sorted naturally. Before cloning names, the UI
rejects more than 100000 selected items, names >4096 B or a total >8 MiB. It does not truncate the batch.
Create file/folder does not copy the selection. Settings exposes create, undo and
redo; shortcuts Ctrl+N, F7/Ctrl+Shift+N, Ctrl+Z/Ctrl+Shift+Z.

Patterns: `{name}`, `{ext}`, `{dotext}`, `{n}`, `{n:03}`, escaped braces; start and
step, literal replacement and case keep/lower/upper. Prefixes/suffixes and
extension are expressed in the pattern. **Regex, dates and advanced editing are still
pending**. There is no exact filesystem normalization/casefold. Selected non-UTF-8
names are rejected without loss; every DirEntry counts toward the limit,
including unselected non-UTF-8 ones. The plan is computed on a single worker,
with cancellation between entries and phases; it is never applied automatically.

Apply revalidates identities and uses exclusive staging + renameat2 NOREPLACE on
Linux. Cycles, case-only changes and non-empty directories have engine tests. Rollback
runs without being cancelled between its steps: Cancel asks to stop the operation,
it does not kill the thread. Close/exit-after requests cancellation and waits for the final ACK before
exiting, even if spatial preferences are discarded. Errors, verified rescue
paths, unverified old ones and omitted counters are shown in a copyable
report. The report does not turn old paths into safe current locations.

A history token is kept; undo/redo revalidates and can be cancelled.
A failed Create keeps its indicators. A successful Create invalidates that history;
**undoing a creation is still pending**. Create uses shared portable validation and a
255 B limit, with mkdirat/openat O_EXCL anchored to the directory; it never overwrites.

There is no durable journal or recovery against kill/power loss: a normal close
waiting for rollback is not atomicity against a crash. The spatial
path+dev+ino identity is not yet migrated when renaming/moving a folder either; its layout may
not be recovered under the new name. Windows/macOS and a blocked remote syscall are not
validated. The UI+planner+plan tree does not yet have an integrated budget
for 100k/8 MiB; the isolated planner benchmark does not prove it.

`python3 scripts/rename-check.py --capture` walks batch 2 from internal handlers,
checks content/names before applying, after apply/undo/redo,
creates a file/folder and captures the framebuffer. `python3 scripts/rename-check.py` runs
the same sequence with a tree sampler and no captures. It includes closing with a pending Apply
through the normal close state, exit 0 and no staging left. It may
cancel before moving or race with completion; on its own it does not test closing
at every engine instruction. The core tests inject failures at every
staging/rollback step; the UI tests check blocking until ACK, not power loss.

Run only after a build, with Cargo/captures/benchmarks mutually exclusive.
Functional/resource evidence for batch 2 is in VALIDATION; it does not prove a 100k batch or a global PASS.
