# Search result locations in background tabs

The result footer's Nerd **Pestaña** button or **Ctrl+Enter** (while results have
keyboard focus) adds the result's location as a background browser tab. A
Directory result targets that directory; a file/symlink result targets its parent
and captures the exact raw leaf as selection/focus/anchor. The action neither
launches a file nor navigates/focuses the active browser. Enter still opens and
Alt+Enter still locates in the active tab. While editing criteria, Ctrl+Enter does
not turn a draft field into a background-tab action. F1 and the button tooltip
expose the shortcut; compact footer buttons fit the existing minimum width.

The search stays visible with query, result order, selection, scroll, progress
and provisional/final status intact. A short button acknowledgment gives the new
tab ID without replacing progress/error counts. Choosing another result clears
that acknowledgment. Close search normally, then select the tab using the tab
strip or ordinary tab keys. There is no result-preview/prefetch/scanner pipeline
per background tab. The active pane/tab/path/generation and worker remain intact
at admission; the new location is listed only on ordinary tab activation.

## Observations and limits

Provisional matches can be collected, but are still observations rather than
filesystem snapshots or mutation authority. Metadata admission performs no stat,
canonicalization, realpath, directory open or content read on the UI. Missing,
replaced or inaccessible paths are handled by the existing listing worker when
the tab is activated. A path may now refer to a different object. Global hidden
and normal listing selection-pruning policies still apply; a hidden match found
using the search's local hidden toggle does not change global hidden preference.

The existing eight-active-tab limit applies, with fresh monotonic IDs and no
silent replacement. The added Saved snapshot has empty history/query and zero
scroll/pan, inherits the active view/order/grid zoom, and captures at most one
leaf. Path validation and borrowed selection estimate precede cloning. Charge
includes empty-history reserve, selection/focus/anchor, path and container slack;
ordinary aggregate3MiB inactive+closed snapshots and the other pane's retained
metadata are checked before insertion. Failures keep tab count, active browser,
search/results and source selection intact with a search error. Current edit,
mutation, dialog and close gates reject admission. No persisted format changes:
new snapshots use the existing session codec and tab-restore mechanism.

These metadata bounds do not prove memory or speed savings. New previews and
PSS/Swap/CPU/startup experiments remain paused/frozen. Native input, AT-SPI,
platform behavior, resource acceptance and full ROADMAP are separate open gates.

## Validation 2026-10-31

945Rust (324core/85helper/536UI;3 existing core ignored), workspace Clippy
all-targets-Dwarnings/release/fmt PASS. Model tests prove raw focus/anchor/selection
on activation, source filter preserved on return, no active-generation change,
provisional directory path without probing, empty-result no-op, malformed path/
leaf and close rejection, eight-tab cap, closed-state and other-pane budget
failure without insertion. Initial assertion incorrectly treated the empty
History's accounting reserve as zero; corrected to test navigation emptiness,
and source charge now explicitly includes its4096B reserve. Original test FAIL
and sidecar are preserved in background-validation.

`evidence/search-order/search-background-ui-1793463564614078345.json` scoped PASS:
10real FLTK framebuffer PNGs, actual search_batches helper, first provisional
batch kept while adding (same active/query/root/result/helper count), sorting
regression, raw0xFF button440→ordinary tab activation/raw selection, directory
CtrlEnter→empty target, eight-tab error retains tabs/search/active, source hashes
unchanged. Root0 after drain ACK;3logged PID/start search helpers absent/reused,
Z not accepted. Provisional/440/limit PNGs inspected. Private handlers are not
native input/OS shortcut/AT-SPI proof; identities are not lifetime coverage.
PSS/Swap/CPU/startup remain NOT_MEASURED under freeze; incidental footer is not
an accepted tree budget/comparison. No new previews or global PASS.

### Marked batches

SEARCH-MARKS extends this action to explicit marked results in visual order,
with whole-batch count/path/joint-budget preflight and all Saved snapshots prepared
before any tab publication. No marks yet retains focused-result behavior; an
explicitly empty/pruned set does not fall back. Single-result raw/dir behavior and
no-preload/source-context contracts remain. One tab per marked result, including
same-parent files with different captured focus; no silent deduplication/truncation.
