# Incremental search results

Ctrl+Shift+F searches names or literal UTF8 content. Matching rows can appear during
traversal, labeled **PROVISIONAL**. Arrow keys/row clicks select; Open/Enter and
Locate/Alt+Enter remain explicit normal path observations, not authority to mutate
or a promise of the original inode. Final root validation does not make an entire
tree snapshot. Filesystem/content work can delay any update; no latency guarantee.

New query/edit/hidden scope/cancel/close immediately removes provisional rows.
An error removes candidates rather than presenting partial failure as success.
The normal final Report replaces candidates only for the current intent after
helper reap. Selection follows the raw relative path into the sorted final list.
A complete empty result is distinct from a failed or bounded partial traversal.
The500-result limit remains explicit; paging/index/semantic search remain pending.

## Wire and validation

SearchBatches / `search_batches` is a separate command. Existing Search/`search`
and SearchStream/`search_stream` retain their prior report/counter wire.
Initial SearchProgress is zero; later counters are monotonic. Each update emits
progress then version1 SearchBatch payloads of<=32 hits with consecutive offsets.
Helper serializes borrowed scanner slices without cloning paths, first nonempty
prefix without waiting128 visits; later checkpoints100ms/128-boundary cadence.
Final Search/Error follows scanner root FD/name/mount revalidation. A callback
prefix never means terminal success: a later root replacement can invalidate it.

The reader opts in, checks ID1/version1/counters, offsets, duplicate/malformed
paths, content mode/line/byte bounds and cumulative500hits/256KiB path capacity.
A rejected batch leaves prior accumulator state unchanged; stream errors poison
the reader and clear candidates. Final reconciliation compares every path/kind/
content position after sorting, not a digest or count alone. A final result needs
exact agreement with counters and candidates; wrong/truncated/trailing/duplicate
or summary-free streams never become successful results.

The supervisor still owns nonblocking pipes,15s external deadline/cancellation,
background kill/reap and Linux PDEATHSIG. Scanner retains10s cooperative budget,
250k visits,64depth/same-mount/no-follow policy and lazy metadata. This does not
solve uninterruptible kernel/offline I/O. UI never joins the reader thread.

## Ownership and bounds

The existing reader thread publishes one latest immutable Arc prefix slot, only
when count changes. Every snapshot is a full prefix, so replacing an unread one
loses no preceding match. No thread per update or unbounded channel. Intent revision
rejects old snapshots/replies; atomic invalidation hides failed candidates at paint/
action access before waiting for terminal ACK. No new helper starts before reap.

Scanner paths<=256KiB; decoder paths<=256KiB. Visible, unread and under-construction
snapshots can each own<=256KiB. Decoder+up to3 snapshots =<=1MiB path capacity; final
parse can coexist with decoder/visible/unread (also4 populations). Hit arrays/Arc
headers/<=500 reconciliation references/JSON and pipe scratch are separate. Per-line
2MiB/message256/supervisor total4MiB output caps remain. This local accounting is
NOT process-tree PSS/Swap evidence, an ideal50MB PASS or a global resource budget.
No new engine, worker thread, persistent cache, dependency or license adopted.

## Acceptance and preserved failures

837Rust258core/82helper/497UI+3existingignored PASS; workspace all-targetClippy
-Dwarnings/release/fmt PASS. Full suite precedes only hidden-fixture raw-byte
presentation and final count wording; final Clippy/release compile those.

Final search-batches-ui-1793432390766040716.json PASS7FB1000/440, all7 inspected:
provisional row before terminal while logged PID/start live, exact40-name final
and selected-path preservation, cancel clearing, root replacement terminal error,
new query ignoring old prefix, raw content at440 and provisional Locate through
normal loaded listing. Source file hashes intact, root0 after drainACK;6logged
helper identities absent/reused, original zombies not accepted. No lifetime claim.

The fixture holds the REAL helper after its first flushed nonempty batch while
`.files-search-gate` exists, at most5s, only when all three flags equal1:
FILES_TEST_HIDDEN_CONTEXT, FILES_TEST_ISOLATE_INPUT,
FILES_TEST_SEARCH_HOLD_FIRST_BATCH. It creates/modifies no gate itself. This proves
UI/wire during an owned artificial hold, not natural timing, kernelD or performance.
Normal runs do not enable it. Earlier7FBPASS1793431914206752394 retained before
final wording `visibles: N`.

Same final binary content-search-ui-1793432406201870889.json PASS17FB,14logged
helpers gone/reused/root0ACK. Four regression PNG inspected: binary/file-limit
partial, complete content, raw Locate,500-limit. Preserved FAIL1793431925772843277
accepted path/selected before painted visible rows; predicate now waits original
full assertion. FAIL1793432050269332012 cleanup checker only matched legacy helper
command; both exact known names now checked with unchanged PID/start/no-Z rule.
Sidecars retain both;1793432088913319951 PASS on prior wording binary is retained.
Initial helper compilation failed on direct Serde import; Frame moved to core's
existing Serde edge (no dependency addition). Foreign Cargo guards delayed final
builds without controlling those processes. All logs/evidence retained.

Resources/startup NOT_MEASURED under freeze; incidental footer PSS is main-only,
not a tree budget. New previews paused. Native input/clipboard/AT-SPI/HiDPI,
full lifetimes/offline/platform coverage and global goal remain unvalidated/open.


2026-10-31 extension: optional Hit.snippet is now included in bounded snapshots
and exact final equality; see SEARCH-SNIPPETS.md. Each population adds<=128000
bytes fragment capacity, up to4 retained UI populations<=512000 bytes beyond path
capacity accounting above. Arrays/allocator/wire remain separate, no PSS claim.
New same-binary regression search-batches-ui-1793434087267418740 PASS7FB includes
fragment data/byte mark before final and existing cancellation/root/latest guards.
