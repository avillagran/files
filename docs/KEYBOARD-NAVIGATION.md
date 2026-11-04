# Browser keyboard navigation and viewport work

The browser shares a filesystem-free navigation handler for List, Groups, Grid,
Columns and the active dual pane. Text/preview/dialog handlers retain their own
key routing; a path editor does not move the underlying browser on PageUp/Down.

- Home/End focus the first/last visible ordered entry; PageUp/Down move a viewport
  sized step. Shift extends or shrinks the contiguous range from its anchor.
- Ctrl plus arrows/Home/End/pages moves operative focus without replacing marks.
  It establishes a new anchor at that focus. Ctrl+Shift adds a range to existing
  marks. Operations continue to use the marked batch, with the existing focused
  fallback only when the user deliberately focused an unmarked row and no marks
  exist. Explicit Clear still leaves no operative focus.
- After Clear, Down begins at the first row and Up at the last. A first Shift
  navigation creates a one-row range; stale anchors fall back to current focus.
- Columns keeps unmodified Left/Right for parent/child navigation; vertical keys,
  endpoints and pages act within the active column. Dual-pane navigation uses
  list rows even when its underlying saved view is Grid or Groups.
- Treemap Espacio keeps its distinct measured-tile navigation and single-selection
  behavior. Retired internal Spatial compatibility is not a new public view.

Groups now orders its model by the same folder/date sections it paints. Shift
ranges, mouse ranges, endpoints and preview neighbors use that order. The original
name/size/date sort stays stable within each section. Metadata changes can move
entries between date sections; derived boundaries update on rebuild and a moved
focused entry is revealed. This is a live listing, not a filesystem snapshot.

The renderer stores only five derived section endpoints, not a second retained per-file
index collection. The sort computes each date key once; the five endpoint probes are logarithmic. Paint
indexes only the visible folder rows and file rows; List likewise indexes its
visible fixed-height rows directly. Reveal no longer treats a partly clipped row
as fully visible and no longer builds temporary folder/member vectors. Focus
strips distinguish keyboard focus from marked batches in List/Groups/Grid, matching
existing active focus strips in Columns/dual pane.

Page counts are estimates based on the current row/tile geometry. They do not
promise an exact number of pixels across the Groups folder-to-file transition.
The existing asynchronous listing/stat request budgets and mutable-filesystem
behavior are unchanged. Structural viewport tests do not validate process-tree
PSS/Swap/CPU/startup or prove a 4 ms frame time; those measurements remain frozen.

## Validation and preserved failure

708 Rust tests (230 core / 72 helper / 406 UI; three existing core ignored),
workspace/all-targets Clippy with warnings denied, release and fmt checks. Key
rank extremes, empty/stale anchors, shrinking/additive ranges, copy payloads,
editing guard, Groups visual order, dual list semantics, partial-row reveal,
100k viewport hit/text bounds, and an immediate empty Groups frame on navigation
are covered. The derived section endpoints clear with the old order before any
new worker batch arrives.

One full suite failed reading the Python fixture's PID marker (ENOENT). That is
retained in target/keyboard-nav-invalidation-tests.log; no host cause is asserted.
The test now records the actual Rust-spawned child PID through a cfg(test)-only
slot and requires /proc/PID/stat to be absent after return; unreadable live metrics
and zombies fail. The 200 ms timeout is unchanged. A deterministic pre-marker
sleep case exercises this state. Output overflow must report the overflow, not
just any error. The suite is not serialized to conceal contention. Production
helper timeout/kill/wait behavior is unchanged. See evidence/keyboard-nav/validation-notes.json.

The preview follows operative focus; F5/F6 keep using the marked batch, with their
existing preflight and captured-source rules. No native OS input or whole-resource
acceptance is inferred from private key-handler fixtures and framebuffer readback.

A later full suite also preserved a KDE fixture failure: the joined reader returned
Interrupted, but a writer still accepted one byte. No actual inheriting process
was identified, and no cause is claimed. The test now checks both original pipe
identities among reader FDs of the owning process (including duplicates), rejecting
unreadable evidence. This establishes owned-reader closure rather than relying on
global pipe writability. Production KDE transport and its deadlines are unchanged.
No test-thread reduction or global serialization was introduced.

Final evidence: keyboard-nav-ui1793398675053039028 PASS, ten FLTK framebuffer
captures at1000×650/800×440. Private commands call the production key handler;
Home/End/pages/ranges/independent focus/additive ranges, raw copy-form names,
Groups date order/folder boundary/navigation roundtrip, Columns parent/child,
Grid and dual-owner isolation, editing/empty guards, treemap single-selection and
callback/drain/exit0 are verified. Source bytes remain unchanged. No native
KeyDown/Shortcut event delivery or exact active-helper-at-close coverage claimed.

Same final binary regressions: selection1793398677117404890,
sizes1793398679365350932 and search1793398681208364663 PASS. Earlier reports remain
under their own hashes and scopes; the final report includes the invalidation and
test-proof corrections. No global PASS, version tag, resource/startup measurement
or new preview expansion.

Source checkpoint `1c6ee2b` installed, with five independently rehashed installed
artifacts and five backups: evidence/0.0.4/installed-checkpoint-1793398741936931963.json.
Appa4a596d8a8cdac81…; helper9df14eb603083eba… and HTML85ea1b8009ef… unchanged.
