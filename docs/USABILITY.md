# Usability programme — 2026-10-30

The goal is an excellent file explorer for keyboard, MC/text-manager and mouse
users. Resource targets remain ideal 50 MB decimal / hard 150 MB process tree.
The resource/startup measurement freeze remains active: source optimizations do
not prove a memory or speed improvement. New media features remain paused.

## First implementation slice (scoped functional validation passed)

Local filtering shares one pure matcher across the main browser, retained
Columns and dual-pane selection/refresh policies. It operates on names already
listed; it does not scan descendants, read file contents or request stat data.

- Type to filter; Ctrl+F or the Nerd search button starts a fresh local filter.
  Escape or the adjacent clear button removes it. Ctrl+L still edits the path.
- Words combine with AND in any order: `report 2026`.
- `!draft` or `-draft` excludes a match. Quote a literal phrase:
  `"annual report"`; `name:"annual report"` also works. An unfinished quote is
  usable while typing. Shell escaping and nested quotes are not a query language.
- `*.pdf` and `note-??.txt` match a full name, with `?` consuming one Unicode
  scalar. Unicode lowercasing is used; normalization/accent folding is not implied.
- `ext:pdf,md` matches either extension. `type:dir`, `type:file`, `type:link`
  and `type:other` use entry kinds. Symlinks remain symlinks.
- At most 4096 query bytes and 32 terms. Invalid type/over-limit filters fail
  closed with an explanatory status. Hidden files still follow the global toggle.
- Original raw names remain unchanged for operations. A pruned selection cannot
  silently include hidden or filtered-out entries.

Keyboard additions reuse existing operations: Insert marks/unmarks then advances
focus without replacing the marked batch; F3 previews; F8 sends to Trash; F5/F6
continue to review Copy/Move and F7 creates a folder. Ctrl+Space opens the inspector
and calculates the selected directory, or cancels its active/pending calculation.

Folder usage reports apparent file bytes and descendant entry count, including
hidden entries. Symlinks add zero bytes and are not followed; hard-linked names
count separately and sparse files contribute logical length. It is not allocated
disk usage or a filesystem snapshot. Root opening is directory-only/no-follow;
refresh cancels and clears previous results. Traversal and reap stay off the UI
thread. Directory entry kinds avoid redundant lstat calls for directories/links;
no measured speedup is claimed.

## Following deliveries — still open

1. Recursive filename search now has a first bounded UI/helper implementation
   (see RECURSIVE-SEARCH.md). Still open: progressive publication/paging beyond
   the 500-hit cap, richer facets and native input/mounted-FS runtime acceptance.
2. Search facets for content, sizes and dates, with expensive probes explicitly
   scheduled off the UI thread; clarify unknown vs zero vs partial results.
3. Folder-size job progress, visible per-row results and sorting, allocated vs
   apparent bytes, mount boundaries, partial/error details and safe invalidation.
4. Discoverable command palette, keyboard-first operation bar and complete
   selection/focus/scroll behaviour across views, panels, tabs and dialogs.
5. Conflict resolution, multi-origin operations and durable recovery, mounted
   SFTP, bookmarks and tools from ROADMAP; no reduced completion claim.
6. Controlled resource/startup tests after the freeze is lifted, plus native
   keyboard/mouse/clipboard acceptance. Internal handler/framebuffer scenes do
   not establish native input or interoperability.

## Evidence for this slice

- 679 workspace tests passed (228 core / 67 helper / 384 UI), 3 existing core tests
  ignored; Clippy workspace/all-targets -D warnings, release workspace and fmt pass.
- `evidence/usability/usability-ui-1793391860625293505.json`: local filters across
  List/Grid/Columns, original raw names, two marked roots with independent focus,
  helper results 8 B/5 entries then 12 B/6 after adding a file, six framebuffers,
  1000×650 and 800×440, callback/drain/exit0. Input uses production handlers,
  painted targets and a fixture command pipe, not native OS keyboard/mouse input.
