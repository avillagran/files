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

## Rich text streaming (Unreleased)

Measured 2026-11-04 on an AMD Ryzen 9 5900X (x86_64), Hyprland 0.56.2,
release build, with `script/richtext-bench` (`examples/richtext_bench.rs`):
an 800×900 floating window on the hidden workspace
`special:omarchy-fltk-richtext`, the process measuring itself (utime +
stime and `Pss + SwapPss` from `/proc/self`). The transcript is 5 000
messages (1.6 MB of Markdown with lists, tables, code blocks and links,
27 501 blocks); the last message then receives Markdown for 10 s. Phases:
settle 1.5 s, idle 3 s, stream 10 s, idle 3 s. Medians of 3 runs (single
runs for the 20 KB/s rows); CPU is % of one core. Raw lines:
[evidence/richtext-20261104/](evidence/richtext-20261104/).

| Case | Parse 5 000 msgs | Idle CPU before / after | Stream CPU | of which paint | of which push (parse + apply) | Push avg / max | PSS idle | PSS after stream |
|---|---|---|---|---|---|---|---|---|
| empty view | — | 0.0 % | — | — | — | — | 12.6 MB | — |
| 5 000 messages, no stream | 31 ms | 0.0 % | — | — | — | — | 16.1 MB | — |
| 1 MB/s at 60 updates/s | 33 ms | 0.0 % / 0.0 % | 19.5 % | 17.2 % (2.9 ms/frame) | 2.0 % | 0.34 / 0.61 ms | 16.2 MB | 31.9 MB |
| 1 MB/s at 30 updates/s | 31 ms | 0.0 % / 0.0 % | 10.0 % | 8.1 % (2.7 ms/frame) | 1.7 % | 0.58 / 0.85 ms | 16.2 MB | 33.8 MB |
| 1 MB/s, worker thread, 8-byte tokens (`sender()`, ≤ 60 updates/s) | 31 ms | 0.0 % / 0.0 % | 19.3 % | 16.8 % (2.8 ms/frame) | — | — | 16.2 MB | 33.8 MB |
| 20 KB/s at 60 updates/s (a fast model) | 31 ms | 0.0 % / 0.0 % | 10.9 % | 10.5 % (1.8 ms/frame) | 0.2 % | 0.03 / 0.07 ms | 16.2 MB | 17.4 MB |
| 20 KB/s at 30 updates/s | 32 ms | 0.0 % / 0.0 % | 5.8 % | 5.5 % (1.9 ms/frame) | 0.1 % | 0.04 / 0.07 ms | 17.0 MB | 18.2 MB |

Idle is the 3 s before and after the stream: no timer runs, nothing is
painted. Only blocks in view (and half a view around) are laid out: 30 of
27 501 after the transcript loads. At 1 MB/s more than a screen of new text
arrives every frame, so every frame lays out a whole screen of new blocks;
that is the worst case. Parsing and applying the stream costs about 2 % of
a core; the rest is painting the whole view through Cairo/Pango once per
update, so 30 updates/s halves the cost (`sender()` caps updates at 60/s).

**Memory.** Blocks are packed into four arenas shared by the document
(src/text/store.rs): the text; 4 bytes per styled run (end offset, style
bits, link marks; 8 in blocks of 4 MiB or more); the rare per-block data
(URLs, list numbers, code languages, table cells) as bytes; 16 bytes of
metadata per block, plus 8 bytes per block of height sums. Only the
blocks near the view are materialized as `Block`s with their layout (at
most ~600; their measured height stays when the layout is dropped).

| Same runs, before → after | Before (a `Block` per block) | After (packed store) |
|---|---|---|
| PSS growth, ~9.75 MB streamed at 1 MB/s (60 Hz / 30 Hz / sender) | 52.7 / 53.1 / 52.8 MB | 15.6 / 17.6 / 17.7 MB |
| Memory per byte of streamed Markdown | 5.4 B/B | 1.6–1.8 B/B |
| PSS idle with the 5 000-message transcript | 26.2 MB | 16.1 MB |
| Transcript alone (minus the empty view) | 13.0 MB for 1.6 MB of Markdown | 3.5 MB |
| Data held per byte of the dense stream (unit test `dense_markdown_costs_under_two_bytes_per_byte`) | — | 1.51 B/B (text 0.88, runs 0.21, metadata and sums 0.29, URLs and languages 0.13) |

