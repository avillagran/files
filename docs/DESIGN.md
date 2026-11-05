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

# Hallazgos de implementación (0.0.1, 2026-10-26)

- **Grupos implica ordenar por fecha.** El `readdir` inicial conserva sólo nombre y
  tipo; un trabajador carga los metadatos para clasificar por fecha después de la
  primera pintura. Mientras tanto hay un grupo transitorio «Por fechar». En Lista
  por nombre sólo se consultan las entradas visibles; ordenar por tamaño/fecha
  solicita el resto. No se crean widgets por entrada.
- Las tarjetas muestran conteo de hijos inmediatos y suma de tamaños de archivos
  inmediatos. No representan tamaño recursivo: `du` sigue como protocolo sin
  implementación. Los enlaces simbólicos no se recorren durante el conteo.
- **FLTK sólo dibuja por software**, sin `enable-glwindow`, con `no-images` para no
  incorporar decodificadores de thumbnails a la UI. Las bibliotecas de texto del
  sistema sí enlazan libpng de forma transitiva (Cairo/FreeType); no se decodifican
  imágenes de archivos en este hito. La barra de estado se invalida por su región;
  los cambios de navegación/layout invalidan el lienzo. No hay animaciones continuas.
- Linux usa una ventana sin decoración de cliente, de acuerdo con el mockup. Evita
  el trabajo y la superficie de libdecor por ventana; dejó diez arranques consecutivos
  bajo 100 ms en la medición guardada. El compositor sigue administrando mover,
  redimensionar y cerrar. No se modificaron reglas persistentes de Hyprland.
- **Límite de FLTK/Wayland en el contorno exterior:** su máscara nativa usa Cairo
  A1 (un bit, sin anti-aliasing alfa fino). La prueba experimental mostró picos de
  48–54 MiB durante redimensionados; también hubo una carrera al fijar la geometría
  desde Hyprland, por lo que ese pico no permite atribuir un coste aislado al recorte.
  No se incorpora esa ruta experimental en 0.0.1; falta resolver anti-aliasing y
  validar su presupuesto de forma aislada. **El contorno exterior lo administra
  Hyprland**; la preferencia square/rounded/omarchy se aplica a la geometría interior.
  Los badges y el recuadro del inspector conservan radios fijos. Es una limitación
  explícita para revisar con otro backend o una corrección upstream.
- El watcher observa `current/theme`, sus padres y el destino resuelto. Tras cambios
  atómicos vuelve a adjuntar las vigilancias; debounce de 100 ms, cola acotada de una
  notificación y conservación de la última paleta válida ante escritura parcial.
- Licencias CC0/ISC/Unicode-3.0: excepciones específicas autorizadas por el usuario;
  ver `DEPENDENCIES.md`. No se amplió la política para otras dependencias.
- CUA 0.28.2 no descubre ventanas en este Hyprland 0.56.2 (`Found 0 windows`;
  identificador Wayland rechazado como `stale`). Las escenas verifican el modelo de
  UI dentro de una ventana Wayland real. No equivalen a prueba de entrega de clics
  del SO. Las capturas finales salen del framebuffer real de FLTK, sin superponer
  otras ventanas ni incorporar su contenido.
- DnD, portapapeles de archivos, thumbnails, vista previa de contenido, operaciones,
  red, búsqueda recursiva y helper `du` real siguen fuera de este hito. El inspector
  muestra tipo/metadatos y apertura; su recuadro representa el tipo, sin decodificar.
- Compilación y prueba de escritorio realizadas en Linux aarch64/Wayland. Windows,
  macOS, X11 y entrega de eventos de ratón por controlador nativo quedan sin probar.

# Actualización explícita del usuario (2026-10-26): alcance y Nerd Fonts

El alcance posterior a 0.0.1 incluye todo el plan y capacidades modernas, shaders
reales opt-in incluidos; la matriz y aceptación observable están en ROADMAP.md.
La tipografía sans proporcional anterior queda sustituida por **Nerd Fonts** en
toda la UI, también toolbar/riel/inspector/espacial. Se resuelve la fuente monospace
Nerd de Omarchy mediante una consulta acotada fontconfig, con fallback legible fuera
de Omarchy, sin enumerar/rasterizar el catálogo y sin cambiar configuración global.
El techo de 150 MB comprende principal y helpers; 50 MB es objetivo, no permiso
para omitir coste de hijos/GPU. El modo EmbeddingGemma de 400–700 MB permanece
bloqueado por incompatibilidad de presupuesto, sin frenar otras capacidades.


## Hallazgos de implementación0.0.2 (2026-10-26)

- Nerd Fonts reemplaza la sans previa por instrucción explícita. Se resuelve la
  familia monospace del host con una consulta fontconfig; glifos rasterizados en
  framebuffer real. Fixtures aíslan sólo FILES_CONFIG_DIR para conservar ese mapping.
