# Calculate marked folders

Status: installed source1c5cae9. Record1793534258501090318 verifies five
installed files and five backup hashes.

The command catalog adds `sizes-marked` / **Calcular tamaños de carpetas marcadas
visibles**. The size manager adds **Marcadas · M**, also available with M from
its queue/details view. This action captures only marked directories in the
current visible order. Files, symlinks, filtered-out marks and an unmarked focus
are excluded. Empty marks do not fall back to the focused directory. Browser
marks, focus and hidden/filter policy are retained.

The action uses the existing whole-request preflight and queue, with the same
128 rows/64KiB names/combined4096B path bounds. An unavailable listing, Espacio
or an existing queued/running/awaiting-ACK batch rejects without changing that
batch or its pause. It is never a toggle into Stop. Failed preflight retains old
observations, IDs and pause. Retry after a completed batch is an explicit new
calculation of those marked names, with fresh attempt IDs. Other observations in
the same queue remain; the manager subtotal still covers all completed queue
rows, not a separate selection-only/deduplicated total.

The catalog validates the existing pane/path/generation and exact selection
proof before dispatch. The modal manager retains its owner/generation/queue-epoch
checks. The controller checks current readiness again. No new engine, worker,
filesystem probe, persistent setting or scan is introduced; helper admission,
progress, cancel/reap ACK and result identity remain unchanged. New root dispatch
opens the existing manager. Ctrl+Space focused and Ctrl+Shift+Space listed/toggle
semantics stay unchanged.

The manager uses a third control row, preserving viewport-only row rendering,
scroll and the existing two-row report controls. All enabled targets fit and do
not overlap at440×440 in the owned fixture. Names in manager rows now use the
common escaped native-name labels; byte/control/backslash labels are never
reinterpreted as paths. Open still uses the record's original name and ID.

The catalog has65 entries after adding this operation. Its old64-entry test
ceiling is replaced with a compile-time96-entry cap and a matching registry test.
The match vector reserves the actual registry count, not96 entries per file;
selection proof remains64KiB and query remains256B. This local bound change is
not a process memory/CPU claim.

## Verification on NODE (2026-11-01)

1232 Rust PASS435core/98helper/699UI+3 historical ignored; workspace all-target
Clippy-Dwarnings, fmt and release PASS. Tests cover raw directory capture,
file/link/filtered/unmarked-focus exclusion, stable marks/focus, busy batch and
pause preservation, >128 whole-preflight rejection and empty selection retaining
an old measured observation. The existing minimum manager bounds test passes.

Wayland1793534032938582542 and X111793534064819898218 PASS10 captures/one root0
with drain ACK each. All19 logged measure_directory helper identities per run
are gone or reused, no zombie accepted. The private fixture marks8B and9B folders,
a file and link while focus is an unmarked empty folder; only the two folders
produce17B. The actual palette and manager M route are exercised, recalculation
has fresh IDs, paused busy rejection has no new helper/cancel, and all original
file hashes stay unchanged. Existing largest-order/AltEnter/stale-context/raw
Open at440 regression passes. Only owned data is measured.

The first functionalPASS1793533918839408159 had no helper logging enabled. It is
retained as functional/root-drain evidence only. Final runs enable logging and
require at least one identity before checking cleanup, avoiding vacuous lifetime
claims. Measurements remain apparent per name, with hardlinks counted per path;
no physical allocation/freeable-space/snapshot or mutation authority is implied.
No native OS input/clipboard/AT-SPI/2×/all-lifetime/PSS/Swap/CPU/startup evidence.


Installed Wayland1793534268171728440 PASS10 captures/one root0+drain ACK/
19 logged measurement helpers gone or identity reused, no zombie accepted.
App SHA2564cbe949e0e8262b828f586113911e46bd23aad4ed5e9559bcc9816568b745ded;
helper/HTML hashes unchanged from the accepted prior checkpoint. See
marked-sizes/install-verification.json. No source push/tag/global PASS.


A separate marked-only report/subtotal is now available via the report scope
button or Shift+I, without changing the queue-wide default summary. See
MARKED-SIZE-REPORT.md; unselected queue observations remain intact.
