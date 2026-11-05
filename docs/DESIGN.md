# files — design v0 (2026-10-26)

A lightweight, cross-platform file browser, initially for Omarchy.

## 1. Why the others are so heavy (measured on this machine, one window, ~/)

| App | RSS | PSS (what it really costs) | Stack |
|---|---|---|---|
| Nautilus | 221 MB | 167 MB | GTK4 (GPU render) + libadwaita + GIO/GVFS |
| Flea | 236–251 MB | 143–153 MB | **Quickshell = Qt Quick/QML** (UI) + Rust backend (`flea --backend`, 4 MB) |

Key conclusion: Flea **is not Rust in the part that weighs**. The Rust binary uses 4 MB.
The 150 MB are the QML engine (JS engine + scene graph) and the GPU context. QML is good
for visual prototyping, but not for building.

Where the MB come from in a "modern" file manager:
1. **GPU context** (Mesa/Vulkan/GL): 30–80 MB of anonymous memory just for existing.
2. **Declarative engine** (QML/JS, GTK4 render nodes, WebView): 40–100 MB.
3. **SVG icons** rasterized in 5 sizes × the whole theme at startup: 10–30 MB.
4. Resident **thumbnails** and decoders (libjpeg, librsvg, gdk-pixbuf): 10–50 MB.
5. **Non-virtualized models**: one object/widget per file, 10k files = 10k widgets.
6. Daemons/indexers (tracker, gvfsd) that the app drags along.

## 2. Target budget (PSS, 1920×1080 window, folder with 10k files)

| Subsystem | Budget |
|---|---|
| Toolkit + libs (file-backed part) | ≤ 12 MB |
| Framebuffers (2 × 1920×1080×4, software, wl_shm) | ~16 MB (unavoidable; scales with the window) |
| Directory model (10k entries × ~120 B) | ~1.5 MB |
| Icons (single atlas, rasterized for the size in use) | ≤ 2 MB |
| Thumbnail cache (LRU with a hard cap) | ≤ 8 MB, configurable, 0 if turned off |
| Fonts (FreeType, 2 faces loaded) | ~3 MB |
| Remaining heap | ~8 MB |
| **Total** | **~50 MB** ✅ · > 150 MB = heresy |

Honest note: on this machine's 2370×2122 screen the two framebuffers alone are
~40 MB. The budget is defined at 1080p and measured with an automated test.

## 3. Engineering rules (non-negotiable)

- **Software rendering** (CPU) by default. No GL/Vulkan. Repaint only the damaged area.
- **Everything virtualized**: only the visible rows exist (+1 screen of margin).
- **Lazy stat()**: `readdir` first (name + type), `stat` only for what is visible or when sorting by size/date.
- **Nothing resident that could be a child process**: thumbnails, extraction, deep search → a helper that starts, does the work, dies.
- **MIME by extension** (own table, ~300 entries); libmagic only on demand.
- **Icons**: a small own set (~48 glyphs) in a simple vector format, rasterized to the requested size and cached. The system icon theme is not loaded at startup (optional later).
- **Memory test in CI**: open ~/ with 10k entries, measure PSS, fail if > 60 MB.
- **Startup < 100 ms** cold.

## 4. Stack: options evaluated

| Option | Typical PSS | Cross-platform | Verdict |
|---|---|---|---|
| QML / Qt Quick, GTK4, Tauri, Electron, Flutter | 120–300 MB | yes | ❌ discarded because of weight |
| Qt6 Widgets (C++, no QML) | 45–70 MB | excellent, native look | ⚠️ right at the limit |
| GTK3 (C) | 35–60 MB | poor on Win/mac | ⚠️ not really cross-platform |
| **FLTK 1.4** (C++ or Rust via fltk-rs) | **10–25 MB** | Linux (X11+Wayland), Windows, macOS | ✅ **recommended** |
| SDL3 + own UI (C/Zig) + FreeType/HarfBuzz | 20–35 MB | yes (SDL3 brings clipboard, DnD, IME, native dialogs) | ✅ alternative, 3× more work |

**Recommendation: FLTK 1.4.** It provides a window, events, text (Pango/Cairo on Linux, native on
Win/mac), clipboard, DnD, HiDPI, native dialogs, and base widgets (tree, table, menus,
input) in a ~1 MB library. We draw the whole file view ourselves (as with
any option), so the Omarchy look does not depend on the toolkit.

Language: **Rust + fltk-rs** (safe FS core, threads for I/O) or **C++17 + FLTK**
(zero intermediate layers). Decision pending from the user.

## 5. Architecture

```
files/
  core/      no UI dependencies
    fs/      streaming listing, lazy stat, sort, filter, watcher (inotify/kqueue/RDCW)
    ops/     copy/move/rename/trash, queue with progress, cancellation, undo
    mime/    extension→type table, default apps (xdg / registry / LaunchServices)
    places/  XDG dirs, mounts, favorites, recents
    theme/   reads ~/.config/omarchy/current/theme (colors) and its own config (TOML)
  ui/        FLTK: window, path bar, sidebar, views, dual pane, dialogs
  helpers/   thumbnailer (separate process, RAM cap), recursive search
  tests/     memory (PSS), performance (100k files), ops
```

## 6. UI (see mockups/v1.html)

