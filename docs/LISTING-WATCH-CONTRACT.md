# General listing watch audit and source foundation

Current Model integration and installation status: see
[LISTING-AUTO-REFRESH.md](LISTING-AUTO-REFRESH.md). Earlier foundation-only
acceptance records below are historical.

## Original audit before Model integration

`theme_watch` watches theme inputs. `media_watch` watches and reconciles preview
sources/documents. Neither is a general directory-listing watcher. The listing
worker returns directory batches, stats and summaries; it has no change-notice
stream. Explicit F5 uses `Model::refresh_current`, capturing lossless selected
names/focus before clearing index-bearing state and restarting its worker.
`dual_pane::refresh_other` already provides an inactive-browser refresh route.
Stored tabs/panes carry their own browser ownership and revisions.

Before the Model integration, a new file created by another application was not
guaranteed to appear without an explicit or operation-triggered refresh. The historical
ROADMAP X02 watcher entry is still incomplete; existing preview watches do not
prove this capability.

## Bounded actor

The source `listing_watch::ListingWatch` owns one control worker and at most eight
native nonrecursive watchers (the existing `columns::MAX_COLUMNS` chain limit).
Complete desired configurations are limited to eight positive unique owner keys and absolute paths of at most4096 raw bytes,
without NUL. Preflight precedes changing the epoch or pending notices. Sorting
targets by owner key prevents reconfiguration merely because pane order swaps.
No filesystem I/O, native watch registration/destruction or thread join occurs
in configure/acknowledge/stop on the caller thread.

Directory metadata checks, native registration, replacement and destruction run
on the control worker. A capacity-one wake channel coalesces signals. Stable targets are not
repeatedly cloned. Ready roots reconcile only resolved directory device/inode
identity every two seconds on that worker; there is no periodic content listing.
A changed/unavailable identity requests a rearm. Failed registrations retry after
two seconds without spawning replacement control workers. With no targets, the
worker waits without a timer. Repeated identical failures do not create repeated dirty notices;
recovery to Ready does. Identity is checked before and after native registration;
a changed root rejects that attachment. These are live observations, not a
filesystem snapshot. Non-Linux behavior has not been functionally accepted; the
identity adapter explicitly fails on non-Unix instead of claiming Ready.

Callback payload paths are discarded. Access events are ignored unless rescan is
requested. Other events coalesce into one sticky dirty slot per target; errors,
rescan, unknown/large path batches and root-self events request reattachment.
Notices contain only target identity/path, per-target lease epoch, event serial, filesystem-event counter,
attachment sequence and status. The event counter advances only for accepted
filesystem hints, independently of attachment/status serial changes. Initial
attachment is not proof of a filesystem change. They are invalidation hints, never operation
targets, file identity proofs or authority for deleting/renaming anything.

The consumer explicitly acknowledges a notice. An ACK is accepted only for the
same current target, epoch and serial; it cannot clear a later event. Old callbacks
and ACKs from a removed or replaced target are rejected. Reconfiguring another
target preserves an unchanged target's lease, pending notice, attachment status,
retry deadline and native registration. The configuration version is separate
from these leases; adding a pane does not reattach an unchanged root or discard
its pending invalidation. The Model likewise retains the owner/path key and its
debounce state for unchanged browsers. Each native registration
also captures a private arm ticket. Callbacks from retired registrations are
rejected within the same target lease, before dirtying or waking it. This permits deferral during
editing/forms without retaining unbounded event queues or losing newer notices.
Every successful attachment emits a dirty hint to close the registration/listing
gap. Attachment sequence permits tests to wait for a completed rearm instead of
assuming a fixed sleep established it.

Stop immediately revokes targets and pending notices, then wakes the worker.
`stopped()` acknowledges only after its native watchers have been dropped. Drop
does not join on the UI thread. A filesystem syscall/native destruction can be
uninterruptible; stop is not a hard deadline and root exit is not a completed
watch-worker drain. No worker/FD/resource lifetime budget is claimed here.

## Integration requirements and known gaps

Before enabling Model integration:

- Root/ancestor alias replacement is now covered by background identity
  reconciliation and owned Linux tests. This detects persistent resolution
  changes, not every transient namespace change between observations, nor inode
  reuse with the same identity. Keep that distinction when describing freshness.
- Bind stable browser owner/root tickets, not each refresh generation, so a refresh
  does not itself cause endless watcher replacement/initial dirty refreshes.
  Preserve notifications during loading and validate ownership again before apply.
- Coalesce/debounce refresh admission. Defer while editing names/paths, dragging,
  operation forms/reviews, search/preview/size manager or other captured contexts
  are open. Do not cancel a user's ongoing size calculation merely for a hint.
- Reuse existing name-based restoration and preserve explicit empty selection,
  focus, range anchor and viewport intentionally. Numeric scroll alone does not
  establish preservation of the same visible entry after insertions above it.
- Handle active/inactive panes and tab restoration. No recursive descendant-change
  claim is provided by a nonrecursive watch; cached sizes remain observations.
- Surface watch failure/manual refresh fallback. Registration on an offline mount
  can block the single control worker; bounded state prevents worker accumulation
  but does not make such a syscall interruptible or prove offline recovery.

## Verification scope

Pure ledger tests cover10,000 events coalescing into one slot, stale ACK/epoch
rejection, atomic rejection of invalid complete configurations, and failure
suppression/recovery. Owned Linux fixtures cover create/rename/remove, raw0xFF
names, two roots, root replacement/rearm, missing root recovery and stop ACK.
Additional tests reproduce the old root-symlink failure, verify its repair, cover
a replaced raw-name ancestor alias, directory-to-file failure/recovery, unchanged
root identity across child writes, and retired native-arm callbacks.
These are watch-actor tests, not automatic-refresh UI/native-input acceptance.
No installation, new content previews, PSS/Swap/CPU/startup experiments or global
acceptance accompanies this source foundation.

Alias acceptance on NODE: baseline positive root-symlink test FAIL retained;
repaired source passed1267workspace tests (443core/105helper/719UI),3historical
ignored, all with normal harness concurrency. Final current watcher9tests,
Clippy and recursive UI rustfmt PASS. Evidence: `docs/evidence/listing-watch-alias`.
The historical folder-view WouldBlock is not claimed fixed by this passing run.
All five installed hashes remain those of the accepted allocation checkpoint.
