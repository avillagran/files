# Editing recursive search drafts

The query and four guided criteria reuse the bounded UTF-8 scalar editor used by
the path field. This section describes recursive search. The browser filter has since adopted
its own focus/cursor integration; see FILTER-EDITING.md.

## Keyboard and pointer

- Ctrl+F focuses/selects the query; Ctrl+A selects the current input. Tab cycles
  the query plus four criteria while F4 filters are open. With the drawer closed,
  Tab alternates query editing and result navigation.
- Left/Right move by scalar, Ctrl+Left/Right by whitespace/slash-delimited
  component; Shift extends selection. Home/End move within the focused input.
  Backspace/Delete remove selection or adjacent scalar; Ctrl removes a component.
- Ctrl+Z, Ctrl+Shift+Z/Ctrl+Y undo/redo input in that field. They never undo a file
  operation while search owns input. Ctrl+C/X use selected input text only while
  an input owns focus; Ctrl+V uses the existing asynchronous text paste gate.
- Click places the caret; drag/Shift+click selects; double-click selects a
  whitespace/slash component. Horizontal scroll keeps the caret visible.
- Up/Down, PageUp/PageDown and wheel switch to results. Home/End then choose the
  first/last result. A result click also switches to results. Ctrl+F returns to
  the query. Left/Right can return to editing at the existing caret.
- Enter with a selected result and result focus opens/locates as before; Enter
  in input submits a finished draft. Enter while the current job runs still
  waits; the explicit Restart button remains available. An open filters drawer
  submits the form. Escape closes the drawer first, then search.

Moving/selecting text retains the running job or validated final report. Actual
input mutation cancels/invalidate results through the existing generation and ACK
rules. A no-op Delete retains results. Focus/cursor/result-navigation changes
advance the input epoch, so a late paste is rejected instead of inserted at a
new target. Rejected oversized input preserves text, selection, capacity, epoch
and history. Copy/Cut route to the existing FLTK publication call; this feature
is not evidence of native clipboard ownership or interoperability.

## Bounds and scope

Query4096 bytes, each criterion32 bytes, composed-query4096/32terms and all search
worker caps stay unchanged. Criteria reject control characters. Query strips
CR/LF/NUL as before and rejects other controls. Preflight precedes draft/history
changes. Cursor histories have the existing16-state/32KiB text-capacity bound per
field (the four small fields are also32B each). Closing releases history and
geometry; histories are not saved to sessions/disk.

Single-line geometry retains exact bounded draft bytes and at most4097 points for
the query,33 per criterion, with reserve_exact before growth. Fixed logical
Courier13 advances are rebuilt on exact text changes or resize, reused for
cursor/progress-only repaint. Pointer lookup is binary; no hash proof or unbounded
per-result text map. This adds bounded text/geometry storage, not a new engine,
thread, timer, dependency or persistent cache. Unit call counts establish reuse,
not measured CPU/PSS benefit. Native HiDPI/font transition/shaping remains pending.

Editing is scalar UTF-8, not extended grapheme clusters/bidi/IME preedit. Unicode
file-path transport and existing result-source validations are unchanged. New
previews remain paused; resource/startup gates remain NOT_MEASURED under freeze.

## 2026-10-30 — Search input cursor, selection and bounded geometry

754Rust PASS (235core/74helper/445UI;3pre-existing core ignored), Clippy workspace
all-targets-Dwarnings, release and fmt PASS. Final logs target/query-editor-cached-
{tests,clippy}.log and query-editor-release.log. Tests cover Unicode middle edits,
selection/word deletion, per-field undo/redo, rejected1MiB preserving text/capacity/
selection/epoch/history, no-op deletion preserving results, live Fake worker not
cancelled by caret motion, delayed paste rejected on caret/result-focus change,
geometry pointer/scroll/stale stamp/cap/release and width-call reuse.

`evidence/search/search-editing-ui-1793409739193969692.json` scoped PASS24FB at
1000×650/560,800×440,440²: Unicode logical selection/Delete/undo/redo, query drag
from actual painted byte positions, private stale paste rejected, execute ext:rs
against actual helper, retained report while moving caret, Tab and Home/End input
vs result focus, criteria middle editing,4096B end/start viewport440, existing
filters/size/date/raw/hidden/500partial/locate/directory/cancel/reopen regressions.
Original fixture hashes unchanged, root0/drainACK;13logged helper PID/start absent
or reused, zombie not accepted. Not all-lifetimes/helper-alive-at-close proof.
No40k progress sampling this scene; live worker retention is a deterministic state
test, not a claim of observed native input during a real scan.

Exact draft cache and point table are bounded and release on close; fixed logical
font13 metrics reuse on unchanged draft/area and binary pointer lookup are unit
call-count/structural checks, not CPU/PSS savings. No engine/dependency/thread or
persistent text cache added. Search-only; browser filter still pending.

CJK sample has missing glyph coverage; UTF8 selected bytes are proven, native CJK
rendering is not. Auxiliary bottom shortcut text still clips in minimum window;
main fields/controls bounded. Notes in query-editing-validation-notes.json retain
these limits. No native input/clipboard/IME/grapheme/bidi/HiDPI/globalPASS/tag0.0.4.
Resource/startupNOT_MEASURED; freeze, preview pause and historical FAILs unchanged.

Source `aa4af0f` installed with five independently rehashed artifacts/backups in
`evidence/0.0.4/installed-checkpoint-1793409847987478976.json`. App `aac5745b950d…`,
helper `1f33f89dad08…`, HTML `85ea1b8009ef…`.

## Content primary field

Texto/F6 alternates an independent <=1024B literal draft with the existing name
query. It adds one bounded Cursor/Geometry (six total); CtrlF returns to the current
primary field, metadata Tab cycling uses that primary plus four existing fields.
Quotes/backslashes are composed/escaped after combined-query preflight. Pointer
lookup visits only the currently painted primary geometry, so an old name-field
rectangle cannot steal content input. Mode changes advance the paste epoch without
restarting a scan; data edits retain latest-intent/cancel/ACK rules.
