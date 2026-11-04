# Changelog

## Unreleased

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