Taken from each reference:
- **Flea / Omarchy**: system theme, mono typography, Places + Devices sidebar, list/columns/grid views, status bar with FS and free space.
- **Finder**: Miller columns, clickable and editable path bar (breadcrumb), "Go to folder".
- **Windows**: quick access / frequent folders, pin favorites, a home view.
- **MC**: dual pane (F key), F1–F10 key bar, quick filter while typing, working without a mouse.
- **Nautilus**: stars/favorites, "N items" on folders, a recents view.

Views: List · Grid · Columns · Dual pane. Always virtualized.
Keyboard: `Ctrl+L` path, `/` or typing = filter, `j/k` or arrows, `Enter` open,
`Backspace` up, `F5/F6/F7/F8` copy/move/mkdir/delete, `F9` dual pane, `Tab` switches pane,
`Ctrl+1/2/3` views, `Ctrl+T` terminal here, `Space` preview (external helper).

## 7. Phases

1. **0.0.1 core + list**: open, navigate, sort, filter, sidebar, breadcrumb, Omarchy theme, PSS test.
2. **0.0.2 operations**: copy/move/trash/rename with progress and undo, DnD, clipboard, watcher.
3. **0.0.3 views**: grid, columns, dual pane, F keys.
4. **0.0.4 extras**: thumbnails (helper), search, open with, mounts, favorites.
5. **0.0.5 Windows/macOS**: native trash, default apps, installers.

---

# Addendum 0.0.1 (2026-10-26, second round)

## A. "Optimized" QML? Measured

Minimal QML app (one ListView with 10k rows, no logic), Qt 6.12, this machine:

| Mode | RSS | PSS | Anon |
|---|---|---|---|
| Qt Quick with GPU (default) | 146 MB | 90 MB | 41 MB |
| Qt Quick `QT_QUICK_BACKEND=software` | 104 MB | 51 MB | 19 MB |

That is: **empty QML is already at the 50 MB limit** with software rendering, and at
90 MB with GPU. Any real feature (icons, thumbnails, model, dialogs) takes it to
70–100 MB. Qt Quick Compiler reduces JS but does not remove the V4 engine nor the scene graph.
Conclusion: QML is **discarded as a runtime**. It is only useful for sketching.

## B. "QML without the weight" alternative: Slint (Rust)

Slint offers what people like about QML (the declarative `.slint` language, reactive properties,
live preview) but **compiles the `.slint` into native Rust** at build time: no JS
engine, no interpreter. A software renderer included. Linux (Wayland/X11), Windows, macOS.
License: GPLv3 **or** Royalty-Free (free for desktop apps, no copyleft) or commercial.

**Measured** (same test as QML: ListView with 10k rows, 900×600 window, Slint 1.x,
`backend-winit-wayland` + `renderer-software`, a 12.8 MB static binary that only links
fontconfig/freetype):

| | RSS | PSS | Anon | Shmem (framebuffers) |
|---|---|---|---|---|
| Slint software | 31 MB | **19 MB** | 4 MB | 8 MB |
| QML software | 104 MB | 51 MB | 19 MB | 7 MB |
| QML GPU | 146 MB | 90 MB | 41 MB | 0 |

Empty Slint costs **2.7× less than QML** in the same mode, and leaves ~30 MB of margin
for the real app within the 50 MB budget.

## C. Stack decision (closed proposal, waiting for your OK)

**Rust + Slint (software renderer)** as the main path:
- Rust is as cross-platform as C++ for this (winit/Wayland/X11/Win32/Cocoa through Slint).
- The pure Rust FS core (no UI) is reusable if the toolkit ever changes.
- Plan B: **Rust + fltk-rs** (10–25 MB) if Slint stumbles on something functional (DnD, IME, native dialogs).
- C++ remains an option only if Qt Widgets are wanted (45–70 MB, right at the limit).

## D. Live Omarchy theme (verified on this system)

- Canonical source: `~/.local/state/omarchy/current/theme/colors.toml`
  (it is what `omarchy-theme-color` reads). There is also `~/.config/omarchy/theme.json`
  with `{"theme":"Hackerman"}` and `~/.config/omarchy/themes/<name>/`.
- When the theme changes, Omarchy runs `~/.config/omarchy/hooks/theme-set.d/*` with the
  theme's name. **We do not need a hook**: a watcher (inotify) on the
  `current/theme` directory that rereads `colors.toml` when the symlink changes is enough. Zero daemons.
- Outside Omarchy: the same parser reads its own `colors.toml` at
  `~/.config/files/theme.toml`; on Windows/macOS the OS light/dark mode can be followed.

Semantic `colors.toml` → UI map:

| Key | Use in files |
|---|---|
| `background` | window background / content area |
| `darker_background` | rail, inspector, omnibar, toasts |
| `lighter_background` | folder cards, hover, path chips |
| `selection` | selected row/card |
| `muted` | borders (with alpha), separators |
| `foreground` / `bright_foreground` | normal text / names |
| `dark_foreground` | metadata, placeholders, inactive icons |
| `accent` | current path segment, focus, progress, primary button |
| `blue` folders · `magenta` images · `cyan` text · `green` scripts · `yellow` archives · `red` destructive |

## E. UI v2 — "outside the box" (mockups/v2.html)

What changes compared to Flea/Nautilus:
1. **A single omnibar**: the path is clickable chips, and the same field filters while typing,
   runs commands with `>` and jumps to places with `:`. The separate address bar,
   search box and breadcrumb disappear.
