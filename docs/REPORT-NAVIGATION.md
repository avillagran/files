# Operation report navigation

The ordinary operation notice supports Up/Down by line,
PgUp/PgDn by a viewport page with one context line, Home/End for first/full-last
page (Ctrl variants work too). Left/Right and Shift+wheel pan long visible lines;
Ctrl+C, Ctrl+R pending retry, Enter/Esc keep their established meanings. Keys stay
in the modal report and never navigate the underlying folder or invoke an editor.

Four Nerd buttons provide first/previous-page/next-page/last. Unavailable
directions are visibly muted with no active hit target. The status displays the
visible line range and total; empty is0–0/0. Wheel and keys share bounded movement.
Each paint derives full visible rows from actual report/recovery-controls geometry,
clamps scroll after resize/report changes, and measures/draws only visible lines.
No blank one-line tail page; no report cloning, prewrapping or all-line width scan.
Horizontal pan is clamped to the actual visible-line overflow, not full text width.

Local row counts and saturating arithmetic are not a process-memory/performance
claim. Resource/startup freeze, preview pause and native-input/clipboard/AT-SPI/
lifetimes/full ROADMAP/global gates remain unchanged. Functional validation uses
the existing search-order scene with --reports; no new measurement instrument.

Validation2026-10-31:975Rust327/85/563+3existingignoredPASS. Full suite precedes
only a test-iterator style fix and tooltip labels; final2focused tests,
Clippyalltargets-Dwarnings/release/fmtPASS. Scene1793471804962465697 scopedPASS
34realFLTK framebuffers, real Move report10lines/7visible440: keys/boundedlastpage
and previous/next/last buttons, first disabled at start; wide resize fits all10
and clamps0, all directions disabled. Actual first-button activation is not a
native-input claim; Home and shared Start command have unit/handler proof.
Operation ID/completed unchanged; root0ACK/6loggedhelpersgone strictZno, not
lifetime coverage. Tests also cover empty/extreme/resize/nonzero overflow pan
and immutable report lines. First disabled-End harness FAIL and2test-only
Clippy iterator FAILs preserved; no product guards relaxed.
See evidence/search-order/report-navigation-validation/checks.json.
