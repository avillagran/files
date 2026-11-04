# Compare folders in Files

Open **Comparar dos carpetas** from Ctrl+Shift+P / Acciones, or **Comparar esta
carpeta…** from the folder background menu. The tool reads data; it does not
synchronize, overwrite or remove anything. C02 synchronization and the full
explorer goal remain open.

Root A starts with the active browser's directory. In Dual, root B starts with the
opposite browser's directory; otherwise enter it explicitly. A/B are comparison
roles, not a promise to switch physical panes. Local paths, relative paths and
explicit local `file://` URIs use the address editor's reversible policy. Raw
bytes and controls use a URI; literal percent signs/backslashes stay literal.
Inactive long fields reveal their end; a URI badge identifies encoded input.
The tool includes hidden names and ignores browser marks/name filters.

| Control | Effect |
|---|---|
| Ctrl+L / Tab / Shift+Tab | Edit root A; move among both roots and results. Fields support selection, UTF8 editing, undo, IME composition and identity-bound text paste. |
| F5 / Comparar | Capture both roots and budget, invalidate old results, await prior readers and start one helper. Editing/restart waits for its ACK. |
| F8 / Límite | Select64MiB,1GiB,16GiB or1TiB of hashed data. Changing it revokes the old report; it does not start a helper. |
| F7 / Filtro | Retained differences, unknowns, or all observations; no new filesystem scan. |
| Up/Down, Home/End, PageUp/PageDown, wheel | Select/reveal rows in the virtualized retained namespace. |
| Left/Right or chevrons | Read the selected long route horizontally; other row names stay visible. Selecting another row resets its offset. |
| Enter / Shift+Enter, Ubicar A/B | Navigate the active browser to that side's parent and request selection of the exact raw name in the fresh listing. No file opening or mutation; hidden policy remains global. |
| Ctrl+C / Informe | Request the full diagnostic text. In an active root editor, Ctrl+C copies its selection instead. The report button also works without an accepted final report and preserves the existing error. |
| Cancelar / Esc / Cerrar | Cancel or close, retaining the slot until the reader's reap ACK. Window close also waits for that slot. |

Rows distinguish content, type, literal link target, basic metadata, one-sided
presence and unknown observations. Shared directory rows aggregate descendants.
A complete observation is not an atomic snapshot or future validity promise.
Unknown/partial/omitted data cannot prove equality or absence. Roots are inventory
containers; full root/ACL/xattr/owner metadata equivalence is not established.
F5 explicitly refreshes observations. A comparison report never grants mutation
authority; synchronization requires a fresh plan, conflict review and recovery.

Roots and results use distinct state. Semantic root/budget changes revoke old
reports/hits; cursor/focus stamps reject late paste responses. Root fields share
the address editor's URI-only expansion bound and the ordinary4096B decoded-path
limit. Geometry is cached and bounded; rows/hits cover only the visible viewport.
The full diagnostic is built completely under16MiB before copying; no truncated
version is offered. Without a final result it explicitly says there is no accepted
comparison verdict. Escaped diagnostic paths are presentation, never parsed back
into operation targets.

The controller pauses size admission, suspends thumbnails and waits for preview,
size, search and SpaceMap readers before spawning. It restores prior pause state
when appropriate. Modal guards prevent another tool or pane from stealing the
captured context. SpaceMap's own projection resumes after panel close. No UI
thread traverses folders or hashes file data; helper/worker ACK remains separate
from a painted terminal state.

## Evidence scope

`evidence/directory-compare-ui/assessment.json` records the accepted source,
binaries, receipts, retained failures and installation. Foundation and strict
paged wire histories are in DIRECTORY-COMPARE-CONTRACT.md. Owned datasets cover
raw URI roots/names versus literal escapes/replacement shadows, Dual capture,
SpaceMap resume, empty/complete/partial results, long paths, full diagnostics,
exact raw Locate, real helper cancellation and root close ACK ordering. Sparse
files exercise read limits/cancellation; no throughput or resource experiment.

Screenshots are owned-window framebuffers at logical1000×650 and440×440 through
private fixture commands. Native resize/input, clipboard delivery, AT-SPI,2x,
other systems, offline blocked syscalls and global lifetimes remain unproved.
No source push/tag/global PASS. The historical wide-X11 missing-name anomaly is
not causally fixed. PSS/Swap/CPU/startup experiments remain frozen; footer values
are not evidence. New preview formats remain paused.


Current installed checkpoint: source40e42ff, app188b95956b9e, helperd2c5cdca4dd4,
record1793571784648344250.1359Rust/Clippy/fmt/release pass. Final source Wayland
1793570966869599974, X111793571242632970884 and installed1793571789721868433
pass14 captures/6normal exits+drain/7logged comparison readers gone or reused each.
NarrowNames and QueueReview regressions pass sequentially; the retained concurrent
Wayland queue-size failure has no confirmed cause and no product fix claim.
Five installed and five backup hashes match. Synchronization remains pending.
