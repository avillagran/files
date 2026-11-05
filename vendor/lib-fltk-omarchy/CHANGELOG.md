# Changelog

## Unreleased

- **Tooltips are drawn by the toolkit** (`tooltip` module), like Omarchy's
  shell tooltips (omarchy#15694): `App::new` — or the first `theme::set`
  in apps that start FLTK themselves, like Files, which never got the
  tooltip style before — turns FLTK's tooltip window off and shows the tooltip of the widget under the pointer as an
  in-window surface with the theme's `[tooltip]` colors and border width,
  `bodySmall` text, `controlPaddingX/Y` padding, the theme corner radius
  (follows `theme::set_corners`), plain text wrapped at 360 px, centered
  above the item (below when there is no room, below the pointer on large
  widgets), kept inside the window. 400 ms delay, the next one at once
  while one is up or for 400 ms after (bar `tooltipWarmTimer`), hidden by
  a click, key or wheel. No app changes: `.tooltip()`, `.bind_tooltip()`,
  header actions, plain fltk `set_tooltip` and canvases that change their
  tooltip per region (Files) all get it (FLTK's window only followed a
  change of the widget under the pointer, so one canvas showed at most
  the first region's tooltip). Restyled on every theme reload;
  bound text relabels a shown tooltip; right-to-left lines are
  right-aligned. Follows `~/.config/omarchy/tooltips.json` live (`shape`,
  `borderMode`, `borderColor`; invalid files keep the last valid
  settings; `OMARCHY_FLTK_TOOLTIPS` overrides the path). API:
  `tooltip::show`, `hide`, `shown`, `settings`, `set_settings`,
  `reload_settings`, `place`, `wrap`; `focus::hover_at` moves the pointer
  in tests. The tooltip stays the accessible description; the surface is
  hidden from screen readers.
- **Fix: pure black theme colors.** `Rgb(0x000000).fl()` was FLTK color
  0, the colormap entry the theme sets to the foreground: themes with a
  black background (vantablack) painted tooltips, windows and fills in the
  text color. It is now `Color::Black`.
- **Motion everywhere, following the system's Animations setting**
  (docs/ANIMATION.md).
  - `anim::enabled()` follows the OS live: Hyprland `animations:enabled`
    (Omarchy's Animations switch; re-read on `configreloaded` and focus
    changes from Hyprland's event socket), GNOME `enable-animations`,
    macOS Reduce motion, Windows animation effects (`platform::animations`).
    App override `anim::set_enabled(Some(bool) / None)` (`set_enabled(false)`
    still works), env `OMARCHY_FLTK_ANIM=on|off`; `anim::source()`,
    `anim::system()`. One blocking watcher thread, no polling.
  - Automatic layout motion: toolkit rows and columns (`row`, `column`,
    `section`, containers you `fit` into, every `Flex` of an `AppWindow`
    at `end`) slide their children when a `fixed` size changes or a child
    is shown, hidden or added (190 ms OutCubic, o3 `PinnedKeyboardPanel`);
    appearing children grow from their start edge and fade in;
    `layout::set_visible` collapses and fades out. Not animated: the first
    layout, window resizes, direction flips, theme or language changes.
    Interruptions snap to FLTK's layout with the real `fixed` sizes.
    Opt out with `FlexExt::animated(false)` / `layout::set_animated`;
    `layout::animating()`.
  - New `Sidebar` (`widgets::Sidebar`, `layout::Sidebar`): a rail that
    expands on hover (350 ms intent, 140 ms leave grace, o3), push or
    overlay, pinned, keyboard (focus expands, arrows, Enter, Esc), right
    to left, accessible (tab list with expanded state).
  - Popups fade in (180 ms) and out (120 ms) with Omarchy's `Style.easeOut`
    (PopupMenu, Dropdown, SearchableDropdown, Menu, ConfirmDialog,
    PopupCard, ShortcutHelp, notifications, OperationToast, completion
    lists); focus comes back at once, the layer stops taking events and is
    deleted after the fade.
  - TabBar selection slides (160 ms); `List::from_model` rows make room,
    arrive, leave and fly when moved, by item ID (o3 thread rows);
    `Tree` branches open and close with the rows below sliding (200 ms);
    a new `RichText` message fades in (streaming costs nothing).
  - `anim::Tween`, `anim::now`, `anim::clock::{freeze, advance, resume}`
    for tests, `anim::ms` (Omarchy/o3 durations), more `Ease` curves
    (`InCubic`, `InOutCubic`, `InOutQuart`, `OutQuint`, `OutBack`,
    `EaseOut` = Omarchy's Bézier), `Anim::value`, `Anim::target`.
  - `script/motion-bench` (`examples/motion_bench`): sidebar hover expand
    on a real window, 60 fps, ≈ 1.2 ms CPU per frame, idle unchanged.
  - Toolkit catalog: "Sidebar" in the 20 languages.

- **Right-to-left layouts.** In Arabic, Hebrew, Persian and Urdu the UI is
  mirrored, live on a locale switch (no window rebuild); docs/I18N.md.
  - `layout::Direction` (= `i18n::Direction`), `layout::direction_of` /
    `is_rtl` (nearest override, else the language), per-container
    overrides `FlexExt::direction(Direction::Ltr)` and
    `layout::set_direction(&w, Some(..))` for media controls, timelines,
    code. `layout::Mirror` (mirror x, points, alignment, glyphs) and
    `layout::logical_key` for custom widgets.
  - Rows mirror their children after FLTK lays them out (`layout::row`,
    `fit`, `close_fitted`, `layout::mirror`; `AppWindow::end` registers
    every `Flex` row of the window, `layout::mirror_rows`). Child order is
    unchanged, so Tab still goes from the start to the end.
  - Mirrored components: header (title right, actions left), Button,
    ButtonGroup, Toggle/ToggleSwitch (knob left when on), Slider and
    Progress (fill from the right), Hero, Row, Label, TabBar (order, close
    glyph, overflow), List/Table/Tree (columns, indentation, expanders,
    scrollbar left), PopupMenu (glyph right, shortcut/check left,
    `open_at` grows left), Menu (`‹` submenu mark, Left enters),
    Dropdown, SearchableDropdown, ShortcutHelp, ConfirmDialog,
    OperationToast (bottom-left), Notification (top-left), TextField
    placeholder. Accessibility bounds follow.
  - Keys: Left/Right swap in mirrored TabBar, ButtonGroup, Slider,
    KeyboardPanel grids (`on_edge` gets the logical key) and Tree
    (docs/KEYBOARD.md).
  - `Table::set_titles` renames columns (live translation).
  - `i18n::is_rtl()` no longer takes a lock (read on every draw).
  - Fix: a `Notification` shown after an earlier toast's window was
    deleted no longer touches the deleted toast.
- **Bidirectional RichText.** Lines follow the Unicode Bidirectional
  Algorithm (new dependency `unicode-bidi` 0.3.18, MIT/Apache-2.0, no
  dependencies): base direction per block from its first strong character
  (RLM/LRM force it), pieces in visual order, right-to-left blocks
  right-aligned with markers and quote bars on the right, tables with the
  first column on the right, code left to right. Hit testing, selection
  rectangles and `text_rect` follow the visual order
  (`Layout::span_x`, `Frag::rtl`, `Line::rtl`, `Layout::rtl`,
  `layout::has_rtl`). Text without right-to-left characters is laid out
  as before; streaming keeps its incremental relayout.
- `examples/rtl`: the same windows in English and Arabic, switched live
  (`--snapshot DIR`); `richtext_bench --lang ar` streams Arabic.
- **`edit::TextArea` and `edit::Composer`: multi-line editing and the chat
  input** (docs/COMPONENTS.md, "TextArea and Composer").
  - `TextArea`: TextField look, word wrap with the `text` line breaker
    (CJK, graphemes), auto-grow `.lines(min, max)` then scrolling,
    placeholder, two-way `bind`, mouse and keyboard selection (word
    jumps, Home/End per visual line, PageUp/PageDown, double/triple
    click, drag with edge scrolling, middle-click paste, context menu),
    undo/redo with word-sized typing groups, grapheme-correct editing,
    atomic chips carrying data (`insert_chip`, `expanded_text`), input
    methods through `Fl::compose` (preedit underlined, Enter commits it)
    with the candidate window at the caret (`fl_set_spot`), Tab leaves
    (`TabKey` to type tabs), RTL alignment, `read_only`, `on_key`.
    Drawn by the toolkit, not `fltk::text::TextEditor`: chips, grapheme
    deletion, undo groups and caret geometry need the text model.
  - Layout per paragraph; a long paragraph is re-wrapped only until its
    line breaks meet the old ones, and word widths are cached. Release,
    720×640 window: 100 KB paste 8 ms (Wayland) / 21 ms (Xvfb), a key with
    redraw 0.18–0.28 ms in the composer, about 2 ms in a 100 KB document,
    idle 0.01 % CPU (no caret blink). `script/edit-bench`.
  - `Composer`: Enter sends (`on_submit`; text and attachments cleared,
    Ctrl+Z brings the text back), Shift+Enter new line or
    `SendKey::ModEnter`, Mod+Shift+Enter `on_alternate_submit`, send
    button (dimmed when empty) → stop button while `busy` (`on_stop`),
    `.trigger(c, Trigger::WordStart | LineStart, provider)` completions
    answered now, later or from a thread (`Completions::feed`), stale
    answers dropped, list at the caret (Up/Down/Enter/Tab/Esc, mouse),
    accepted items as chips, attachment chips from a `ListModel`
    (`Attachment::file`), files from drops and `file://` pastes
    (`on_files`, `edit::parse_files`), `toolbar(|row| ..)`, RTL mirroring.
  - Accessibility: TextArea → MultilineTextInput with value, caret and
    selection, SetValue / SetTextSelection / ReplaceSelectedText; the
    completion list is a ListBox under the input with
    `Node::active_descendant` (new) on the highlighted option; send and
    stop buttons and attachment remove buttons are named.
  - `focus::type_text("日本")` sends committed text through the key path
    (tests); `script/edit-input-check` sends real X11 keys (CJK and emoji
    keysyms, a dead key, Shift+Return, BackSpace) in a private Xvfb.
  - Toolkit strings (20 locales): `Write a message…`, `Send`, `Stop`,
    `Attach`, `Attachments`, `Remove {name}`, `Suggestions`,
    `Searching…`, `No results`. AGENTS.md rule 18.

- **Multiplatform: Linux (Wayland and X11), macOS and Windows.** All OS
  code moved into a new `platform` module (`src/platform/`: `linux`,
  `macos`, `windows`, shared `generic` fallbacks, portable `poll`
  watcher), with a working fallback on each OS; the rest of the crate is
  portable. Linux behaviour is unchanged. Guide and verification:
  docs/PLATFORMS.md.
  - `Cargo.toml`: fltk's `use-wayland` only on Linux (one fltk-sys build
    per target, `gpu` unchanged); `libc` only on Unix.
  - Native window move/resize (Wayland/libdecor) and glass FFI live in
    `platform::linux`; `native`, `glass` and `hyprland` keep their API.
    Hyprland IPC is only available on Linux with Hyprland running.
  - `AppWindow` floating drag/resize: native Wayland → Hyprland IPC → new
    FLTK fallback that moves/resizes the window itself (X11 window
    managers, macOS, Windows). Verified under X11 with `script/x11-check`.
  - Theme: built-in dark (Tokyo Night) or new `theme::FALLBACK_LIGHT_COLORS`
    following the OS appearance on macOS/Windows (`platform::prefers_dark`,
    `OMARCHY_FLTK_APPEARANCE`); `theme::set_theme_dir` /
    `OMARCHY_FLTK_THEME_DIR` load any Omarchy theme directory on any OS.
    Omarchy paths come from `platform::dirs` per OS.
  - Live reload: inotify on Linux (unchanged, falls back to polling if
    inotify is unavailable); 1 s mtime polling elsewhere, which also
    follows the OS appearance.
  - Fonts: fontconfig `monospace` on Linux, Menlo on macOS, Consolas on
    Windows, with the face names each FLTK backend expects
    (`platform::fonts`).
  - Shortcuts: `Mod` (also `Primary`, `CmdOrCtrl`) is the primary
    modifier, Cmd on macOS and Ctrl elsewhere (`keys::PRIMARY`,
    `keys::primary_held`, `keys::label`). Popup menu hints and the F1
    help show specs the platform's way. Keyboard panels, Table and Tree
    use the primary modifier for toggle, select all and click-to-toggle.
    Examples use `Mod+,` for Preferences.
  - `native::pixels_per_unit` uses the platform layer: the Wayland
    output scale on Linux, FLTK's own value (Retina included) on macOS
    and Windows.
  - New: `platform::{OS, Os, wayland, prefers_dark, process_cpu_time,
    dirs, fonts}`; `examples/platform_check`; `script/x11-check`;
    `.github/workflows/ci.yml` (Linux X11 + Wayland, macOS, Windows,
    cross-target type check). `tests/ui.rs` skips without a display on
    Linux and reports, without failing, pixel differences on
    macOS/Windows (`OMARCHY_FLTK_UI_STRICT=1` makes them fatal).
- `native::pixels_per_unit(&widget)`: device pixels per FLTK unit including
  the Wayland output scale. fltk-rs' `Window::pixels_per_unit` ignores it on
  Linux (1.0 on a 2× output), which halves the resolution of images sized
  from it.
- Translations (`i18n`, docs/I18N.md): catalogs in o3's flat JSON
  (`{locale: {English source: translation}}`, `{name}` placeholders), the
  toolkit catalog `i18n/toolkit.json` in o3's 20 languages (component
  strings plus common ones) with app catalogs on top, o3's fallback order
  (regional → language, nb/nn → no, English, the key), CLDR plural forms
  as `key|category` entries next to the plain `other` key (`trn!`), RTL
  argument isolation as in o3. The language comes from the system: gettext
  variables on Linux/BSD, CoreFoundation on macOS, the Windows UI languages
  API, English otherwise; `i18n::set_locale` switches it live (the locale is
  a `State`; `i18n::text`/`map`/`live` give live Strings; `tr!`/`trn!`).
  ConfirmDialog, OperationToast, SearchableDropdown and Menu draw their own
  strings translated. New bindings: `Button::bind_tooltip`,
  `ActionButton::bind_tooltip`, `TextField::bind_placeholder`,
  `Toggle::set_title`/`set_description`/`bind_title`/`bind_description`,
  `AppWindow::set_title`/`bind_title`, `Label::left`; labels without an
  explicit alignment follow the reading direction. `examples/i18n_check`
  reports missing/extra keys, placeholder mismatches, plural gaps and
  unlisted source strings. The gallery's Preferences switches the language.
- `Button::set_text` refits a fitted button to its new natural width (an
  unfitted one too unless it was sized by hand), so translated labels fit.
- Widget bindings keep their State alive: a temporary such as
  `Label::new("").bind(&count.map(..))` used to stop updating once the
  derived State was dropped at the end of the statement.
- ConfirmDialog buttons widen for long labels (88 px minimum).
- **Rich text and Markdown** (`text` module), for chat clients (o3-code):
  - `text::Document` / `Block` / `Inline` / `Style` / `ColorRole`: blocks
    (paragraph, heading 1–6, list items bullet/ordered/task with nesting,
    quotes with GFM alert tones, code blocks with a language, rules,
    tables) of one string each with styled runs (bold, italic, strike,
    inline code, link, theme color role). Plain data and `Send`.
  - Markdown with `pulldown-cmark` 0.13.6 (MIT, no default features):
    CommonMark + GFM tables, task lists, strikethrough, alerts, autolinks
    (`<url>` and bare `https://`/`www.` URLs). `MarkdownStream::push`
    re-parses only the open tail (the last top-level block or list item
    that can still change); the result equals a one-shot parse (tested at
    every split point and byte by byte), except a reference link whose
    definition arrives after its block was committed. Code highlighting
    hook (`MarkdownOptions::highlight`) with a small built-in highlighter
    for diffs, comments, strings, numbers and keywords.
  - Layout with FLTK measurement: word wrap, CJK breaks between ideographs
    (with kinsoku), grapheme-safe breaks inside long words
    (`unicode-segmentation`, MIT/Apache), tab stops, tables with column
    alignment. Cached per block; a block that grew keeps its lines before
    the change. Long code lines wrap. Arabic/RTL runs are shaped by
    FLTK/Pango per fragment; no bidi reordering (documented).
  - `text::RichText`: virtualized view (prefix sums of block heights,
    estimates off screen, anchored scroll when estimates are corrected),
    shared smooth wheel scrolling, `stick_to_bottom`, `append_markdown` /
    `push_markdown` / `finish`, `sender()` (`Send`, coalesced to ≤ 60
    updates/s), `bind_markdown(&State<String>)`, `append_block(s)`,
    selection across blocks (drag, double/triple click, Shift+click),
    Ctrl+A / Ctrl+C (Cmd on macOS; primary selection on X11), links with
    `on_link`, hand cursor and Tab / Enter, code copy buttons, focus ring,
    `plain_text()` / `block_ranges()` for accessibility, `text_rect(word)`
    and `paint_stats()` for tests and tuning. Theme changes re-color without
    re-parsing or laying out.
  - `text::RichLabel`: one paragraph of inline Markdown at its natural size.
  - `scroll::Scroll::shift` keeps a running wheel motion when content above
    the view changes height. The theme maps the italic fonts too.
  - `examples/richtext.rs`, `examples/richtext_bench.rs` +
    `script/richtext-bench` (numbers in docs/PERFORMANCE.md: idle 0 %,
    5 000 messages parsed in ~33 ms, 1 MB/s streamed at 60 updates/s ≈ 18 %
    of a core, mostly painting), gallery and docs entries.
- **Data models** (`model`, docs/MODELS.md): `ListModel<K, T>` keeps items
  by the app's stable key with model-assigned `ItemId`s. Granular
  operations (insert, remove, remove_range, move, upsert, patch,
  patch_with, retain, sort_by, clear) and `apply(Vec<Diff>)`; listeners get
  `Changes` (inserted/removed/moved ranges in order, updated/inserted/
  removed IDs, reset, `map_index`). `sync_from(snapshot)` diffs a full
  snapshot by key: common prefix/suffix only compared, removals, minimal
  moves along a longest increasing subsequence, inserts, and replacements
  only for items that are not `==`; unchanged snapshots notify nobody.
  `bind_panel` keeps a `KeyboardPanel`'s cursor and selection by ID.
- **Views from models**: `List::from_model`, `Table::from_model` and
  `Tree::from_model` (flat model + parent key) invalidate only the visible
  rows whose item changed (fltk damage areas; clipped rows are not built),
  keep the first visible item in place when rows are inserted or removed
  above the viewport (`scroll::Scroll::shift` keeps a running wheel
  animation), and keep selection by key. `invalidations()`,
  `scroll_offset`/`set_scroll_offset` on the three views,
  `List::selected_id`. Count-based constructors are unchanged.
- **Batching**: `state::batch(|| ..)` (re-exported as `batch`) and
  `ListModel::transaction` notify each State and model once at the end.
  `task::drain` (the app loop) runs posted jobs inside one batch, so a
  burst of updates is one notification and one paint.
- **State**: `set_if_changed`, in-place `modify(|v| -> Option<Edit>)`,
  `subscribe_edit` / `watch_edit` with the `Edit` hint
  (`Appended { from }` / `Replaced`, mergeable with `Edit::then`),
  `State<String>::push_str` and `set_text` (appends when the new text
  extends the old one), `State<Vec<T>>::push` / `extend`. Listeners read
  the value in place instead of a clone per notification.
- **Thread feeds**: `ListModel::feed()` (`model::Feed`: diffs from any
  thread, applied once per loop iteration, snapshots drop queued diffs,
  optional `max_rate(hz)` via one fltk timeout), `State::feed()`
  (`StateFeed`, latest value wins) and `State<String>::text_feed()`
  (`TextFeed`, chunks joined into one append); `max_rate` on all three.
  No timer while idle.
- `examples/stream_bench.rs` + `script/stream-bench`: o3-style load
  (2 000 threads, ~1 MB/s of text) naive snapshots vs `sync_from` vs
  `Feed`; results in docs/PERFORMANCE.md (1200×800, 100 events/s: UI
  thread 18.4 % → 11.1 % with `sync_from`, 6.7 % with feeds at 60 Hz).
- **Accessibility (`a11y` feature, off by default, needs Rust 1.85).**
  Screen readers can read and drive apps through AccessKit 0.25: AT-SPI
  on Linux/BSD (Orca), UI Automation on Windows (NVDA, Narrator),
  NSAccessibility on macOS (VoiceOver).
  - Every Omarchy component describes itself: role, name from its visible
    text or tooltip, value, states and actions.
  - List, Table, Tree and keyboard panels expose the rows in view (plus
    the cursor row) with their position in the whole set.
  - Menus, popups, dialogs and toasts appear in the tree while open.
  - Focus follows the keyboard router.
  - AT actions (click, focus, set value, increment/decrement, expand,
    collapse, scroll into view, text selection) run the keyboard code
    paths.
  - Plain fltk widgets get a generic node by type.
  - Costs nothing until an AT activates. Then the tree is built in the
    existing pre-flush check, and later updates send only changed nodes.
  - New API: `a11y::{Accessible, register, describe, on_action, set_name,
    set_description, changed, announce, is_active, snapshot, install}`,
    `AccessibleExt::accessible_name/description` (also inherent on
    `Button` and `ActionButton`), and `KeyboardPanel::accessible_labels`,
    `item_label` and `current_layout`.
  - `OMARCHY_FLTK_A11Y=off` keeps the adapters off;
    `OMARCHY_FLTK_A11Y_DEBUG=1` prints update sizes and build times.
  - Measured on `bench_minimal`: +2.0 MB binary, +1.8 MB PSS and 3
    threads while no AT is active, startup unchanged, 0 idle CPU.
  - Verified on Linux by unit tests, `tests/ui.rs` (trees fed to
    AccessKit's consumer) and `script/a11y-check` (real AT-SPI, 20/20).
    Windows and macOS type-checked only.
  - Guide: docs/ACCESSIBILITY.md.

- **RichText memory: 1.6–1.8 bytes per byte of streamed Markdown instead
  of 5.4.** Blocks live in a packed store of four arenas shared by the
  document (text; 4-byte runs with style bits and link marks, 8 bytes in
  blocks of 4 MiB or more; per-block URLs, list numbers, code languages
  and table cells as bytes; 16 bytes of metadata per block) instead of a
  `Block` with its own `String` and `Vec<Run>` each. Blocks near the view
  are materialized with their layout in the bounded cache, as before.
  9.75 MB streamed at 1 MB/s grows PSS by 15.6–17.7 MB (was 52.7–53.1
  MB); 5 000 messages idle at 16.1 MB PSS (was 26.2 MB); push and paint
  times are unchanged (alternating A/B runs; docs/PERFORMANCE.md). Public types (`Block`, `Inline`, `Run`,
  `Style`) are unchanged.
- **RichText and RichLabel are accessible** (feature `a11y`): a
  `RichText` is a read-only Document whose virtual children are the
  blocks near the view (Paragraph, Heading with level, List/ListItem with
  "n of m" and task state, Code with a Copy button, Table/Row/Cell/
  ColumnHeader), each with its text as name and one text run per laid out
  line, and Link children with their URL. Click on a link or the Copy
  button runs the Enter path; Focus and ScrollIntoView scroll the view;
  the selection is exposed and can be set by the AT. Children are in
  content coordinates under a scrolled Document, so a scroll changes one
  node and a streamed token changes the last block's nodes. A `RichLabel`
  is a Label with Link children. Verified with `tests/ui.rs` (AccessKit's
  consumer) and the real AT-SPI check (33/33, `script/a11y-check`);
  `script/richtext-a11y-bench` measures streaming with an AT active.
- `a11y`: `Node::url`, `Node::scroll(y, max)`, `Node::text_run(key, line,
  bounds)`, `Node::text_selection_in(..)`, `Role::Code`, `Role::Table`, and
  `Value::TextRange` for selections over runs of virtual children.
- `a11y` fix: positions were sent 1-based although AccessKit's
  `position_in_set` is zero-based, and the set size only on the items
  (AccessKit reads it from the container). AT-SPI and UIA reported list,
  tab, menu and tree positions one too high and without a set size.
- RichText: Space on a focused code copy button clears the check mark
  after 1.5 s, like Enter.

- Glassy windows: `glass` follows Omarchy's window transparency
  (Appearance > Windows > Window Opacity: opaque, transparent, glassy) from
  `$XDG_STATE_HOME/omarchy/toggles/hypr`, live through `ThemeWatcher`.
  Under Glassy, surfaces painted with `glass::fill`/`glass::surface` get the
  system glass opacity (the theme's alacritty `[window] opacity`, else
  Omarchy's 0.65) while text, glyphs and images stay opaque; the window's
  Wayland opaque region is cleared so Hyprland blurs what is behind, and
  restored when leaving Glassy. `AppWindow` and its header use it. X11 and
  RGB24 image/offscreen surfaces stay opaque. No GPU feature needed: the
  compositor blurs. `OMARCHY_FLTK_GLASS=off|on|<opacity>` overrides it.
  `examples/glass_check.rs` reads the real Wayland buffer alpha.

- Overlay redraw protection now initializes automatically when any layer opens,
  including consumers using plain FLTK rather than App::new or focus::install.
  The previous fix incorrectly depended on keyboard runtime initialization.
  The regression example now deliberately uses plain FLTK.

- Overlays remain visible on plain FLTK windows when a custom canvas or
  another child repaints. The existing pre-flush check redraws the window
  stack only while it is dirty and has an open overlay; no idle timer.
  `examples/popup_damage.rs` checks the mapped framebuffer, release,
  repeated canvas updates, idle paint count and closing cleanup.
  Explicit flushes outside the event loop use `fltk_omarchy::app::flush`.
- Shared `scroll::Scroll` pixel motion: full signed wheel magnitude,
  120 ms easing, immediate reversal, bounds and cancellation on external
  navigation. List, Table, Tree and PopupMenu use the same implementation;
  other row-based popup wheel handlers also preserve delta magnitude.
  Animations-disabled mode remains immediate. This does not manufacture
  touchpad momentum from FLTK's integer events without gesture phases.

## 0.0.10 — 2026-10-28

For Files (issue #8):
- **Extended theme colors.** `Palette::darker_background`,
  `lighter_background` and `bright_foreground` come from `colors.toml`,
  validated with the rest of the theme. Defaults apply when a key is
  absent: darker is black over the background at 45 % (dark) / 10 %
  (light); lighter is white at 8 % (dark) / black at 5 % (light); bright
  is the foreground. A failed reload keeps the whole last valid palette.
  The three names also work as `shell.toml` roles. Documented in
  docs/COMPONENTS.md ("Theme colors for custom painting").
- `Palette` has three new public fields. Code that builds a `Palette`
  with a struct literal must set them; reading the palette is unchanged.

## 0.0.9 — 2026-10-28

For Files (issue #7):
- **Keyboard panel focus events.** `focus::on_panel_focus(|e| ..)` (with
  `remove_panel_focus`) and `KeyboardPanel::on_focus(|focused| ..)`. They
  report every change of the focused panel, with the panel handle and a
  cause: `Tab`, `ShiftTab`, `Focus`, `HostChanged`, `Overlay`, `Restored`,
  `Other`. Events are synchronous, sent before the next event and before
  the repaint, one at a time even when a listener moves focus, and only on
  real changes. Guarantees are in docs/KEYBOARD.md ("Panel focus events").
- `focus::active_panel_of(&host)`: a host's active pane even while focus is
  elsewhere.

Fix:
- Tabbing into a panel host from an ordinary control did not repaint the
  host when its active pane stayed the same, so the focus ring could be
  stale. Hosts now repaint on every panel focus change.

## 0.0.8 — 2026-10-28

For Files (issue #6):
- **App shortcuts work while a popup is open.** In PopupMenu, Dropdown,
  SearchableDropdown, Menu and ShortcutHelp, a key the popup does not use
  closes it and then goes on to the app: F1, F10, `Ctrl+,` reach
  `AppWindow::shortcut` once. PopupMenu reports `Dismiss::Closed`, and
  focus goes back first. Esc keeps `Dismiss::Escape`, and Tab stays
  trapped. Popups no longer swallow `Event::Shortcut`. Policy in
  docs/KEYBOARD.md ("Popups and app shortcuts").
- **`j` / `k` navigate only with no modifier.** `Ctrl+j`, `Alt+k` and
  `Shift+j` are app keys. Arrows, Enter, Home/End and PageUp/PageDown with
  Ctrl / Alt / Super are app keys too.
- ConfirmDialog still keeps the keyboard until answered (documented).

Fix:
- Closing an overlay layer from its own key handler freed the layer while
  fltk was still using it (`app::delete_widget` deletes right away). Layers
  now use fltk's deferred `Fl::delete_widget`.

## 0.0.7 — 2026-10-28

For Files (issues #2, #3):
- **TabBar overflow.** ‹ › arrows, wheel scrolling, and the selected tab
  (close glyph included) kept in view; `overflowing`, `scroll_offset`,
  `tab_rect`.
- **ShortcutHelp.** Home/End, PageUp/PageDown, scrolling bounded to the
  content, a scrollbar, and a handle with `scroll`/`max_scroll`.
  **Breaking:** `open` returns the handle (single-expression closures need
  a `;`).
- **OperationToast.** `labels(..)` for localization,
  `set_indeterminate`, the card follows the window corner on resize, the
  buttons fit their labels, and `rect()`.
- **PopupCard.** Any widgets in an anchored, clamped card. Esc, a click
  outside or `close()`; Tab stays inside (`focus::push_modal`) and focus
  comes back.
- **`table::Table` and `table::Tree`**, virtual and on a KeyboardPanel:
  - multi-selection and cursor by ID;
  - Table: header sort, resizable columns;
  - Tree: lazy, cached children; Right/Left expand and collapse; chevron
    click.
- **Native window gestures** (`native::begin_move` / `begin_resize`):
  the libdecor/xdg_toplevel interactive move and resize, with the real
  press serial, no compositor IPC. `AppWindow` uses them for the title
  drag and the corner grips, with Hyprland IPC as a fallback. Validated
  with a real mouse.

Fixes found by testing with real input:
- **The first real click on a button panicked** (a re-entrant `RefCell`
  borrow in `take_focus`).
- **Partial redraws erased controls and the content behind menus.**
  Custom-drawn groups no longer run cfltk's pre-draw, and the window
  redraws the children that overlap the damaged area, in stacking order.
- **A Hyprland window opened floating could not be tiled or resized** (it
  was mapped with a fixed size). It now maps normally and is floated right
  away.
- `TextField`'s input paints its own background: fixes overlapping glyphs
  on partial redraws.
- `anim` no longer touches widgets deleted mid-transition.

Also: `examples/playground.rs` (every component, actions logged),
`examples/redraw_check.rs`, `examples/mode_check.rs`, `examples/os_*`,
`script/os-mouse-check`.

## 0.0.6 — 2026-10-28

One design, the toolkit picks how to paint:
- **`gfx::sl` portable shaders.** Built once from Rust functions
  (`Shader::new(|p, u| ..)`). The toolkit runs them as generated GLSL on
  the GPU or with a row interpreter on the CPU, with the same result: the
  integer-hash noise matches on both, and a real-window read-back matched
  the CPU evaluation exactly.
- **`View` picks the backend.** It uses the GPU when the app is built with
  `--features gpu`, the shader is portable and the GL layer starts;
  otherwise the CPU. The GL layer starts once the window is on an output.
  It falls back to the CPU on a GL error and says why (`backend_reason`).
  To force one: `.backend(..)`, `gfx::set_backend`, or
  `OMARCHY_FLTK_RENDER`. On the GPU, `paint()` and the focus ring are
  uploaded as an overlay texture only when they change.
- **`Rate::Display`** follows the refresh rate of the window's monitor
  (Hyprland, re-checked every 3 s; 60 Hz fallback). Measured at 120.0 fps
  on the 120 Hz panel, using 2.1–2.5 % CPU. On the CPU it is capped at 30
  fps by default (`gfx::set_cpu_fps_cap`).
- Built-ins `gfx::shaders::{noise, gradient, accent_glow, blobs, rings}`
  are portable now; the closure versions moved to `gfx::shaders::cpu`.

Breaking:
- `View::shader` takes an `sl::Shader`; Rust closures go to
  `View::cpu_shader` (always CPU).
- `View` derefs to a `Group` (it may hold its GL layer).
- `animate` takes `impl Into<Rate>`; `animate(30)` still works.

Also: `focus::control_group` (a group that is one Tab stop) and
`hyprland::refresh_hz_for`.

## 0.0.5 — 2026-10-28

- **Control transitions** (`anim`) with Omarchy's durations and curves:
  - Button 120 ms; ActionButton and rows 60 ms; Toggle 100 ms;
  - ToggleSwitch 120 ms color, with the knob on OutCubic;
  - Slider position 140 ms OutCubic (none while dragging), knob scale
    110 ms.

  One shared 60 fps timer redraws only the widgets mid-transition. Theme
  changes repaint at once. Turn them off with `anim::set_enabled(false)` or
  `OMARCHY_FLTK_ANIMATIONS=0`. `paint::control_look`, `draw_look` and
  `anim::{Anim, Look, Ease}` are available for custom widgets.
- **Multi-core CPU shaders**: `View::threads(n)` (bands of rows,
  `std::thread::scope`). Shaders are now `Send + Sync`.
- **GPU shaders** behind the optional `gpu` feature, **off by default**:
  - `gfx::gpu::GpuView` (GLSL `vec4 shade(vec2 p)` with the theme uniforms,
    `animate`, `set_params`, `last_error`, `read_pixel`);
  - the feature enables fltk's `enable-glwindow` and `glow`.

  Without the feature, fltk-sys features are unchanged.
- **Real keyboard validated** through Hyprland: `script/os-input-check`,
  `examples/os_input_check`, results in docs/KEYBOARD.md.
- Measurements: `script/gpu-bench` (GPU vs CPU, 1 and 4 threads); data in
  `docs/gpu-bench-2026-10-28.jsonl`.

Notes:
- FLTK's Wayland GL subwindow blocks until its window is on an output.
  `GpuView` waits for its window before showing itself, but an app that
  starts on an unseen workspace still waits until it is viewed.
- **Breaking:** `View::shader` requires `Send + Sync` (closures that
  capture nothing qualify).

## 0.0.4 — 2026-10-28

Custom shapes and CPU shaders (`gfx`, software only; fltk features
unchanged):
- `Path`:
  - lines, quadratic and cubic Béziers, arcs, polygons;
  - rectangles with a radius per corner, circles, ellipses, superellipses
    (squircles);
  - translate, scale, rotate;
  - `contains` and `near_outline` for hit-testing.
- `Canvas`:
  - anti-aliased `fill` and `stroke`, with width, joins (miter, round,
    bevel) and caps (butt, round, square);
  - `Paint`: solid, linear, radial and conic gradients;
  - `clip` by shape and `layer` with group opacity;
  - `shadow` and `blur`, limited to 32 px;
  - `BlendMode`: Normal, Multiply, Screen, Add, Darken, Lighten.

  Pixels are premultiplied RGBA8.
- `Shader`: a per-pixel function with `Uniforms` (time, frame, size,
  pointer, theme palette, four params). Examples: `noise`, `gradient`,
  `accent_glow`, `blobs` (for the background-blur demo), plus
  `value_noise`, `fbm` and `hash2`.
- `View` widget:
  - `paint`, `shader`, `post`, `animate(fps)`;
  - `max_area`: large views render at a lower resolution and are scaled
    up;
  - `budget_ms`: tiled rendering continues on the next draw;
  - reused buffers;
  - animation paused while hidden;
  - `hit_shape` for non-rectangular widgets, and `on_click` with keyboard
    focus and a ring along the shape;
  - `stats()`.
- `examples/shaders.rs`, `examples/gfx_bench.rs`, `script/gfx-bench`,
  docs/SHADERS.md with pictures and the measured cost.

Fix: a `Label` bound before `fit` (`Label::new("").bind(..)`) kept the
width of the empty text and drew nothing. `layout::refit` now returns
whether the widget was fitted.

## 0.0.3 — 2026-10-28

For Files (issues #1, #3, #4, #5):
- **PopupMenu fits the window** (#1). A menu taller than the window is
  clamped to it and scrolls with the wheel and with Up/Down, PageUp/PageDown
  and Home/End, keeping the active entry visible. It opens below the anchor,
  or above it when there is more room. Wide labels end in "…" and keep
  their shortcut.
- **PopupMenu lifecycle** (#4). `open`/`open_at` return a `PopupHandle`:
  - `.on_dismiss(|Dismiss|)` reports `Picked(i)`, `Escape`, `ClickOutside`
    or `Closed`.
  - `.close()` closes it without picking.
  - `.is_open()`, `.dismissed()` and `.rect()` report its state.

  Focus still comes back to the control that had it. If the window is
  deleted with the menu open, no callback runs. **Breaking:** a closure that
  returned the old `()` from `PopupMenu::open(..)` now needs a `;`.
- **Keyboard panels** (#3, `panel::KeyboardPanel`, `focus::host`). One
  canvas can hold several Tab stops, each with a virtual cursor and a
  selection kept by stable IDs:
  - List and grid navigation.
  - Ctrl (cursor only), Shift (range) and Ctrl+Shift (add a range).
  - Ctrl+Space and Ctrl+A; select a range, invert, clear.
  - `on_activate`, `on_edge`, `on_key`, `on_change`.

  The focused panel sees keys first. Keys it does not use (an app's own
  Ctrl+Tab) are handled once.
- **Validated theme reload** (#5). `Theme::try_load()` and a strict TOML
  parser (`toml::parse_strict`). `reload()` keeps the last valid theme when a
  file is cut, malformed, missing required colors or mid-rename, and
  `theme::last_error()` says why. The built-in fallback still loads when
  Omarchy is not installed.
- `focus::click_at(&win, x, y)` sends a click through fltk's path (tests).

Fix: the natural-size registry of `fit` was keyed by widget address. A new
widget at a reused address could inherit a deleted widget's size. Entries
are now checked against the widget's deletion tracker.

## 0.0.2 — 2026-10-28

Keyboard first:
- One keyboard router installed by `App::new` (`focus` module). Tab and
  Shift+Tab move through every control in layout order and wrap. Arrows go
  only to the focused control and its arrow scopes (`focus::scope`); fltk's
  per-group arrow navigation is off.
- Menus, dialogs and popups give focus back to the control that opened
  them. A modal layer keeps Tab.
- New keyboard support:
  - TabBar: focusable, Left/Right/Home/End, Delete closes, and
    Ctrl+Tab / Ctrl+Shift+Tab / Ctrl+PageUp / Ctrl+PageDown from anywhere.
  - ButtonGroup: one Tab stop, Left/Right.
  - OperationToast: Tab reaches it, Left/Right/Enter/Space/Esc, and
    `focus()`.
  - Notification: Tab reaches it, Enter/Space/Delete/Esc dismiss it.
- The List draws a focus ring on the cursor row.
- The focus ring is drawn only while the keyboard is in use
  (`focus::keyboard_mode`). New `FocusRing::{Accent, Theme}`
  (`theme::set_focus_ring`). The accent ring is used when the theme's focus
  looks like its hover cursor, which is the Omarchy default.
- `focus::press("Tab")` sends a key through the real path (tests, scripted
  tours).

Fixes:
- Component bindings now belong to the fltk widget, not the Rust handle
  (`state::bind_widget`):
  - `row.fit(&*ButtonGroup::new(..))` used to stop updating once the
    temporary was dropped.
  - Label and Button bindings used to leak through an Rc cycle.
- Theme listeners of deleted windows are removed (`theme::on_change_while`).
  Restyling a deleted window used to panic.
- A Dropdown popup that loses focus to another layer now closes on the next
  loop turn. Closing it inside fltk's focus change used to crash.
- Closing a layer twice is a no-op.
- The declared minimum Rust version (1.80) really builds: `Option::is_none_or`
  (1.82) is replaced.

Docs: docs/COMPONENTS.md with a picture, code and keys per component
(`examples/docs_shots.rs`), docs/KEYBOARD.md, and losslessly recompressed
PNGs.

## 0.0.1 — 2026-10-27 (unreleased)

Added after review of the Files coordination thread: interior corner
preference (omarchy/square/rounded, fixed badge radius), `Mode::Fullscreen`
and mode parsing, PopupMenu (anchored and context), SearchableDropdown,
TabBar, ShortcutHelp, OperationToast; fltk features aligned with Files
(`use-wayland`, `no-images`).

First version: Omarchy theme (colors.toml, shell.toml, user override,
Hyprland tokens, live reload), classic components, overlays, virtual list,
AppWindow with normal/floating modes, State, tasks, keys, layout helpers,
offscreen snapshots, UI tests, real-window scenario and QML comparison.
