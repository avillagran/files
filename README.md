# files 0.0.4 — in development

A lightweight file browser in Rust + FLTK: its own UI behind `Painter`, software
rendering, Groups/List/Grid/Columns views and a Space treemap, with Omarchy's Nerd Fonts. MIT.

```sh
cargo build --release
./target/release/files ~/Downloads
cargo test --workspace
make memcheck
```

Requires stable Rust, CMake, a C++ compiler and the development libraries for Wayland,
xkbcommon, Cairo/Pango and X11 (for the FLTK fallback). It builds into `target/`, and the
synthetic test folders live in `~/.cache/files-test`. Cargo uses `.cargo/tmp` for its
temporary files and unit-test fixtures unless the user sets an explicit TMPDIR; never `/tmp`.

- Type to filter; `Esc` clears. `Ctrl+F` edits the query without erasing it,
  with cursor/selection and undo; `Enter`/`Tab` return to the list.
  [Filter editing](docs/FILTER-EDITING.md). `Ctrl+L` edits the path; `Enter` confirms.
  In the path: arrows/Home/End, Shift selection, component jumps with Ctrl,
  text undo/redo and confirm/cancel buttons. [Contract](docs/PATH-EDITOR.md).
- `Ctrl+Shift+P` or Actions → Commands: searchable catalog of tools, aliases
  and shortcuts, with availability reasons. [Contract](docs/COMMANDS.md).
- `Ctrl+Shift+F` searches recursively: combine names, `ext:pdf`, `size:>10MiB`,
  `after:2026-01-01` and `before:2027-01-01` (UTC dates). The Filters button/F4 sets
  minimum/maximum size and dates; Tab cycles fields. [Filters](docs/SEARCH-FACETS.md).
  [Visible progress](docs/SEARCH-PROGRESS.md) while walking; paths are published
  once the walk finishes and is validated, with no made-up percentage.
  Query and criteria support cursor, selection, undo/redo and mouse editing;
  Ctrl+F returns to the field and ↑/↓ move through results. [Editing](docs/SEARCH-EDITING.md).
- Arrows select; `Enter` or double-click navigates into folders or hands files to the
  system `open`, with no shell and no fallback. If it is missing, a visible error and the
  alternative is PENDING. `Backspace` goes up. [Opening and closing](docs/OPEN-AND-CLOSE.md).
- `Ctrl+1/2/3/4/5`: Groups/List/Columns/Space/Grid; List sorts by name/size/date.
- Space: treemap proportional to the real apparent bytes of files and folders,
  with selection/inspector, enter/back and cancellable computation; independent of Columns.
  [Limits and partial states](docs/SPACE-MAP.md).
- `Ctrl` toggles selection, `Shift` selects a range; `Ctrl+A` selects all, `Ctrl+I` inverts.
  Filter with `*.rs`/`ext:pdf,md`; `Ctrl+Shift+A` marks the matches and shows everything.
  `Ctrl+Shift+D` clears marks and focus; equivalent selection menus exist.
  `Insert` marks/unmarks and advances without losing the batch; the footer shows how many are marked.
  `Home/End` jump to the ends; `Shift` extends the range and `Ctrl` moves focus without
  replacing marks. `Ctrl+Shift` adds a range. `PageUp/PageDown` move by page.
- Finder-style Columns: select a folder to show its contents on the right,
  `←/→` change column and `↑/↓` change selection; `Shift+wheel` scrolls horizontally.
  Operations apply to the focused column. Replaces Spatial; old layouts
  are kept and legacy sessions migrate without reusing icon coordinates.
  [Contract and limits](docs/COLUMNS-CONTRACT.md).
- F2 renames with preview, patterns/sequence/literal/case; Ctrl+Z/Shift+Z undo/redo.
  Ctrl+N creates a file; F7/Ctrl+Shift+N creates a folder, never overwriting.
  [Renamer limits and validation](docs/RENAME.md).
- Expandable 52px rail; inspector with metadata and a cancellable recursive
  **Calculate size**; Ctrl+Shift+Space computes the filtered folders. Actions can
  pause upcoming computations, resume, and retry only the errors without repeating valid
  totals. `Ctrl+Alt+Space` opens the manager with rows, scrollable errors and
  per-attempt controls. [Manager](docs/SIZE-MANAGER.md), [queue and limits](docs/FOLDER-SIZES.md). It uses an
  ephemeral helper that does not follow symlinks; it is not a treemap nor physical blocks.
