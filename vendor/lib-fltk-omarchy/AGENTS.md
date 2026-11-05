# AGENTS.md — building Omarchy apps with lib-fltk-omarchy

For coding agents and humans. The crate is **fltk-rs + Omarchy**: use plain
fltk-rs when you need it, and the Omarchy components for everything with a
classic Omarchy look. Import as `fltk_omarchy`.

## Rules

1. `let app = App::new("dev.you.app");` first, on the main thread. It loads
   and live-follows the Omarchy theme. End with `app.run()`.
2. `let mut win = AppWindow::new("Title", w, h);` leaves the **body** column
   open: widgets you create next go into it. Call `win.end()` then `win.show()`.
3. Put each component in its container with `flex.fit(&*widget)` (keeps the
   Omarchy size; un-fitted widgets stretch). Nested rows:
   `let mut r = layout::row(); r.fit(&*a); r.fit(&*b); r.close_fitted();`.
   In fltk the creation order decides the parent; `fit` also moves the
   widget into the flex, so "create, then fit" is always safe.
4. Data lives in `State<T>`; bind it (`TextField::bind`, `Label::bind`,
   `Button::bind_text`, `Toggle::new(.., &state)`…). Change it with
   `state.set(v)` / `state.update(|v| ..)`. Never from a thread.
5. Background work: `Task::new(|ctx| { ctx.sleep(d)?; ctx.progress(p); Ok(v) })
   .on_progress(..).on_done(..).start()`; call `ctx.check()?`/`ctx.sleep()?`
   often so `handle.cancel()` works. `task::post(|| ..)` hands work to the UI thread.
6. Lists of any size: `List::new(&count, |i| ListRow::new(..))` — never one
   widget per item. Data that changes: rule 13.
7. Actions go in the header (`win.header().action(glyph::SETTINGS, "Preferences", ..)`);
   preferences go in their own window or section. Icons are Nerd Font
   glyphs (`widgets::glyph`).
8. Use theme colors (`theme::with(|t| t.palette.accent)`), never hard-coded
   hex, so `omarchy-theme-set` restyles the app live.
9. Preferences and Actions are separate menus: a gear (`Mod+,`: Ctrl+, or
   Cmd+, on macOS) and a bolt (`F10`) header action, each opening a `PopupMenu`. Offer
   `theme::set_corners(..)` in Preferences; use `badge_radius()` for thumbnails.
10. Keyboard first: every component is usable from the keyboard already
   (Tab/Shift+Tab between controls, arrows inside the focused one, Enter or
   Space to activate, Esc closes menus and dialogs and focus comes back).
   Do not write your own Tab handling. For your own group of controls use
   `focus::scope(&group, |key| ..)`. A custom widget must accept
   `Event::Focus` and draw its ring when `focus::show_ring(w)`. Give every
   action a shortcut (`Button::shortcut`, `AppWindow::shortcut`) and list
   them in the F1 `ShortcutHelp`. Use `Mod+…` for app shortcuts (Cmd on
   macOS, Ctrl elsewhere); `Ctrl` is always the Control key.
   A canvas that draws several regions (lists, grid, dual pane) uses
   `panel::KeyboardPanel` + `focus::host(&canvas, &[&a, &b])`: one Tab stop
   per region, cursor and selection by stable IDs, no widget per item.
11. Custom visuals: `gfx::View::new(w, h).paint(|c, u| ..)` with `gfx::Path`
    and colors from `u` (`u.accent`, `u.background`…), never hex. Animated
    shaders cost CPU (35–45 % of a core at 484×300 @ 30 fps): keep them small
    (`max_area`) and only where they add something. Write shaders once with
    `gfx::sl` (`View::shader(sl::Shader::new(|p, u| ..))`) and animate with
    `Rate::Display`: the toolkit runs them on the GPU when the app enables
    the `gpu` feature (monitor fps, ~2 % CPU, +40 MB PSS for the GL driver)
    and on the CPU otherwise (30 fps cap). Do not pick a backend unless
    there is a reason (`.backend(..)`). Guide: docs/SHADERS.md.
    Motion is automatic and follows the system's Animations setting
    (Hyprland `animations:enabled`, GNOME, macOS Reduce motion, Windows):
    controls, layout changes of toolkit rows/columns (`fixed`, show/hide;
    `layout::set_visible(&w, false)` collapses and fades), `Sidebar`
    (hover-expanding rail), popups, tabs, list/tree rows. Do not animate
    by hand; opt a container out with `FlexExt::animated(false)`. Tests and
    screenshots that need final states call `anim::set_enabled(false)`;
    intermediate frames: `anim::clock::freeze()` / `advance(..)`.
    Guide: docs/ANIMATION.md.
12. Every user-visible string goes through `i18n`: `tr!("Open")`,
   `tr!("Delete {name}?", name = f)`, `trn!("{count} files", n)` for
   counts. Bind live text so a language switch re-labels without a rebuild:
   `Button::new("").bind_text(&i18n::text("Open"))`,
   `Label::new("").bind(&i18n::map(&count, |n| trn!("{count} files", *n)))`,
   `win.bind_title(..)`, `.bind_tooltip(..)`, `i18n::bind(&w, || ..)` for the
   rest. App strings live in an o3-style catalog (`i18n::load(include_str!(..))`,
   English source keys); check it with `cargo run --example i18n_check --
   app.json --sources src`. Never build a sentence from translated pieces;
   use placeholders. Right-to-left languages (ar, he, fa, ur) mirror the
   layout and components live and swap Left/Right in mirrored controls:
   build left to right and do not mirror by hand. Keep media controls,
   timelines and code left to right with `row.direction(Direction::Ltr)` /
   `layout::set_direction(&w, Some(Direction::Ltr))`; custom widgets draw
   through `layout::Mirror::of(&w)` and handle arrows with
   `layout::logical_key`. Guide: docs/I18N.md.