- Persistencia espacial y bordes: límites previos a asignación, validación de datos
  anteriores, temp exclusivo+rename/sync, snapshots Arc coalescidos hasta ACK. Fallos
  sobreviven navegación/cierre; aliases usan identidad canónica resuelta fuera UI.
  Mover/renombrar carpeta requiere futura migración de identidad path+dev+ino.
- El cierre espera guardar o descarte explícito, y cancelar/recoger helper. Esto no
  aporta journal durable frente a kill/power loss ni aislamiento de escritores hostiles.
- El sampler mide árbol PSS+Swap y registra salidas confirmadas durante lectura.
  Completo significa lecturas verificables de supervivientes, no cobertura de lifetimes;
  máximos muestreados siguen siendo límites inferiores, sin GPU. Detalle VALIDATION.
- Calcular tamaño usa helper du efímero FD/no-follow con límites y supervisor; muestra
  tamaño aparente por nombre, no bloques físicos ni snapshot. Treemap sigue pendiente.
- La geometría observada2x y captura FLTK lógica acreditan layout, no toda la nitidez
  física del compositor. El contorno exterior y eventos SO conservan límites anteriores.
- GLES3 hardware Apple M2 Pro comprobado; cuatro shaders MIT portados/dibujados en
  fixtures. Aún no hay efectos reales integrados en Painter ni presupuesto ON/OFF app.
- Arranque estricto sigue FAIL(max111,423ms sin sampler); no se oculta con mediana.
  Objetivo ideal50MB incumplido en Grupos100k(55,59MB), no techo150MB ampliado.

- Regresión final0.0.2: baseline árbol+Swap40,81MiB pico,28,63MiB estable; FAIL del
  umbral de análisis40MiB. Se conserva evidencia anterior. Lectura estática FLTK
  Wayland1.5.23 permite pools simultáneos y retención de current_pool grande
  (10MB mínimo o2×framebytes), pero no prueba causalidad. Diagnósticos de mappings
  tienen acceptance=false; Size no es PSS. Sin parche global ni supuesto ahorro.
  Usuario suspende nuevas mediciones0.0.2 y autoriza continuar integración0.0.3;
  rendimiento pendiente no se convierte en PASS por cerrar checkpoint.

## Hallazgos de implementación0.0.4 (2026-10-26)

- fltk-rs1.5.23 `app::wait_for` convierte retorno de C a i32 y acepta sólo0/1;
  backend Unix devuelve número de FD listos de poll, legítimamente>=2. Fixture
  aislada de dos pipes: raw2/callbacks2/errno0, wrapper Rust lo rechazaría. UI ahora
  llama la misma función C enlazada mediante wrapper local, acepta raw finito>=0,
  captura errno inmediato, reintenta sóloEINTR con pausa1ms y conserva errores reales.
  Error fatal cancela/drena operaciones/helpers antes de devolver fallo al binario.
  Sin parche de registry ni dependencia nueva. Traza en escena confirma raw4/3
  exitosos; retorno del fallo original no se instrumentó y no se inventa causalidad.
- Clipboard de Wayland necesita foco/serial de compositor. Fixture sin foco publica
  set_selection serial0 y wl-paste devuelve Nothing is copied; selección interna
  exacta no equivale a clipboard externo probado. Se conserva FAIL y falta repetir
  con foco real de la fixture, sin sustituir app::copy por proceso externo.
- Preview de texto dibuja fuente con primitiva literal de Painter y mapa byte→display
  para tabs/controles; preserva selección/copias de bytes originales. Sólo documento
  activo acotado, sólo líneas visibles; render por línea limitado4096B con aviso.
  HTML/Markdown fuente no se considera preview renderizado, que continúa pendiente.

- P05 experimental: litehtml/Gumbo nativo entrega comandos limitados a Painter,
  sin browser/JS/red. No se usa código UC/Sun strtod fuera de allowlist; bridge
  MIT aislado. document_container se llama después de que Gumbo crea su árbol:
  cap de callbacks NO es cap previo de parser. RLIMIT_AS64MiB de UI no es PSS.
  Métricas escalares0.6×size muestran recorte CJK/celdas; no se declara shaping ni
  preview HTML completo. Falta allocator/recursos/contraste/packaging y presupuesto.
  Fase actual sólo integración/capturas: sin nuevas mediciones hasta liberación.

## Decisión explícita de ventana (2026-10-26, 0.0.4)

Usuario sustituye el borderless Linux forzado por Normal por defecto, gestionada
por el compositor sin imponer flotante/panel/overlay. Preferencias y Acciones
separados con Nerd Fonts; modos normal/pantalla completa persistidos
independientes de corners/tema. Header libre y grips interiores24px solicitan
move/resize Wayland con serial real, sin robar controles ni tocar config global.

