# Review while the queue is active

Copy, move, create ZIP and extract can open their review while another copy
is active or paused. MC, palette, menu, inspector and Action use `copy_review`
to tell that review apart from admitting another job. F5 keeps its
contextual behavior: copy in listings, recalculation in Space.

| Intent | Effect |
|---|---|
| Open another destination | Captures the selection and opens the form; it does not change active or pending jobs. |
| Review a move | Uses the existing FD preparation; a pending token or its cancelled ACK prevents opening another review. |
| Cancel the form | Releases that review; it does not cancel the active job or remove admitted pending ones. |
| Confirm with a full queue | The controller rejects admission and keeps the form, names, options and reason. |
| Clear pending | Removes only the pending jobs; the paused active job keeps its identity. |
| Resume | Continues the active job and then the jobs that are still admitted. |

Eight pending jobs, entry/FD charges, increasing IDs, the
result/ACK barrier and each engine's validations are kept. No jobs are reserved and the
FS is not opened while checking availability for painting. Closing, a pending load/refresh,
an applied mutation and an earlier undrained review still block.
A read-only rename preparation is not confused with an
already applied mutation nor with an independent queued copy.

The selection is validated on execution before sanitizing. Invalid, hidden or
filtered-out marks are not turned into another capture by falling back to the focus. Space
keeps its policy of ignoring the retained filter and using the marks of the root
listing; the size report is not authority to operate. The palette's exact
selection proof, owners, generations and FD tokens do not change.

Progress sits above MC, respecting the inspector that is actually visible. In a
narrow body it lays the controls out in two rows and separates status, counters
and path; it does not force a width that overflows 440 px with the sidebar expanded.
The last-result button also respects MC and the compact notice. The fixture
uses the real canvas dimensions for navigation/menu, and tells apart entries
of the active pane or active column to avoid acting on another owner.

Source 9ddd253: 1306 normal tests (447 core/105 helper/754 UI), three historical ones
ignored; Clippy all targets with warnings as errors, format and release PASS.
Wayland queue 1793560078115479975, Xvfb X11 1793560081082575235 and installed
Wayland 1793560381173496871: each 35 captures/5 normal closes/drain, five
profiles, exact content and retained sources. A full queue and a retained form
are tested in the controller/model; no UI capacity experiment with 8 jobs was done.
Space regression Wayland 1793560075965694320 and isolated Xvfb X11
1793560198200409125: each 29 captures/4 normal closes/15 known helpers absent
or reused. The first Space Xvfb wrapper exited 1 due to the cleanup error
“kill: No such process” even though its functional receipt was PASS; it was kept,
and the same binary/fixture alone exited 0. The exact cause was not shown. Avoiding two
Xvfb wrappers with automatic display assignment at the same time does not serialize the Rust tests.

Installation 1793560331250393795, five installed hashes and five backups
verified; app a715c17069d6aa85475d39016a0a725b09847a53befc95eda1b52dc47a41c6e6.
Receipts in `docs/evidence/queue-review/assessment.json`.

The inspection revealed a separate pending item: at 440 with the sidebar expanded the
dual-pane name has no width (`dual_view.rs`: max(r.w-116,0)); the control at 1000
does show it. The compact checks in this checkpoint prove queue
controls, not the readability of pane names. It is not causal evidence for the historical
wide X11 failure. Next: prioritize the name over optional metadata.
Private handlers and the framebuffer do not prove OS input/clipboard,
AT-SPI, 2x, other OSes, syscall deadlines or the global lifetime of helpers. The complete
goal, ROADMAP, the X11 anomaly and the PSS/Swap/CPU/startup freeze remain active;
no preview formats are added and resource experiments are not resumed.

Reconciliation 2026-11-01: the geometric name reservation from the previous pending item
was fixed in NARROW-NAMES.md, source f6a735f; the earlier receipts remain as
evidence of the previous state. This does not causally resolve the wide X11 anomaly.
