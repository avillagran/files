# Folder sizes in the browser

Ctrl+Space calculates the focused directory; Ctrl+Shift+Space calculates the
currently filtered visible directories, or cancels that queue. The inspector,
Actions menu and background context menu expose the same controls. Selecting a
different row does not redirect or cancel a submitted calculation.

- `∑` marks a completed observation, including a valid zero for an empty folder.
- `≥` with a byte unit marks visited bytes, never a completed total or percentage.
  `≥1024 directas` instead refers to a bounded direct-entry count, not bytes.
- Queued, cancelled and failed calculations remain distinct. An error is not zero.
- Inspector details include apparent bytes, visited entries and measurement time.
- List, Grid, Groups and the matching root in Columns/dual panel use those facts.
  Size sorting uses measured folder bytes; unknown/error folders remain last within
  their folder group in either direction. Progress only repaints; it does not sort every update.

## Visible manager

Acciones → Gestor de tamaños or Ctrl+Alt+Space shows current-root queue rows,
per-attempt cancel/retry and wrapped error details. It borrows this same state and
uses the same helper/admission, including pause/ACK. See SIZE-MANAGER.md.

## Pause and retry controls

**Acciones → Pausar siguientes tamaños** stops admission of the next directory.
The currently running helper continues and may publish its measured result only
through the normal reap/ACK contract. Queued rows show **En pausa** and the status
bar reports pending/active/error counts. While work/errors are present the status
displaces rotating help and secondary free-space text, retaining a separate
self-PSS field. Below640px a shorter queue/error label fits the bar.
**Reanudar tamaños** admits the next
queued directory when the shared engine is available. Closing or cancelling
still uses the normal drain path; pause is not SIGSTOP or cancellation.

**Reintentar tamaños fallidos** queues only `Failed` rows, retaining the existing
root and original names, including opaque Unix names. Good totals and manually
cancelled rows are untouched; no repeated successful traversal. Retry preserves
pause, so a paused batch needs an explicit Resume. New requests in that same root
also retain pause. Refresh/navigation invalidation and Cancel All clear pause.
Preflight failure leaves the previous queue and pause untouched. Pause while idle
is a no-op. These controls do not add a separate queue, thread or filesystem probe.

The same three explicit actions are searchable in **Ctrl+Shift+P**, including
English `pause`, `resume` and `retry` aliases. Unavailable catalog commands stay
visible with a reason and have no Execute hit. Escape closes the catalog without
cancelling the batch. The existing Ctrl+Shift+Space still means calculate/cancel
the listed batch, including a paused batch; it does not mean resume.

The queue admits at most 128 folder names and 64 KiB of name storage. It captures
the current root and preflights the whole request before cloning names or changing
the previous queue. One helper runs at a time, sharing admission with previews,
search and Espacio. Cancellation retains the active slot until kill/reap ACK;
the next helper is not started on the ACK pump. Closing drains admitted helpers.
A stale Stop action cannot start a new calculation or erase a completed result.

`measure_directory` streams bounded JSON lines through the existing supervisor.
Progress uses one replaceable slot, not an unbounded queue. The receiver rejects
wrong IDs/types, regressing progress, duplicate finals, oversized lines and a
missing final response. A total is published only after the process is reaped.
Legacy DU/PreviewDirectory keep their single-response protocol.

The helper opens a directory without following its final symlink and retains its
root FD; replacing the named root before the final check rejects the result.
Symlinks contribute zero and are not traversed. Counts include visited directory
entries. Bytes are logical/apparent file sizes, with each hardlinked name counted;
this is not allocated disk space or a filesystem snapshot. Existing traversal
limits and error semantics remain; errors do not turn a partial sum into a total.
Progress is rate-limited to approximately 100 ms plus initial/final samples.
A blocking filesystem syscall cannot be interrupted by that cooperative limit.

Measurements are local to the current browser root. Navigation, pane/tab/root
changes, refresh, hidden-policy changes and owned mutation ACKs invalidate them.
Opening search, previews or Espacio cancels pending calculations rather than
silently resuming later. External deep changes require refresh/recalculation;
there is no recursive watcher or persistent size cache in this slice. Mount
boundaries, offline I/O and a live filesystem snapshot remain separate work.