- The first scene `usability-ui-1793391790104300795.json` passed too; the second
  adds a more useful MC screenshot with two marks and separate focus. Same binary.
- Installation: `evidence/0.0.4/installed-checkpoint-1793391834122584562.json`.
  App SHA256 `11c95e69974231ab25085947349b1fb456f3ec362f19c1f82014683e635e40a4`;
  helper `e7f835ba69a90bf0619d696abc1da3faf0c06a68a66d5ffcef82074cf23594e3`.
  All five installed artifacts and five backups were independently rehashed.
- Build/test logs in `target/usability-*`: initial navigation test compilation
  used a wrong fixture field name, corrected; a later Clippy pass caught three
  redundant parentheses, corrected. Those logs remain; final checks are clean.
- Resource/startup NOT_MEASURED; historical baseline/startup/clipboard failures
  remain open. No global PASS or 0.0.4 tag.

## Queued folder sizes

Ctrl+Space measures the focused folder; Ctrl+Shift+Space measures the visible
filtered directories or stops that queue. Inspector/menu controls share those
handlers. Completed observations appear beside rows (`∑`), live partial sums use
`≥`, and errors never become zero. Selection changes do not redirect work.
Refresh/navigation/owner changes and mutation ACKs invalidate measurements.
Size sorting keeps unknown folders last and does not rerun on every progress
sample. See [FOLDER-SIZES.md](FOLDER-SIZES.md) for queue, protocol and validity limits.

## Selection patterns and MC workflows

Filter, then Ctrl+Shift+A marks matching rows and returns to the complete visible
folder. Ctrl+A marks all, Ctrl+I inverts, Ctrl+Shift+D clears marks and operative
focus. Mouse equivalents are in Actions/right-click menus; the footer shows the
marked count or `Sin selección`. See SELECTION-TOOLS.md for hidden/refresh/raw
semantics. Focus stays stable under Select All, and refresh picks a surviving
marked row if the old focus no longer matches.

## Filter text focus

Ctrl+F now preserves/selects the existing filter. Typing activates the bounded
editor; cursor/selection/Delete/undo and text clipboard own input until Enter/Tab
or a file-navigation key returns to the listing. Ctrl+Shift+A/D remain explicit
MC selection tools even during editing. Normal Ctrl+A/C/X/Z affect text while
focused. See FILTER-EDITING.md for pane/tab/paste/limit and Escape semantics.

## Optional MC operation strip

Ctrl+Alt+B, Preferences and the command catalog toggle a persistent operation bar
for either one or two panes. Function-key labels/Nerd buttons expose existing
reviewed operations and explain unavailable actions; reserved viewport space
keeps rows/inspector/drop targets clear. See OPERATION-BAR.md for pointer-editor
guards, source-context validity and remaining native/resource gates.

## Literal text search

The recursive panel now combines names and metadata with one opt-in exact UTF8
content criterion. Texto/F6 switches a separate bounded draft without erasing the
name query; quoted advanced syntax remains available. Regular-file streaming,
post-read revision checks, binary/limit/change counters and first-match lines
are specified in CONTENT-SEARCH.md. Regex/case folding/full encodings/indexing/
semantic search and broader acceptance remain open.

## Favourites and quick folder hub

Ctrl+D captures the current loaded folder; Ctrl+Alt+D/the rail star opens one
keyboard/mouse hub for favourites, existing XDG places and the current tab's
back/forward history. Type to filter labels, arrows/Enter to enter, F2 to edit a
favourite label, Delete to remove only its bookmark. Captured pending intent
survives navigation; stale row commands cannot silently affect another row.
Persistence runs on existing AuxIo, with bounded wire and cooperating-window
merge. See BOOKMARKS.md for limits, explicit retry/omit and validation scope.

## Operation destination editing