2. **A collapsed 52 px rail** that expands on mouse hover, instead of a fixed sidebar.
3. **Groups view** by default: folders as cards with count, size and "2 h ago";
   files grouped by date (Today / This week / Earlier) with a colored type badge.
4. **Inspector** on the right: preview, metadata and actions. It replaces the context
   menu for mouse users. `F9` turns it into a **second pane** (MC mode).
5. **Space view**: a size treemap computed by a helper that dies when it finishes.
6. **Operations as a toast** with progress, pause, cancel and undo. No modal dialogs.
7. **Shortcuts on demand** (`?`), not a permanent F bar; MC's F keys still work.
8. Proportional typography (sans), 40 px rows, 10–14 px corners. Mono only in `kbd`.

---

# Addendum 0.0.2 (2026-10-26, third round): MIT license and corners

## F. New constraint: permissive open source (MIT ideally)

Slint is out: its license is GPLv3 or a proprietary "Royalty-Free" one, neither of them permissive.
The permissive alternatives were measured with the same test (10k rows, 900×600 window):

| Stack | License | PSS | RSS | Note |
|---|---|---|---|---|
| Slint (reference, discarded) | GPLv3 / proprietary | 19 MB | 31 MB | |
| **fltk-rs** | MIT (FLTK: LGPL with a static-linking exception) | **27 MB** | 44 MB | 1.6 MB binary; draws with Cairo/Pango on Wayland |
| iced + tiny-skia | MIT | 142 MB | 159 MB | no virtualized list in the library: it instantiates 10k widgets; discarded |
| winit + softbuffer + tiny-skia + cosmic-text | MIT / Apache-2.0 / BSD-3 | 41 MB | 57 MB | 100% own UI; cosmic-text loads the system font database (~20 MB anon) and softbuffer uses 14 MB of shmem. It could be optimized, but today it loses to FLTK |
| QML software (reference) | LGPL | 51 MB | 104 MB | |

## G. Stack decision (0.0.2)

**Rust + fltk-rs**, using FLTK only as a platform layer: window, events, text,
clipboard, drag and drop, HiDPI, native dialogs. **The whole visible interface is
drawn by hand** on the FLTK canvas (own `draw()`), just as we would with any
option, so the mockup's look does not depend on FLTK's widgets.

Architecture rule: the drawing layer exposes a minimal interface (`fill_rounded_rect`,
`text`, `clip`, `image`). If FLTK gets in the way, it is swapped for tiny-skia + softbuffer without touching
the views. That is the emergency exit. Measured, the raw stack costs 41 MB today without optimizing, so FLTK is the cheapest **and** the most mature option among the permissive ones.

## H. Corners: follow Omarchy, with an own preference

- Live source: `hyprctl getoption decoration:rounding -j` → `{"int": 0}` on this machine
  (Omarchy uses 0 by default; the no-gaps toggle also forces 0). It is queried at startup
  and when the theme hook arrives; no daemons.
- In-app preference: `corners = "omarchy" | "square" | "rounded"`. "omarchy" maps
  `rounding = 0` to square and `rounding > 0` to rounded, scaling a factor `r` (0 → 1).
- Every radius in the UI is expressed as `base_px × r`: window, omnibar, path chips,
  cards, rows, buttons, toasts, progress bar.
- **Fixed exception**: thumbnails, type badges and the inspector's preview always keep
  their radius (6–10 px). The user wants them rounded even if everything else is square.

## I. Mockup v3 adjustments (mockups/v3.html)

- Smaller section headers (10 px, weight 500, less vertical space).
- Size column with tabular numerals and the unit aligned in its own sub-column,
  so that "774 kB" and "1.2 MB" line up.
- Rows 2 px more compact.
- A "Corners: Follow Omarchy / Square / Rounded" selector as a demonstration of the preference.

---

# Addendum 0.0.3 (2026-10-26, fourth round): GPU?

Same test (10k rows, 900×600), this machine (Asahi, Apple M2 AGX; hardware OpenGL
through Mesa/asahi; **no hardware Vulkan ICD installed**, only the software lavapipe):

| Stack | Render | License | PSS | RSS | Δ vs the same stack in software |
|---|---|---|---|---|---|
| fltk-rs | software (Cairo) | MIT | 27 MB | 44 MB | — |
| fltk-rs + GlWindow | OpenGL (EGL, live context, not drawing) | MIT | 42 MB | 57 MB | **+15 MB just for the GL context** |
| egui/eframe + glow | OpenGL (EGL/asahi) | MIT/Apache | 62 MB | 71 MB | |
| egui/eframe + wgpu | Vulkan → fell back to **lavapipe (CPU)** | MIT/Apache | 68 MB | 78 MB | |
| QML Qt Quick (ref.) | GPU | LGPL | 90 MB | 146 MB | +39 MB vs QML software |

Takeaways:
1. **The GPU context costs 15–40 MB before drawing a single pixel** (driver, shaders, buffer
   pools). That is half of the total budget.
2. The permissive GPU-first toolkits (egui, iced-wgpu, Floem, Xilem/Vello, GPUI, Makepad)
   start at 60–70 MB. None goes below 50 MB with an empty window.
3. wgpu is truly cross-platform (Vulkan/Metal/DX12/GL), but on a machine without
   hardware Vulkan it falls back to lavapipe: **rendering on the CPU with the GPU's RAM cost**.
