# Selection tools for keyboard and mouse

Local filters double as selection patterns. Write `*.rs`, `ext:pdf,md`, or a
combination such as `2026 !draft type:file`, then use **Ctrl+Shift+A** or Actions
→ **Marcar filtrados y mostrar todo**. The current visible matching set becomes
the marked set, the query clears, and the browser shows the full visible folder.
This replaces marks; it does not accumulate hidden selections across queries.
The existing global hidden preference still applies. Names are matched for
presentation, while operations retain original `OsString` names, including raw
Unix bytes. No filesystem probing, regex engine or path redecoding is added.

- **Insert:** toggle current mark and advance independently of existing marks.
- **Ctrl+A:** mark all visible rows without moving a valid focus.
- **Ctrl+I:** invert visible marks. It updates the existing set rather than building
  a second full selection set.
- **Ctrl+Shift+D:** clear both marks and operative focus. Rebuild/metadata refresh
  must not silently repopulate it; F5/F6 cannot capture a fallback focused file.
- The native Actions popup exposes these controls directly; the painted fallback
  uses a compact `Selección…` submenu with the same four commands. Entry and
  background right-click menus expose selection commands too. The
  status bar shows marked count first, or `Sin selección`.

Pattern selection rejects empty/invalid queries before changing marks or query.
Zero matches remain a true empty selection after clearing the query. Marked focus
is revealed when returning to the full listing. All commands target the current
loaded browser, including its active Columns root or dual pane; they do not affect
an inactive pane/tab. A partial refresh, modal, preview, editing path, closing or
Espacio view does not accept these browser commands. Existing operation admission
and selection budgets are unchanged.

Refreshing a selected batch now recovers the first surviving marked row in visible
order if the former focus was filtered out. The anchor follows that valid focus
when its old name no longer survives. This preserves both an operable focus and
an exact marked operation set; an entirely empty snapshot remains empty.

## Validation scope

701 Rust tests pass (230 core / 72 helper / 399 UI; three existing core tests
ignored), plus workspace/all-targets Clippy -Dwarnings. New tests cover raw pattern
matches and exact copy-form names, stable focus under All, Invert→empty and Clear
with no fallback after rebuild, no-copy with zero marks, invalid patterns and
partial-load blocking. Existing tab/filter/hidden refresh regression passes with
the surviving-focus correction. Initial regression and missing test import logs
remain in `target/selection-tools-*`.

No resource/startup gate is measured under the existing freeze. Avoiding a second
selection set is a source-level optimization, not a measured process-tree saving.
Native OS keyboard/mouse/clipboard and broader ROADMAP acceptance remain open.

The background menu keeps its existing deselect-on-open policy; its selection
commands produce a new marked set that must survive menu activation. The dedicated
selection submenu preserves the current marks while opening, so Invert acts on
that set. A regression exercises the real background activation path, not only a
direct selection command.

Initial GUI scenes tested direct actions and copy successfully, but their Actions
screenshots still showed the old painted fallback. Visual inspection identified
that gap; those original PASS reports remain scoped to what they tested. The
fallback submenu and background post-action clearing were then corrected. A first
background unit fixture pointed into the inspector, corrected to the actual list
background; both logs remain. GUI fixture initially failed to dismiss the expected
empty-copy diagnostic before typing; that timeout report remains too.

Final release/fmt and painted-target scene
`evidence/selection-tools/selection-tools-ui-1793396623817639503.json` PASS:
ten framebuffers1000×650/800×440, selection submenu→actual painted context target,
background Select All retains its result, invert/clear/no-copy-empty, raw hidden
scope, Grid/Columns, exact copy review and completed bytes to the other pane,
owner isolation, callback/drain/exit0. No helper lifetime/OS-input acceptance.

Final same-binary size scene1793396638238738889 and recursive search
scene1793396640181618221 PASS too. Earlier direct-action scenes and pre-menu
regressions remain under their own binary hashes; no report is upgraded to native
popup/input proof. Full resource/startup freeze and historical FAILs remain.

Source `09ca26c` installed with five independently rehashed artifacts and five
backups: `evidence/0.0.4/installed-checkpoint-1793396665910433933.json`.
App686ddaf2db84324a…, unchanged helper9df14eb603083eba…/HTML85ea1b8009ef….

## Zero-selection intent when clearing filters (2026-11-01)

After explicit Clear/empty inversion, removing the local filter must not select
a default file again. ClearFilter now preserves that intent; Escape from the
filter editor uses the same action. The default initial focus still returns when
a filter had no matches and selection was not explicitly cleared. No copy source
is captured after an explicit zero selection.

The preserved RED model test fails on the old ClearFilter resetting
`preserve_empty_selection`; the fixed tests also check no copy form and Escape
in List/Grid/Groups/Columns. This is selection state proof, not OS input proof.
