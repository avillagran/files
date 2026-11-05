# Platforms

lib-fltk-omarchy builds on **Linux (Wayland and X11), macOS and Windows**.
Everything that depends on the operating system lives in one module,
`src/platform/` (`fltk_omarchy::platform`), with a working fallback on
each OS. The rest of the crate is portable and has no
`#[cfg(target_os = ..)]`. The public API is the same everywhere; on Linux
it behaves exactly as before the platform layer.

## What works where

| Feature | Linux Wayland | Linux X11 / XWayland | macOS | Windows |
|---|---|---|---|---|
| Components, layouts, State, tasks, lists, gfx, snapshots | yes | yes | yes | yes |
| Keyboard routing, focus, shortcuts | yes | yes | yes, primary modifier is Cmd | yes |
| Omarchy theme files (`colors.toml`, `shell.toml`) | `~/.local/state/omarchy/current/theme` | same | `~/Library/Application Support/omarchy/current/theme` if present | `%LOCALAPPDATA%\omarchy\current\theme` if present |
| Built-in theme when no files exist | Omarchy dark | Omarchy dark | dark or light, following the system appearance | dark or light, following "app mode" |
| Theme directory from code/env | `theme::set_theme_dir`, `OMARCHY_FLTK_THEME_DIR` | same | same | same |
| Live theme reload | inotify, blocking (no CPU while idle) | same | 1 s mtime poll + appearance | 1 s mtime poll + appearance |
| Hyprland rounding/gaps, IPC | when Hyprland runs | when Hyprland runs | never | never |
| Default font | fontconfig `monospace` (follows `omarchy-font-set`) | same | Menlo | Consolas |
| `Mode::Floating` header drag / corner resize | compositor (`xdg_toplevel.move/resize` via libdecor) | Hyprland IPC, else FLTK moves/resizes the window | FLTK moves/resizes the window | FLTK moves/resizes the window |
| `Mode::Floating` at map | Hyprland IPC | Hyprland IPC | the OS window is already floating | same |
| Glassy windows (`glass`) | translucent surfaces, Hyprland blurs | opaque | opaque | opaque |
| `native::pixels_per_unit` | FLTK scale × Wayland output scale | FLTK scale | FLTK's value (Retina included) | FLTK's value |
| Config/state/cache dirs (`platform::dirs`) | XDG | XDG | `~/Library/Application Support`, `~/Library/Caches` | `%APPDATA%`, `%LOCALAPPDATA%` |
| GPU shaders (`gpu` feature) | yes | yes | builds (OpenGL through FLTK) | builds (OpenGL through FLTK) |
| Animations on/off (`platform::animations`, `anim`) | Hyprland `animations:enabled` (live: event socket), else GNOME `enable-animations` (`gsettings`) | same | `NSWorkspace.accessibilityDisplayShouldReduceMotion` | `SPI_GETCLIENTAREAANIMATION` |

"FLTK moves/resizes the window" is the portable fallback in `AppWindow`:
the header drag sets the window position from the pointer's root
coordinates, the bottom corners resize it (not below the window's minimum
size). The window manager's own title bar works as usual on macOS and
Windows.

## The platform module

| Item | What it gives |
|---|---|
| `platform::OS`, `Os` | the OS of this build (`Linux`, `MacOs`, `Windows`, `Other`) |
| `platform::wayland()` | FLTK runs on its native Wayland backend |
| `platform::animations()` | the OS motion setting (`Some(false)`: reduce motion); read by `anim` on its watcher thread (docs/ANIMATION.md) |
| `platform::prefers_dark()` | the system appearance: `Some(true)` dark, `Some(false)` light, `None` unknown (Linux: the Omarchy theme decides) |
| `platform::process_cpu_time()` | this process's CPU time (benchmarks) |
| `platform::dirs::{home, config, data, state, cache, app_config, app_data, app_state, app_cache}` | per-OS user directories, no dependencies |
| `platform::fonts::{default_family, faces}` | default UI family and the FLTK face names (regular, bold) for a family |
| `keys::PRIMARY`, `keys::primary_held()`, `keys::label(spec)` | the primary modifier (Cmd on macOS, Ctrl elsewhere) and a spec shown the platform's way |

