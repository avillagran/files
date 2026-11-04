# Raw-path marks in recursive search

Search has an independent focused result and explicit marks. Click a row to
choose it; click its Nerd checkbox or Ctrl+click to toggle. Shift+click or
Shift+arrows/Home/End/Page extends the displayed-order range from the anchor;
Ctrl+Shift adds that range. Arrows alone move focus while preserving marks, as in
MC. Space toggles the focused mark; Insert toggles and advances only if admitted.
Ctrl+A marks all currently published results, Ctrl+D empties marks. The footer
Todo/Ninguno button provides mouse-only all/clear. Ctrl+A in a query/filter editor
still selects text. F1 and tooltips describe the actual controls.

Marks are raw relative paths, never mutable result indices or display labels.
The focused row has a visible accent edge; checkboxes show marks independently.
Changing sort keeps the chosen paths. Provisional→new batch→final intersection
retains only paths still published and reports a pruned count. Query/start/new
root/close clears old marks. A cancelled provisional result is unavailable to
actions. Row/checkbox hits carry a monotonic result epoch; a click captured before
a new batch/final result or cancellation cannot mark a different indexed row.

No explicit marks yet preserves the original focused-result behavior. Explicitly
empty marks—including marks pruned to empty—do **not** silently fall back to the
focused row for a batch action. Select a row or mark it again. Open/Enter and
Locate/Alt+Enter continue to act on the focused result; they do not bulk-launch
marked files. The footer count distinguishes marks and discarded paths from the
existing provisional/final/error status.

## Bounded metadata and batch action

At most128 distinct paths and64KiB conservative retained charge (paths,128B/node,
4096B anchor reserve) are admitted. Borrowed preflight rejects an entire range or
Select All without replacing previous marks/focus. No cloned hit list, filesystem
probe or decoder is added. Reconciliation sorts borrowed path references rather
than repeatedly comparing every selected path to every published hit. Updating
marks can retain old and replacement sets transiently; local caps are not an
app/PSS/performance acceptance result.

The Pestaña / Ctrl+Enter action now uses marks in current **visual order**.
One result creates one background tab: directories target themselves, files or
symlinks their parent plus raw leaf selection. It keeps search/active context.
It does not collapse same-parent results or silently omit overflow. The existing
8-tab/aggregate stored+closed+other-pane budget is preflighted for the whole batch;
all Saved snapshots are constructed before any tab/ID is published. A last invalid
path, limit or explicit empty selection leaves every tab unchanged. No listing,
helper, prefetch or new session wire format is introduced by admission.

These are observations, not inode/content snapshots or authority to mutate.
Copy/Move/Trash of multi-origin search marks still require separate worker-side
Source/target proof and queue admission; marks alone never supply that authority.
They are not OS clipboard MIME and do not replace the private files clipboard.
New previews remain paused; PSS/Swap/CPU/startup experiments remain frozen.

## Validation 2026-10-31

951Rust (324core/85helper/542UI;3 existing core ignored), workspace Clippy
all-targets-Dwarnings/release/fmt PASS. The final full suite precedes only compact
acknowledgment/tooltip/help wording; final Clippy/release and scene cover it.
Pure tests cover visual ranges/toggle/reorder, raw paths, disappeared paths with
explicit empty intent, count/byte limits and all-or-none selection updates.
Model tests cover old-epoch rejection, atomic three-tab admission, insufficient
remaining slots, invalid last target before any publication/ID allocation and
Name-sorted visual order rather than helper-index order. Existing other-pane/
closed-tab budget and raw activation tests remain green.

`evidence/search-order/search-marks-ui-1793464566350502738.json` scoped PASS:
15real FLTK framebuffer PNGs, actual search_batches helper, painted provisional
checkbox survives final epoch, old-epoch checkbox ignored, raw selection and
background activation regression, keyboard ShiftDown range3→atomic3tabs in visual
order, Insert preserves other marks, Ctrl+A42→whole tab-batch rejection,
painted Ninguno→explicit empty→no focused fallback, eight-tab cap and source hashes
unchanged. Three new PNGs inspected, including minimum440². Root0 after ACK;
3logged PID/start helpers absent/reused, Z rejected; not lifetime coverage.

Original E0502 private fixture closure and Clippy too_many_arguments logs are
preserved with explanation. No guard/lint/deadline was disabled. No native input,
OS shortcut/clipboard, Copy/Move Source capture, filesystem snapshot or resource
acceptance follows from this scene. PSS/Swap/CPU/startup NOT_MEASUREDfreeze, footer
incidental only. Full ROADMAP and new-preview pause remain active.

Copy/Move admission from marks is now implemented with captured FD proofs and
the shared review/queue. See [SEARCH-TRANSFERS.md](SEARCH-TRANSFERS.md); OS
clipboard, batch EXDEV and native-input/global/resource acceptance remain open.
