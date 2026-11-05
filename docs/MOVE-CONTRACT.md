# Move — M24 contract / EXDEV dependency

Status: M24 native rename implemented and functionally accepted; EXDEV is still
pending as an explicit dependency, without marking Move complete.
Resource/startup freeze in effect. Reuse the global queue, pause/cancel/ACK controls,
path preflight and the destination UI; F6, the context menu and the inspector have real access.
Do not confuse copy with move nor mark O01/O03/O06 complete with this unit.

## Review and capture

The Move dialog receives the captured raw names/source path. Reviewing the destination runs
outside the UI on the existing AuxIo and returns a token: source/destination parents open,
an O_PATH|O_NOFOLLOW FD per root, dev/ino/type identity and lossless paths. Confirm
only enqueues that reviewed token; it does not reinterpret the current selection.
Editing the destination discards the token and requires a new review. Cancelling/closing during
a review requests cancellation and drains the ACK; a stale response only frees FDs.

First Linux unit: at most 128 roots per move, 128 root FDs + 2 parents. The queue
additionally budgets metadata/FDs and limits the FDs of admitted tokens to 256; a
single token under review/an unconsumed response can coexist with that queue
(up to 130 additional FDs: at most 386 FDs of simultaneous tokens, plus bounded
temporaries from the ancestor walk/diagnostics). The active job and a worker result
not yet ACKed keep the FD charge until the response is consumed. Limits are checked before
cloning/opening; exceeding them asks to reduce the batch. They are not PSS limits nor a
ulimit guarantee; a failure to open keeps the originals. Other OSes explicitly Unsupported.

## Same filesystem

`renameat2(RENAME_NOREPLACE)` relative to the captured parents. A symlink is moved
literally, its target is never followed; files/folders keep their inode and
metadata/ACL/xattrs through rename, without walking/copying descendants. Never replace
any destination, including broken symlinks. Detect duplicates in the batch and self-moves
by parent identity+name; reject a destination inside the source, even through an
alias, with a bounded walk of the destination FD's ancestors (or a safe error if
it cannot be verified). Do not rely only on string comparison for confinement.

Revalidate the entry and the root FD before and after the syscall. Linux has no
inode-conditional rename: there is a check→rename interval against a hostile writer.
A later mismatch => unconfirmed state, keep everything, report the review folders
verified by FD and counters; do not blindly delete/clean/roll back a
replaced entry. Do not promise absolute protection against hostile changes.

Pause/cancel is evaluated before each publication. A cancel after the last
checkpoint may arrive while the rename is already in progress: one root may be published.
Earlier roots stay moved, the remaining ones stay in the source. The result distinguishes
complete/partial/cancelled/unconfirmed; there is no global rollback nor durable undo
for this batch in M24. Close keeps the worker until ACK and shows rescue/errors.

Capture source and destination by ID on admission. When finished, invalidate/refresh BOTH
folders if the active one matches either; going back to an inactive one relists. Do not
refresh another tab because of a matching name nor destroy its scroll/selection.
Captured displayed paths may become stale if a third party renames parents;
results verify current paths by FD when possible, and never rebuild
fake rescue paths as an authorization to delete.

## Explicit EXDEV, no silent degradation

The first M24 checkpoint only does native rename. EXDEV (preflight by device or
syscall) => visible error: cross-filesystem pending, source kept; NO
copy+delete fallback. EXDEV error tests with an injected backend are not equivalent to
a test on a real mount. This limitation does not close the complete Move requirement.

The next EXDEV block needs a bounded per-entry manifest, a source FD and a
validated revision/content (root inode/mtime does not prove the descendants), a destination
copied and verified/durable before deleting any source; changes => keep the
source and a partial result. CopyReport.completed is only paths/counters, not
permission for remove_dir_all/unlink. Define preservation of permissions/ACL/xattrs/hardlinks
without deleting the original if the backend cannot preserve them; a journal/safe
recovery as an explicit dependency for large trees and process failures.
Do not adapt the copy engine to fake a move with an unsafe delete.

## Acceptance

Private fixtures: file/non-empty folder/literal symlink/raw, inode/bytes/metadata,
existing and in-batch/self/descendant/alias collisions, source replaced between review
and Apply, parent renamed after capture, cancel before and after some roots,
pause/cancel/close/ACK without SIGTERM, refresh of A and B with another tab active. Suite,
serialized Clippy/release/fmt, captures of review/result/error and a checked
filesystem. Handlers/framebuffer do not test OS input. Install only a validated
checkpoint with backup/hashes. Metrics NOT_MEASURED; the rest of the ROADMAP stays open.


## M24 evidence — 2026-10-27

537 Rust 148 core/67 helper/322 UI, Clippy workspace/all targets -Dwarnings, release and fmt
PASS. Final logs move-final-test, move-second-clippy, move-release and move-final-fmt;
copies with the earlier failures in move-checkpoint-validation.json. Engine regressions:
pinned parents against a replaced path, replaced source, destination descendant
moved after review, pre-cancel/cancel after one root, injected EXDEV and post-rename
uncertainty even with the source recreated as a hardlink to the original. Explicit published/verified;
identity is checked again after walking the ancestors, right next to the syscall.

The queue test counts active/unread ACK+pending within 256 FDs and frees on ACK; input
metadata is charged in addition to paths/capacities. Close signals ALL the
subsystems before readiness: a pending review does not delay cancellation of the
queue/helpers. Stuck kernel IO is still cooperative and is not declared interrupted.
Model tests refresh A/B but not C/query/focus; the menu looks up Properties/Move
semantically, without indices that depend on the new row.

`move-ui-1793113502303715506.json` PASS 6 FB 1000×650. Exact fingerprints before
Confirm and after Confirm-Paused prove that reviewing/pausing publishes nothing.
After Resume, 4 roots (a 64 KiB file, a folder + Unicode text, a literal symlink and raw 0xFF)
keep dev/ino/mode/uid/gid/mtime/ns/bytes and the xattrs present; the symlink keeps
its text. The active Target keeps the selection untouched after refresh; going back to
Source shows the roots absent and an empty selection. An existing collision is not overwritten;
/proc on another device rejects EXDEV in preflight without copy/delete.
Cancelling a paused active job and closing with a paused active job keep their sources; close
exit 0 after ACK. Cancel after some roots only tested through an engine callback,
not UI input timing. No evidence of EXDEV transfer nor crash resume/undo.

Method: Actions/internal handlers and the logical framebuffer; no OS input/clipboard nor
native 2x sharpness. The footer's incidental PSS does not count. Resources/startup NOT_MEASURED
because of the freeze. Migrating the spatial layout when a folder's path changes is still
pending (its current key includes the canonical path); keeping that
preference is not claimed just because file/folder inodes are preserved.

Next O01/O03 block: manifest/metadata/durability and safe recovery
for EXDEV; do not enable deleting the source using only CopyReport.completed.
