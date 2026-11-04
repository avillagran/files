# Automatic listing refresh integration

Installed on NODE from source81837e5, checkpoint1793544731266983878.
Installed watch1793544742660384859 PASS6captures/2root0+drain; allocation regression
1793544743043408636 PASS22captures/2root0+drain/25loggedhelpers gone/reused.
Five installed and five backup hashes verified; app
`8f7733dfce15f9809521f7a8246d728cf8420ea09f8fba2beae906e788d2dd1b`.

LISTING-WATCH-CONTRACT.md describes
the bounded notification actor and the live root-identity reconciliation.

`listing_sync::State` lazily starts one watch service per Model. It watches the
active browser and visible inactive pane (two roots), or the bounded Columns
chain (at most eight roots, sharing the existing column limit). Targets
bind stable `(browser ID, tab ID)` plus path; refresh generations/revisions do not
recreate watches. Desired targets are cloned only when their ownership/path set
changes. A newly attached/restored tab gets a dirty hint to close the gap while
it was not watched. Stored hidden tabs do not retain native watchers.

## Admission and interaction

Notifications remain in sticky actor slots until admitted. The client waits for
180ms quiet or at most one second of continued events before trying a refresh.
These are cooperative scheduling intervals, not filesystem latency guarantees.
The callback's epoch/serial ACK rejects a newer event arriving during admission.
Manual refresh acknowledges matching existing notices after successful admission;
it does not reset the watch and cause an endless initial-dirty refresh loop.

Automatic refresh defers during path/name/filter edits, drags/native transfers,
menus, forms/reviews/dialogs, search/preview, size manager/calculations, copy queues,
and in-flight listing/metadata viewport restoration. It does not cancel a paused
size queue to make room for a notification. Deferred notices survive these states.
`Cambios pendientes` is visible in the footer; watch failure has an explicit F5
fallback. A failed service-thread start can be retried through explicit Refresh.

SpaceMap consumes root hints as explicit observation-change/watch-gap flags,
without automatically repeating scans (SPACE-MAP-CHANGES.md). Descendants are not
watched recursively. Spatial waits for layout readiness.
Columns integration is described in LISTING-COLUMNS-REFRESH.md; its new source
acceptance is separate from the installed checkpoint above.

## User context

The existing asynchronous reload invalidates index-bearing hit/state generations,
then reconciles selected raw names, focus, explicit empty selection and the actual
range anchor. Refresh capture keeps the existing8MiB selection budget/error path.
Snapshots are retained across coalesced refreshes, not overwritten by empty batches.

A painted, scrolled viewport captures the first visible raw name and its logical
offset. List/Grid/Groups reuse the same entry-position geometry used to reveal
selection; dual panes preserve row offset independently. Required stat metadata
finishes before viewport restoration for groups and non-name sorting. Spatial
uses its retained layout position. A viewport at the top remains at the top so a
new first entry becomes visible; an unpainted viewport retains numeric scroll.
Deleted/filtered anchors fall back to existing scroll/clamping behavior. Tab saves
keep their existing numeric scroll semantics while their listing is reconciled
on restore; this does not prove named viewport preservation while a tab is away.

Refresh viewport metadata is included in browser data/metadata charging. The
inactive pane refreshes through its own worker/revision, without switching active
focus. Cached size observations for the refreshed root are invalidated only when
the size queue is idle; no live physical/freeable-space claim is introduced.

## Lifecycle and tests

Closing revokes watch targets and waits for the service's stop ACK in close
readiness, without joining on the UI thread. A cancelled close resumes watching
only after the previous actor acknowledges stop, avoiding worker accumulation.
Blocking native registration/metadata/destruction remains an explicit limitation;
this does not establish a hard offline close deadline or global lifetime proof.
The control worker rechecks stop/configuration before probing another root.

Legacy unit Model fixtures with synthetic entries leave live watching disabled
under `cfg(test)`. Dedicated real-filesystem tests explicitly enable the same
production bridge and keep normal harness concurrency. They cover external
changes, raw marked names/focus/range anchor/empty selection, scrolled viewports in
List/Grid/Groups, edit and paused-size deferral, inactive pane refresh, stable watch
keys across F5, restored tabs and restart after stop. Production defaults to enabled.

`scripts/listing-watch-ui-check.py` exercises the actual watch -> Model -> async
listing -> painted framebuffer chain against owned files, including alias-root
replacement and deferral during path editing. Private handlers are not native OS
input. No PSS/Swap/CPU/startup experiments, new content previews, clipboard delivery,
AT-SPI,2x or global acceptance is established by these checks.

Source acceptance:1272Rust443/105/724+3historical ignored,5model filesystem
integration tests, Clippy/rustfmt/release PASS. Watch UI Wayland1793544227414221009
and X111793544230415153296 each6captures/2root0+drain. Allocation regression
Wayland1793544348456367078/X111793544351424203679 each22captures/2root0/25helpers
gone/reused. Evidence and initial corrected failures are retained in
`docs/evidence/listing-model/assessment.json`. No global acceptance is implied.

Columns installed on NODE from source `ac02206`, receipt
`installed-checkpoint-1793546728652462098.json`; app
`b3d6204df2e923bcd8a8363f9419d57774688ada4a5b8d6084a4fa910ec984e9`.
Five installed and five backup hashes verified. Installed Columns receipts
1793546741743502286 and1793546823293042852 each passed7captures/root0+drain;
installed allocation1793546792633964223 passed22captures/2roots0+drain and
logged helper identity checks. Initial fixture parse failure is retained in the
Columns assessment; the completed log had80valid JSON markers and the reader
now waits for complete lines. No global acceptance or resource claim.
