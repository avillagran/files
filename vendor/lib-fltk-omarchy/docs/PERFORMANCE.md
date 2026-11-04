# Performance: lib-fltk-omarchy vs QML (0.0.1–0.0.2)

Measured 2026-10-27 on Asahi Linux (Apple M2, aarch64), Hyprland 0.56.2,
Qt 6 `qml6`, release build of this crate (`opt-level = "s"`, thin LTO,
static FLTK 1.4.5 from fltk-sys 1.5.23 with `use-wayland`).

## Method (`examples/measure.rs`, `script/bench`)

- Each workload runs as a real Wayland client launched with
  `hl.dsp.exec_cmd(cmd, { workspace = "special:omarchy-fltk-bench silent", float = true })`:
  hidden workspace, floating at the requested 520×400 from the first frame,
  the user's focus is never touched.
- **startup_ms**: from the dispatch to the window appearing as mapped in
  Hyprland's client list (5 ms polling). Same method for every toolkit.
- **pss_kb**: `Pss + SwapPss` from `/proc/<pid>/smaps_rollup`, summed over
  the whole process tree, 2 s after mapping. Never RSS.
- **idle_cpu_s**: utime + stime of the tree over 5 s without input
  (10 ms tick resolution).
- 5 runs per case, medians reported. Runs were sequential; no Files build
  was running (checked with `pgrep`).

Workloads (`examples/bench_*.rs`, `bench/*.qml`), functionally equivalent:
- **minimal**: a label and a button.
- **app**: header, text field bound to a label, two buttons, a progress
  bar and a 100 000-row virtualized list with two columns. The Rust app
  also runs the live Omarchy theme watcher.

## Results

Two sessions, each comparing everything within the same session. PSS
splits shared library pages among every process that maps them (the
Omarchy shell also loads Qt), so absolute numbers move between sessions;
compare rows of the same session.

### Session 2 — 2026-10-28 (`fltk` features `use-wayland`, `no-images`)

| Case | startup (median) | PSS (median) | PSS (max) | idle CPU / 5 s |
|---|---|---|---|---|
| lib-fltk-omarchy minimal | 47 ms | 13.7 MB | 13.7 MB | 0.00 s |
| QML software minimal | 133 ms | 39.5 MB | 39.5 MB | 0.00 s |
| QML GPU minimal | 153 ms | 80.3 MB | 80.3 MB | 0.00 s |
| lib-fltk-omarchy app | 51 ms | 13.8 MB | 13.9 MB | 0.00 s |
| QML software app | 123 ms | 45.5 MB | 48.7 MB | 0.00 s |
| QML GPU app | 192 ms | 90.0 MB | 90.7 MB | 0.00 s |

App workload: 3.3× less memory than QML software, 6.5× less than QML GPU;
first frame 2.4–3.8× sooner.

### Session 1 — 2026-10-27 (`fltk` feature `use-wayland`)

| Case | startup (median) | PSS (median) | PSS (max) | idle CPU / 5 s |
|---|---|---|---|---|
| lib-fltk-omarchy minimal | 57 ms | 16.0 MB | 16.0 MB | 0.00 s |
| QML software minimal | 100 ms | 61.7 MB | 61.8 MB | 0.00 s |
| QML GPU minimal | 155 ms | 111.5 MB | 112.4 MB | 0.00 s |
| lib-fltk-omarchy app | 63 ms | 16.0 MB | 16.1 MB | 0.00 s |
| QML software app | 134 ms | 72.6 MB | 72.8 MB | 0.00 s |
| QML GPU app | 168 ms | 125.4 MB | 126.5 MB | 0.00 s |

Session 1, app workload: 4.5× less memory than QML software, 7.8× less
than QML GPU; first frame 2–2.7× sooner. Binary size
of the release `bench_app`: 1.26 MB (FLTK statically linked).

## Session 3 — 2026-10-28, 0.0.2 (keyboard router, widget-owned bindings)

Same script and machine, 5 runs each, under load (load average 4.4).

| Case | Startup (median) | PSS median | PSS max | Idle CPU 3 s |
|---|---|---|---|---|
| lib-fltk-omarchy minimal | 69 ms | 14.9 MB | 14.9 MB | 0.00 s |
| QML software minimal | 104 ms | 46.2 MB | 46.3 MB | 0.00 s |
| QML GPU minimal | 169 ms | 93.3 MB | 94.0 MB | 0.00 s |
| lib-fltk-omarchy app | 70 ms | 15.0 MB | 15.0 MB | 0.00 s |
| QML software app | 127 ms | 59.5 MB | 59.8 MB | 0.00 s |
| QML GPU app | 175 ms | 106.9 MB | 108.4 MB | 0.00 s |

Two more batches of 5 runs of the lib app, right after: startup medians
61 ms and 47 ms (single runs 40–78 ms), and PSS 15.0 MB in every run. The
startup spread follows the machine load. PSS is in the range of the
earlier sessions, and QML moved by more between sessions. The keyboard
router adds no measurable cost.

## CPU shaders (0.0.4)

The cost of an animated `gfx::View` against a static one, measured in the
same session: [SHADERS.md](SHADERS.md#cost-measured). Animation adds CPU
(35–45 % of one core for 484×300 at 30 fps) but no memory. 0.0.5 adds
multi-core shading and the optional GPU path (0.7 % CPU, +35 MB PSS for the
GL driver), measured in the same session:
[SHADERS.md](SHADERS.md#gpu-vs-cpu-005).

## Window size matters

Memory grows with window area for any software-rendered toolkit (two
wl_shm buffers plus a draw buffer). The real-window scenario
(`script/real-window`) maps a normal tiled window at ~1900×1030 first and
reports ~52 MB PSS. `Mode::Floating` avoids the large first buffers by
mapping with a fixed size so Hyprland floats it at the requested size.

## Not claimed

Other workloads, other machines, GPU-heavy content, and long-running
memory behaviour were not measured. Numbers from the abandoned Ruby
prototype are not comparable and are not used here.
