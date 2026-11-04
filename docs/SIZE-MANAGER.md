# Folder size queue manager

Open **Acciones → Gestor de tamaños**, **Ctrl+Alt+Space** or the searchable
command catalog (`sizes-manager`). The browser selection and focus are unchanged.
The tool borrows the current-root size queue; it is not a second scanner/cache.
Close/Escape does not cancel that queue. Closing the app retains normal helper
drain/ACK requirements. Native OS shortcut acceptance is a separate pending gate.

## Rows and controls

Rows distinguish queued/paused, running `≥` partial bytes, measured `∑` totals,
error and cancelled. A valid empty directory is0B; errors/cancellations are not0.
The summary shows row/queued/error counts and pause state; a drained cancellation
waits for ACK rather than pretending completion. No percentages are invented; a completed-only apparent subtotal is explicitly
qualified below, never a deduplicated filesystem total. Only viewport rows create text/hits.

Click or Up/Down/Home/End/Page keys select a row. Enter, double-click or the Nerd
Info button toggles a wrapped, scrollable diagnostic with available status/error,
escaped raw root/name and measurement details. Up/Down/Page keys/wheel scroll that
view; Enter/Escape returns to rows. Escape in rows or X closes the tool. A stale
context can always close with Escape. There is no native clipboard-copy claim.

- **Pausar siguientes / Reanudar**, or P: the active calculation finishes through
  normal ACK; admission of the next one pauses. No SIGSTOP/fake suspended child.
- **Cancelar fila**, Delete: cancels pending/current selected attempt; an active
  helper still requires cancellation/reap ACK. **Parar todo**, Ctrl+Delete cancels
  all pending/running attempts, preserving completed observations.
- **Reintentar**, R: only an error or explicitly cancelled row; successful totals
  are untouched. Retry preserves pause and must still wait for any old active ACK.
- **Sólo errores**, Shift+R: requeues failures, not manual cancellations or good
  observations. Disabled buttons have no hit target.
- **Lista visible**, L: admits current filtered visible directories through the
  existing whole-batch preflight. It never toggles into Stop when another batch
  appears; finish/cancel that batch first. Loading/SpaceMap/helper-drain conditions
  disable this control. This merges/recalculates names in the current-root queue;
  it does not silently discard measurements outside the current filter.

Changing the browser context, including an async refresh/own-mutation refresh,
invalidates the control context. Old rows cannot be operated. **Usar actual** or
Ctrl+R explicitly recaptures current pane/tab/path/generation and queue epoch;
this is recapture, not a hidden filesystem refresh. Old panel-stamp controls are
rejected even if a new batch is already present. Terminal rows can now be forgotten without closing/refreshing the browser; see below. Browser Refresh still invalidates observations through the existing context policy.

## Display order

The Nerd order button or **S** cycles **Cola → Mayor → Nombre → Cola**.
Cola preserves admission order. Mayor sorts completed apparent-byte observations
largest first; unknown/running/error/cancelled rows follow, never as zero or a
finished total. Ties and unknown rows sort by original filename bytes on Unix.
Nombre sorts original names, including opaque names, without decoding labels.

Only the presentation indices change: selection stays on the same record,
details/cancel/retry still resolve that record ID, and calculation admission order
is untouched. Keyboard arrows/Home/End follow the displayed order. As observations
finish, Mayor can reposition a row; selection remains on that row and is revealed.
There are at most128 temporary indices, no copied names, worker or second scan.
The order is local to the open tool and resets to Cola on recapture/reopen.
No new resource/startup measurement; caps do not prove memory use.

## Identity and bounds

Every enqueue/retry creates a new monotonic per-model row attempt ID, including
same-name recalculation. Active state stores that ID rather than another filename
copy. Progress/finish validates the active epoch/ID. Cancellation plus retry cannot
publish an older positive result into the successor; old Cancel/Retry row controls
are rejected. IDs do not pin the original filesystem inode: these are observations
of the path actually opened and validated by the existing helper, not a snapshot
or mutation proof. External deep changes still require Refresh/recalculate.

The queue still has128 records/64KiB of name storage. The manager holds only its
bounded captured root, scalar positions/stamp/context and short notice, borrowing
record names/status. Diagnostic errors retain512B UTF8 and now mark omitted suffix
with `…`; there is no silent conversion to zero. Wrapped diagnostic work is capped
at32Ki characters (enough for the existing4096B combined root+leaf validation and
escaped paths plus error); rendering visits only visible lines. No new engine,
worker, thread, timer, dependency, filesystem probe on UI, session format or
persistent cache. Pause/cancel/next-pump ACK admission and all helper caps remain.

