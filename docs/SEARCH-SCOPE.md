# Search root navigation

Sourceb405554 installed; scoped functional/visual validation passed. Search stays open while the
user chooses parent, folder of a result (directory itself or file's parent), or
previous root. Click the root caption to reveal the Nerd Font menu. Alt+Up/Right/
Left use the same actions; menu arrows/Enter choose and Escape dismisses.

Criteria (name/content/filters/applied date/hidden/order) survive; a pending date choice blocks root navigation instead of silently discarding it. report/provisional rows,
marks, selected row, page cache and geometry do not carry across roots. No search
starts automatically. Explicit Search/Enter replaces it using the existing single
helper admission and old-helper reap ACK. Late responses cannot populate a new root.
The menu's result index/epoch is captured on open and revoked on result refresh;
parent/back use root epoch so streaming results do not make them unresponsive.

Root operations are lexical and observational, not a new filesystem authority.
Result relative paths are validated before joining, raw bytes preserved; roots up
to4096B, history8/32KiB path capacities without result snapshots. No FS probes on
UI. The normal helper handles missing/replaced roots, no-follow/mount/error policy.
Close/reopen clears this history. Back restores roots, keeping current criteria,
not a previous result snapshot. No durable search-session history is implemented.

New previews remain paused. Resource/startup freeze unchanged. Internal key/pointer
handlers and FLTK readbacks do not prove native OS input/clipboard/AT-SPI or budgets.

## Evidence

1121Rust398/91/632+3existingignored, Clippyworkspacealltargets-Dwarnings/release/fmt
PASS. Seven new regressions: criteria/stale controls, old-task ACK, bounds/history,
raw/invalid paths, revoked menu result, responsive parent during result refresh and
pending date not discarded. Initial test compile put Fake in an inaccessible test
module; log preserved, fixed separate test module. No test serialization/softening.

Final scene1793499783655547501 scopedPASS8PNG1000/440, three own apps: parent/result/
back by keys and painted controls, raw0xFE root, preserved query/no auto helper,
content-mode caption, callbackroot0ACK. Old helper PID/start live before scope change;
new root report has expected prefixed paths and no old unprefixed needle rows.
Six logged helper identities subsequently gone/reused, strictZnotaccepted; not full
lifetime/overlap/kernelD coverage. Original1793499682385206785 PASS retained, final
only strengthens the absence assertion to cover every old unprefixed row. No PSS,
Swap, CPU/startup experiments. See evidence/search-scope/checks.json.