The rest of the growth over the data held is the allocator: arenas grow by
doubling, and an old buffer freed inside the heap stays resident. Push and
paint did not get slower: the same session alternating the two binaries
four times (1 MB/s, 60 updates/s) gives push 0.31 → 0.30 ms, paint 2.84 →
2.75 ms per frame, process 19.0 → 18.3 % of a core (medians; single runs
vary by ±15 %).

With an assistive technology active (`script/richtext-a11y-bench`: the
same bench built with `--features a11y` in a private AT-SPI session), each
update rebuilds and diffs the window's accessibility tree in 0.2 ms
(about 120 nodes: only the blocks near the view). At 20 KB/s an update
sends ~23 nodes (the new blocks, the last block and the document); the
process uses 15.1 % of a core instead of 10.9 %, the difference being
AccessKit's AT-SPI work on its own thread. At 1 MB/s a screen of new
blocks arrives per frame and nearly every exposed node is new (47 %).

Release size (`opt-level = "s"`, thin LTO, stripped), measured again
with the packed store: `bench_minimal` is 1 307 656 bytes and
`richtext_minimal` (the same app with a `RichText`) 1 781 064 bytes: the
`text` module, `pulldown-cmark` (no default features), and
`unicode-segmentation` add 473 KB (487 KB when first measured, with
`bench_minimal` at 1 276 040 bytes).


## Text editing (Unreleased)

Measured 2026-11-04 on an AMD Ryzen 9 5900X, release build, with
`script/edit-bench` (`examples/edit_bench.rs`): a 720×640 window with a
`TextArea` (4..16 lines) and a `Composer`, keys sent with `focus::press`
(the real key path, no OS input) and `app::flush()` after each key, so a
key's time includes its redraw. Wayland: Hyprland 0.56.2, hidden
workspace `special:omarchy-fltk-test`; X11: Xvfb. Medians of 3 runs
(raw: [edit-bench-2026-11-04.jsonl](edit-bench-2026-11-04.jsonl)).

| Case | Wayland (Hyprland) | X11 (Xvfb) |
|---|---|---|
| Key in the composer (avg, 300 keys) | 0.18 ms | 0.28 ms |
| Key with the completion list open | 1.19 ms | 1.15 ms |
| Paste 100 KB (2 000 lines, CJK mixed) + redraw | 7.7 ms | 10.4 ms |
| Key at the end / start of that 100 KB text | 1.84 / 2.01 ms | 0.83 / 0.87 ms |
| Set 100 KB as one paragraph (no newline) | 5.1 ms | 3.8 ms |
| Key at the end of that paragraph | 2.34 ms | 0.85 ms |
| Key at its start (avg / p95) | 2.78 / 6.35 ms | 1.81 / 6.59 ms |
| PageDown + redraw in the 100 KB paragraph | 4.6 ms | 0.82 ms |
| Idle CPU (3 s, after all of the above) | 0.01 % | 0.01 % |
| PSS at start → after the 100 KB tests | 10.7 → 26.8 MB | 7.7 → 15.2 MB |

How: the text is laid out per paragraph and only edited paragraphs are
laid out again; inside a long paragraph the wrap restarts one line before
the edit and stops as soon as a new line starts where a (shifted) old line
started, because greedy wrapping depends only on the text after a line
start. A key at the start of one long paragraph can move every line break
after it (each line gives a word to the next), which is the p95 above.
Word widths are cached, so laying out 100 KB measures each distinct word
once. Nothing runs while idle: the caret does not blink. On Wayland each
key redraws through FLTK's full-window buffer, which is most of the time
per key. In a debug build (`cargo test`), the same keys take about 0.04 ms
in an empty area and 6 ms at the start of the 100 KB paragraph.