Hallazgo: fltk-rs no expone resize interactivo Wayland. Adaptador C++ local contra
headers privados del pin FLTK usa libdecor, validando foco/ventana/evento/serial;
no se parchea registry. Build sin metadata de headers exige discovery acotado de
la fuente fijada y configs idénticas o override explícito. No prueba de gesto SO
ni accesibilidad AT-SPI; límites y verificación en WINDOW-MODES.md. Se mantiene
freeze de recursos/startup y ningún PASSglobal de0.0.4.

### Prioridad explícita del usuario — 2026-10-26
Tras Propiedades: investigar cierre Super+W en bind efectivo/compositor/libdecor,
sin modificar configuración global; apertura por comando `open` del sistema con
argv directo, sin shell ni fallback xdg-open (alternativa pendiente si falta).
Integrar después Espacio `#v-map` como treemap proporcional de bytes reales con
helper cancelable, selección/inspector y navegación: distinto de Espacial y du.
No reproducir el texto ficticio «0 MB residentes» del mockup. Freeze de recursos y
arranque continúa; sólo integración, pruebas funcionales y capturas en esta fase.

### Ocultos globales y Espacio — decisión explícita 2026-10-26

Ocultos por prefijo punto desactivados por defecto. Espacio, Preferencias y Ctrl+H
controlan el MISMO estado global persistido; sin overrides de carpeta/sesión.
Selección/operaciones excluyen elementos ocultados. Treemap normaliza áreas entre
hijos visibles; total aparente y carpetas incluyen descendientes ocultos y se indica.
Estilo se acerca a v3 mediante bloques de tema contiguos y header compacto.
Frame label nativo vacío elimina ayuda detrás del contenido; tips pasan a bottom
bar con clip/repaint regional/pausas. Contextual real B3 usa Actions existentes.
Detalles, límites de foco/minimización y evidencia en HIDDEN-CONTEXT-AND-STATUS.md.
Freeze recursos/startup continúa, sin nuevas métricas ni PASSglobal de0.0.4.


### Medios bajo demanda: publicación e invalidación (2026-10-26)

Prioridad explícita del usuario: previews/miniaturas antes de más funciones
periféricas. MEDIA-LIFECYCLE conserva stack y motores: firma dev/ino/size/mtime/
ctime del FD real, petición imagen checked y source efectivo, generación UI,
watch de padre con época monotónica y reconciliación fuera UI. O_NOFOLLOW final
no confina symlinks de padres. Un worker readonly atascado no se reemplaza por
más hilos ni se join en UI; ACK de esa sonda se distingue de helpers reap. Kernel
ininterrumpible puede demorar terminación; offline sigue pendiente. M1 Linux
seleccionado verificado funcionalmente; LRU/miniaturas/embedded/otrosSO y métricas
siguen abiertos, sin disco nuevo ni ampliación de presupuesto.


### Miniaturas y superficie eliminada (2026-10-26)

M2: mismo motor imagen/vídeo a160px, caché owning con préstamos Grilla/inspector,
capacity+reserva antesdecode,32entradas y demanda visible acotada; sin disco nuevo.
Radio8px CPU sobre imagen real (incluido letterbox), independiente de corners.
WidgetTracker FLTK eliminado produjo un panic aislado sin backtrace: causa abierta,
no atribuir a cleanup. Guardas al volver del wait/flush detectan ventana/canvas
eliminados y cancelan/drenan por salida de fallo sin usar su puntero. Fixture de
C++destrucción propia/helpervivo verifica esa ruta, NO Super+W/inputSO ni causa del
fallooriginal. No parche registry, no cambio stack. MEDIA-THUMBNAILS distingue
cachelocal de presupuesto global/PSS y mantiene freeze y pendientes.

### Recursos incrustados ligados al FD (2026-10-27, 0.0.4)

RELATIVE-SOURCE/DOCUMENT-MEDIA: rootFD/fileFD esperado antesdelectura yefectivo
enrespuesta; candidatoMDoculto hasta watcher/probefinal/token/época estables.
Un solo worker readonly existente,32directorios/8recursos, intencionesausentes
para reconciliar recreaciónsinRGBviejo. Watchopcionales pueden recurrir a1s
reconcile ysu diagnósticoUIindividual queda pendiente. Rootaliasdirectopreservado,
contextorelativoNOFOLLOW, no sandbox ni snapshotatómico. StopreadonlysinjoinUI
se distingue dereaphelpers; stopACKfalse se conserva. Métricasfreeze intacto,
sin ampliarobjetivo/techo ni declararP09global/otrosformatos completos.