4. A file manager does not need a GPU: the compositor already composites on the GPU; we
   repaint only the damaged region in software. The mockup's "animations" (hover,
   an expanding rail, toasts) are 150–250 ms interpolations that the CPU draws with ease.

**Decision 0.0.3: no GPU in v1.** The drawing layer stays abstracted (`Painter` with
`rect`, `rounded_rect`, `text`, `image`, `clip`) and an **optional** GL/wgpu backend is added
only if a real case appears (e.g. a grid of thousands of thumbnails with smooth zoom), always
enabled through a preference and never by default. v1 goal with software FLTK: ≤ 35 MB
PSS with 10k files, leaving room for thumbnails and operations.

---

# Addendum 0.0.4 (2026-10-26): optional shaders, omarchy-t3 style

Reference: `~/Work/omarchy-t3-code/ui/assets/shaders/` (GLSL compiled to `.qsb` for Qt):
- `threadaura.frag` — a per-row aura: a comet orbiting the border (working), a breathing border
  with flashes (attention), a shooting star on state changes (burst). Several styles.
- `reveal.frag` — a show/hide transition per region with `progress` 0..1: nebula,
  warp with RGB separation, accordion fold.
- `starfield.frag` — a procedural star background with parallax, twinkling and meteors.
- `activity.frag` — a Gaussian glow for activity text.

Common pattern: **a fragment shader per rectangular region**, `time/clock` uniforms,
`progress`, `style`, theme colors and the panel's coordinates; optionally a `sampler2D`
with the content already painted underneath (`source`).

## How it fits in files without paying for the GPU by default

```
trait Painter {                      // software backend (FLTK/Cairo) and GPU backend (future)
    fn rect / rounded_rect / text / image / clip ...
    fn effect(&mut self, region: Rect, id: EffectId, u: &Uniforms) -> bool; // false = not supported
    fn capture(&mut self, region: Rect) -> Option<TextureId>;             // `source` for shaders
}
```

- **Software backend**: `effect()` returns `false`. Each effect declares a cheap **CPU fallback**:
  aura → border in the accent color; reveal → fade+slide 180 ms; starfield → nothing
  (flat background); activity → no glow. The app is complete without a GPU.
- **GPU backend** (optional, preference `effects = off | on`, `off` by default):
  wgpu or GL. The whole UI painted by the CPU is uploaded as a texture once per damaged frame and
  the effects are drawn as fragment shader passes over/under that texture. The shaders
  are written in WGSL or GLSL (portable: `.frag` → `naga` translates them), and **rust-gpu is
  viable here** if writing them in Rust is preferred, isolated in the GPU backend crate.
- Expected cost when enabling effects: +15–40 MB (context). Acceptable because it is opt-in and
  shown in the status bar ("RAM 58 MB · effects").
- The same set of uniforms as omarchy-t3 (`time`, `progress`, `style`, colors from
  `colors.toml`) so `threadaura` and `reveal` can be ported almost as they are.

Planned use cases: an aura on operations in progress (a row being copied = working comet;
a conflict = attention), reveal when opening the inspector/second pane, an optional starfield in the
background of the Groups view when the folder is empty.

---

# Addendum 0.0.5 (2026-10-26): preview, network, copy, search

Cross-cutting rule: **the main process never loads decoders, network stacks, models
nor indices**. All of that lives in `files-helper <subcommand>`, a second binary in the same
repo that starts on demand, talks to the app over stdio (JSON lines + blobs through
shared memory/a temporary file) and dies when it finishes or after N idle seconds.

## J. Preview (Space)

- `Space` opens a floating panel over the window (no modal dialog); `Space` or `Esc`
  closes it; arrows navigate without closing it. The `i` key is still the fixed inspector.
- It is rendered by `files-helper preview <path>`, which delivers a bitmap through shared memory
  (or already formatted text). When it closes, the helper dies: **0 MB resident** for previewing.
- v1 types: images (`image`/`zune-*` crates, downscaled to the panel size, never the
  full image in RAM), text and code (light highlighting, first 2,000 lines), Markdown,
  PDF (page N through `mutool draw` if available, otherwise the optional `pdfium-render`), video (frame +
  duration through `ffmpeg` if available), audio (metadata), archives (listing),
  folders (count + size). Office: extracted text first; faithful rendering later.
- Cross-platform: the list of renderers is resolved at runtime according to the tools
  present; no hard dependency on any of them.

## K. Network (SMB, SFTP, WebDAV, FTP, NFS)

- A `Vfs` layer in `core`: `list`, `stat`, `open_read`, `open_write`, `rename`, `remove`,
  `mkdir`, `watch?`. Backends: `local` (in-process) and the network ones, **all in
  `files-helper net`**, so that TLS, SSH and libsmbclient do not live in the main process.
- SMB: libsmbclient through FFI (LGPL, dynamic linking; on Windows native UNC is used, on macOS
  the system's `smb://` as an alternative). SFTP: `russh` + `russh-sftp` (Apache/MIT, pure
  Rust). WebDAV: `reqwest`. FTP: `suppaftp`. NFS: a system mount, not an own client.
- Optional discovery: mDNS (`mdns-sd`) and WS-Discovery for SMB. Credentials in the
  system keyring (`keyring` crate: Secret Service / Keychain / Credential Manager).
- URLs in the omnibar: `smb://nas/fotos`, `sftp://user@host/path`. They appear in the rail under
  "Network" with a connection status. Copy operations go through the same engine (section L)
  with streaming, without downloading to a temporary file unless the destination requires it.
