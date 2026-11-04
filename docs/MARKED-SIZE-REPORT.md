# Captured size reports for marked folders

Status: installed sourcea595bb9. Record1793535529326396307 verifies five
installed files and five backup hashes.

The size manager's report adds **Sólo marcadas / Toda la cola**. Shift+I in
queue/details or report captures marked visible directories directly. Plain I
keeps its existing all-queue report/back behavior. F5 refreshes the chosen scope;
it does not start a recursive scan. The report labels its scope and input order.
A new capture replaces text explicitly; background completions do not rewrite it.

Marked reports include only marked visible directory names, with their current
queue observations when available. Files, symlinks, filtered marks and unmarked
focus are excluded, as for marked calculation. A marked folder without a cache
row appears as **SIN OBSERVACIÓN**, not0. Completed zero, queued/running/error/
cancelled and missing observations remain distinct. Subtotal includes completed
observations only, with checked u64 overflow; no measured rows is not zero.

The all-queue report and its existing counter semantics remain available. Other
queue observations are retained when switching scope. This is a read-only view
over loaded entries/cache, with no filesystem probe, helper or second traversal.
It does not add physical allocation, hardlink deduplication, freeable-space or
filesystem-snapshot semantics. Observations may come from different times.

## Identity and bounded output

Root pane/path/generation and queue epoch still gate every command. Report
selection is preflighted before formatting: at most128 unique directory leaves,
64KiB aggregate raw names, existing4096B combined paths. Names stay borrowed from
loaded entries; no label-to-path decoding. Output remains64KiB with explicit
truncation, summary counts cover the whole captured scope, and the existing
wrapped-line cache/virtual scrolling remains. Empty/invalid scope leaves the
previous captured report intact with an error.

The factory also captures an at-most128-element measured-attempt-ID slice.
**Olvidar** in a marked report binds exactly that slice, never all completed queue
rows. An old all-queue ID payload rejects after switching to a narrower scope.
Actual status/ID/epoch validation remains atomic in the queue. Refresh/forget
recaptures the same scope, so forgotten marked names become unobserved. Unselected
records and their IDs survive. This forgets observations only, never files.
Copy still requests the captured text through the existing API; OS clipboard
delivery is unverified. No copied report text authorizes filesystem operations.

## Verification on NODE (2026-11-01)

1235 Rust PASS435core/98helper/702UI+3 historical ignored; workspace Clippy
all-targets-Dwarnings, fmt and release PASS. New tests cover subset17B vs unrelated
100B, captured ID order, unknown/empty/duplicate scope, all statuses/valid0,
u64 overflow, old broader ID rejection and preservation of unrelated observation
through scoped Forget. Existing all-queue report/layout tests pass.

Wayland1793535194990134695 and X111793535284990930976 PASS15 captures each,
one root0+drain ACK and19 logged measurement helper identities gone/reused, no
zombie accepted. Real queue60B -> marked17B -> scoped Forget -> unselected43B;
original unselected IDs and all source bytes/directory/link fixtures remain.
No extra helper is admitted during scope/refresh/forget. Controls are positive,
within440×440 and nonoverlapping. Existing marked-batch, busy-pause, largest-order,
stale-context and raw Open regression is retained. Handlers/framebuffer are not
native keyboard/mouse/clipboard/AT-SPI/2× or all-lifetime/resource acceptance.

App SHA25613253b6adcd6eedab16252e4992b5bbf01c6e475830d0501eae2699339b0a5b7;
helper/HTML unchanged from the previously accepted checkpoint. PSS/Swap/CPU/
startup experiments and new content previews remain frozen. Original intermittent
X11 left-pane anomaly remains unconfirmed; no rendering fix is claimed here.


Installed Wayland receipts1793535611390640623 and1793535752955443428 PASS15
captures/one root0+drain ACK/19 logged measurement helpers gone/reused each. The
latter additionally executes Shift+I from the queue after scoped Forget: it
captures two unobserved marked names without launching a helper. Same accepted
app/helper/HTML hashes. See marked-size-report/install-verification.json.
No source push/tag/global PASS; full explorer goal remains active.
