# lib-fltk-omarchy

Omarchy toolkit for [fltk-rs](https://github.com/fltk-rs/fltk-rs): the
classic Omarchy components (the omarchy-shell `Ui/` kit) ready to use, with
the live Omarchy theme, Hyprland window modes and small app helpers
(reactive state, cancellable background tasks, virtual lists, offscreen
snapshots). Import as `fltk_omarchy`. License MIT. Version 0.0.10, not on
crates.io (use the git dependency below).

**Keyboard first**: every app built with it can be used without a mouse,
with no keyboard code in the app. Tab and Shift+Tab move through the
controls, arrows act inside the focused one, and menus give focus back when
they close. See [docs/KEYBOARD.md](docs/KEYBOARD.md).

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
- **Components**: [docs/COMPONENTS.md](docs/COMPONENTS.md) (pictures,
  code, keys). **Keyboard**: [docs/KEYBOARD.md](docs/KEYBOARD.md).
- **Capabilities vs QML**: [docs/CAPABILITIES.md](docs/CAPABILITIES.md).
- **Measured** (same session, 520×400 app with a 100 000-row list, 0.0.2):
  15.0 MB PSS and 70 ms to first frame vs 59.5 MB / 127 ms (QML software)
  and 106.9 MB / 175 ms (QML GPU); method, three sessions and limits in
  [docs/PERFORMANCE.md](docs/PERFORMANCE.md).
- **For agents**: [AGENTS.md](AGENTS.md).

```sh
cargo run --example gallery      # all components
cargo run --example overlays     # dropdown, menu, confirm, notification, list
cargo run --example app_shell    # Preferences/Actions menus, tabs, search, F1 help, operation toast
cargo run --example docs_shots -- docs/components   # re-render the pictures of docs/COMPONENTS.md
cargo run --example shaders      # shapes, gradients, effects, animated CPU shaders
cargo run --release --example playground   # every component in one window, actions logged to build/playground.log
cargo run --release --features gpu --example gpu_bench -- check build/gpu-check.json   # GPU shader pixel check
cargo test --offline             # unit + doc + main-thread UI tests
```

Use it from another crate (same fltk pin and features as this crate):

```toml
[dependencies]
fltk = { version = "=1.5.23", features = ["use-wayland", "no-images"] }
lib-fltk-omarchy = { git = "https://github.com/avillagran/lib-fltk-omarchy", tag = "v0.0.10" }
```

Requires the build dependencies of fltk-sys (cmake, a C++ compiler,
Wayland/X11/pango/cairo development files).