- No GVFS nor KIO: they are permanent daemons and Linux-only.

## L. Hyper-optimized copy

The `ops` engine in the main process (threads, not a helper): it needs fine-grained progress and immediate
cancellation; its memory is a fixed pool of buffers (max 4 × 4 MB).

Order of attempts per file:
1. **CoW cloning** when source and destination are on the same FS that supports it:
   Linux `FICLONE` (btrfs, XFS, bcachefs), macOS `clonefile`/`copyfile(COPYFILE_CLONE)`
   (APFS), Windows ReFS block clone. Instant and without using extra space.
2. **In-kernel copy** without going through user space: Linux `copy_file_range`
   (also across FSs on modern kernels), macOS `fcopyfile`, Windows `CopyFileEx`.
3. **Streaming** with 1–4 MB buffers, read-ahead and, for large files,
   optional `posix_fadvise(SEQUENTIAL)`/`O_DIRECT`; io_uring on Linux when available
   (optional feature).
- Adaptive parallelism: many small files → N workers (N = cores, cap 8);
  few large ones → one stream per destination device. Throughput is measured and adjusted.
- Sparse files (`SEEK_HOLE/SEEK_DATA`), xattrs, times, permissions, symbolic links.
- Same device → `rename` (moving is instant). Across devices → copy + optional
  verification (xxhash) + delete.
- Resumable, pausable, cancellable; a single queue visible in the toast; undo (move → move
  back; copy → delete what was copied; delete → own trash per the freedesktop spec /
  Recycle Bin / macOS Trash).
- Target metric: match `cp --reflink=auto` and beat Nautilus on trees with many
  small files (where parallelism rules).

## M. Search

Three levels, all from the omnibar with a prefix, results merged into a single list:

| Prefix | What | How | Where it runs |
|---|---|---|---|
| (none) | instant filter of the current folder | in memory, fuzzy (`nucleo`) | main |
| `/text` | names across the whole tree or the disk | own index (see below) or `plocate`/`fd` if available | `files-helper find` |
| `?text` | literal/regex content | the `ripgrep` engine (`grep` crates, MIT) with limits | `files-helper grep` |
| `~phrase` | semantic, "the notebook invoice from September" | embeddings + vector index | `files-helper semantic` |

Name index: only for roots the user marks (Home by default). A file on
disk (`~/.local/share/files/index/`), a trigram structure + path table; incremental
updates through inotify/FSEvents/ReadDirectoryChangesW while the app is open and a
rescan at startup. It is never fully loaded into RAM: mmap and paged queries.

Semantic (EmbeddingGemma):
- Model: **EmbeddingGemma 300M** (Google), GGUF format, run by `llama.cpp`
  (`llama-server --embedding` or `llama-embedding`), which is already on this machine and runs on
  Linux/macOS (Metal)/Windows. Alternative without llama.cpp: `candle`/`ort` inside the helper.
- The model **is not distributed with the app**: it is downloaded on demand when the feature is enabled
  (Gemma Terms of Use license, not MIT; explicit acceptance in Preferences).
- Pipeline in `files-helper semantic`: extract text (txt/md/code/pdf through `mutool`/
  `pdf-extract`/office through `zip`+XML), chunk into ~512 tokens with overlap, embed, store in an
  on-disk vector index (`usearch` or an own HNSW; Apache). Query: embed the phrase,
  top-k by cosine, re-rank by name/date. The indexed folders are chosen in
  Preferences; the helper dies when it finishes and the model leaves RAM with it.
- Cost: ~400–700 MB of RAM **only while indexing or querying**, in another process. The main process
  stays within its budget. It is shown in the status bar while it happens.

## N. Helpers (summary)

`files-helper {preview|thumb|du|net|find|grep|semantic}` — one binary, several processes.
Protocol: one JSON line per request/response; bitmaps and blobs through `memfd`/a temporary
file. Each helper has a RAM cap (`RLIMIT_AS` on Linux, job objects on Windows) and a
configurable idle lifetime (30 s by default).

## Phases (updated)

1. 0.0.1 core + Groups view + live theme + PSS test.
2. 0.0.2 operations with the copy engine (cloning, kernel copy, parallelism), trash, undo.
3. 0.0.3 remaining views, inspector, second pane, preview with Space (helper).
4. 0.0.4 search: filter, `/` name index, `?` content.
5. 0.0.5 network: SFTP and SMB first, WebDAV/FTP later.
6. 0.0.6 semantic with EmbeddingGemma; thumbnails; Space view.
7. 0.0.7 Windows and macOS; optional GPU effects.

---

# Versioning note (2026-10-26)

The user's global rule: versions advance slowly (0.0.1, 0.0.2, …, 0.0.25, …). The
phases called 0.0.1…0.0.7 here are read as milestones **0.0.1, 0.0.2, …**; the exact number is
set by each delivery. Any jump to 0.1.0 or 1.0.0 is proposed and authorized by the user.

# Implementation findings (0.0.1, 2026-10-26)

- **Groups implies sorting by date.** The initial `readdir` keeps only name and
  type; a worker loads the metadata to classify by date after the
  first paint. Meanwhile there is a transient «Undated» group. In List
  sorted by name, only the visible entries are queried; sorting by size/date
  requests the rest. No widgets are created per entry.