The modal owns input/hits, blocks background file actions, context menus, native
file-transfer admission, clipboard-target capture and window gestures. Async copy
jobs may finish; any resulting context change makes controls stale. Source queues
are not redirected to another tab/pane. The catalog is bounded (see COMMANDS.md) and F1 lists
the manager shortcut. Resource/startup freeze and new-preview pause remain active.
Local bounds/removed active-name clone do not prove a process-tree memory saving.
Native input/AT-SPI/clipboard, all helper lifetimes, kernel-D/offline, filesystem
snapshots/mount policy, platforms and complete resource acceptance stay pending.


### 2026-10-31 display-order validation

820 Rust PASS (252 core/79 helper/489 UI;3 existing core ignored), workspace
Clippy all-targets-Dwarnings/release/fmt PASS. The full suite precedes only a
fixture Home action; final Clippy/release compile it. Core helper is unchanged.
Pure sort test includes a Running u64::MAX progress, completed zero, equal totals
and failed unknown; only measured totals rank. Model test preserves selected
record across order changes, arrows/Home/End follow visual positions, Delete
cancels correct underlying row; source queue IDs/order never mutate.

First scene size-manager-ui-1793429877969598708.json PASS16FB retained. Three
new/order/minimum PNG inspected: selected E remains revealed in Largest, which
legitimately scrolls B out; follow-up adds Home capture of actual largest row.
No production logic changed for that follow-up. Incidental footer51.0MiB at440
is NOT tree+Swap/resource proof and exceeds ideal at face value. No PSS/CPU/startup
experiment/comparison under freeze. Unknown partial progress was not positively
observed at runtime; pure tests cover ranking it. No native input/OS shortcut or
full helper lifetime coverage; logged identity cleanup is separate evidence.

Final size-manager-ui-1793429969089468901.json PASS17FB; Home shows40kB/9B/8B, NameEnd E7B detail. Three final PNG inspected;9logged helpers gone/reused, root0ACK, not lifetime coverage. Final440 footer50.8MiB incidental/main-only, no resourcegate.

## Open selected folder

Alt+Enter or the Nerd folder button opens the selected row using the ordinary
browser/history path. Enter still toggles details and double-click keeps its
existing details behavior. Row controls bind panel stamp, owner/generation/queue
epoch and selected row ID; a stale/retried/differently selected row never silently
redirects an old Open action. No filesystem probe is added to the UI thread.
The queue already enforces raw name/root component and4096-byte joined path caps.
The raw OsString is joined directly, never parsed from a display label.

Navigation closes the manager, clears old size observations and cancels an active
size job through existing generation/cancel/reap handling; it does not claim to
retain the parent queue while browsing a child. If navigation cannot start (for
example layout persistence rejection), the manager remains open with an error.
A measured row is an observation rather than an inode binding: later replacement,
missing path and permissions use normal browser navigation/error behavior, not a
promise of the same previously measured inode. No new helper/scanner/cache.

855 Rust PASS269core/85helper/501UI(+3 existing core ignored), in
`target/sizes-open-final-tests.log`; fmt PASS. New tests cover selected-ID mismatch,
old context/window stamp, recapture and raw Unix Alt+Enter path. Original directed
run `target/sizes-open-tests.log` preserves 7PASS/1FAIL: fixture failed to reselect
row after Recapture reset selection; corrected fixture selects explicitly, no
production relaxation. Workspace/all-targets Clippy-Dwarnings PASS (`target/sizes-open-clippy.log`).
Release PASS (`target/sizes-open-release.log`).

`evidence/sizes-open/sizes-open-ui-1793437301816876363.json` scoped PASS:
five real FLTK framebuffer PNGs1000×650/440², all five inspected. Four directories
measured with the existing helper (32/9/8/0B); Largest/Home→Alt+Enter opens32B
folder; ordinary history back; refresh makes old Open control unavailable;
recapture/recalculate, raw0xFF row→painted Nerd button opens exact byte path440.
Source file hashes intact. Root normal exit0 after drain ACK. This scenario does
not establish helper PID identity/lifetime coverage or helper-alive exact close.

Original1793437284794884475 FAIL preserved+sidecar: harness attempted to click
Sizes without opening its containing Actions menu. Corrected route opens Actions
first; no production change/deadline increase. No native input/protocol proof.
F1 and button tooltip expose Alt+Enter; Enter/double-click retain details.

