# Browser filter editor

Typing still filters the current browser immediately. Ctrl+F or the magnifier now
**focuses and selects the existing query without clearing it**. Click within a
shown query places the caret, Shift+click/drag selects, and double click selects a
whitespace/slash component. Empty focused input shows a filter placeholder.
The pencil remains a separate mouse route to Ctrl+L/path editing even with a
nonempty query. Focusing/choosing text alone does not rebuild or alter file marks.

## Focus and keyboard

- Left/Right, Home/End and Shift select/move within text. Ctrl+Left/Right move
  components. Backspace/Delete and Ctrl delete text, never files while focused.
- Ctrl+A selects query text. Ctrl+C/X uses selected query text through the existing
  FLTK publication call, leaving the private file clipboard unchanged. Ctrl+V
  uses the asynchronous text paste gate. No selection means no copy/cut.
- Ctrl+Z, Ctrl+Shift+Z/Ctrl+Y undo/redo the query; successful changes rebuild the
  live visible set. File rename/copy/move/create/trash hotkeys cannot fall through
  while the field owns input. F2/F5/F6 are consumed; they do not start a file tool.
- Enter or Tab ends editing, keeping the filter and returning focus to files;
  a second Enter then opens the focused file. Backspace in the list navigates
  to the parent, including with a retained nonempty filter; it no longer trims
  query text outside the editor. Up/Down/PageUp/PageDown end editing
  and perform normal file navigation. Escape ends editing and clears the filter.
- Ctrl+Shift+A remains the explicit MC conversion: mark matches and show all.
  Ctrl+Shift+D still clears file marks/focus. These explicit selection commands
  leave the input first; ordinary Ctrl+A remains text selection.
- Ctrl+L switches to path editing; Ctrl+Shift+F carries the query to recursive
  search. Existing global commands/help/tab navigation may leave this field.
  Explicit mouse actions outside the field also leave it before file dispatch.
- Plain Tab while editing does not switch the dual pane; after returning to the
  list, Tab switches panels as before. F9/Ctrl+T/tab commands are navigation, not
  text edits. A focused filter shows its short Enter/Escape guide in the footer
  instead of rotating general tips; errors/size-status keep precedence.

Ctrl+F or typing from the Espacio treemap first returns to List, using the existing
cancellation/ACK contract. The filter always applies to a listing, never silently
to the treemap layout. Ordinary list syntax/raw path semantics are unchanged.

## Ownership, bounds and lifecycle

The focused input captures pane/tab owner and the exact raw root (<=4096B, borrowed
validation before cloning). It is transient app state, not an extra browser or
worker. Switching pane/tab/path disposes cursor history/geometry and leaves the
existing per-pane/tab query text persistence in charge; it does not restore an
old text selection/history in another tab. Path/filter editors are exclusive.
Same-root listing refresh may retain text focus; the paste key also contains the
normal listing generation, rejecting old callbacks across refresh.

A monotonic nonrepeating filter epoch plus normal owner/generation distinguishes
text from file clipboard. Caret/selection movement, blur, owner changes and close
invalidate pending paste before accepting anything at a successor position.
Oversized active-field paste preserves draft/capacity/cursor/epoch/history. Native
clipboard bytes/publication/interoperability remain a separate unvalidated gate;
private fixture Copy reports the selected literal without calling OS publication.
Native DnD admission and direct file-buffer capture are blocked during
text focus. Background file jobs may keep running with their existing snapshots.

Query4096B, existing scalar Cursor16states/32KiB text-capacity history, bounded raw
root, and shared line_field exact-draft cache/<=4097 point table remain structural
bounds. No new engine, dependency, thread, timer, persistent cache or session wire
schema. Geometry reuses fixed logical font13 widths on unchanged draft/area and
binary pointer lookup. Compose accepts an explicit preedit deletion byte count,
aligned to a UTF8 scalar boundary; this is not native IME/preedit acceptance,
grapheme/bidi handling or measured CPU/PSS benefit. Closing releases focused
editor state before the existing helper drain; new previews stay paused and
resource/startup measurement freeze stays active.

## 2026-10-30 — Browser filter focus and editing

