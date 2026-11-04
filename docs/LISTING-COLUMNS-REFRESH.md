# Automatic refresh in Columns

Source acceptance: 1,278 workspace tests PASS (443 core, 105 helper, 730 UI),
three historical ignored; Clippy all targets warnings denied, format and release
PASS. Wayland1793546575995551884 and Xvfb X111793546622163571675 each passed
seven captures and one root exit0 with close drain. General listing and allocation
regressions passed. Receipts and retained initial failures are in
`docs/evidence/listing-model/columns/assessment.json`. Installation is recorded
separately after acceptance.

The active column uses the existing Model reload. Cached ancestors and the
lookahead column use the single reusable Columns worker, sequentially. No worker
is added per column. Native watch targets include the active browser plus cached
columns, at most eight, derived from `columns::MAX_COLUMNS`. Dual-pane and Columns
are exclusive modes; hidden tabs retain no watch targets. Existing 16 MiB shared
data, 100,000 entry and 8 MiB name limits remain in force.

Each column has a stable watch owner separate from its interaction ticket. A
cached reload rotates the interaction ticket before dropping index-bearing data;
old hits cannot activate a different entry. The watch owner and native attachment
survive reloads and unrelated configuration changes. Switching the active browser
still validates the destination through the Model worker before operations.

Before clearing a cached listing, admission captures selected raw names, focus,
actual range anchor and a scrolled viewport's first row name plus offset. The
snapshot and viewport reserve must fit the shared data budget while existing
storage is still retained. Failure leaves the listing, ticket and notice pending.
Explicit empty selection remains empty. Done/error or a partial-budget terminal
result restores only names present in the current visible order. Missing viewport
anchors retain the existing numeric scroll/clamping fallback; top stays at top.

Cached reloads wait for the current reusable-worker listing to finish. Its batch
budget and stat requests route to the actual pending column, including ancestors,
rather than assuming the last column owns that worker. An active reload or branch
truncation preserves an in-flight cached reload if its column survives; lookahead
creation waits rather than restarting that worker mid-reload. UI entry activation
waits for pending cached selection restoration.

If a completed ancestor listing no longer contains its next branch as a directory,
all cached branch tickets are revoked. Files keeps the current active path and
Model data as the root of a fresh chain and shows a branch-changed notice; it does
not navigate to a similarly positioned entry. An unavailable active path presents
its reload error; native reattachment can recover when that path returns. These
are live observations, not inode identity guarantees for operations.

The common interaction deferrals apply to every column: editors, menus, forms,
reviews, native transfer/drag, search, previews, size calculations and copy queues.
Notifications remain sticky until admission; a later event cannot be ACKed by an
older notice. Closing uses the same actor stop acknowledgement.

Normal-concurrency filesystem tests cover raw-name lookahead changes, stale-ticket
rejection, ancestor marks/anchor/named scroll, missing branch revocation/recovery,
and a chain growing beyond eight columns while the retained oldest root receives
external changes. The owned UI fixture is `scripts/columns-watch-ui-check.py`.
Neither internal handlers nor framebuffer captures prove native OS input,
clipboard delivery, AT-SPI, 2x sharpness or global resource/lifetime acceptance.

Columns installed on NODE from source `ac02206`, receipt
`installed-checkpoint-1793546728652462098.json`; app
`b3d6204df2e923bcd8a8363f9419d57774688ada4a5b8d6084a4fa910ec984e9`.
Five installed and five backup hashes verified. Installed Columns receipts
1793546741743502286 and1793546823293042852 each passed7captures/root0+drain;
installed allocation1793546792633964223 passed22captures/2roots0+drain and
logged helper identity checks. Initial fixture parse failure is retained in the
Columns assessment; the completed log had80valid JSON markers and the reader
now waits for complete lines. No global acceptance or resource claim.