2026-10-27 — Hallazgo de audio en Space: el prototipo ffplay devuelve exit0
incluso ante cierto fallo de dispositivo, y su EOF nativo tras seek agotó timeout
mientras dummy pasó; causa interna no atribuida sólo por esa diferencia. La
salida actual usa ffmpeg/Pulse del SO, mismoFD/supervisor, stderr acotado estricto
y controles propios. Pulse no creó contexto bajo AS512MiB y el fixture sí pasó
con AS768MiB (como vídeo), sin cambiar el techo REAL150MB ni medir PSS. Evidencia
funcional silencio/EOF/cancel/ACK en AUDIO-PLAYBACK; recursos/startup congelados.


2026-10-27 — Vídeo audio opt-in: mismo ffmpeg directo con outputs RGB/Pulse,
muted por defecto. Contexto Pulse con AV falló mmap bajoAS768MiB/arenasdefault;
MALLOC_ARENA_MAX2 sóloenchild permite contexto conservandoAS768. Contrato
funcional no equivale a PSS/CPU/techo150MB. Source failure tipado exige nuevo
watch/probe, error de salida conserva frame verificado y permite mute/retry.
VIDEO-PREVIEW documenta evidencia y límites; freeze sin nuevas mediciones.


## Decisión del usuario — Columnas sustituye Espacial (2026-10-27)

El usuario indica que Espacial no agrega valor y pide reemplazarla por Columnas
Miller tipo Finder. Espacio/treemap permanece. Selector Ctrl+3/ayuda/CLI pasan a
Columnas, --spatial conserva alias compatible. Sesión tag3 legado → Columns con
zoom/pan/scroll de iconos descartados; tag5 nuevo conserva scroll de Columnas.
Layouts anteriores se conservan sin borrar ni reutilizar coordenadas como
columnas. COLUMNS-CONTRACT detalla origen de operaciones en columna enfocada,
virtualización y ventana deslizante de hasta8 listados sin límite de profundidad.
Previews nuevas pausadas, métricas congeladas; doble panel queda siguiente.


## Implementación M27 — doble panel MC (2026-10-27)

F9 convierte inspector en segunda lista viva y Tab cambia foco fuera de inputs.
Un Model + Browser movible, dos listados virtualizados y un worker secundario
transferido exclusivamente desde/hacia Columnas. Una cola/journal/medios/AuxIo.
F5/F6 capturan destino raw opuesto revisable; IDs (panel,tab), selección/query/
historial/scroll independientes. F9off libera datos invisibles; snapshots3MiB
conjunto y listado32MiB local, sin PSSclaim. Persistir ambas mitades, input SO y
gates reales siguen pendientes; DUAL-PANE-CONTRACT delimita la entrega.


### 2026-10-27 — M28 clipboard privado y frontera MIME

CtrlC/X/V archivos usa captura raw/FD en AuxIo, diálogo revisable y cola existente;
Cut parcial consume sólo publicaciones confirmadas por generación y subset.
Primera entrega interna a ventana, O06 parcial: no sustituye MIME SO/DnD. En
Linux fijado la consulta Rust text/plain pasaba CString pero Wayland compara
punteros; puente local usa Fl::clipboard_plain_text sin tocar registry. Escena
texto propio inmediato y tras eventloop verifica callback; foco/serial externo
sigue sin validar. Para M29 sólo gesto/ventana propia y serial real; no data-control
para sortear foco ni leer/apropiar clipboard sin acción. Freeze/previews pausadas.


### 2026-10-27 — M29a clipboard nativo y coexistencia DnD (parcial)

Bridge propio wl_data_device/listeners, GesturePermit de serial/foco reales y
epoch invalidada antes de borrow Model; transporte FD bounded en AuxIo. Capas
codec/pipe/State/UIrechazo verificadas; MIME SO NO validado, sin data-control ni
fabricar foco. FLTK1.5.23 Wayland negocia DnD/roundtrip en callbacks ajenos aun
con handler false; no adoptar doble-device como prueba de coexistencia. Estudiar
hook opt-in reproducible bajo repo antes enter/motion/drop, default y cleanup
ajenos preservados. M29/O06 siguen abiertos, KDE y multi-origen pendientes.
Previews pausadas, métricas congeladas; NATIVE-TRANSFER-CONTRACT.md delimita caps.


### 2026-10-27 — hook FLTK y receptor DnD parcial

Patch opt-in de la TU Wayland pinned, fuente+licencia FLTK y guard fuerte de
enlace por hash, sin editar registry; receptor COPY y destino raw fijado con
revisión antes cola. 642tests y fixture privado/verificación visual aprobados
en alcance; no hay prueba de compositor ni DnD entre apps. Sender/KDE/varios
padres/O06 completos siguen pendientes. Ver NATIVE-TRANSFER-CONTRACT.md.
