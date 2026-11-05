# Multi-source transfer — implementation contract, 2026-10-27

Status as of 2026-10-28: preparation and provisional same-FS execution, and holding
the partial result in the queue, are tested in fixtures. There is no visible
entry point for multi-parent batches, no integration with MIME/Cut and no retry from the
UI; multi-parent Copy, batch EXDEV and OS acceptance are still pending. This is not
a functional checkpoint and it does not change the installed binaries. Workspace suite:
667 tests passed, 3 ignored; memory and startup were not measured.

Scope: up to 128 roots from up to 32 raw parents in a local MIME offer.
Existing Copy and Cut first; do not change the meaning of EXDEV nor declare
the batch complete until review, execution, retry and UI are connected.
Do not truncate or convert a non-UTF-8 path. A wrong or changed offer is
rejected as a whole before opening a review.

- Group borrowed paths after a complete preflight (per-path and aggregate limits),
  count parents and roots before reserving/cloning, and check for destination names
  duplicated by raw bytes across parents. If there is a collision, show both sources
  and ask to reduce/fix the batch; no item is queued or moved.
  Also reject overlapping roots (one selected under another). The lexical
  preflight detects apparent paths; in AuxIo check ancestors and physical aliases
  through descriptors/dev+ino, including linked parents and bind mounts,
  before the review, and repeat the location-dependent checks on
  execution. If independence cannot be proven, reject the batch with a notice.
  Walking `..` and comparing dev+ino does NOT prove independence for a bind mount
  whose source is a subtree: that walk can climb through another mount.
  Conservative condition under development: `statx` on the parent FD and each root;
  if two parents share a device but have different mount IDs,
  or a directory root and any parent have different IDs (even on
  different devices), or a directory root and the effective destination have
  different IDs, reject as an ambiguous alias. If IDs are missing,
  reject. For equal mounts, identities and anchored ancestors are compared,
  and both are stored/revalidated before preparing the engine. This does not cover every
  virtual FS semantic nor a hostile race between verification and syscall;
  the capture stays provisional until validation of the effective FDs is connected.
- AuxIo captures a Source anchored per parent, with individual signatures for each
  root and cancellation; a single failed group discards the whole review. The
  total of capabilities, FDs and scratch must be admitted before cloning or opening;
  the review, queue, active job and un-ACKed result share the existing limits.
  No per-path verification followed by a recapture authorizes a substitute:
  compare each FD captured by an engine against the group signature.
- A logical batch takes **one** slot in the global queue (or an equivalent admission
  that truly admits 32 parents), not 32 partial `enqueue` calls against eight slots.
  The same worker runs groups/roots sequentially, with the ID and the captured
  source/destination paths; changing pane, tab or focus does not redirect
  the operation. Do not reserve more than 256 FDs across entries, review, active job,
  ACK and scratch, and reject the whole batch before any mutation.
- The review shows each parent and its count, the destination and conflicts. Stable execution
  order; if a group fails or is cancelled, report results per root and keep
  the pending ones for retry with their **original signatures**, without recapturing.
  Do not promise a global rollback after earlier roots were published. Cut only consumes
  roots confirmed by the move engine; an old ACK does not consume a new
  clipboard. CopyReport.completed does not authorize deleting EXDEV sources.
  The ledger belongs to one specific preparation, not just to the set of signatures:
  two reviews of the same source to different destinations cannot swap
  states. The partial result must keep the preparation and the ledger together,
  with their memory and FD charge, until the receiver consumes the ACK or explicitly
  discards the retry; do not release the charge when the worker finishes.
- SameFS uses the existing safe rename; EXDEV only uses its engine/journal and its
  current limits, without silently degrading metadata or deleting after an
  unverified copy. The unsupported case keeps the source and shows the reason.
  Hardlinks across roots/groups are rejected before mutating until there is a
  batch ledger that keeps topology and its own revisions.
- Refresh both affected folders/panes based on the captured paths/identities,
  not just the active pane; a closed pane is marked to reload when it returns.
  Closing cancels, drains the single worker and keeps reports/rescue.

Acceptance: 32 parents/128 roots, raw names, a collision across parents without mutation,
limit tests and atomic rejection with a full queue/budget, a source change
before the effective capture, partial cancellation/retry and a stale ACK, focus
changes/close and Copy/Cut with fixtures. A scene with UI/review and a filesystem
check must state that it uses private handlers and pipes; OS integration is
validated separately. Do not infer a PSS budget from local caps; the freeze is in effect.