- F9 toggles dual pane; Tab switches focus outside inputs/modals. Each pane keeps its own
  path, history, filter and selection. F5/F6 propose the opposite pane as the destination for review,
  with a single shared queue. [Scope and persistence](docs/DUAL-PANE-CONTRACT.md).
- F5 copies with a queue, pause/cancel and error/rescue diagnostics; Ctrl+R reloads
  preserving the selection. F6 moves via native rename, or via EXDEV for a single regular
  file without hardlinks, with a journal and explicit recovery; EXDEV trees/batches are pending.
- Space previews text, images, Markdown with local resources, experimental HTML,
  multi-page PDF, on-demand animated GIF, video with opt-in audio, audio metadata and playback,
  folder summary, or a ZIP catalog without extraction. Esc/Space closes and waits to reap the helper.
  P plays/pauses video/audio; M toggles video audio, muted by default.
  PDF: T toggles Page/selectable extracted Text; Ctrl+PgUp/PgDn changes page,
  Ctrl+wheel zooms the raster.
  [Limits and evidence](docs/ROADMAP.md). Wayland clipboard with focus is still pending.
- F2: renamer with tokens/sequence and literal or explicit regex replacement,
  with captures and a preview before Apply. [Syntax and limits](docs/RENAME-REGEX.md).
- Trash (Actions or Delete): local send and Undo without replacing taken names,
  up to 64 items per batch. Shift+Delete does not delete permanently.
  [Scope and recovery](docs/TRASH-CORE-NOTES.md).
- Alt+Enter or Actions → Properties shows real metadata and lets you prepare
  r/w/x permissions for an object you own, with FD-based confirmation and rejection of
  external changes. [Linux scope and limits](docs/FILE-PROPERTIES.md).
- Preferences (, Ctrl+,): Normal window by default, Fullscreen,
  Omarchy/square/rounded borders and theme. Actions (, F10): file
  operations. Preferences are written atomically and keep unknown fields.
- Free header for moving and four inner grips for native Wayland resize in
  Normal mode; floating is not forced. [Modes and input limits](docs/WINDOW-MODES.md).

Theme: `~/.local/state/omarchy/current/theme/colors.toml`; outside Omarchy,
`~/.config/files/theme.toml`. `FILES_THEME=/path/colors.toml` lets you try a palette.
`FILES_CONFIG_DIR=/isolated/path` changes only files' configuration and keeps the
desktop's font resolution. No hooks and no daemon; a partial TOML keeps
the last valid palette. The outer border is still managed by Hyprland.

The last baseline measured before the new previews is still **FAIL**:
56.743936 MB peak and 41.455616 MB steady for the PSS+SwapPss tree (10k/1080p).
External startup: 90.644 ms p50 and 151.978 ms max, 7/10 under 100 ms;
**FAIL on the strict gate**. Those figures are not extrapolated to the current binary.

The current phase allows integration/screenshots and keeps resource/startup
measurements suspended. The installed previews have functional evidence, a NOT_MEASURED
budget, and no global PASS or 0.0.4 tag. The main PSS footer of a
screenshot does not measure the process tree or swap; the video clip also introduces readbacks.

Method, data and limits: [VALIDATION](docs/VALIDATION.md). Real framebuffer
screenshots: [Hackerman](docs/evidence/0.0.2/hackerman.png),
[Tokyo Night](docs/evidence/0.0.2/tokyo-night.png),
[Spatial after reopening](docs/evidence/0.0.2/spatial-reopen-1x.png),
[du in the inspector](docs/evidence/du/inspector.png). Scenes use internal handlers:
physical OS events are still pending for lack of a compatible driver.

```sh
python3 scripts/tabs-ui-check.py --columns
python3 scripts/tabs-ui-check.py --sessions
python3 scripts/du-ui-check.py
```

The Spatial 0.0.2 screenshots and scripts are historical: Columns replaces them.
Resources/startup remain frozen; do not run benchmarks in this phase.