Source `f58652b` installed through
`evidence/0.0.4/installed-checkpoint-1793437358813676667.json`.
Five installed artifacts and five backups independently SHA256 rehashed exact.
App `380328da787773f880bfecbe9a8edc500dbb8cb32aae9d8b73aef58910339118`;
helper `d6bc5579a3072be862d3f62c3ebf88e6ac29ca85c25b520faf071fa2256ebe82`;
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install, no source push/tag/global config/window control. Resource and
startup freeze/new-preview pause remain; no speed or PSS claim.

## Captured report and measured subtotal (2026-10-31)

The queue shows a subtotal of **completed apparent-byte observations only**.
Queued/running/error/cancelled rows are counted separately, never added as zero.
No measurements is distinct from a measured empty directory (0 bytes). Checked
addition reports overflow without inventing a numeric total. Measurements can
have different observation times; the subtotal does not deduplicate hardlinks,
represent physical allocation/freeable space, or create a filesystem snapshot.

The Nerd report button or **I** opens an immutable text capture in admission
order, with exact byte counts, row attempt IDs, per-measurement/capture Unix UTC
timestamps and debug-escaped raw names/errors. Existing calculations may continue;
the report changes only on **F5 / Update**, returning to the queue, recapture or
close. Arrows/Page/Home/End/wheel scroll. **I / Enter / Escape** returns to the
queue, retaining row selection/order. Queue-changing controls are unavailable in
report mode; even directly delivered commands cannot change the queue there.
A changed owner/pane/tab/path/generation/queue epoch disables report refresh and
copy; the Current-context footer button or Ctrl+R recaptures the context, Escape always permits closing stale
state. Close does not cancel calculations.

**Ctrl+C / Copy** submits the bounded captured text through the existing text
clipboard API. The notice states that a copy request was sent; this does not
prove OS ownership, bytes delivered to another app, native input or stable global
clipboard behavior. No clipboard read, data-control fallback or file output is
added. The previously intermittent clipboard issue remains open.

The report is capped at64KiB with an explicit omitted-content suffix; summary
counts precede rows. The text is not an operations manifest and escaped paths
must not be reparsed. A cached layout stores at most4096 UTF8 byte ranges over the
same immutable String, recomputed for capture/width/font changes; repaint draws
only visible ranges without cloning line Strings. If the line-view cap is hit,
a visible notice distinguishes it from the text cap; copying retains the whole
bounded text. These local limits do not prove PSS or timing savings. No new
scanner/worker/thread/dependency/disk cache or UI filesystem probe is introduced.

Validation:942Rust/Clippy/release/fmt PASS; final private-handler/real-helper scene `evidence/folder-sizes/size-manager-ui-1793462910855146190.json` scoped PASS21FB/9loggedhelpersgone/root0ACK. First stale mouse-recapture FAIL is preserved with sidecar. See VALIDATION and report-validation/checks.json for limits; no OS clipboard or resource acceptance.

## Recalculate one completed folder (2026-10-31)

F5 in the queue/details or the selected measured row's Nerd **Recalcular** button
replaces only that completed observation with a new queued attempt. Other row
IDs, measurements, names, admission order, selection and pause are retained.
The old apparent-byte total is removed immediately from subtotal and browser
size sorting; queued/partial progress remains unknown until successful ACK.
This is explicit remeasurement of the current pathname, not an inode promise,
watcher or automatic cache invalidation. Original files are not changed by Files.

The action binds panel owner/generation/queue epoch, selected row and attempt ID;
a control for another row, old attempt or context is rejected. Queued/running
rows are not interrupted by F5; cancellation and retries keep existing controls.
A fresh attempt cannot dispatch before the existing helper's ACK. No additional
scanner/thread/helper concurrency, filesystem probe in UI, or disk cache.

F5 in a captured report still refreshes the report only: direct recalculation
commands are rejected there. Close keeps calculations; recapture/navigation
retain their existing invalidation behavior. Help F1 lists both F5 contexts.

Validation:997 Rust (334core/85helper/578UI), three existing ignored, PASS;
after adding immediate browser-sort invalidation, the focused sort/focus test
also passes. Final Clippy workspace/all-targets -Dwarnings, release and fmt PASS.
`evidence/folder-sizes/recalculate-validation/checks.json` records the scoped
scene `size-manager-ui-1793477083884390572`:24 real framebuffer captures, raw Unix
folder9→19 bytes via F5 and19→29 via painted Recalculate440px, exactly two helper
admissions, all other IDs/measurements unchanged, selected fresh attempt retained.
Root exit0 after drain ACK;11 logged helper PID/start identities absent/reused,
strict Z rejection, not lifetime coverage. Original failed build (missing local
test imports) and first scene checker KeyError preserved with sidecars; the
checker now reads IDs from manager rows instead of the size summary. No resource/
startup measurements, native input/clipboard delivery proof or global PASS.

