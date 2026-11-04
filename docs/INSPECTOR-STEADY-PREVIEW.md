# Keep the Inspector image stable during periodic rechecks

The thumbnail worker rechecks file revisions every second. Previously dispatch
cleared `Demand::verified`, and painting also hid the bitmap as soon as the
check aged past one second. Any repaint during that asynchronous check showed
a placeholder, even when the subsequent FD probe confirmed the same revision.
This explains a periodic blink without any file change.

Recheck expiry now schedules validation without discarding the last confirmed
visual snapshot. Painting borrows that same cached RGB while the check is
pending. No second image cache, extra worker, or filesystem work in paint is
introduced. `ready()` and the diagnostic `snapshot().ready` still require fresh
proof: a displayed snapshot is not a fresh observation or mutation authority.
Known invalidations, directory changes, unavailable/failed sources and close
still revoke the image through the existing paths.

The new regression failed on the previous implementation and passes after the
fix: expired proof is not ready, but painting before/during the recheck retains
the image without growing cache RGB; explicit invalidation hides it and frees
that cache. The workspace passes 1621 tests (708 core,112 helper,801 UI), with
three historical ignored tests unchanged.

The mapped media scene now waits for the first real Inspector image, then
asserts 48 passive steady-image samples across multiple recheck intervals,
before the existing right-press/hold/release and overlapping-menu checks.
It uses private fixture data and internal FLTK events, not OS pointer input.

The first installed-M2 scene also caught the initial directory-watch handshake
reloading the listing just after the first decoded image. Tracing showed the
watcher's initial attachment/refresh (generation1→2), not a periodic source
change. Those failed reports are retained. The steady-image scene now waits
for `listing_sync.settled()` as well as a painted, ready image before measuring
periodic checks. Initial listing synchronization and actual invalidations are
not disabled. Diagnostic state on failure is retained in the test log.

Installed and checked on NODE and M2: five backup/installed hashes verified;
post-install scenes passed. Assessment and retained failures/raw frame archives:
`docs/evidence/inspector-steady-20261104/assessment.json`. NODE app264a3e12f278,
M2 app17ee12aaa4f4. Existing windows were not restarted.
