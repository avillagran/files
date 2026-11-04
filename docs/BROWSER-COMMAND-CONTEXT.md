# Browser command context

SpaceMap uses F5 to recalculate the map and renew its root listing. Ordinary
listings use F5 to open copy destination review. The real key dispatcher and the
MC bar now obtain this action from the same `keyboard_nav::f5_action` function.
The existing actual-map predicate requires SpaceMap with dual panes disabled.

The map inspector's Copy button and entry menu still open copy review. Their
captions no longer advertise F5. Copy now has a generic action tooltip describing
destination review. F6 movement captions and operation engines are unchanged.
No operation is started by opening or closing a copy review.

The palette's existing Refresh action is presented as “Recalcular Espacio y
actualizar carpeta” with F5 / Ctrl+R in the map. Searching “recalcular” finds it.
The action still retries listing observation and invokes the existing refresh
controller. In other views its caption and Ctrl+R shortcut stay unchanged.
Map selection labels, the refresh label and shortcuts are derived by the same
functions for painting and fixture metadata.

Palette and MC-bar map restrictions now use the same actual-map predicate. Tests
cover a synthetic model with both the SpaceMap tag and dual panes enabled, where
listing commands must apply. This is an invariant defense, not evidence of a
reproduced user-facing dual-pane failure: normal dual activation currently changes
the view tag to List. Existing map restrictions on other palette/MC commands are
retained; this checkpoint does not declare the full command matrix complete.

Acceptance:1296 workspace tests (447core/105helper/744UI) and three historical
ignored, normal concurrency; final Clippy all targets warnings denied, format and
release PASS. Wayland1793556363461275656, XvfbX111793556366342518840 and installed
Wayland1793556407655363636 each passed19captures/3normal exits+drain/7logged
SpaceMap helper identities absent or reused. Captures inspected palette, compact
copy button, entry-menu wording and two-name copy review in a partial map.
Install1793556391638203701 has five verified installed and backup hashes;
app5a9660a4ed5c66083bd8b5c641ab426ebee137a8b3edc9fbd73795026a3bf59f.
Receipts: `docs/evidence/listing-model/space-map/command-context/assessment.json`. Captures
and shared internal handlers do not establish native OS key input, clipboard,
AT-SPI, 2x rendering, global helper lifetimes or resource/startup performance.
The full explorer goal and unresolved ROADMAP gates remain active.

A normal concurrent workspace run exposed an intermittent watcher-test failure:
its Create+Modify write could be consumed in separate callback deliveries around
an explicit refresh. An owned Linux inotify audit confirms that write produces
both root events; publication by rename from an unwatched subdirectory produces
one root event. This supports fixing the test stimulus, but does not identify
the exact callback ordering of the historical failure. The test now stages
before watch attachment and publishes by rename. It also verifies that another
write after refresh still restores the sticky change hint without automatic
refresh. Production watcher logic is unchanged; no quiet-time sleep, ignored
late callback or serialized test harness was introduced. The failure log and
kernel audit remain in `docs/evidence/listing-model/space-map/command-context`.