- Cards show the count of immediate children and the sum of the sizes of immediate
  files. They do not represent a recursive size: `du` is still a protocol without an
  implementation. Symbolic links are not walked during the count.
- **FLTK only draws in software**, without `enable-glwindow`, with `no-images` so as not
  to bring thumbnail decoders into the UI. The system text libraries do link
  libpng transitively (Cairo/FreeType); no file images are decoded
  in this milestone. The status bar is invalidated by its region;
  navigation/layout changes invalidate the canvas. There are no continuous animations.
- Linux uses a window without client decoration, in line with the mockup. It avoids
  the work and surface of libdecor per window; it gave ten consecutive startups
  under 100 ms in the saved measurement. The compositor still manages moving,
  resizing and closing. No persistent Hyprland rules were modified.
- **FLTK/Wayland limit on the outer outline:** its native mask uses Cairo
  A1 (one bit, without fine alpha anti-aliasing). The experimental test showed peaks of
  48–54 MiB during resizes; there was also a race when setting the geometry
  from Hyprland, so that peak does not allow attributing an isolated cost to the clipping.
  That experimental path is not included in 0.0.1; anti-aliasing has yet to be solved and
  its budget validated in isolation. **The outer outline is managed by
  Hyprland**; the square/rounded/omarchy preference applies to the inner geometry.
  Badges and the inspector's box keep fixed radii. It is an explicit
  limitation to revisit with another backend or an upstream fix.
- The watcher observes `current/theme`, its parents and the resolved target. After atomic
  changes it re-attaches the watches; a 100 ms debounce, a bounded queue of one
  notification and the last valid palette kept on a partial write.
- CC0/ISC/Unicode-3.0 licenses: specific exceptions authorized by the user;
  see `DEPENDENCIES.md`. The policy was not extended to other dependencies.
- CUA 0.28.2 does not discover windows on this Hyprland 0.56.2 (`Found 0 windows`;
  the Wayland identifier rejected as `stale`). The scenes verify the UI model
  inside a real Wayland window. They are not equivalent to a test of OS click
  delivery. The final captures come from FLTK's real framebuffer, without overlaying
  other windows nor including their content.
- DnD, the file clipboard, thumbnails, content preview, operations,
  network, recursive search and a real `du` helper remain outside this milestone. The inspector
  shows type/metadata and opening; its box represents the type, without decoding.
- Build and desktop test done on Linux aarch64/Wayland. Windows,
  macOS, X11 and mouse event delivery through a native driver remain untested.

# Explicit user update (2026-10-26): scope and Nerd Fonts

The scope after 0.0.1 includes the whole plan and modern capabilities, real opt-in
shaders included; the matrix and the observable acceptance are in ROADMAP.md.
The earlier proportional sans typography is replaced by **Nerd Fonts** across
the whole UI, also toolbar/rail/inspector/spatial. Omarchy's monospace Nerd font
is resolved through a bounded fontconfig query, with a readable fallback outside
Omarchy, without enumerating/rasterizing the catalog and without changing the global configuration.
The 150 MB ceiling covers the main process and the helpers; 50 MB is a goal, not permission
to leave out the cost of children/GPU. The 400–700 MB EmbeddingGemma mode stays
blocked because it is incompatible with the budget, without holding back other capabilities.


## Implementation findings 0.0.2 (2026-10-26)

- Nerd Fonts replaces the earlier sans by explicit instruction. The host's
  monospace family is resolved with a fontconfig query; glyphs rasterized in the
  real framebuffer. Fixtures isolate only FILES_CONFIG_DIR to keep that mapping.
- Spatial persistence and corners: limits before allocation, validation of earlier
  data, exclusive temp+rename/sync, Arc snapshots coalesced until ACK. Failures
  survive navigation/close; aliases use the canonical identity resolved outside the UI.
  Moving/renaming a folder requires a future path+dev+ino identity migration.
- Close waits for a save or an explicit discard, and for the helper to be cancelled/reaped. This does not
  provide a durable journal against kill/power loss nor isolation from hostile writers.
- The sampler measures the PSS+Swap tree and records confirmed exits during a read.
  Complete means verifiable reads of survivors, not lifetime coverage;
  sampled maximums are still lower bounds, without GPU. Detail in VALIDATION.
- Calculate size uses an ephemeral FD/no-follow du helper with limits and a supervisor; it shows
  the apparent size by name, not physical blocks nor a snapshot. The treemap is still pending.
- The observed 2x geometry and the logical FLTK capture prove the layout, not the compositor's whole
  physical sharpness. The outer outline and OS events keep their earlier limits.
- Hardware GLES 3 on Apple M2 Pro checked; four MIT shaders ported/drawn in
  fixtures. There are still no real effects integrated into Painter nor an ON/OFF app budget.
- Strict startup is still FAIL (max 111.423 ms without a sampler); it is not hidden behind the median.
  The ideal 50 MB goal is missed in Groups 100k (55.59 MB); the 150 MB ceiling is not extended.

- Final 0.0.2 regression: baseline tree+Swap 40.81 MiB peak, 28.63 MiB steady; FAIL of the
  40 MiB analysis threshold. Earlier evidence is kept. A static reading of FLTK
  Wayland 1.5.23 allows simultaneous pools and retaining a large current_pool
  (10 MB minimum or 2×frame bytes), but it does not prove causality. Mapping diagnostics
  have acceptance=false; Size is not PSS. No global patch nor assumed savings.
  The user suspends new 0.0.2 measurements and authorizes continuing the 0.0.3 integration;
  pending performance is not turned into a PASS by closing the checkpoint.

