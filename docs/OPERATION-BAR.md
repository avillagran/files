# MC-style operation bar

**Preferences → Barra de operaciones MC**, **Ctrl+Alt+B**, or the command catalog
can show a persistent compact operation strip. It defaults off for the compact
browser. It works in one or two panes and follows the active pane/tab. Turning it
on reserves36 logical pixels above the status bar; it never overlays listing
rows, native drop targets or inspector controls. List/Grid/Groups/Columns/dual
reveal/page calculations use the reduced viewport. The preference uses the
existing atomic config snapshot/ACK path and preserves unrelated fields.

Eight buttons expose existing actions: F1 Help, F2 Rename, F3 Preview, F5 Copy,
F6 Move, F7 New folder, F8 Trash, F10 Actions. At narrow widths, Nerd icons and
F-key labels remain visible; each has its full accessible tooltip name. Larger
windows show operation names. No new operation/preview backend is introduced.

Dim buttons can explain why an action is unavailable, but cannot launch it:
loading/refresh, no selection/focus, text editor, Espacio view, pending operation
or modal. Copy/Move continue to review source and destination before confirmation.
A click uses the current marked selection like its F-key, not a cached file list.
The painted owner (pane,tab) and listing generation must still match; a stale click
cannot operate on a new pane/directory. Underlying operation preflight/FD proof,
no-clobber, queue budgets and ACK rules remain authoritative.

Pointer routing consumes strip clicks before blurring text fields, so a dim
Copy click cannot become enabled merely because that click ends editing. Right
click/wheel over the strip cannot clear marks or scroll an underlying list. The
normal F-key and modal handlers remain in charge of keyboard operations. Hiding
the strip restores the viewport and reveals focus without changing marks/query.

No additional thread/timer/dependency/cache or filename payload is retained.
Local geometry/allocation facts are not a measured CPU/PSS improvement. Native
OS input, AT-SPI, HiDPI, all helper lifetimes/global resource gates remain pending;
resource/startup freeze and the pause on new preview functionality remain active.