Run benchmarks serially, with no concurrent builds or screenshots.
`du` has limits/supervision: [DU-HELPER](docs/DU-HELPER.md). Real GLES shaders
are ported and drawn in hardware fixtures, **still without Painter integration**.
Move, conflicts/journal, dates, advanced previews, the remaining views/network
and Windows/macOS adapters continue in the [full matrix](docs/ROADMAP.md); isolated modules
are not considered shipped features. Only Linux aarch64/Wayland is tested.

Experimental HTML requires `make html-helper` to prepare the optional adapter
next to the release build (pinned/verified sources, no global install). PDF uses
pdfinfo/pdftoppm and video/audio use the system ffprobe/ffmpeg when available.

[Approved design](docs/DESIGN.md) · [Mockup v3](mockups/v3.html) ·
[Licenses](docs/DEPENDENCIES.md).

Versioning: each release bumps only the last number: 0.0.1, 0.0.2, 0.0.3…
Changing the first two requires the user's authorization.

Hidden files are off by default: Space, Preferences and Ctrl+H share the
same persisted global option. Context menu for batch/item/background and help
in the bottom bar; Space uses color blocks and real sizes as in v3.
See [scope and functional evidence](docs/HIDDEN-CONTEXT-AND-STATUS.md).

Grid: Nerd button or Ctrl+5, zoom with Ctrl+wheel/±, selection and navigation by
visible rows. Visible PNG/JPEG/WebP/video thumbnails and a cache shared with the inspector;
up to 32 sources on demand.
[Contract and evidence](docs/GRID-VIEW.md).

Back/Forward: Nerd buttons next to the path or Alt+arrows. Bounded per-tab
history. Ctrl+T opens, Ctrl+Tab switches, Ctrl+W closes and Ctrl+Shift+T reopens;
up to eight tabs per instance. With no path in argv the last session is restored;
an explicit path neither restores nor autosaves. Preferences → Save session
enables explicit saving; `--no-session` disables it for that instance.
Another window keeps the session read-only while an owner exists. The omnibar rejects pastes larger than 4096 B
before copying them. [History](docs/NAVIGATION-HISTORY.md), [tabs](docs/TABS-CONTRACT.md) and [session](docs/SESSION-CONTRACT.md).

Ctrl+Alt+B or Preferences enables the optional MC-style operations bar, with Nerd
buttons and the F1/F2/F3/F5/F6/F7/F8/F10 keys. [Contract](docs/OPERATION-BAR.md).

F1 or Preferences → Help shows the available shortcuts, paginated, closed with Esc.
[Validation and limits](docs/SHORTCUT-HELP.md).

Selected image/video previews invalidate their pixels when the original changes,
is deleted or replaced. Revision checked on the decoder's FD and
reconciliation outside the UI. Grid/inspector thumbnails with a bounded LRU are already visible; a global cache across
formats/Space and measurements are still pending.
[Contract, evidence and limits](docs/MEDIA-LIFECYCLE.md).

[Thumbnails, invalidation and real limits](docs/MEDIA-THUMBNAILS.md).

Non-Unicode Unix paths: startup via args_os and byte-based IPC, including the full
image in Space. [Evidence and limits](docs/RAW-PATH-WIRE.md).

Markdown resources: [root signature and effective FD](docs/RELATIVE-SOURCE.md),
with swaps between probe and decoder rejected. [Invalidation and reconciliation](docs/DOCUMENT-MEDIA.md) are in place for
local Markdown; resources/startup still have no new measurements.

[12/24 MP progressive JPEG](docs/JPEG-DC-PREVIEW.md): 1/8 reduced preview
with the same coefficient limit; still without new resource measurements.

[12/24 MP PNG by rows](docs/PNG-ROWS-PREVIEW.md): downscaling without a full
raster, alpha, Adam7 and EXIF orientation. Functional screenshots verified;
resources/startup without new measurements because of the freeze.

[Audio pre-listening](docs/AUDIO-PLAYBACK.md): play/pause and seek with the system
Pulse/ffmpeg, on demand and with a verified source. No autoplay;
the position is estimated and resource acceptance is still pending.