## Streaming updates (Unreleased)

How much an app pays to show data that changes all the time, with the
three ways the toolkit offers ([MODELS.md](MODELS.md)). Measured
2026-11-04 on an AMD Ryzen 9 5900X (x86_64), Linux 7.2.8, Hyprland 0.56.2,
native Wayland, release build, one session (load average 2–4), 3 runs per
row, medians. Raw lines: `docs/stream-bench-2026-11-04.jsonl`.

**Workload** (`examples/stream_bench.rs`, run by `script/stream-bench`): an
o3-like client window with a 2 000-thread list and the open conversation's
text below it. A background thread produces 30 or 100 events/s: 60 %
status changes, 25 % new messages (unread count, preview, bump to the top),
15 % reorders (bump to the top); 40 % of the events hit the 15 first
(visible) threads. Every event also appends 1 MB/s ÷ rate of text to the
message (a new message starts past 256 KB). The message view keeps line
offsets and rescans only what its `Edit` hint says is new.

- **naive**: every event re-sends the whole state (thread list + message
  text); the UI sets a `State<Vec<Thread>>` and a `State<String>` and
  redraws a count-based `List` (the pre-model way).
- **diff**: the same snapshots, reconciled with `ListModel::sync_from` and
  `State::set_text` (the protocol unchanged, the UI stops rebuilding).
- **feed**: the producer sends diffs through `model::Feed` and the text
  through `TextFeed` (the protocol sends changes).
- **feed@60**: `feed` with `max_rate(60.0)` on both feeds.

**Method**: same as the other benches: launched with `hl.dsp.exec_cmd` on
the hidden workspace `special:omarchy-fltk-bench silent`, floating; no input
sent, the user's focus untouched. The process measures itself over 10 s
after a 1.5 s warm-up: process CPU (`CLOCK_PROCESS_CPUTIME_ID`, producer
thread included), UI-thread CPU (`CLOCK_THREAD_CPUTIME_ID`), **frame CPU**
= UI-thread CPU of each loop iteration that painted (apply + paint;
sleeping is not counted), list rows built, bytes handed to the UI thread,
notifications, PSS (`Pss + SwapPss`) at the end, and the CPU of the 3 s
after the stream stops (idle).

### 520×400 window (the size of the other benches)

| Events/s | Mode | Process CPU | UI thread CPU | Frame CPU median / p95 | Paints/s | Rows built/s | To the UI | PSS | Idle after |
|---|---|---|---|---|---|---|---|---|---|
| 30 | naive | 3.5 % | 3.0 % | 0.97 / 1.56 ms | 30 | 184 | 16.8 MB/s | 13.9 MB | 0.00 % |
| 30 | diff | 3.0 % | 2.5 % | 0.65 / 1.54 ms | 30 | 93 | 16.8 MB/s | 12.3 MB | 0.00 % |
| 30 | feed | 1.9 % | 1.8 % | 0.47 / 1.04 ms | 30 | 93 | 1.0 MB/s | 11.3 MB | 0.00 % |
| 30 | feed@60 | 2.0 % | 1.8 % | 0.49 / 1.04 ms | 30 | 93 | 1.0 MB/s | 12.8 MB | 0.00 % |
| 100 | naive | 9.9 % | 8.6 % | 0.89 / 1.17 ms | 100 | 594 | 55.9 MB/s | 12.0 MB | 0.00 % |
| 100 | diff | 9.1 % | 7.8 % | 0.62 / 1.46 ms | 100 | 327 | 55.9 MB/s | 13.7 MB | 0.00 % |
| 100 | feed | 6.0 % | 5.8 % | 0.50 / 0.95 ms | 100 | 327 | 1.0 MB/s | 12.9 MB | 0.01 % |
| 100 | feed@60 | 4.2 % | 4.0 % | 0.41 / 0.89 ms | 60 | 261 | 1.0 MB/s | 13.0 MB | 0.00 % |

### 1200×800 window, 100 events/s