Backends: `src/platform/linux.rs` (FLTK Wayland internals, Hyprland Unix
socket, inotify), `macos.rs` (appearance through CoreFoundation),
`windows.rs` (appearance from the registry, CPU time from
`GetProcessTimes`), `generic.rs` (the fallbacks shared by every non-Linux
OS), `poll.rs` (the portable theme watcher), `other.rs` (other Unixes).
Existing modules keep their API and delegate: `native`, `glass`,
`hyprland`, `app::ThemeWatcher`, `theme`.

### Shortcuts

Write `Mod` for the primary modifier (`Primary` and `CmdOrCtrl` also
work): `win.shortcut("Mod+,", ..)` is Preferences on every OS (Ctrl+, on
Linux and Windows, Cmd+, on macOS). `Ctrl` always means the Control key,
so existing specs keep working unchanged. Popup menu hints
(`PopupItem::shortcut`) and the F1 `ShortcutHelp` show specs through
`keys::label`: on Linux and Windows a spec is shown exactly as written,
with `Mod` as `Ctrl`; on macOS with the system symbols (`⌘,`, `⇧⌘P`,
`⌃Tab`).

Keyboard panels (`panel::KeyboardPanel`, `table::Table`, `table::Tree`)
use the primary modifier for toggle (`Mod+Space`), select all (`Mod+A`),
cursor-only moves and click-to-toggle. TabBar keeps Ctrl+Tab on every OS
(macOS uses Ctrl+Tab for tabs too).

### Theme