Move: F6, context menu or inspector → review destination → confirm in the queue.
Native move within the same filesystem, never overwriting, up to 128 roots.
Across filesystems (EXDEV), a single regular file without hardlinks uses a verified copy,
synced publication and a journal before deleting the source. Actions →
Interrupted moves lets you review and explicitly finish/restore.
EXDEV trees/batches/hardlinks are still pending. [Contract and evidence](docs/EXDEV-JOURNAL-CONTRACT.md).

New preview improvements are paused; features and UI take priority:
dual pane, cut/paste, DnD, operations and mounted SFTP.

[Finder-style Columns](docs/COLUMNS-CONTRACT.md): a chain whose focus/operation source is
independent of the visible child, branch pruning and a sliding window
with no depth limit. Replaces Spatial; old tag3 sessions migrate without
icon coordinates and the new tag5 keeps scroll. Framebuffer and real copy
verified, without OS input/live resize/new measurements.

Ctrl+C/Ctrl+X/Ctrl+V and the menus copy/cut/paste through a
**files file clipboard that is internal to this window**. Paste shows
source and destination before accepting the operation; Cut keeps pending items after a
conflict or cancellation. In editable fields Ctrl+V pastes system text.
Cross-application file MIME and DnD are still pending: [contract and
validation](docs/FILE-CLIPBOARD-CONTRACT.md). A Cut is not resumed when a session is opened.

M29a adds the Wayland bridge and "Paste from system" (Ctrl+Shift+V), still without
cross-application MIME validation. Codec/pipe/State and rejection without a real gesture are
verified; DnD/KDE/multi-source are pending. See docs/NATIVE-TRANSFER-CONTRACT.md.
No new metrics while the freeze remains in effect.

The DnD COPY receiver and review through a private pipe have tests and
screenshots; Wayland interoperability between applications is still unvalidated.
Sender, KDE and multiple parents are still pending. Details and licenses for the FLTK
hook in docs/NATIVE-TRANSFER-CONTRACT.md and vendor/fltk-wayland-transfer/.

Search (Ctrl+Shift+F) combines names/metadata with literal UTF-8 content.
Text/F6 keeps an independent draft, with limits and matching lines.
[Content contract](docs/CONTENT-SEARCH.md).

Bookmarks and places: **Ctrl+D** saves the current folder and **Ctrl+Alt+D** or the
Nerd star opens quick access. Filter labels, navigate with arrows/Enter and
edit a label with F2; Delete removes the bookmark without deleting its folder.
[Contract and limits](docs/BOOKMARKS.md). It does not replace the recursive search.

The Copy/Move destination supports cursor, mouse/keyboard selection, editing in the
middle of the path and Ctrl+Z/Ctrl+Y. Editing invalidates the prepared review; moving the
cursor keeps it. Captured raw paths are only replaced as a whole with
Ctrl+A. [Editor contract](docs/DESTINATION-EDITOR.md).

In Copy you can enable **Skip existing (F4)** to continue with the other
roots in the batch. It is off in every dialog; it never merges folders nor
overwrites. The result separates publications and skips by requested path.
[Conflict contract](docs/COPY-CONFLICTS.md). The option does not apply to Move; resolving conflicts by replacing or renaming
is still pending.

To **copy under another name**, open F5 and press **F2**: it changes the destination name
of the current item. **Alt+↑/↓** or the Nerd buttons move through the selection;
**Ctrl+L** goes back to the destination folder. The restore button brings back the plan's
original name, without renaming files. It also allows duplicating within the same
folder. [Contract and limits](docs/COPY-NAMES.md). Copy only for now; it does not
replace occupants and rejects duplicate destination names before queueing.

Ctrl+L → `>command` or `:place` → Enter opens the filtered catalog. Then choose the action/destination; the first match is never run automatically. `./>name` and `./:name` remain literal paths. See docs/OMNIBAR-CATALOGS.md.

Recursive search: matches appear during the walk labeled PROVISIONAL. You can explicitly open/locate an observation; cancellation, an error or a new query withdraws candidates. The final report validates the root and keeps the selected file. Explicit 500 limit, no index and no new cache. See docs/SEARCH-BATCHES.md.

The content search shows an observed snippet of up to 256 bytes with the match highlighted, also while searching. Ellipses mark truncation; whitespace/control characters are normalized only for display. No preview is opened and the file is not read again. See docs/SEARCH-SNIPPETS.md.