758Rust PASS (235core/74helper/449UI;3pre-existing core ignored), workspace
all-targets Clippy-Dwarnings/release/fmt PASS. Final logs:
`target/filter-editor-focus-{tests,clippy,release}.log`. No serial test-thread flag.
New/updated cases: CtrlF retains query and selected text, path/treemap exit plus
type-to-filter→List, middle edit drives visible results, per-input undo/redo,
text-copy result not private-file copy, F5 suppression, huge paste preserves
capacity/epoch/selection, delayed cursor/blur paste reject, pane/tab/path focus
release, MC CtrlShiftA kept, Backspace text vs parent after Enter.

Final `evidence/usability/filter-editing-ui-1793411701021271474.json` scoped PASS:
12 real FLTK framebuffers1000×650/560,800×440,440². Owned fixture, painted byte-point
click/drag, Unicode ñ range/edit/undo/redo, private selected-text copy (no OS
publication), F2/F5/F6 no dialogs,1MiB rejection, stale caret/pane/tab paste rejected,
queries retained per pane/tab and by CtrlF, path pencil reachable with query,
4096B viewport start/end440, listing Backspace→owned parent then root return.
Existing name/raw/filter/view/MC selection and real helper sizes8B/5entries→12B/6
also pass. Original file hashes intact, root0 after drainACK;2logged helper
PID/start identities absent/reused, zombie not accepted. No helper lifetime or
helper-alive-at-close proof.

Same final binary recursive search regression
`evidence/search/search-editing-ui-1793411753809197070.json` PASS24FB,13logged
helper identities cleaned (same strict rule), actual name/size/date/raw/hidden/
500partial/locate/directory, input/criteria editing and delayed paste regressions.
No40k progress sampling or performance comparison.

Originals preserved with `filter-editing-validation-notes.json`:
-1793411028309635495 FAIL fixture expected oversize error cleared by SelectAll;
 explicit known-error state now checked and next valid edit must clear it.
-1793411079837629215 FAIL fixture omitted existing FILES_TEST_TABS opt-in;
 no unknown command accepted; flag enabled only for explicit editing fixture.
-1793411105881791432 FAIL old1000×650 dimension after explicit1000×560 resize;
 actual dimensions now asserted per invocation, no product resize changes.
-1793411146396526185 functionalPASS12FB with visual guide clipped at440; final
 short guide and contextual inspector hint inspected. Existing informational
 Panel cerrado toast still clips at minimum width; remains pending.

Initial workspace failure retained: CtrlF-erases-query assertion intentionally
replaced with preserve/select plus path/treemap assertions. Initial check warnings
(fetch_update deprecation, unused append) removed with existing CAS convention and
Cursor-backed retained preflight tests. No new dependency/engine/thread/timer or
session wire. Active editor bounds/history/owner/epochs in FILTER-EDITING.md.

Foreign Cargo observed during final unit run; own Clippy mistakenly launched after
printing a new competitor, finished clean. Mistake recorded, no foreign signal.
Final release uses an idle-snapshot launch guard. No resource/startup gates or
comparisons; timings incidental, freeze/new-preview pause and historical FAILs
remain. No native input/OS clipboard/IME/grapheme/bidi/HiDPI/kernelD/globalPASS/tag.

Source `cfd78d5` installed per-user, five artifacts and five backups independently
rehashed in `evidence/0.0.4/installed-checkpoint-1793411936765024376.json`. App
`5c57898aaabf…`, helper `1f33f89dad08…`, HTML `85ea1b8009ef…`.

## Per-pane controls (2026-11-01)

Dual headers have a Nerd filter/edit button, plus a clear button only while
filtered. Edit explicitly focuses that pane and selects the current query using
the existing editor. Clear acts on the exact pane ID+listing revision without
switching focus, resetting the other query, or invalidating DU observations.
Stale/refreshing targets reject. Explicit empty marks remain empty after clear.

Pane counters lead with visible/loaded entries and marks, followed by the query
on wide panes. Loaded counts include hidden loaded entries; these are current
listing counts, never recursive totals or a new filesystem scan. Narrow panes
use a filter icon and compact counters. No new watcher/thread/cache/session wire.

Compact error/success notices now reserve48px above the optional MC button strip
instead of painting over per-pane counters. Normal body/reveal calculations share
that reservation; full notices and closing do not reserve compact-notice space.