Copy/Move destination uses the same bounded text editor as search/path fields:
caret/selection, UTF8 scalar and word motion, mouse placement/drag, undo/redo and
horizontal reveal. A fresh dialog/caret/draft epoch rejects old paste receipts.
Captured raw paths stay bytes until explicit complete replacement; that boundary
starts fresh history, never a lossy unescape. Existing source capture, no-clobber
and async move review remain. See DESTINATION-EDITOR.md. General conflict policy
and recovery remain broader ROADMAP work.

## Explicit per-copy conflict policy

Default Stop remains. Copy's explicit Skip Existing/F4 continues independent
roots, preserving symlink/file/directory occupants without merging. Counts and
requested paths of skipped roots are separate from verified publications. Late
NOREPLACE collision may skip only after complete identity-aware staging cleanup;
any rescue/uncertainty remains an error. Per-job option isn't persisted or applied
to Move. See COPY-CONFLICTS.md for report caps and proof scope.

## Per-root alternate copy names

F5 keeps source basenames immutable while each selected root can receive an
explicit target basename. F2 edits the current name; Alt+Up/Down or Nerd buttons
visit the captured order; restore resets that proposed name, Ctrl+L edits the
folder. Single shared name Cursor/Geometry, bounded override map, source Arc
identity and per-row/field epochs avoid stale paste or source rename. Duplicate
names/invalid components reject before queue admission. COPY-NAMES.md distinguishes
Copy from pending Move/replace/automatic suffix work and documents FD proof.

Size manager: the Nerd sort button or S cycles Queue/Largest/Name. Largest only uses finished totals; partial progress is never confused with a final size. The selection is kept per row, arrows/Home/End walk the visible order, without changing the calculation queue.

Ctrl+L → `>command` or `:place` → Enter opens the filtered catalog. Then choose the action/destination; the first match is never run automatically. `./>name` and `./:name` remain literal paths. See docs/OMNIBAR-CATALOGS.md.

Recursive search: matches appear during the walk labeled PROVISIONAL. You can explicitly open/locate an observation; cancellation, an error or a new query withdraws candidates. The final report validates the root and keeps the selected file. Explicit 500 limit, no index and no new cache. See docs/SEARCH-BATCHES.md.

The content search shows an observed snippet of up to 256 bytes with the match highlighted, also while searching. Ellipses mark truncation; whitespace/control characters are normalized only for display. No preview is opened and the file is not read again. See docs/SEARCH-SNIPPETS.md.

## Copy conflicts: Keep Both

Copy adds an explicit third F4 policy, numeric Keep Both, with no overwrites and
actual destination paths in results. Fresh dialogs remain Stop. Same-folder
duplication, compound extensions and raw Unix names are supported. See
[COPY-KEEP-BOTH.md](COPY-KEEP-BOTH.md) for limits and scoped evidence.

## Navigate from folder-size results

The size manager now opens its selected directory with Alt+Enter or the Nerd
folder button. Sort Largest, Home, then open directly; Enter remains details.
Raw names are passed unchanged to ordinary browser navigation/history; stale
context/selected-ID mismatch is rejected. Navigation invalidates parent sizes
and cancels old work via existing lifecycle. See SIZE-MANAGER.md for evidence
and the distinction between a past measurement and a current inode binding.

## Search result order

F7 or the Nerd order button cycles Ruta/Nombre/Tipo; the same raw selected result
survives order changes and provisional→final reconciliation. Numeric name ordering
uses existing natural keys; keyboard navigation follows visual positions. Kind
groups directories/files/links/other; no additional stat/content read or sort by
unknown metadata. Closing releases index. See SEARCH-ORDER.md for costs/limits.

## SFTP preparation and capability diagnostic

Actions and command catalog `sftp` open a bounded four-field target form. Local
capability checks use the shared I/O worker. Ctrl1–4/AltUpDown, existing rich
cursor/undo/text paste and mouse fields are available; review is explicit and
does not connect. Missing sshfs is visible. No mount transport is advertised.
See SFTP-PREPARATION.md and SFTP-MOUNT-CONTRACT.md; N01 stays open.
