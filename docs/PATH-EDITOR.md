# Single-line path editor

Ctrl+L or the pencil button selects the current path. While editing, the field takes header space and
Nerd-font confirm/cancel buttons are available. Enter confirms a directory;
Escape or cancel abandons the text edit. Clicking another browser control ends
editing. No filesystem probing is performed by cursor/selection edits.

- Left/Right and Home/End move the cursor; Shift extends a UTF-8 scalar selection.
- Ctrl+Left/Right moves by slash/whitespace components; Ctrl+Backspace/Delete removes
  the corresponding span. Backspace/Delete work at the cursor, not the string end.
- Click/drag selects text; double-click selects a component. Selection, caret and
  horizontal scrolling use the painted monospace advance map. Border padding is
  part of the edit hit area, so clicking it does not reset user text.
- Ctrl+A selects all, Ctrl+C/X returns selected text to the existing clipboard
  adapter, Ctrl+V uses the existing asynchronous paste gate. Undo/redo inside the
  field is text undo/redo (Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y), never file-operation undo.
  File-operation function keys do not mutate the browser while editing.
- FLTK composition replacement is a byte count before the cursor, applied in the
  same preflighted edit as the new text. Rejected edits retain text, cursor,
  selection, history and capacity. Native IME/preedit delivery remains unvalidated.

The field cap is4096 UTF-8 bytes, preserving the existing newline/CR/NUL filtering
contract on input. Before allocation, edits account for the selected/deleted span
and filtered inserted bytes. Undo+redo retain at most16 states and32KiB of text
capacities; the painter retains at most one boundary per scalar plus the end
boundary (max4097 nominal points). These are local bounds, not process-tree PSS.
History/geometry are dropped on cancel, committed navigation or edit replacement;
no caret timer or worker is added. Short replacement text resets horizontal scroll.

Every cursor/selection/text revision changes the paste target, and input/mouse
intent invalidates pending callbacks. A late paste cannot insert into a different
selection after cursor movement. Repainting a selected field does not itself
change the paste revision. The existing slot remains held until callback drain.

Ordinary UTF-8 paths are literal: `%20` in an ordinary name remains `%20`, and a
scheme-looking substring inside an absolute path remains literal. Existing `~/`
and relative-directory behavior remain. For non-UTF8 or control-containing paths,
Ctrl+L presents a bounded explicit local `file://` URI using the existing raw URI
codec, decoded exactly once when the user confirms. Other IPC paths are unchanged
and never URI-decoded. Oversized encoded display rejects without substituting a
lossy path. Only one local URI is accepted; remote schemes remain pending.

## Scope and validation

Scalar-safe editing is not full grapheme segmentation, bidi caret shaping or a
complete native text widget. Native mouse/keyboard/clipboard ownership, IME
preedit, text context menus, completion and editing a filter with the same caret
model remain work. Directory navigation is asynchronous through the existing
browser; this slice does not add file launching from a typed file path.

Pure tests cover Unicode midline replacement/delete, byte-based composition and
undo, bounded history, rejection atomicity, clipboard revision and no file undo,
repaint revision stability, horizontal-scroll reset, hit padding, raw URI exact
roundtrip, literal percent/scheme components and bounded codec expansion. All
resource/startup gates remain NOT_MEASURED under the freeze; preview expansion is
still paused. Existing clipboard/baseline/startup failures are not upgraded.

Below800px the browse header uses a compact six-item view selector, keeping a
usable path field plus Preferences/Actions. While editing, confirm/cancel replace
the view toolbar. At440px the footer omits the secondary free-space label to keep
status and self-PSS separate; that displayed self-PSS is not a resource-tree gate.
Initial screenshots and check logs remain in evidence/path-edit with scope notes.

## Final evidence

719 Rust tests pass (231 core / 72 helper / 416 UI; three existing core ignored),
workspace/all-targets Clippy -Dwarnings, release and fmt pass. The final lifecycle
regression verifies that switching from path editing to search releases geometry
and rejects its old pending text paste.

path-edit-ui1793400883075229579 PASS11 framebuffer captures at1000×650,800×440 and
440×440: middle UTF8 edits, delete/undo, oversized paste with identical selection,
mouse drag/component through painted geometry, confirm/cancel/pencil controls,
Unicode-directory navigation and exact raw URI navigation/unchanged confirmation,
stale/fresh paste callback, horizontal scroll, compact header/views selector and
callback/drain/exit0. Files in the owned fixtures retain their bytes. No actual
OS keyboard/mouse/clipboard publication or native IME delivery is claimed.

Same-binary regressions PASS: history1793400883822026213,
keyboard1793400885767519762, sizes1793400887737740092 and search1793400889575792607.
Earlier screenshots/reports retain their own scopes/hashes and visual review notes.
No global PASS or version tag; full ROADMAP, baseline/startup/clipboard failures
and resource/startup freeze remain.

Source `ab62544` installed with five independently rehashed artifacts and five
backups in evidence/0.0.4/installed-checkpoint-1793400934116802110.json.
App3bb614ac2875484e…; helper9df14eb603083eba…/HTML85ea1b8009ef… unchanged.
