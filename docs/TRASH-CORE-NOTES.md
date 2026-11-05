# Linux Trash: partial core

2026-10-26. Partial Linux integration, visible in Actions/Delete and Undo. Functional and unit results are recorded below; resources and startup NOT_MEASURED because of the user's explicit suspension. It adds no dependencies and does not declare O04 complete.

The [freedesktop Trash Specification 1.0](https://specifications.freedesktop.org/trash/1.0/) defines the main trash at `$XDG_DATA_HOME/Trash`, with `files` and `info` directories. Each object has a unique name and a matching `.trashinfo`. The metadata is reserved first with exclusive creation; `Path` stores the path bytes URL-escaped and `DeletionDate` uses local date/time. This engine applies that format to an explicit main trash root. It does not choose or implement per-volume trashes, nor does it interpret a stored name as a restore path.

## API

- `TrashContext::open(root) -> Result<TrashContext, Failure>`: opens or creates only `root`, `root/files` and `root/info`. The parent of `root` must exist.
- `context.trash(source, cancel) -> Result<Receipt, Failure>` and the convenience `trash(source, root, cancel)` for one object.
- `restore(&receipt, cancel) -> Result<RestoreReport, Failure>`: returns the object to the **original parent's descriptor**, even if that directory was renamed. The original final name is kept, without replacing any new occupant.
- `Receipt::original_path()` is historical and matches the metadata. `current_original_path()`, `trashed_path()` and `info_path()` resolve current locations through `/proc/self/fd` and verify identities. A resolution error is never turned into an old path presented as rescue.
- `Failure` keeps `cause`, `preserved_paths`, `cleanup_paths`, `rollback_errors`, `unresolved_paths` and `restore_retryable`. The latter starts as `false` and, when a restore fails, only becomes `true` if a fresh check finds both the expected object and its expected metadata still in their Trash slots. The error type does not decide the retry; any error while checking identities keeps `false`. Paths of preserved objects and of leftover metadata have different roles; the caller must show them.
- `RestoreReport` confirms that the restore move and the object's identity were already verified. `restored_path: Option<PathBuf>` contains `Some(verified_current_path)` or `None` if only the diagnostic resolution after the commit fails. In that case `unresolved_paths` increases and `cleanup_errors` explains the failure, without inventing a historical path or returning a move error. The caller must remove that receipt from the history on any `Ok(report)`, even with warnings, and keep showing the diagnostics. Cleaning up its own `.trashinfo` is attempted anyway. `cleanup_paths/cleanup_errors` also report leftover metadata; each leftover path that cannot be resolved increases `unresolved_paths`.

The context is `Clone` through `Arc` and shares three FDs: root, `files`, `info`. Each `Receipt`, without `Clone`, keeps three FDs: original parent, `O_PATH` object and metadata. The last two avoid confusing an inode reuse with the confirmed object. A UI batch of 64 receipts with a shared context holds roughly 195 FDs, on top of whatever the process already has. The caller must bound the batch and the history; this API does not promise 100000 receipts nor an unbounded history. Dropping the receipt removes nothing from the trash.

## Security and limits

The root, `files` and `info` must belong to the effective UID and have mode exactly `0700`; the created `.trashinfo` files are regular, single-link and `0600`. Existing permissions are not silently fixed. Each managed component is walked without following symlinks. Ancestors must belong to root or to the effective UID and must not be group/other writable; the immediate parent must belong to the user. A `~/.local/share` parent with mode `0755` is valid. `create_dir_all` is not used to create unchecked ancestors. The caller must safely prepare `$XDG_DATA_HOME` if it is missing and pass that root; the library does not infer the global configuration or use personal data in tests.

Input paths are at most 4096 bytes, absolute, without NUL, `.` or `..`; `/` is not accepted. The source's parent is resolved once, keeping the last component as the object without following its symlink. The source can be a file, a non-empty directory or a symlink; `renameat2(RENAME_NOREPLACE)` keeps the object and its content/metadata when moving it. A source inside this trash, or one that is the root or one of its ancestors, is rejected, also checking directory identities by FD.

Before moving, the exclusive metadata and its directory are written/synced. Context, object and metadata are revalidated, and cancellation is checked right before the move. Cancellation is cooperative: it does not interrupt a blocked syscall; after a confirmed commit it returns success. The later check detects observed substitutions. Recovery uses exclusive moves and never overwrites to recover. If it cannot prove a current path, it keeps an unresolved count instead of inventing one.

Linux has no general "rename/unlink only if this inode is still here" syscall. A concurrent actor can change the interval between check and syscall; identities and FDs do not turn the whole sequence into a conditional transaction. Processes with the same UID can modify a `0700` folder. This is not isolation against a hostile actor with the same credentials and does not guarantee a snapshot of content modified in place. Rollback failures are exposed; an absolute absence of races is not claimed.

There is no copy fallback. A different device or `EXDEV` produces a clear error and keeps the source. The `.Trash/$uid`/`.Trash-$uid` trashes, mounts, restoring persisted receipts, cleaning/emptying, redo and crash recovery are pending. User content is never permanently deleted. Interrupting the process can leave an orphaned metadata file or an object pending recovery; the earlier `fsync` calls are not a journal nor atomicity against power loss. The optional `directorysizes` cache is not implemented.

Stored names use a bounded raw prefix of the original name plus PID/sequence; up to 128 exclusive candidates are tried. `.trashinfo` stays below the typical Linux `NAME_MAX`. Original names stay lossless in the `Receipt` and in the percent-encoded `Path`, including Unicode, `%`, line breaks and non-UTF-8 bytes. Diagnostic text is never treated as an operable path.

A single-object operation keeps constant state: metadata under 13 KiB with a 4096-byte path, bounded names and at most four diagnostic paths per collection of 8192 bytes and messages of 1024 bytes. These are bounds on retained data, not a process PSS measurement nor a UI batch budget. The resource/startup measurement phase has not been opened.

## Test coverage

The Linux tests use only their own folders under `~/.cache/files-test`: content/round trip, non-UTF-8 names and escaping, no-clobber on restore, symlinks not followed, non-empty directory, cancellation before commit, source substitution, renamed/replaced parent, relocated trash root, replaced object/metadata, repeated names, occupied destination, permissions/ancestors and cleanup that rejects foreign metadata. The pure tests cover format/escaping and preflight. The integrated suite and the UI scene are verified serially at the root. EXDEV needs an isolated fixture on a second filesystem, never a personal mount/data by inference.

## Fixing the diagnostic after restore

The integration review found a static case: after moving the object and confirming its identity, a failed `/proc/self/fd` resolution could return `Err`. The history would then keep a receipt whose location in `files` was already empty. The fix returns `Ok(RestoreReport { restored_path: None, unresolved_paths: 1, ... })` for that diagnostic failure, continues the revalidated cleanup of the metadata and keeps an explicit warning. A confirmed commit is not rolled back just because a display path is missing. Errors before the commit and identity failures after it are still `Failure` with recovery/diagnostics.

Regression prepared with an injected resolver: inside the callback it checks that the object is already in the original parent with the expected identity, that the Trash slot is empty and that the metadata still exists; it forces `PermissionDenied`; it requires `Ok`, path `None`, one unresolved location and the later revalidated removal of its own metadata. The regression is part of the core's integrated suite.

## External substitution during restore

If after the move the original parent's name holds a different identity, that foreign occupant is not moved to the Trash as a supposed rollback: a substitution before the syscall cannot be told apart from one after it. The observed occupant stays where it is, the original object is located through its `O_PATH` FD, and the metadata is kept for manual recovery. The report shows the verified current locations, without running new relocations on foreign identities.

`Failure.restore_retryable` is computed when leaving the restore wrapper from the current identities of both slots, not from `AlreadyExists`/cancellation. A normal conflict and cancellation keep `true`; a moved/replaced object, replaced metadata or an unavailable check leave `false`. The UI must separate retryable receipts from receipts kept only for recovery, keeping the total bounded and without offering Undo for a stale token. Fixtures were added for substitution after the move and for cancellation with metadata replaced afterwards, and the asserts of the earlier pre-operation replacement case were extended. No Cargo, processes or measurements were run during this adjustment.

## Visible integration and bounded recovery

Actions/Delete sends the selection to the single mutation worker; it does not open or probe
the filesystem on the UI thread. The borrowed preflight rejects batches of more than 64
objects, repeated names or more than 256 KiB before cloning. It never silently truncates a selection.
The first confirmed commit replaces the previous history; earlier failures
keep Undo. A failed creation does not clear the history either. There is no
Trash redo nor atomic rollback of a batch: receipts of the confirmed objects
are kept and errors and rescue paths are shown.

Undo restores in reverse order. An occupied destination keeps the new occupant and
allows retrying as long as object and metadata are still the expected ones. A
non-retryable receipt is set aside for manual recovery, without offering a stale Undo;
it holds its FDs until the next confirmed mutation or close. The total of
usable and set-aside receipts is still limited to 64. Close cancels and drains the
worker's result before exiting. The functional scene verifies a normal close
after ACK; it does not test closing during a blocked syscall nor crash recovery.

Errors use a panel with vertical scroll and horizontal scrolling through
Shift+wheel or the arrows. Debug paths are diagnostic, including non-UTF-8 ones;
they are never reinterpreted as instructions. Success uses a compact status.

The review found an important symmetry: both when sending and when restoring,
an external substitution after the move never authorizes moving the foreign
occupant as a supposed rollback. Only returning the expected inode is attempted, with
a fresh check and an exclusive destination; if it is no longer in its slot, everything is kept
in its observed location and manual recovery is reported. A regression leaves
the original name empty so that no-clobber cannot hide the wrong case.
The check→syscall interval against another process with the same UID remains explicit.

## Integrated verification

314 Rust tests (98 core, 37 helper, 179 UI), Clippy workspace/all-targets
`-D warnings` and `cargo build --release` PASS. Final scene
`trash-ui-1793045192155795906.json` functional PASS for a batch of four, conflict and
retry, raw non-UTF-8, directory/symlink and rejection of 65 items; normal close
after ACK. Exact captures and limits in VALIDATION. Resources/startup NOT_MEASURED.