| Mode | Process CPU | UI thread CPU | Frame CPU median / p95 / max | Paints/s | Rows built/s | To the UI | PSS | Idle after |
|---|---|---|---|---|---|---|---|---|
| naive | 20.5 % | 18.4 % | 1.71 / 3.47 / 7.31 ms | 100 | 1 528 | 55.9 MB/s | 16.4 MB | 0.00 % |
| diff | 12.3 % | 11.1 % | 1.14 / 2.01 / 2.61 ms | 100 | 956 | 55.9 MB/s | 16.3 MB | 0.00 % |
| feed | 10.3 % | 10.1 % | 1.06 / 1.87 / 4.37 ms | 100 | 956 | 1.0 MB/s | 15.6 MB | 0.00 % |
| feed@60 | 6.9 % | 6.7 % | 0.27 / 0.44 / 0.59 ms | 60 | 661 | 1.0 MB/s | 15.6 MB | 0.00 % |

CPU % is of one core. Process CPU ranges over the 3 runs are in the raw
file (e.g. 1200×800 naive 15.8–22.8 %, feed@60 6.6–7.2 %).

**Reading**:

- At the larger window, `sync_from` alone (protocol unchanged) cuts the UI
  thread from 18.4 % to 11.1 % of a core and the worst frame from 7.3 to
  2.6 ms; diffs through a `Feed` reach 10.1 %; capping the feeds at 60 Hz
  reaches 6.7 % (−64 % vs naive) with 0.27 ms median frame CPU. At 520×400
  the same order holds with smaller absolute numbers (8.6 → 4.0 % at
  100 events/s).
- Rows built per second fall by 37–45 %: an update to an offscreen thread
  repaints nothing, one to a visible thread repaints one row. Bumps to the
  top while the list is at the top shift every visible row (that is a real
  change on screen), which is why rows are not cut further.
- The bytes handed to the UI thread fall from 55.9 MB/s (whole snapshots)
  to the 1 MB/s of actual changes only when the protocol sends diffs
  (`feed`). `diff` still copies the snapshots; it removes the rebuild,
  not the transfer.
- The naive mode here is already virtualized (only visible rows are built)
  and its "rebuild" is a redraw, so the gap is smaller than for a client
  that recreates its delegates per snapshot. Protocol (de)serialization
  (JSON) is not included in any mode.
- PSS stays within 11–17 MB in every mode (no data copies kept). The CPU
  of the 3 s after the stream stops is 0.00–0.01 %: feeds and batching add
  no timer while idle.

Not measured: other machines, macOS/Windows/X11 (the code is portable;
the numbers are from Wayland on Linux), physical displays at high refresh
rates, JSON protocol costs.

## Motion (Unreleased)

`script/motion-bench` (`examples/motion_bench.rs`), 2026-11-04, Asahi
Linux (M2), Hyprland 0.56.2, release build: a real 900×560 window on the
hidden workspace `special:omarchy-fltk-bench`, a `Sidebar` beside a
100 000-row list. The hover goes through the toolkit's own path
(`Sidebar::hover`: hover intent, then the layout motion), never synthetic
OS input. 10 expand/collapse cycles, medians
([motion-bench-2026-11-04.json](motion-bench-2026-11-04.json)):

| Case | Frames | Interval avg / max | CPU while moving |
|---|---|---|---|
| Expand 44 → 240 px | 13 | 16.9 / 19.7 ms | 15.2 ms in 226 ms (6.7 % of a core) |
| Collapse 240 → 44 px | 13 | 16.9 / 18.8 ms | 17.0 ms in 227 ms (7.5 %) |
| Expand, animations off | 1 | — | 1.8 ms |
| Idle before / after (5 s each) | — | — | 0.72 / 0.93 ms |

60 fps from the shared timer, about 1.2 ms of CPU per frame (layout of
the row and a full redraw of the sidebar and the content), and the timer
stops when the motion ends (`anim::pending() == 0`). The animations
setting was read from Hyprland (`source: System`). Details:
[ANIMATION.md](ANIMATION.md#cost).

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
