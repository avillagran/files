# SpaceMap observations and change hints

Source acceptance:1,281 workspace tests (443core/105helper/733UI) PASS with
three historical ignored, Clippy all targets warnings denied, format and release
PASS. Wayland1793547926193182418 and XvfbX111793547929157505544 each passed
7captures/root0+drain and5manual helpers gone/reused. Legacy SpaceMap
1793547949176254397 passed9captures/11helpers/root0drain; general listing and
Columns regressions passed. Initial failed fixtures and limits are retained in
`docs/evidence/listing-model/space-map/assessment.json`. Installation is recorded
separately after source acceptance.

SpaceMap is an observation of apparent regular-file bytes, by name. A complete
walk is not an atomic snapshot and does not imply that data remains current.
The shared listing watcher is nonrecursive: it watches the opened root, not every
descendant visited by the calculator. Recursive native registration has no bounded
watch count here and is deliberately not introduced as hidden background work.

The map displays this scope next to its controls, including at minimum width.
Root change hints latch a visible `Cambios detectados` state without discarding
the report, changing selection/generation or launching another calculation.
A failed native root registration or reattachment after an earlier attachment latches a separate
`Vigilancia interrumpida` state. Reports and their original completeness/partial
status remain visible. These hints are not authority for operations on files.

The notification actor has a filesystem-event counter separate from its serial
(which also advances for attachment/status notices). Initial Ready attachment
alone does not invent a file change. The bridge stores the last admitted counter
and attachment. In SpaceMap, it converts matching root notices into sticky flags
and ACKs the exact serial, including while calculations/editors/forms are busy:
this only updates presentation state, never reloads an interacting browser.
A later event survives an older ACK and is consumed on a later pump. Failed-watch
footer status remains explicit; root hints do not create an indefinitely pending
promise of automatic refresh.

Flags survive calculation completion, cancellation, and a queued open/dispatch.
A new explicit observation intent clears them before listing/scan work. Events
after that boundary latch again; completion does not clear them. Path/view changes
reset observation context. No clean flag asserts absence of internal changes or
lost events. Descendant-only modifications require explicit recalculation.

Both F5 and the labeled Recalcular button now reload the directory listing and
queue a new map scan. Renewing the listing is essential: report nodes are mapped
to raw names in that listing to create selection/navigation hits. Previously the
button only reset the calculator, so a new child could appear as a measured block
without a corresponding current listing entry. Refresh still preserves lossless
selection under the existing admission rules and uses the single-flight scan
cancel/result/reap ACK barrier before replacement. Hints never bypass that barrier.

Tests separate watch attachment from events, verify no restart on invalidation,
retain flags across Loading -> Ready/cancel/queued dispatch, and verify that root
hints do not reload the browser until explicit recalculation renews its names.
The unit executable has no sibling calculator helper; actual helper dispatch,
bytes and painted selection hits are verified by the owned release UI fixture.

`scripts/space-map-watch-ui-check.py` covers direct root create/rename/delete,
unchanged observed report until explicit button/F5, nested changes requiring F5,
root removal/replacement with persistent watch-gap warning, new child hits and
440-wide controls. The existing `space-map-ui-check.py` retains partial/error,
geometry, navigation, cancellation and helper-close coverage. Internal handlers
and framebuffers do not prove OS input, clipboard, AT-SPI, 2x sharpness, hard
blocked-syscall deadlines or global resource/lifetime acceptance. No PSS/Swap/CPU/
startup experiments or new content previews are part of this change.

The no-area list reserves up to three complete rows for visible zero/unknown/
partial nodes, with remaining rows scrollable. The prior fixed reserve exposed
only one row and made the zero-entry mouse fixture depend on enumeration order.
Keyboard reveal and tiny-positive-area list sizing still require a separate
audit; this change does not claim those edges complete.

Installed on NODE from source `a3f5094`, receipt
`installed-checkpoint-1793548043316382038.json`; app SHA256
`e0c108956754939f51c96926b99b11130f8959036e170251400e7d4d9a8a077b`.
Five installed and five backup hashes verified. Installed watch
1793548057716806204 passed7captures/root0+drain/5loggedhelpers absent or
reused. Installed allocation1793548058562662926 passed22captures/2roots0+drain/
25loggedhelpers absent or reused. Scope and unresolved gates remain as above.

F5 binding correction: SPACE-MAP-NAVIGATION.md replaces the old global Copy
binding specifically in SpaceMap and updates the MC strip/help. The original
change-hint fixture called Action::Refresh directly and did not establish the
advertised F5 dispatcher behavior. The navigation fixture now shares the real
F5 handler and verifies the context-sensitive MC mouse action as well.
