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
   widget per item.
7. Actions go in the header (`win.header().action(glyph::SETTINGS, "Preferences", ..)`);
   preferences go in their own window or section. Icons are Nerd Font
   glyphs (`widgets::glyph`).
8. Use theme colors (`theme::with(|t| t.palette.accent)`), never hard-coded
   hex, so `omarchy-theme-set` restyles the app live.
9. Preferences and Actions are separate menus: a gear (`Ctrl+,`) and a
   bolt (`F10`) header action, each opening a `PopupMenu`. Offer
   `theme::set_corners(..)` in Preferences; use `badge_radius()` for thumbnails.
10. Keyboard first: every component is usable from the keyboard already
   (Tab/Shift+Tab between controls, arrows inside the focused one, Enter or
   Space to activate, Esc closes menus and dialogs and focus comes back).
   Do not write your own Tab handling. For your own group of controls use
   `focus::scope(&group, |key| ..)`. A custom widget must accept
   `Event::Focus` and draw its ring when `focus::show_ring(w)`. Give every
   action a shortcut (`Button::shortcut`, `AppWindow::shortcut`) and list
   them in the F1 `ShortcutHelp`.
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
    Control transitions follow Omarchy durations automatically; tests and
    screenshots that need final states call `anim::set_enabled(false)`.
12. Verify visually: `win.snapshot("shot.png")` renders offscreen without
   showing a window; look at the PNG. Verify keys with
   `focus::press("Tab")`, `press("Down")`, `press("Enter")`… (same path as
   a real key).

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
  input tools on the user's focused window.
- Build in this checkout (`target/`), never in /tmp. Versions move 0.0.x;
  bumping the first two numbers needs the owner's approval. No AI
  attribution in commits or docs.