13. Verify visually: `win.snapshot("shot.png")` renders offscreen without
   showing a window; look at the PNG. Verify keys with
   `focus::press("Tab")`, `press("Down")`, `press("Enter")`… (same path as
   a real key).
14. Multiplatform (Linux Wayland/X11, macOS, Windows): no OS-specific code
   (`#[cfg(target_os)]`, FFI, OS paths) outside `src/platform/`, and every
   platform function has a working fallback on every OS. Paths come from
   `platform::dirs` (never `$HOME/.config` by hand), the appearance from
   `platform::prefers_dark()`. Check other OSes with
   `CARGO_CHECK=1 cargo check --target x86_64-pc-windows-msvc --all-targets`
   (and `aarch64-apple-darwin`). Guide: docs/PLATFORMS.md. (Exception:
   Linux-only diagnostic examples such as `glass_check` gate their own FFI.)
15. Chat, Markdown and any long or streamed text → `text::RichText`, never
   a widget per message or a rebuilt label. `append_markdown(md)` adds a
   message, `push_markdown(more)` streams into the last one (O(tail)),
   `.stick_to_bottom(true)` follows the end, `view.sender()` streams from a
   worker thread (`Send`, at most 60 updates/s). Blocks built in code use
   `text::Block` + `text::Style` with `ColorRole`s, never hex. One line of
   inline Markdown (headers, rows, toasts) → `text::RichLabel`. Tests find
   words with `view.text_rect("word")`.
16. Collections → `model::ListModel` (`ListModel::new(|t| t.id.clone())`)
   shown with `List::from_model` / `Table::from_model` / `Tree::from_model`.
   Protocols send diffs (`patch`, `upsert`, `move_key`, `remove_key`) or,
   if they still send snapshots, go through `model.sync_from(snapshot)`;
   never rebuild a `State<Vec<_>>` or a view from a snapshot. Many changes:
   `model.transaction(..)` / `state::batch(..)`. From threads: `model.feed()`,
   `state.feed()`, `text.text_feed()` (never touch State from a thread).
   Streaming text: `push_str` and `subscribe_edit` (`Edit::Appended`).
   Guide: docs/MODELS.md.
17. Screen readers (feature `a11y`, docs/ACCESSIBILITY.md): **icon-only
   actions need accessible names**. Header actions and `ActionButton` use
   their tooltip; anything else without visible text gets
   `.accessible_name("..")` (`Button::icon`, `ToggleSwitch`, `Slider`,
   `NumberField`; `use fltk_omarchy::a11y::AccessibleExt`). Give
   `KeyboardPanel`s `accessible_labels(|id| ..)`. A `RichText` is already
   a Document of the blocks near the view (headings, paragraphs, list
   items, code, tables, links): name it (`.accessible_name("Conversation")`).
   A custom widget implements `a11y::Accessible` and exposes rows near the
   viewport as virtual children, never one node per item. Check names with
   `a11y::snapshot(&*win)`.
18. Chat input (prompts, messages) → `edit::Composer`; any other
   multi-line text → `edit::TextArea` (not `TextField`, not fltk's
   `TextEditor`). Bind the draft (`.bind(&draft)`, persistence is the
   app's), `.on_submit(|text| ..)` (Enter sends, Shift+Enter new line),
   `.busy(&running)` + `.on_stop(..)`, completions with
   `.trigger('@', Trigger::WordStart, |q, reply| reply.set(items))`
   (`reply.feed()` from threads; `CompletionItem::chip(data)` for atomic
   tokens), files with `.on_files(..)` and `composer.attachments()`.
   Tests: `focus::press(..)` / `focus::type_text("日本")`, then
   `composer.completion_state()`, `area.caret_rect()`.

## Minimal app

```rust
use fltk::prelude::*;
use fltk_omarchy::{layout::{self, FlexExt}, widgets::*, App, AppWindow, State};

fn main() {
    let app = App::new("dev.example.hello");
    let mut win = AppWindow::new("Hello", 420, 240);
    let mut body = win.body();
    let name = State::new(String::new());
    body.fit(&*TextField::new().placeholder("Your name").bind(&name));
    body.fit(&*Label::new("").bind(&name.map(|n| format!("Hello, {n}"))));
    let mut row = layout::row();
    let n = name.clone();
    row.fit(&*Button::new("Clear").bordered().on_click(move || n.set(String::new())));
    row.close_fitted();
    win.end();
    win.show();
    app.run();
}
```

## Working on this repository

- `cargo test --offline` runs unit tests, doctests and `tests/ui.rs`
  (main-thread UI checks with pixel assertions, fake Omarchy state dir).
- `cargo run --example gallery` / `overlays` show every component;
  `-- --snapshot <path|dir>` writes PNGs instead.
  `cargo run --example docs_shots -- docs/components` re-renders the
  pictures of docs/COMPONENTS.md; keep each snippet in that file equal to
  the code that draws it.
- `script/real-window` and `script/bench` open real windows on a hidden
  Hyprland special workspace without taking focus. Do not use synthetic
  input tools on the user's focused window. `script/x11-check` drives an
  X11 window inside its own Xvfb display only (so does
  `script/edit-input-check`, real keys into a `TextArea`);
  `script/edit-bench` measures the editor on the hidden workspace.
- `.github/workflows/ci.yml` builds and tests on Linux (X11 and Wayland),
  macOS and Windows; it runs when the owner pushes.
- Build in this checkout (`target/`), never in /tmp. Versions move 0.0.x;
  bumping the first two numbers needs the owner's approval. No AI
  attribution in commits or docs.