On macOS and Windows there are no Omarchy files, so the built-in theme is
used: `theme::FALLBACK_COLORS` (Omarchy's default dark, Tokyo Night) or
`theme::FALLBACK_LIGHT_COLORS` (Tokyo Night Day) when the system asks for
light windows. `OMARCHY_FLTK_APPEARANCE=light|dark` forces one. An
Omarchy theme directory (a copy of `~/.local/share/omarchy/themes/<name>`)
can be used on any OS with `theme::set_theme_dir(Some(dir))` or
`OMARCHY_FLTK_THEME_DIR`, and is followed live. `OMARCHY_FLTK_STATE_DIR`,
`OMARCHY_FLTK_USER_SHELL` and `OMARCHY_FLTK_FONT` work everywhere.

## Building

The fltk dependency is declared per target in `Cargo.toml`: `use-wayland`
(FLTK's hybrid Wayland + X11 backend) is enabled only on Linux; macOS and
Windows use FLTK's Cocoa and WinAPI backends. Cargo merges the features of
the same dependency, so there is exactly one fltk-sys build per target,
and the `gpu` feature adds `enable-glwindow` on every OS.

| OS | Needs |
|---|---|
| Linux | cmake, a C++ compiler, and the X11, Wayland, xkbcommon, pango/cairo, OpenGL development files (see `.github/workflows/ci.yml` for the Ubuntu package list) |
| macOS | Xcode command line tools and cmake |
| Windows | Visual Studio (MSVC) build tools and cmake |

A dependent crate repeats the fltk pin. To keep the same features on every
OS:

```toml
[dependencies]
fltk = { version = "=1.5.23", features = ["no-images"] }
lib-fltk-omarchy = { git = "https://github.com/avillagran/lib-fltk-omarchy", tag = "v0.0.10" }

[target.'cfg(target_os = "linux")'.dependencies]
fltk = { version = "=1.5.23", features = ["use-wayland", "no-images"] }
```

The crate's Rust code can be type-checked for another OS from Linux
without building FLTK: fltk-sys skips its C++ build when `CARGO_CHECK` is
set.

```sh
rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin
CARGO_CHECK=1 cargo check --target x86_64-pc-windows-msvc --all-targets
CARGO_CHECK=1 cargo check --target aarch64-apple-darwin --all-targets --features gpu
```

## Tests per OS

- `cargo test` runs unit, doc and UI tests. `tests/ui.rs` needs a display
  on Linux (Wayland or X11, e.g. `xvfb-run`) and is skipped without one.
  Its pixel expectations hold on every OS; run locally on macOS and
  Windows, failures are printed but not fatal unless
  `OMARCHY_FLTK_UI_STRICT=1` (CI sets it).
- `script/x11-check` runs `examples/platform_check` in an isolated Xvfb
  display with FLTK's X11 backend and drags its header and corner with
  XTest (only on that Xvfb display): the window must follow through the
  FLTK fallback.
- `examples/platform_check` prints what the platform layer detected (OS,
  Wayland, Hyprland, appearance, font, dirs, primary modifier) on any OS.
- `script/real-window`, `examples/glass_check` and the bench scripts are
  Linux + Hyprland tools.

## CI (GitHub Actions, 2026-11-04)

`.github/workflows/ci.yml` passes on every job: Linux X11 (Xvfb), Linux
Wayland (headless sway: FLTK 1.5.23 crashed in its `output_done` callback
under weston 14 `--backend=headless`), macOS (arm64), Windows (MSVC) and the
cross-target type check. Unit, doc and every UI check pass on all of them,
strict on macOS and Windows, with and without `gpu` and `a11y`. The first
real runs found: FLTK's C++ event statics need the MSVC name mangling on
Windows (`platform::fl_abi`), tests must press `Mod+…` (Cmd on macOS),
Windows' GDI blit is ±1 per channel, Menlo/Consolas bold adds less ink.

## What was verified (2026-11-04, on main a967007)

On Linux (Omarchy, Hyprland 0.56.2):

| Check | Result |
|---|---|
| `cargo test` with a Wayland display | 41 unit, 31 UI, 9 doc: pass |
| `cargo test` under Xvfb (X11 backend) | 41 unit, 31 UI, 9 doc: pass |
| `cargo test --features gpu` (Wayland) | 41 unit, 31 UI, 10 doc: pass |
| `script/real-window` (real Wayland window, hidden workspace) | pass: Wayland backend, input, task cancel, live theme reload through inotify, Hyprland floating, 0.0012 s CPU over 3 s idle |
| `examples/glass_check` with Glassy on (real Wayland window) | pass: surfaces alpha 166 (0.65), text alpha 255, opaque region restored |
| `examples/popup_damage` under Xvfb (X11) | pass |
| `script/x11-check` (X11, no Wayland, no Hyprland) | pass: header drag moved the window by (150, 80); corner drag resized 420×300 → 480×340 |
| `CARGO_CHECK=1 cargo check/clippy --all-targets` (with and without `gpu`) for `x86_64-pc-windows-msvc`, `x86_64-pc-windows-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin` | pass, no warnings |
| `tests/ui.rs` without any display | skipped with a message, exit 0 |
| `examples/docs_shots`, main vs this change, same machine | 23 of 23 pictures byte-identical |
| Files (`~/Work/files` copy with this crate as its vendored toolkit) | `cargo check --workspace --all-targets` pass; `cargo test --workspace` 1629 passed, 0 failed |
| clippy, Linux, `--all-targets --features gpu` | same warnings per file as main; none in new code |
| `examples/bench_minimal` release size | 1 275 208 → 1 280 968 bytes (+5 760, +0.45 %) |

Not verified locally: building FLTK's C++ and linking for macOS and
Windows (no Apple SDK or MinGW/MSVC toolchain on the Linux machine), and
running anything there. The CoreFoundation, registry and
`GetProcessTimes` calls, the macOS/Windows font names and the FLTK
move/resize fallback on those OSes are type-checked only; the
`.github/workflows/ci.yml` macOS and Windows jobs build, test and run
`platform_check` there.