Resource and startup gates remain NOT_MEASURED under the existing freeze. Local
caps and fewer UI allocations do not establish a process-tree memory saving.

## Validation

697 Rust tests passed (230 core / 72 helper / 395 UI; three existing core tests
ignored), workspace/all-targets Clippy with warnings denied, release and fmt pass.
The final compact-inspector change is covered by Clippy/release and the final
functional scene, after that suite.

`evidence/folder-sizes/folder-sizes-ui-1793395687494116398.json` is the final scoped
PASS: six framebuffers at 1000×650 and 800×440, real MeasureDirectory IPC, 8 B/5
entries, 40,000 one-byte files, empty zero, raw directory, directory replaced by a
regular file rejected, selection-independent batch, late Stop preserves results,
known sizes sorted before errors, Grid, compact inspector, refresh→13 B/6 entries,
navigation invalidation, callback/ACK/exit0 and logged helper identities cleaned
(absent/reused; zombie not accepted). This is not helper lifetime coverage or
proof of an OS helper alive at close. No native input or filesystem snapshot.

The earlier scene `folder-sizes-ui-1793395587181766247.json` passed before the
compact-inspector layout change and observed positive partial progress. The final
scene did not catch a positive partial sample; it is not rerun to manufacture
one. Both are retained with their actual binary hashes. Initial controls Clippy
failure (tuple mismatch), prior logs and screenshots remain.

Same final binary regressions passed:
`evidence/usability/usability-ui-1793395692067448178.json` (six captures) and
`evidence/search/recursive-search-ui-1793395693275772935.json` (eight captures).
Resource/startup NOT_MEASURED, historical FAILs and full ROADMAP acceptance remain
open. No global PASS or 0.0.4 tag.

Source checkpoint `34c2013`, installed with five independently rehashed artifacts
and backups in `evidence/0.0.4/installed-checkpoint-1793395732017451371.json`.
App `b229969d45f5ca9c…`, helper `9df14eb603083eba…`, unchanged HTML `85ea1b8009ef…`.

## 2026-10-30 — Batch pause and failed-only retry

748 Rust tests PASS (235 core / 74 helper / 439 UI; three pre-existing core ignored),
workspace/all-targets Clippy with warnings denied, release and fmt PASS. Logs:
`target/size-controls-footer-{tests,clippy,release}.log`. Tests include finishing
an active measurement while paused, old cancelled ACK admission barrier, retry
only errors preserving measured/cancelled rows, invalid preflight preserving pause,
navigation/Stop reset, normal Model dispatch and catalog availability, plus status
geometry at 440/800/1000. No new dependency, engine, queue or worker.

Final `evidence/folder-sizes/folder-sizes-ui-1793408838174586064.json` scoped PASS:
ten real framebuffers at 1000×650/560,800×440,440². The real batch is paused before
admission, resumes via catalog painted Execute; directory→file replacement remains
Error, restoring that directory then Retry starts exactly one new MeasureDirectory
helper. Other measured labels/selection remain; retry disables after success.
Raw filename, filtered hidden scope, 40k directory/positive partial sample, zero,
sort, Grid, refresh/recalculate, navigation and root0/drain ACK are checked. Nine
logged PID/start helper identities are absent/reused; zombie is not accepted.
This is not helper lifetime coverage or helper-alive-at-pause/close proof.

Initial `folder-sizes-ui-1793408604914973870.json` scoped functional PASS remains,
with visual FAIL because rotating help clipped paused status; original nine PNGs
and `size-controls-validation-notes.json` retained. Final suppresses help/secondary
free space while size status owns the bar, using a compact label below640px.
Earlier command regression `evidence/commands/command-palette-ui-1793408614103585218.json`
PASS11FB verifies 49 commands/disabled and modal context/raw dual copy/440 layout
on the binary before the footer-only correction; not claimed as final-binary run.
Final size scene rechecks catalog resume/retry plus their disabled states.

Handlers/painted targets and owned filesystem fixtures, not OS input/clipboard,
filesystem snapshot, kernel-D/offline, process-tree resources or startup proof.
Freeze/preview pause, historical FAILs and broader ROADMAP remain open.

