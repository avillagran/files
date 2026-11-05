# Tabs — V10 contract, M22

Status: M22 functionally accepted. The final suite
`tabs-checkpoint-test.log` records 513 tests PASS (141 core, 67 helper, 305 UI);
Clippy workspace/all-targets with `-D warnings` PASS in `tabs-empty-clippy.log`.
The final scene `tabs-ui-1793109776417107918.json` passed with nine captures. It does not change
the approved design nor open another preview engine.

The next visible unit is opening/switching/closing/reopening tabs with Nerd
Fonts, Ctrl+T, Ctrl+Tab, Ctrl+Shift+Tab and Ctrl+W. Closing the last tab requests
the normal window close and keeps the existing drain/ACK. It does not replace the
compositor's Super+W close path, which is still pending verifiable OS input.

## State and budget

At most eight tabs and eight recoverable closed ones. Each tab has a monotonic
identity, a lossless path, a bounded back/forward history, view, sort/descending/grid_zoom/pan, filter, scroll
and anchor/focus/selection by raw names subject to an aggregate preflight. The snapshot matches the implemented Model; eight listings,
bitmaps, layouts or workers are not kept for convenience. Only the active tab owns the listing,
hits and helpers. On switching, generations and targets are invalidated before draw,
the spatial snapshot is saved by ACK and preview/du/treemap are cancelled/drained.

Already confirmed operations stay in the global queue with their captured paths;
they are not redirected to the active destination nor removed when a tab closes. Closing
an active dialog must explicitly resolve/cancel its state. Hits with a stale
tab id do not act. Show an error when limits are exceeded, without hidden
truncation nor cloning before the budget. These are not process PSS limits.

Durable restoration is integrated in M23: see SESSION-CONTRACT for argv,
inter-process ownership, limits, invalid/future and the close ACK. V10 is not marked
complete while authorized resource validation and other OSes/native input are missing.

## Observable acceptance

1. Open A and B, navigate inside B, go back to A and verify path/view/filter and
   raw selection; two switches before the first batch do not publish a stale listing.
2. Ctrl+Tab/reverse and the buttons share Actions; closing the active tab picks a neighbor,
   reopening a closed one recovers its location, the limit of eight shows a recoverable error.
3. Switching with an active helper cancels and reaps before the next one; only the last
   destination is pending. Switching during a copy keeps the real job/source/destination.
4. An empty/error/hidden folder does not inherit hits nor an invisible selection. Resize
   keeps controls visible; long paths are clipped without losing identity.
5. Captures of the real UI and an own filesystem, callback+ACK lifecycle without SIGTERM
   as proof; handlers/framebuffer are not equivalent to OS input. Install only after
   suite/Clippy/release/scene PASS, with backup and consistent hashes.

Resource/startup freeze in effect: NOT_MEASURED, no new baseline comparison nor
promise of meeting 50 MB. SFTP as an OS mount, integrated shaders, move/
journal, dual pane/Miller and i18n are still pending in the ROADMAP and are not abandoned.


## M22 wiring

`tabs.rs`/`tabs_ui.rs`: only snapshots, eight active and eight closed at most,
monotonic IDs (reopening gets a new ID); no pending item by index. Synchronous switch
on the UI, the listing stays asynchronous with the existing worker's generation. There is no
new thread/listing/bitmap per tab. Active/inactive global show_hidden is not copied.
Listings of inactive tabs are discarded: coming back always relists, so copies
finished in another folder are seen on return without wrongly refreshing A.
`copy_locations` keeps at most one lossless 4096 B parent per UI job
(up to 9 already bounded jobs), freed on ACK/cancel. The queue keeps the original paths.

Snapshot preflight before cloning names: selection <=256 KiB of conservative
charge (payload+containers+64 KiB fixed), name 4096 B, snapshot <=1 MiB
including History/query/path. `History::retained_bytes` counts payloads/deques and
slack per path; History is still limited to 64 locations. The sum of inactive+closed
snapshots <=3 MiB before admission; an additional 1 MiB reserve for the active/
transition and 128 KiB for rows/paths/containers. Exceeding it keeps the state and a visible
error: it may prevent opening/switching even before eight tabs. It does not limit the whole
heap, the live listing, the existing refresh Snapshot (cap 8 MiB), thumbnails or
helpers; it does not prove PSS. History is not cloned: ownership is moved.

Pending snapshots are moved when switching before Done; only the Done of the current
generation restores names. Selection/focus/anchor are intersected with the current hidden+query
without a quadratic search. Scroll is kept during incomplete batches;
it is clamped only with the final geometry: listing loaded, spatial layout ready or treemap
resolved. A restoration pruned to empty stays empty against Stats/rebuild;
explicitly selecting or editing the filter allows recovering focus.
A busy Apply/undo/redo, dialogs or path editing
block tab switching, keeping the mutation refresh invariant.
A confirmed copy operation can keep running while the tab changes.