## Implementation findings 0.0.4 (2026-10-26)

- fltk-rs 1.5.23 `app::wait_for` converts the C return value to i32 and only accepts 0/1;
  the Unix backend returns poll's number of ready FDs, legitimately >=2. An isolated
  two-pipe fixture: raw 2/callbacks 2/errno 0, which the Rust wrapper would reject. The UI now
  calls the same linked C function through a local wrapper, accepts a finite raw >=0,
  captures errno immediately, retries only on EINTR with a 1 ms pause and keeps real errors.
  A fatal error cancels/drains operations/helpers before returning a failure to the binary.
  No registry patch nor new dependency. The trace in the scene confirms successful raw 4/3;
  the return value of the original failure was not instrumented and no causality is invented.
- The Wayland clipboard needs the compositor's focus/serial. A fixture without focus publishes
  set_selection with serial 0 and wl-paste returns Nothing is copied; an exact internal
  selection is not equivalent to a tested external clipboard. The FAIL is kept and it has yet to be
  repeated with the fixture's real focus, without replacing app::copy with an external process.
- The text preview draws the source with a literal Painter primitive and a byte→display map
  for tabs/control characters; it keeps selection/copies of the original bytes. Only the bounded
  active document, only the visible lines; per-line rendering limited to 4096 B with a notice.
  HTML/Markdown source is not considered a rendered preview, which is still pending.

- Experimental P05: native litehtml/Gumbo delivers bounded commands to Painter,
  without a browser/JS/network. No UC/Sun strtod code outside the allowlist is used; an isolated
  MIT bridge. document_container is called after Gumbo creates its tree:
  the callback cap is NOT a cap before the parser. The UI's RLIMIT_AS 64 MiB is not PSS.
  Scalar 0.6×size metrics show CJK/cell clipping; neither shaping nor a
  complete HTML preview is declared. Allocator/resources/contrast/packaging and a budget are missing.
  Current phase is integration/captures only: no new measurements until it is lifted.

## Explicit window decision (2026-10-26, 0.0.4)

The user replaces the forced borderless Linux window with Normal by default, managed
by the compositor without imposing floating/panel/overlay. Preferences and Actions
are separate, with Nerd Fonts; normal/fullscreen modes persisted
independently of corners/theme. A free header and 24 px inner grips request a
Wayland move/resize with a real serial, without stealing controls nor touching the global config.

Finding: fltk-rs does not expose interactive Wayland resize. A local C++ adapter against
the FLTK pin's private headers uses libdecor, validating focus/window/event/serial;
the registry is not patched. A build without header metadata requires a bounded discovery of
the pinned source and identical configs or an explicit override. No test of an OS gesture
nor AT-SPI accessibility; limits and verification in WINDOW-MODES.md. The
resource/startup freeze is kept and there is no global 0.0.4 PASS.

### Explicit user priority — 2026-10-26
After Properties: investigate the Super+W close in the effective bind/compositor/libdecor,
without modifying the global configuration; open through the system `open` command with
a direct argv, without a shell nor an xdg-open fallback (alternative pending if it is missing).
Then integrate Space `#v-map` as a treemap proportional to real bytes with a
cancellable helper, selection/inspector and navigation: different from Spatial and du.
Do not reproduce the mockup's fake «0 MB resident» text. The resource and
startup freeze continues; only integration, functional tests and captures in this phase.

### Global hidden files and Space — explicit decision 2026-10-26

Dot-prefixed hidden files disabled by default. Space, Preferences and Ctrl+H
control the SAME persisted global state; no per-folder/session overrides.
Selection/operations exclude hidden items. The treemap normalizes areas across
visible children; the apparent total and folders include hidden descendants, and this is stated.
The style moves closer to v3 through contiguous theme blocks and a compact header.
An empty native Frame label removes the help behind the content; tips move to the bottom
bar with clip/regional repaint/pauses. A real B3 context menu uses the existing Actions.
Details, focus/minimization limits and evidence in HIDDEN-CONTEXT-AND-STATUS.md.
The resource/startup freeze continues, with no new metrics nor a global 0.0.4 PASS.


### On-demand media: publication and invalidation (2026-10-26)

Explicit user priority: previews/thumbnails before more peripheral
features. MEDIA-LIFECYCLE keeps the stack and engines: a dev/ino/size/mtime/
ctime signature of the real FD, a checked image request and an effective source, UI generation,
a parent watch with a monotonic epoch and reconciliation outside the UI. The final O_NOFOLLOW
does not confine parent symlinks. A stuck readonly worker is not replaced with
more threads nor joined on the UI; the ACK of that probe is distinguished from the helpers' reap. An
uninterruptible kernel state can delay termination; offline is still pending. M1 Linux
selected functionally verified; LRU/thumbnails/embedded/other OSes and metrics
remain open, with no new disk nor an extended budget.


### Thumbnails and a removed surface (2026-10-26)

M2: the same image/video engine at 160 px, an owning cache with Grid/inspector borrows,
capacity+reservation before decode, 32 entries and bounded visible demand; no new disk.
An 8 px CPU radius on the real image (including letterbox), independent of corners.
A removed FLTK WidgetTracker produced an isolated panic without a backtrace: the cause is open,
do not attribute it to cleanup. Guards when returning from wait/flush detect a removed
window/canvas and cancel/drain through the failure exit without using its pointer. A
C++ fixture with own destruction/a live helper verifies that path, NOT Super+W/OS input nor the cause of the
original failure. No registry patch, no stack change. MEDIA-THUMBNAILS distinguishes
the local cache from the global budget/PSS and keeps the freeze and the pending items.

