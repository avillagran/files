# Dual pane — M27 (2026-10-27)

The unit after Columns, following DESIGN 187: F9 replaces the inspector
with a second pane in MC mode, not two inspectors nor two Models. New previews
paused; resource/startup freeze intact. Bounded M27 functional acceptance; installation per checkpoint. No global PASS.

## Use, focus and operations

F9 toggles one/two lists. In this MC checkpoint both panes use List; choosing
another view leaves dual mode. The active pane has a clear border/color and title;
Tab toggles focus only outside the omnibar/inputs/modals. Clicking a row of the
other pane focuses it and applies the selection by identity/token, never by the index of
a different list. Enter/double click navigate its folder; history, filter,
selection/range/anchor and scroll are independent. Scrolling the inactive pane does not
change focus. Places/omnibar/shortcuts/the normal inspector act on the active one.

F5/F6 prefill the opposite pane's directory and keep an explicit review.
The dialog keeps the captured destination's raw PathBuf separate from the display
label; until edited, EXACTLY that PathBuf is used. The first valid
edit replaces the destination with an explicitly typed UTF-8 path; a rejected
paste does not discard the earlier raw value. No unwrap_or_default/to_string_lossy
is ever turned into an operational path. Name 0xFF→destination 0xFE has a fixture.

The operation captures source/destination and owner (pane ID, tab ID) on admission; it does not query
the new focus when running/ACKing. The queue, job IDs, cancel/pause, journal and the media/operation
supervisors are single and global. Completion refreshes both matching
directories (captured paths plus FD-verified locations) and keeps
selection/query/scroll. Changing or closing a tab/pane neither redirects nor cancels the queue.
Rename/permission Apply and a modal review keep focus changes blocked.

## Tabs, close and persistence

Each pane has its own Tabs, up to 8 active/8 closed. Local tab IDs are only
valid together with a stable monotonic pane ID; a closed pane's ID is never reused.
Job owner and diagnostics use the pair, never a u64 tab ID alone. Jobs keep
paths/proofs even if their owner no longer exists. ACKs do not change another tab's focus.
The 3 MiB limit for inactive+closed snapshots is SHARED by both groups, not
3 MiB per pane; admission only subtracts what the modified group frees.

F9 when leaving keeps the focused path as the single list and suspends the
other pane's listing, freeing entries/order/stat-demand after capturing a bounded raw
selection; it keeps Tabs/history/query/scroll and that selection within the
shared budget. When coming back its path is revalidated. Closing the last tab of a dual pane
keeps the other as a single pane, without closing the window or cancelling the queue. The
closed group is removed (no implicit reopening through an ACK); Ctrl+Shift+T works on the closed
tabs of the remaining group. Closing the window does cancel/drain the queue and saves as in M25.
The durable M23 session still saves only the FOCUSED group; an explicit notice in dual
mode. Restoring both halves or resuming operations is not promised. Extending the format
for both is an explicit pending item; existing sessions are not reinterpreted.

## Ownership and local budgets

One active Model; the secondary Browser holds only data/listing/worker. When focus changes,
Vec/HashSet/worker/history/Tabs and pending snapshots are moved, not list clones.
Each reusable worker keeps latest-pending/generation and the global FS limit of 4;
threads are not replaced when a syscall blocks. The earlier Columns lookahead is
suspended when entering MC mode: the same secondary Columns worker is reused when switching modes; 2 requested
listings, without creating another thread when toggling. An old blocked syscall is not
considered interrupted or drained; the global FS limit of 4 still applies, with no secondary
bitmaps/previews. Disabling/closing cancels what is pending, without faking an interrupted kernel D-state.