A 32 px bar under the header, clipped labels with a 24 px close and stable IDs; Nerd
arrows walk the tabs and the active one is revealed in a bounded strip. The header grab does not cover
the bar. Compact/clipped inspector before actions: at 500/440 px
Calculate size is omitted if it does not fit whole, and kept at 650 px. No invisible target.
scripts/tabs-ui-check.py prepared: UI handlers/framebuffer, A/B with the same name and
different content, snapshots/rapid/hidden/raw, limit/reopen, helper B and close,
resize of a file with metadata and a folder at 800×500/440. The final run passed: nine captures at 1000×650 and 800×500/440, normal close
exit 0 after ACK and one helper identified by PID/start gone, no zombies.
Changes outside the strip use Actions; the visible targets use pointer
handlers. This is not equivalent to OS input nor a test of a helper alive exactly at close.


## Kept failures and scope

The three original visual reports are kept: `tabs-ui-1793108574031992206.json`
(wrong Hyprland socket in the harness), `tabs-ui-1793108595977296407.json`
(scroll 120 reduced to 0 by a paint before the listing completed) and
`tabs-ui-1793108996440627071.json` (an empty selection after hiding implicitly restored
to another folder by a rebuild). The asserts are not weakened to
accept them; both state regressions are covered in the suite.

Undo keeps the captured path of its history even if another tab failed
to create. A known result only refreshes its folder; a disconnect/unknown
id keeps a conservative refresh of the current one. The captured path does not
prove resolving the FD's effective directory if a third party renames the parent.

V10 is still partial because of the resource gates and evidence outside Linux/native input.
M23 adds durable versioned persistence of tab metadata, not of tasks. No functional validation replaces the earlier
memory/startup FAILs nor resolves the clipboard or native input.


Final validation: `tabs-checkpoint-test.log` 513 PASS,
`tabs-empty-clippy.log` workspace/all-targets `-D warnings` PASS,
`tabs-checkpoint-release.log` and `tabs-checkpoint-fmt.log` PASS.
The restored selection keeps scroll 120, zoom 125%, filter/sort/anchor and the batch;
A/B contain the same names with different bytes. Raw non-UTF-8 names,
global hidden files, eight tabs/reopening with a new ID and metadata
ready on resize were also checked. The recorder requires Calculate size present at 650 px for a folder
and omitted at 500/440 px; the small captures confirm the omission without overlapping actions.

Resources/startup are still NOT_MEASURED because of the freeze. The footer's incidental PSS excludes
Swap/children and is not budget evidence. The physical geometry declared by the
compositor does not prove native 2x sharpness of these logical framebuffer captures.

M23: SESSION-CONTRACT, 527 Rust and tabs-session-ui-1793111266679001110 PASS 6 FB/8 processes;
it neither repeats nor replaces the three historical M22 visual FAILs.


M26: Columns replaces the Spatial UI. The tab keeps the focused column's path
and selection/filter/scroll; not the whole Miller chain in this unit.
Updated ownership: one active Model/listing plus one reusable lookahead
worker in Columns; caches of up to 8 listings without workers. Queue/media/AuxIo
single. The earlier statement of exactly one pipeline only applies outside
Columns. Limits and evidence in COLUMNS-CONTRACT; no PSS claim/freeze intact.


M27 dual pane: one active Model and a movable Browser with its own listing. The Tabs
of each group use local IDs, always qualified by a stable pane ID; the global
queue keeps (pane ID, tab ID) and captured paths. The 3 MiB snapshot budget is shared by both
groups, including the hidden one. F9 off frees entries/order/stat-demand and keeps only
bounded metadata; reopening reloads. Ctrl+W on the last dual tab closes that pane and keeps the
queue, while the last single tab keeps the ACK close. The secondary worker
is the same ownership transferred from/to Columns, not new threads when toggling.
See DUAL-PANE-CONTRACT. The durable session still covers only the focused group.

## Background result locations

SEARCH-BACKGROUND-TABS adds metadata-only Saved snapshots without changing active
tab/worker/generation/search. It uses the existing joint inactive+closed+other-pane
budget, eight-tab limit, monotonic IDs and restore/selection pruning; empty-history
reserve is charged. Result-directory vs raw parent/leaf is captured before later
activation; no stat/listing/preload is added to admission. Existing session wire
format is unchanged. Native input/resource acceptance remain separate pending gates.
