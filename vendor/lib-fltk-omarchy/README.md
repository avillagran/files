# lib-fltk-omarchy

Omarchy toolkit for [fltk-rs](https://github.com/fltk-rs/fltk-rs): the
classic Omarchy components (the omarchy-shell `Ui/` kit) ready to use, with
the live Omarchy theme, Hyprland window modes and small app helpers
(reactive state, cancellable background tasks, virtual lists, offscreen
snapshots). Import as `fltk_omarchy`. License MIT. Version 0.0.10, not on
crates.io (use the git dependency below).

**Multiplatform**: Linux (Wayland and X11), macOS and Windows. The OS
specifics live in one `platform` module with a fallback on each OS
(Omarchy theme files and Hyprland on Linux; a built-in dark/light theme
following the system on macOS and Windows; `Mod+,` is Ctrl+, or Cmd+,).
What works where and what was verified: [docs/PLATFORMS.md](docs/PLATFORMS.md).

**Keyboard first**: every app built with it can be used without a mouse,
with no keyboard code in the app. Tab and Shift+Tab move through the
controls, arrows act inside the focused one, and menus give focus back when
they close. See [docs/KEYBOARD.md](docs/KEYBOARD.md).

**Motion that follows the system**: with Omarchy's Animations switch on
(Hyprland `animations:enabled`; Reduce motion on macOS, animation effects
on Windows, GNOME's setting) layout changes slide, a `Sidebar` expands
when the pointer rests on it, popups fade, tabs and list rows move, with
Omarchy's and o3's durations and no animation code in the app. Off, it
all jumps. See [docs/ANIMATION.md](docs/ANIMATION.md).

![app window](docs/components/app-window.png)

| | |
|---|---|
| ![buttons](docs/components/buttons.png) | ![text fields](docs/components/text-fields.png) |
| ![toggles](docs/components/toggles.png) | ![list](docs/components/list.png) |
| ![popup menu](docs/components/popup-menu.png) | ![searchable dropdown](docs/components/searchable-dropdown.png) |
| ![menu](docs/components/menu.png) | ![confirm dialog](docs/components/confirm-dialog.png) |
| ![operation toast](docs/components/operation-toast.png) | ![keyboard focus](docs/components/keyboard-2-tabs.png) |

Each component, with its picture, the code that draws it and its keys:
[docs/COMPONENTS.md](docs/COMPONENTS.md).

**Screen readers** (optional `a11y` feature): every component is exposed
through AccessKit to Orca (AT-SPI), NVDA/Narrator (UI Automation) and
VoiceOver. Nothing is built until an assistive technology asks. See
[docs/ACCESSIBILITY.md](docs/ACCESSIBILITY.md).

**Custom shapes and shaders** (`gfx`): write a shader once with
`gfx::sl`, and the toolkit runs it on the GPU (with the optional `gpu`
feature, at the monitor's refresh rate) or on the CPU: vector paths
with per-corner radii and squircles, gradients, shadows, blur, blend modes,
clips, non-rectangular hit-testing, and fragment-style shaders with an area
limit and a time budget per draw. Pictures, API and measured cost:
[docs/SHADERS.md](docs/SHADERS.md).

![shapes and shaders](docs/components/gfx-gallery.png)

- **Theme**: reads `~/.local/state/omarchy/current/theme/{colors,shell}.toml`
  and `~/.config/omarchy/shell.toml` with the same keys and defaults as
  omarchy-shell, Hyprland rounding/gaps, the `monospace` font; follows
  `omarchy-theme-set` live. Every `colors.toml` color, including
  `darker_background`, `lighter_background` and `bright_foreground`, comes
  from one validated load ([theme colors](docs/COMPONENTS.md#theme-colors-for-custom-painting)).
- **Rich text and Markdown** (`text`): a virtualized, selectable
  `RichText` view for chat transcripts that stream (only the open tail is
  parsed again, only blocks in view are laid out, idle is 0 % CPU) and a
  one-line `RichLabel`
  ([picture and code](docs/COMPONENTS.md#richtext-and-richlabel-markdown-chat-streaming)).
- **Text input and chat composer** (`edit`): a multi-line `TextArea`
  (wrap, auto-grow, undo, input methods, grapheme-correct editing, atomic
  chips) and a `Composer` (Enter sends, stop while busy, `@`/`$`/`/`
  completions at the caret, attachment chips, file drops)
  ([picture and code](docs/COMPONENTS.md#textarea-and-composer)).
- **Menus with submenus** (`popups::PopupMenu`): header and context menus
  group long lists into nested submenus (`PopupItem::submenu`), opened by
  hover, click or keys, beside their entry and inside the window; picks by
  path or id (`open_path`, `open_id`)
  ([picture and code](docs/COMPONENTS.md#submenus-long-menus)).
- **Components**: [docs/COMPONENTS.md](docs/COMPONENTS.md) (pictures,
  code, keys). **Keyboard**: [docs/KEYBOARD.md](docs/KEYBOARD.md).
- **Platforms**: [docs/PLATFORMS.md](docs/PLATFORMS.md).
- **Translations**: [docs/I18N.md](docs/I18N.md): o3-compatible catalogs,
  the toolkit's own strings in 20 languages, plural forms, the system
  language on Linux, macOS and Windows, live language switching, a
  catalog checker (`cargo run --example i18n_check`), right-to-left
  layouts mirrored live and bidirectional rich text
  ([Arabic](docs/I18N.md#right-to-left-languages)).
- **Data that changes**: keyed `ListModel` with granular changes,
  `sync_from` for snapshot protocols, batching and thread feeds; views
  redraw only the rows that changed and keep scroll and selection by key
  ([docs/MODELS.md](docs/MODELS.md)).
- **Capabilities vs QML**: [docs/CAPABILITIES.md](docs/CAPABILITIES.md).
- **Measured** (same session, 520×400 app with a 100 000-row list, 0.0.2):
  15.0 MB PSS and 70 ms to first frame vs 59.5 MB / 127 ms (QML software)
  and 106.9 MB / 175 ms (QML GPU); method, three sessions and limits in
  [docs/PERFORMANCE.md](docs/PERFORMANCE.md).
- **For agents**: [AGENTS.md](AGENTS.md).

```sh
cargo run --example gallery      # all components
cargo run --example overlays     # dropdown, menu, confirm, notification, list
cargo run --example composer     # chat composer: @files, $skills, /commands, drops
cargo run --example app_shell    # Preferences/Actions menus, tabs, search, F1 help, operation toast
cargo run --example docs_shots -- docs/components   # re-render the pictures of docs/COMPONENTS.md
cargo run --example shaders      # shapes, gradients, effects, animated CPU shaders
cargo run --example richtext     # Markdown chat transcript (-- --stream streams a reply)
cargo run --release --example playground   # every component in one window, actions logged to build/playground.log
cargo run --release --features gpu --example gpu_bench -- check build/gpu-check.json   # GPU shader pixel check
cargo test --offline             # unit + doc + main-thread UI tests
cargo run --example platform_check   # what the platform layer detected on this OS
script/x11-check                 # X11 window (isolated Xvfb): move/resize fallback
```

Use it from another crate (same fltk pin and features as this crate;
`use-wayland` only on Linux):

```toml
[dependencies]
fltk = { version = "=1.5.23", features = ["no-images"] }
lib-fltk-omarchy = { git = "https://github.com/avillagran/lib-fltk-omarchy", tag = "v0.0.10" }

[target.'cfg(target_os = "linux")'.dependencies]
fltk = { version = "=1.5.23", features = ["use-wayland", "no-images"] }
```

Requires the build dependencies of fltk-sys: cmake and a C++ compiler
everywhere, plus Wayland/X11/pango/cairo development files on Linux
(Xcode command line tools on macOS, MSVC build tools on Windows).