### Embedded resources bound to the FD (2026-10-27, 0.0.4)

RELATIVE-SOURCE/DOCUMENT-MEDIA: expected root FD/file FD before reading and effective
in the response; a hidden MD candidate until watcher/final probe/token/epoch are stable.
A single existing readonly worker, 32 directories/8 resources, absent intents
to reconcile recreation without old RGB. Optional watches can fall back on the 1 s
reconcile and their individual UI diagnostic is pending. A direct root alias preserved,
a NOFOLLOW relative context, not a sandbox nor an atomic snapshot. A readonly stop without a UI join
is distinguished from the helpers' reap; a false stop ACK is kept. Metrics freeze intact,
without extending the goal/ceiling nor declaring global P09/other formats complete.

2026-10-27 — Audio finding in Space: the ffplay prototype returns exit 0
even on a certain device failure, and its native EOF after seek hit the timeout
while the dummy passed; the internal cause is not attributed just from that difference. The
current output uses the OS's ffmpeg/Pulse, the same FD/supervisor, a strictly bounded stderr
and own controls. Pulse did not create a context under AS 512 MiB and the fixture did pass
with AS 768 MiB (like video), without changing the REAL 150 MB ceiling nor measuring PSS. Functional
evidence for silence/EOF/cancel/ACK in AUDIO-PLAYBACK; resources/startup frozen.


2026-10-27 — Opt-in video audio: the same direct ffmpeg with RGB/Pulse outputs,
muted by default. The Pulse context with AV failed mmap under AS 768 MiB/default arenas;
MALLOC_ARENA_MAX 2 only in the child allows the context while keeping AS 768. The functional
contract is not equivalent to PSS/CPU/the 150 MB ceiling. A typed source failure requires a new
watch/probe, an output error keeps the verified frame and allows mute/retry.
VIDEO-PREVIEW documents evidence and limits; freeze without new measurements.


## User decision — Columns replaces Spatial (2026-10-27)

The user says Spatial adds no value and asks to replace it with Finder-style Miller
Columns. Space/treemap stays. The Ctrl+3 selector/help/CLI move to
Columns, --spatial keeps a compatible alias. The legacy tag 3 session → Columns with
icon zoom/pan/scroll discarded; the new tag 5 keeps the Columns scroll.
Earlier layouts are kept without deleting them nor reusing coordinates as
columns. COLUMNS-CONTRACT details the operation source in the focused column,
virtualization and the sliding window of up to 8 listings without a depth limit.
New previews paused, metrics frozen; dual pane comes next.


## M27 implementation — MC dual pane (2026-10-27)

F9 turns the inspector into a second live list and Tab changes focus outside inputs.
One Model + a movable Browser, two virtualized listings and a secondary worker
transferred exclusively from/to Columns. One queue/journal/media/AuxIo.
F5/F6 capture the reviewable raw opposite destination; IDs (pane, tab), independent selection/query/
history/scroll. F9 off frees invisible data; 3 MiB of shared
snapshots and a local 32 MiB listing, without a PSS claim. Persisting both halves, OS input and
real gates are still pending; DUAL-PANE-CONTRACT delimits the delivery.


### 2026-10-27 — M28 private clipboard and the MIME boundary

Ctrl+C/X/V for files uses a raw/FD capture in AuxIo, a reviewable dialog and the existing queue;
a partial Cut consumes only publications confirmed per generation and subset.
The first delivery is internal to the window, O06 partial: it does not replace OS MIME/DnD. On
pinned Linux the Rust text/plain query passed a CString but Wayland compares
pointers; a local bridge uses Fl::clipboard_plain_text without touching the registry. A scene
with own text immediately and after the event loop verifies the callback; external focus/serial
remains unvalidated. For M29, only a gesture/own window and a real serial; no data-control
to get around focus nor read/take over the clipboard without an action. Freeze/previews paused.


### 2026-10-27 — M29a native clipboard and DnD coexistence (partial)

Own wl_data_device/listeners bridge, a GesturePermit with real serial/focus and an
epoch invalidated before the Model borrow; bounded FD transport in AuxIo. The
codec/pipe/State/UI rejection layers verified; OS MIME NOT validated, no data-control nor
fabricated focus. FLTK 1.5.23 Wayland negotiates DnD/roundtrip in foreign callbacks even
with a false handler; do not adopt a double device as proof of coexistence. Study a
reproducible opt-in hook in the repo before enter/motion/drop, preserving the default and the
foreign cleanup. M29/O06 remain open, KDE and multi-source pending.
Previews paused, metrics frozen; NATIVE-TRANSFER-CONTRACT.md delimits the caps.


### 2026-10-27 — FLTK hook and partial DnD receiver

An opt-in patch of the pinned Wayland TU, FLTK source+license and a strong link guard
by hash, without editing the registry; a COPY receiver and a pinned raw destination with
review before the queue. 642 tests and a private fixture/visual verification passed
within scope; there is no compositor test nor DnD between apps. Sender/KDE/several
parents/complete O06 are still pending. See NATIVE-TRANSFER-CONTRACT.md.