Data admission for BOTH lists before each batch and selection expansion:
32 MiB shared; at most 100k entries/8 MiB of names per list. An explicit partial/error
if exceeded; never claim an empty folder. Separate reservations: 3 MiB of shared snapshots,
≤1 MiB transient snapshot and 256 KiB of group metadata/paths, channels 8×256 and
bounded jobs per worker, sort keys, stacks/allocator/FLTK/media/global queue according to
their contracts. The existing active Model and the secondary one are counted together;
do not attribute total memory to a chunk/cap nor declare PSS with theoretical limits. Under the
freeze every real budget is still NOT_MEASURED and the earlier FAILs remain active.

## Acceptance

Extend the existing tabs scene: F9/Tab through shared handlers, A/B with the same
name and different content, independent navigation and filter, multi-selection,
F5/F6 operations to the opposite pane with review; raw destination/name 0xFE/0xFF; a job from B
finishes while A is focused, the right one refreshes without overwriting A's selection/filter;
a tab/pane closed during a paused queue neither cancels nor redirects; closing the window
normally 0 after ACK. Shared limits and IDs in unit tests, real captures at
1000×650 and a narrow initial window. No OS input/clipboard/live resize nor metrics.


## M27 validation

603 Rust 194 core/67 helper/342 UI PASS (three pre-existing core tests ignored), Clippy
workspace/all-targets -Dwarnings, release and fmt clean.
`dual-ui-1793126826929914048.json` PASS: eight framebuffers with verified IHDR,
six 1000×650 and two of the initial 800×440 instance; two processes normal 0 after
ACK. A/B navigate folders/back separately; the context menu on the other
pane's batch keeps the selection, and background and stale targets do not redirect it. F5 copies
raw 0xFF bytes to the 0xFE destination, source and occupants intact, while B keeps
focus/query same and A shows the copy filter. F6 review and pause publish nothing; closing
B with a paused job keeps the queue and resuming moves to the captured destination. Both
panes on the same path refresh after rename/Undo in tests; a hidden group without data
and the shared metadata budget tested. Twelve toggles with a blocked backend use
the same secondary worker (a test, not a guarantee of interrupting an OS syscall).

Titles show the basename first and the parent context, with filters visible in each
header as well as the footer. That way A/B can be told apart when narrow and the success
notification does not cover the only filter indicator. Narrow initial instance, not live
resize; handlers/Actions/framebuffer, no OS events. No new preview and no clipboard
reads. The footer's incidental PSS is main-process only, not a gate.

The first compile/Clippy FAILs and the context-menu fixture that clicked an
unselected folder are kept. The fixed fixture verifies a batch member by raw name
and a separate out-of-batch case→selection 1, without changing that semantics.
The first functional PASS scene with the old titles/indicators is kept with a
review sidecar. `dual-checkpoint-validation.json` keeps logs/hashes.
Persisting both groups, other OSes/native input, different views per pane and
resources/startup are still pending. Next: cut/paste/DnD and operations,
without reopening previews or lifting the freeze.


M27 report correction: `dual-title-fmt.log` was a FAIL on a single line because of
wrapping, even though it had been described as PASS. Original preserved. Formatting applied
with no functional change and `dual-format-correction.log` records fmt --check exit 0.
The earlier Clippy/release/scene/installed hashes remain valid; M28 WIP is not rebuilt
or installed for this whitespace-only correction.

## Sort controls (2026-11-01)

Both lists expose Name/Size/Modified buttons and the selected direction. Clicking
the current criterion reverses it; selecting another defaults Name to ascending,
Size/Modified to descending. Compact panels show Nerd icons with names/tooltips.
The same header is used by a normal single list.

Inactive pane sorting uses its ID+listing revision; stale or refreshing targets
are rejected. It preserves active owner, generation, selections, and shared DU
observations instead of switching panels. Metadata requests still use the existing
worker. No filesystem calls in the header/sort handler, no new worker or cache.
Keyboard: Ctrl+Shift+P, search ordenar nombre/tamaño/fecha, Enter sorts the active
panel; repeating reverses. Unknown folder sizes remain at the end both ways.