Source `aab8809` installed with five artifacts and five independent backup rehashes matching `evidence/0.0.4/installed-checkpoint-1793477129713826452.json`; app95ff2dcdb315/helper59187351e32c/HTML85ea1b8009ef. Per-user installation only, no source push/tag/global configuration.

## Forget observations (source8347b65 installed; scoped validation passed)

**O** forgets the selected terminal row (measured/error/cancelled); the Nerd button
is shown in its Info/details view. Queued/running or cancelled-but-not-reaped rows
are protected. It never cancels a helper, deletes a file or resets the queue epoch.
It frees that record/name/error slot; monotonic IDs are not reused. Selection moves
to the next displayed row, or previous at the end; browser selection is unchanged.

**Shift+O** opens the report from rows. In the report, the explicit Nerd button or
Shift+O forgets only the completed IDs captured in that report, not measurements
that finish later, failures, cancellations or pending work. All IDs validate before
any removal. Recalculate/stale/duplicate IDs reject the whole request. Close/report
mismatch rejects the command. The new report reflects remaining observations and
states no measurements when none remain; it does not invent0B or free disk space.

Display indices for active/inactive Size-sorted browsers are rebuilt after removal,
without filesystem refresh. Pause/active ACK and pending order stay intact. Report
and panel controls get a fresh stamp; old controls cannot operate on a new dataset.
There are at most128 captured IDs (Arc slice); no new scan/thread/cache. Clearing
successes may leave error/cancelled rows, which can be forgotten individually after
ACK. Native key/clipboard delivery, process budgets and full live-sort propagation
outside these actions remain separate gates.

1128Rust398/91/639+3existingignored; fullworkspace/Clippy/release/fmtPASS. Core128-slot,
active-ACK, atomic frozen subset and both-browser reorder unit regressions. Real
scene1793501340564075091 scopedPASS4PNG1000/440, actual DU8B/0B/raw, same browser
selection/generation, no extra helper on removal/readmission-paused, fresh IDs,
4loggedhelpersgone strictZfalse, callback0ACK. Complete own fixture tree fingerprint
unchanged. No PSS/Swap/CPU/startup sampling (freeze), no native input/lifetime proof.
Initial shadowed stamp/tuple/Arc-marker compile errors preserved; two pause tests
initially tried pausing a drained queue, fixed with a real pending row (raw FAILs
preserved). No production pause semantics weakened, no serialized tests.

## Live ordering across dual panes (2026-11-01)

An accepted recursive measurement now updates an inactive Size-sorted browser,
even when the focused browser is sorted by name. Recalculation from the browser
or size manager immediately removes the previous size key in both panes.
Unknown remains unknown until the new helper result arrives. Selection and pane
identity remain stable; no filesystem probes are added to sorting.

`State::finish` signals only a fresh measured result (zero included), never an
error or canceled/stale ACK. The central pump rebuilds the active browser once;
the inactive browser receives the same accepted change.

The preserved RED regression shows the inactive order staying stale with an
injected completed Job, not a real-helper reproduction. Three new unit tests
cover completion, both recalculation routes, and zero/error/stale signaling.

Real-helper scene `size-sync-ui-1793502610961034290.json` scoped PASS:
5 actual1000×650 FLTK framebuffers, completed20→1→16B updates and paused
browser/manager invalidation, active Name order and inactive selection/id/path
stable.4 logged helper PID/start identities absent/reused (Z not accepted),
root exit0 after ACK. Owned actor changes Z only; A source bytes unchanged.
Handlers/fixture keys, NOT native OS input/live resize/lifetime/resource proof.


## Marked-folder admission

**Marcadas · M** and M in queue/details capture only the browser's marked visible
directories. The extra third control row retains virtual scrolling; report mode
keeps its existing controls. Busy batches/preflight rejection keep prior state,
never toggle into Stop. Old unselected observations remain in the shared queue,
so its completed subtotal still includes them. MARKED-FOLDER-SIZES.md records
identity, raw-label, layout and functional acceptance details.


## Report scope

The captured report now offers **Sólo marcadas / Toda la cola**, with **Shift+I**
for marked visible directories. Unknown cache rows are explicit sin observación;
F5 refreshes the scope without a new scan. Forget binds only measured IDs in that
capture, preserving unrelated observations. See MARKED-SIZE-REPORT.md for full
scope, bounds, overflow/status behavior and end-to-end acceptance.