Source checkpoint `61ef838`, installed per-user with five artifacts and five
backups independently rehashed in
`evidence/0.0.4/installed-checkpoint-1793408895110742258.json`.
App `aab1d53b41e3…`, helper `1f33f89dad08…`, HTML `85ea1b8009ef…`.


## 2026-10-31 — Direct observations are not recursive sizes

Groups previously displayed direct-child regular/symlink metadata bytes beside
an entry count, with no scope qualifier. It skipped nested folders and treated
failed stats/listing as zero. The ZIP fixture's32B card vs63B payload was this
first-level sum, not a failed recursive measurement. The old automatic path also
called backend.stat on the directory and each non-directory child.

Automatic cards now use typed folder_summary::Summary: direct entries only,
Complete/Limited/Partial/Unavailable. No byte field exists in that observation.
The local worker enumerates ReadDir without asking for child names, types or
metadata. At most1024 iterator results are consumed, with no extra EOF peek;
exactly1024 is still a valid lower bound. Entries include immediate hidden names
and links, but no link target or subtree is traversed. No root stat is added for
the count: modification time comes from the ordinary visible-entry Stats request.
One reusable worker/generation/4-FS-permit path remains, no new timer/thread/helper.
This is an iterator/API-work cap, not a guarantee about kernel read-ahead, I/O
latency, network/offline behavior or process memory.

Cards say `N directas · sin calcular`, `≥1024 directas · límite`, partial or
unavailable. A read failure is never empty. Explicit Ctrl+Space/Calculate still
uses the SAME measure_directory helper and full recursive contract above. It
replaces the card with `Total:` only on a measured result (including valid0B).
The inspector puts the no-link-following policy on its own line and the shortcut
on Calculate/Recalculate; it no longer clips the combined hint.

Core listing, active Model and inactive pane share compare_sizes: known values
before unknown in both directions within each group, zero is known. Normal
folders-first grouping remains. Plain directory stat length is
never a recursive size. The inactive pane borrows measured values for the same
root, and preserves late direct-summary replies so returning to it does not leave
`contando…` forever. Size orders are rebuilt when pane switching invalidates
measurements; other sorts do not incur an extra full rebuild for this change.
Modified/Name policies and the existing per-root measurement invalidation remain.

1100Rust388core/91helper/621UI+3existingignored PASS. Added7regressions: bounded
entry iteration/cancel/read-failure semantics; backend child-listing with no stat
or child descent; core/inactive zero-vs-unknown/metadata-vs-total sorting; late
reply across pane activation. Clippyworkspacealltargets-Dwarnings/release/fmt and
final real-window evidence are recorded in evidence/folder-sizes/summary-validation.
No process/resource/startup measurements or native input acceptance. Initial suite
ETXTBSY helper fixture failure + poisoned test mutex remain in original logs,
without changing that test or serializing the suite. The first GUI harness
incorrectly assumed ascending Size first; product starts descending. Original
FAIL+2PNGs/sidecar retained; final harness checks both directions. Full goal,
historical gates, freeze and new-preview pause remain open.


Installed source7983c0f; app43a09343ee995de0fdb8127a2a692e70970e88d1a59ebd9398d1c1af942805f3,
helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. Record1793494152752973446; independent
5installed+5backup hashes PASS in summary-validation/install-verification.json.
No running user windows controlled, no source push/tag/global acceptance.

### Dual-pane live size keys (2026-11-01)

Accepted DU results propagate to an inactive Size-sorted pane even with active
Name sorting. Browser and manager recalculation invalidate both orders before
the next measurement. This changes in-memory order only, without UI filesystem
queries or selection/focus changes. Resource/startup gates remain frozen.


## Marked visible folders

The command catalog offers **Calcular tamaños de carpetas marcadas visibles**;
the manager offers **Marcadas · M**. It queues only marked visible directories,
ignoring files/links and unmarked focus, with atomic whole-batch preflight and no
implicit cancel of an existing batch. This uses the same apparent-size queue,
not a selection-only physical-space total. See MARKED-FOLDER-SIZES.md.
