# Copy conflicts

Default remains Stop. Explicit Skip Existing is captured per Copy job, never a
preference silently carried into another dialog or Move. It skips an occupied
selected-root destination and continues other roots without merging directories,
following a destination symlink or replacing any occupant. A missing/changed
source, permission error, invalid self-copy or internal staging conflict remains
an error. Explicit numeric renaming is now available through [Keep Both](COPY-KEEP-BOTH.md). General overwrite/merge and Move conflict resolution remain open.

A publication-time RENAME_NOREPLACE collision may skip only when this exact
publication syscall returned AlreadyExists, the root was never published and
all temporary cleanup identities are confirmed removed with zero omitted/error/
unresolved/rescue entries. Any uncertain cleanup stops with the original report.
Copied-byte/entry counters may include discarded staging work; skipped-root count
and bounded requested destination paths (occupancy observed in the opened parent, not a current-location snapshot) are separate from confirmed publications.
Cancellation and per-job options keep existing supervisor and queue semantics.

Completed+skipped path details share the old report-detail count cap. Counters
remain complete and omitted counts explicit; no unbounded result history or extra
process. Existing queue reserve stays4MiB including2MiB ACK headroom, with revised
capacity derivation/tests. No PSS/Swap/CPU/startup benchmark during freeze, no new
preview. General conflicts/Move/journals/other-platform/native input stay open.

## UI and reports

Copy review now offers a Nerd button cycling Stop → Skip → Keep Both with F4.
Every new dialog starts at Stop; Move neither shows nor accepts the policy. An occupied selected directory is
omitted in its entirety; no child merge is attempted. The result distinguishes
published roots, occupied requested destinations and report paths not retained.
Shared completed+skipped details use max_report_details (UI default64, hard128),
not an unbounded per-file history. All selected-root counters remain complete;
requested omitted paths are not current-location/deletion proof. Staged bytes
and entries discarded after a late collision remain in logical-work counters.

Counters precede potentially long diagnostics so output truncation cannot hide
their totals. Compact completion messages lead with counts. Toast geometry only
reserves inspector space when it is actually shown; previously a hidden inspector
at440 subtracted280px and clipped the skipped count. Existing report scrolling/
horizontal pan remain needed for long paths and details below the fold.

## Scoped validation — 2026-10-31

806Rust PASS249core/79helper/478UI,3 existing core ignored. Full conflict engine,
policy/report/budget code covered by `target/copy-conflicts-report-tests.log`;
last suite precedes only compact toast visibility/wording and short dialog hint.
Final workspace/all-targets Clippy-Dwarnings/release/fmt PASS in
`target/copy-conflicts-notice-*`. No warning suppression or new dependency.

Core tests: default no-clobber preserved; explicit skip initial file/symlink,
publication race with clean stage continues, race with actor data in stage returns
error and preserves it, missing/self-copy source not masked, directory never
merged, skipped roots obey entry budget, shared detail cap/counters/omissions.
Capacity fixture includes both completed+skipped Vec capacities, shares payload
slots and keeps the same2MiB ACK reserve under the4MiB queue budget. This is source
accounting, not a PSS proof. UI tests cover fresh default, F4 and Move rejection;
report counters remain visible when cleanup text exceeds the visual limit.

`evidence/conflicts/copy-conflicts-ui-1793425513683762662.json` scopedPASS8realFB
1000×650/440²: default Stop publishes1 before collision, explicit F4 Skip
publishes2/skips2, raw filename bytes copied, destination symlink and existing
directory/sentinel preserved, next dialog off by default, mouse enables Skip at440
and all4 existing roots skip/0 publish, details/counts, Move F4 no effect. Source
hashes/occupants intact, no staging directories, root0 after drainACK. All8 PNG
inspected; counts/buttons complete440, long paths require existing pan/scroll.

Original scene1793425188533528112 functionalPASS8FB but compact440 visualFAIL
preserved with sidecar; final source fixes hidden-inspector reserve. Initial
E0425 publication flag in wrong closure corrected; original log retained. Guard
refused foreign Cargo1595892/1614730, then admitted own serial checks after idle
snapshots; no foreign edit/control or whole-lifetime absence/performance claim.

No native keyboard/mouse/clipboard/AT-SPI/HiDPI/kernelD/mount/powerloss/lifetimes/
helper-alive-close/global acceptance. Publication races are actual core fixtures,
not the UI scene. PSS/Swap/CPU/startup NOT_MEASURED freeze; footer incidental is
not a gate. New previews paused, historical failures and full ROADMAP remain open.

Source `fdc8efa` installed through
`evidence/0.0.4/installed-checkpoint-1793425631915265384.json`. Five installed
artifacts and five backups independently rehashed exact. App
`ae7461f282a338ec73b6a0dc00c3436e83a8315535f82fa3fbbd894f09cd473f`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install, no source push/global config/window control.
