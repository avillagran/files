# Searchable command catalog

Open **Acciones → Comandos** or **Ctrl+Shift+P**. Type a tool name, alias or
shortcut; independent words AND together, fuzzy scores order the matches, and
registry order breaks ties. Spanish labels, accent-free aliases and some English
aliases (`copy`, `rename`, `size`, `mc`) help discovery. There are 65 registered
commands; the visible count updates as the query changes. It is a catalog of
existing actions, **not a terminal or shell interpreter**.

Up/Down, Home/End and PageUp/PageDown choose a row; mouse wheel scrolls the
selection. Enter, the Execute button, or a row double click dispatch the chosen
command. A single row click only selects. Escape or Ctrl+Shift+P closes the
catalog. Ctrl+A/Ctrl+F select the query for replacement; Backspace removes the
trailing Unicode scalar. Query input is append/replace, not a full text editor.
Full caret selection, IME/preedit, AT-SPI/native input acceptance remain pending.

Disabled commands remain visible with a reason, and Execute is dimmed without a
hit target. Enter on a disabled row explains the reason; it never silently picks
a different enabled row. File tools retain their normal workflow: rename opens
preview, Copy/Move open destination review, Paste uses the existing private files
buffer/review, and folder sizes use the existing shared queue/helper. Size pause
finishes the active measurement but stops the next admission; Resume continues
pending rows and Retry queues only failed rows, preserving pause and good totals.
The three commands show availability reasons when idle/already paused/no errors. Trash is
explicitly labelled **Enviar selección a Papelera** and invokes the normal Trash
action on explicit execution. There is no new permanent-delete command.
Inspector uses the normal action, which returns to a single panel from dual mode.
Settings and file Actions retain separate toolbar buttons/menus; the general
catalog also offers navigation, selection, views, help and preferences.

## Context and bounded work

Opening or typing does not change file focus, marks or the active pane/tab. The
catalog owns input; file clipboard capture, DnD, window gestures and underlying
file actions are blocked while it is open. Existing copy jobs may continue.
Dialogs with editable/operation state must be finished or closed first. A notice
or operation dialog arriving later blocks dispatch too; closing the catalog
leaves that notice intact. The
catalog suspends thumbnail demand rather than starting previews or another engine.

Dispatch rechecks pane/tab ownership, listing generation, loading/busy conditions
and availability. Focus/selection actions also compare the exact captured focus
and mark membership, even when the count is unchanged. A changed context does not
redirect the action. **Usar contexto actual** explicitly recaptures current state
without discarding the command query. Reopening also recaptures.

The selection proof is either sorted sparse indices or dense bits, whichever is
smaller, capped at 64KiB of vector backing storage before allocation. For 100,000
dense indices the vector has 1,563 u64 words =12,504 bytes (~12.21KiB). No filenames,
file contents or RGB buffers are cloned. Unrepresentable/invalid snapshots disable
selection-dependent commands; listing/navigation commands remain available when
otherwise valid. Recapture releases the old proof before constructing its successor.
This UI identity check is not an FD/inode filesystem proof or a snapshot of a
changing tree. Every actual filesystem operation still uses its existing worker,
FD preflight, conflict rules, confirmation and ACK/recovery contract.

The query has a 256-byte preflight, rejecting controls/overflow before scanning
or copying a large paste. Matching visits a fixed registry, not the filesystem.
Result capacity is fixed to the registry size; painting visits only visible rows.
Folder-presence metadata is derived once when capturing context; full selection
comparison happens only when executing a selection/focus action, not per paint.
No worker/thread/timer, dependency, persistent command history, arbitrary process
launcher or cache is introduced. These are structural bounds, not a process-tree
resource gate or measured performance improvement.

Pointer rows and Execute carry the painted input epoch, so a click on an old
result after a query change cannot dispatch its new occupant. Keyboard Enter
intentionally executes the current query selection. Input epochs never repeat
across catalog instances; the existing paste gate retains invalidated requests
until callback drain and rejects stale callbacks. This proves internal routing,
not OS clipboard publication or interoperation.

Native clipboard MIME export/import and DnD are not new catalog commands: their
real gesture/serial contracts and unvalidated interoperability are unchanged.
New media features remain paused; resource/startup measurements remain frozen.

### Sort discovery (2026-11-01)

The palette now includes Modified date alongside Name and Size. Search
`ordenar fecha` after Ctrl+Shift+P and press Enter; it applies to the active
browser. Repeating a criterion reverses direction. Per-pane header buttons have
matching static names/tooltips; an inactive header never transfers operation focus.


`marcadas` / `sizes-marked` measures only marked visible directories and opens
the existing size manager. An unmarked focus is not a fallback. Busy batches
reject rather than turning into Stop. The registry now has65 commands under a
compile-time96-entry cap; actual-count match allocation and64KiB selection proofs
remain unchanged. See MARKED-FOLDER-SIZES.md for scope and acceptance.
