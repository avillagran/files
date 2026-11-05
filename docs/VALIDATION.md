# Validation of the files 0.0.2 checkpoint

2026-10-26 · Linux aarch64/Apple M2 Pro, native Hyprland/Wayland.
Functional checkpoint; **no global PASS**: strict startup and several extended
scenarios are still pending. UI source: 6fc0fe7, readiness 1785d41, NaturalKeyRef 7a62cdc,
du 22cef80/1cb4880. Shaders ab3e3ba are independent fixtures, not effects in the app.

## Memory with the tree and a truly loaded scene

10k/100k under `~/.cache/files-test`, a 960×540 logical window/output 2 = 1920×1080 physical,
FLTK scale 1, effects OFF. Requested interval 25 ms, five seconds after the geometry
**and** the loaded/view/count/stats or layout signal; steady = the median of the following second.
TreeSampler identifies PID+starttime and descendants/reparented processes already observed;
total PSS+SwapPss, never RSS. `complete` verifies reads of readable surviving
identities: **it does not guarantee complete lifetimes nor a simultaneous snapshot**. Exits
confirmed during a read generate events with identity/phase/reason and memory
unavailable; PermissionError/corrupt parse/an unreadable live process invalidate.

| Scenario | Sampled peak MiB | Peak MB decimal | Steady PSS+Swap KiB | Status |
|---|---:|---:|---:|---|
| Base 10k final checkpoint | 40.807 | 42.789 | 29320 | FAIL on the 40 MiB analysis threshold; no global PASS |
| Base 10k after visible du (earlier) | 33.980 | 35.631 | 30309 | PASS base gate; 35 MiB goal met in this run |
| Spatial 10k | 34.267 | 35.931 | 29305 | Sampled PASS with a warning: an exit during a read with memory unavailable |
| Groups 100k, complete metadata | 53.015 | 55.590 | 54241 | PASS ceiling; 50 MB goal NOT met |
| Spatial 100k, layout loaded | 47.152 | 49.443 | 42076 | PASS sampled gate/50 MB goal |
| du 100k, main+helper | 33.481 | 35.108 | 33543 | PASS own scenario; the geometry does not validate the 1080p baseline |

Data: `evidence/0.0.2/memory-*-ready-*.json`, `memory-baseline-after-du.json` and
`evidence/du/memory-100000.json`. The 100k cases predate the du button, whose 10k regression
does use the integrated binary. The first ready baseline gave a 39.730 MiB peak and 28.388 MiB
steady; it is kept together with the last one, without replacing the worst with the median.

Base after du: at most 13 FD/5 threads/1 process observed; steady 11 FD/5 threads, idle CPU
median/max 0%, Swap 0 in that run. du 100k: at most 20 FD/7 threads/2 processes observed,
the helper identified by PID/start/exe and reaped, steady one process. Independent maximum SwapPss
3168 KiB; **it is not equivalent to Swap 0 nor is it added to another temporal PSS peak**.
CPU at the moment of the memory maximum 77.93% of one core; it is not the global load maximum.
GPU memory is excluded from /proc; there is no effects context in these runs.

`memory-before-save-ack.json` FAIL 53.36 MiB and `memory-spatial-10k.json`
UNVALIDATED are kept. `geometry()` used to force a resize on every observation, causing transient
work; now it only requests a change if one is missing. The baseline after that fix gave
37.657 MiB. The first runs did not attest loaded/count: they are kept with
that limitation. The 35 MiB goal differs from the base stop 40 MiB and hard 60 MiB; in modern
scenarios 50 MB is the ideal, 150 MB the absolute ceiling, or an explicit specific budget.

## Startup

- Without the sampler: **81.419 ms p50, 111.423 ms p95/max,
  9/10 <100 ms, FAIL** (`evidence/0.0.2/startup-timing-only.json`).
- With the sampler: 87.508 ms p50, 198.730 ms maximum, 7/10 <100 ms, FAIL
  (`startup-tree.json`). It includes incomplete exit intervals that are not hidden;
  later ESRCH/expected-end fixes were tested with fixtures and do not yet
  have a new series for the binary. The /proc cost may delay receiving the marker.
- `POSIX_FADV_DONTNEED` is requested for the executable, without dropping global caches of
  fonts/libraries/OS. It is not the compositor's physical presentation latency.
- Non-blocking selectors/a monotonic deadline even with stderr open after the marker;
  any run >=100 ms fails, not just the median. The maximum still has to be stabilized.

## Functional acceptance and limits

- Integrated Cargo workspace: 95 tests (71 core, 16 UI, 8 helper), including 23 copier and 20 rename.
  The exported engines prepare the next interfaces; they are not complete UI
  features nor validated throughput/PSS. Logs in evidence/0.0.2/cargo-test.log.
- du UI 16 tests/Clippy/release; text helper 8 fixtures and du 6 fixtures. The text CLI still has no Space,
  selection nor highlighting; 256 KiB/2000 LF, UTF-8/UTF-16 BOM; see TEXT-PREVIEW and DU-HELPER.
- Python 32 tests of the sampler/gates/timeout/readiness/exits. They are not equivalent to a benchmark.
- `spatial-check.json`: pointer handlers on painted hit targets; position 72/48
  and zoom 1.15 recovered in a new instance. Normal close after ACK; selection/range,
  full queue/failure/retry/close, aliases, atomic multi-icon drag, incompatible TOML preserved.
- `ui-check.json`: Hackerman/Tokyo themes, partial TOML/symlink/replacement, corners and
  preservation of effects/future keys in config; Groups/List/rail/Spatial.
- Nerd FiraCode confirmed; review without evident tofu at 1x/2x/resize. FLTK captures
  1000×650 logical coordinates even when the compositor observes 4000×2600 physical:
  a test of layout/scale, **not native sharpness of every 2x pixel**.
- Visible apparent recursive du, without following symlinks, cancel/reap when the selection changes,
  navigating, hiding/closing. du 10k UNVALIDATED: the helper is too fast to be observed.
  Treemap, adapters and OS syscall cancellation are still pending.
- CUA does not enumerate the window; **OS events remain unchecked**, alternative
  clicks are not improvised. Offline navigation uses at most 4 workers and the first batch before
  statvfs; blocked syscalls still need later isolation/recovery.
- Missing: repeated navigation/multi-tab/multi-pane, OS event latency, global load
  CPU, future previews/operations/network/shaders and the matrix's platforms.

Binary `files` after the du UI (SHA256): `7bd0fe159f2c724beb9fd01620d1aaf9866d908f223725bd52cfe0ee1ec5ca7b`.
The 100k table refers to the earlier candidate; keep hashes/scenarios, do not
relabel its data as measurements of the new binary.

---

# Validation of files 0.0.1

2026-10-26 · Linux aarch64 (Apple M2/Asahi), Hyprland 0.56.2, native Wayland.

## Memory

A synthetic folder with **10,000 files**, without per-entry widgets. The geometry is
checked and five seconds pass before reading `/proc/<pid>/smaps_rollup`.
Window: **960×540 logical × output scale 2 = 1920×1080 physical**. FLTK's interface
scale is set to 1 to make the content reproducible.

| Preference | Real PSS | SwapPss |
|---|---:|---:|
| Follow Omarchy (rounding 0) | 29.03 MiB | 0.00 MiB |
| Inner rounded corners | 34.13 MiB | 0.00 MiB |

Both measurements are under the 35 MiB goal. The analysis gate fails
above 40 MiB and the requested limit stays at 60 MiB. PSS is used, never RSS.
Earlier measurements ranged between 26.40 and 33.39 MiB; one had 4 MiB of SwapPss.
The readings depend on page residency/sharing and on the physical size.
The main test does not capture images nor enable native window clipping.

Data: [memory.json](evidence/memory.json), [memory-rounded.json](evidence/memory-rounded.json).
Reproduce: `make memcheck`. Temporaries are removed on exit and live under
`~/.cache/files-test`; `/tmp` is not used to build or generate fixtures.

## Startup

Ten new processes, requesting `POSIX_FADV_DONTNEED` only for the
binary's pages before each launch. **The font, library and OS caches are not
globally dropped**. The external clock starts before creating the process and ends
when the signal emitted at the end of the first complete draw callback arrives.
It does not measure the compositor's physical presentation instant nor the end of the listing.

- Final series: **84.59 ms median**, **111.85 ms maximum**;
  **8/10** runs under 100 ms.
- An earlier series with the same architecture without client decoration got
  an 89.83 ms median and a 95.02 ms maximum (10/10 under 100 ms).
- **The strict goal of every startup under 100 ms is still pending**:
  there is variation in this shared session. The median is not presented as a limit.

Full data: [startup.json](evidence/startup.json).
Reproduce: `python3 scripts/startup-check.py`.

## Tests and limits

- `cargo test --workspace`: six tests passed, including streaming of 10k
  entries without an initial stat, formats, MIME and the watcher's atomic replacements.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `cargo build --release`: passed, without warnings.
- Helper protocol: a JSONL `unsupported` response correlated by ID,
  a 64 KiB request limit; `du` still has no implementation.
- `python3 scripts/ui-check.py`: Groups, List, expanded rail and persisted corners
  scenarios; Hackerman theme → invalid TOML kept → Tokyo Night through a
  symlink change → atomic replacement at the new target. All passed.
- Framebuffer captures of the **real Wayland window**: not mockups nor HTML.
  Capture is only enabled with `--ui-smoke` + `FILES_TEST_CAPTURE`; it is not part
  of the memory measurement. It is used to exclude overlapping foreign windows.
- CUA does not discover these windows; the scenarios apply internal actions to the model.
  **Delivery of physical keyboard/mouse events remains unverified through a driver**.
- **The outer outline is managed by Hyprland**. The corners preference modifies
  the inner UI; own clipping was left out because of FLTK's anti-aliasing limit
  and the incomplete validation of its cost. Detail in the design Findings.
- Windows/macOS and X11 have not been built nor run in this session.

Validated binary (SHA-256):
`78c296c932c65fea11c831b6c49a917dc8615eaa03115c625ab284010179ebbd`.

## Later diagnosis and suspension of measurements

`memory-checkpoint-final.json` keeps the peak FAIL 41786 KiB = 40.8066 MiB,
PSS 39434 KiB + SwapPss 2352 KiB at the same instant; steady 29320 KiB, Swap included.
First internal paint 138.447 ms. This peak is not replaced with the median nor with
the earlier PASS. Its binary includes the text helper and the exported engines, before
the UI integration of the renamer. The latter has no memory gate yet.

`memory-mappings-diagnostic.json`, `memory-mappings-geometry.json` and
`memory-mappings-static-mmap.json` are **acceptance=false diagnostics**: non-simultaneous
reads, different instrumentation, not gates nor a proven optimization.
In the first, heap 32144 KiB stays constant, while memfd:libdecor drops
25384→7112 KiB; a delta of 18272 KiB close to the tree's drop of 18173 KiB, without proving causality.
The corrected run records an initial logical geometry of 854×1065 and a final one of 960×540;
the Size of an mmap is not equivalent to PSS. Setting malloc's mmap threshold in another run
did not show a conclusive improvement, so it was not adopted as a fix.

Static review of FLTK 1.5.23: `create_shm_buffer` reserves 10000000 B by default,
or 2×frame bytes if that exceeds this size; pools can coexist. `buffer_release`
frees draw_buffer, defers a wl_buffer in use; `do_buffer_release` only tears down
an empty pool other than current_pool. This allows retaining the last large pool,
but it does not by itself attribute the observed peak. The Cargo registry and the global
backend were not patched. The optimization remains pending, keeping the FAIL.

The user suspended new 0.0.2 measurements/captures and asked to continue with 0.0.3.
The checkpoint is functional with known limits, **not a global performance
approval**. Later Rust integration tests are recorded separately.

## 0.0.3: visible renamer and creation

`c462fa1` integrates its own F2 form, patterns/sequence/literal/case, a before/after
preview, Apply/undo/redo and exclusive file/folder creation. A compact success notice
with Undo; errors/rescue keep a large copyable report. Unique generations
discard late previews; close/exit-after cancels and drains the ACK even with
discarded preferences. 106 Cargo tests (73 core/25 UI/8 helper), Clippy and release clean.

- `evidence/0.0.3/rename-ui.json`: functional PASS; bytes/names checked in the
  batch 2 + untouched fixture, the preview does not modify, apply→undo→redo, creation and absence
  of staging; exit 0 with the exact ACK of Apply-close. Framebuffer/internal handlers,
  **not OS events**. The `*-compact.png` captures show the already reloaded result.
- `initial-modal/` keeps the first UI, later fixed because it blocked after a success.
  The published `rename-preview.png` keeps its historical path. A capture without a sampler
  proves the root's exit; its cleanup does not prove the tree of descendants.
- `rename-resources.json`: **UNVALIDATED**, observed peak 42.402816 MB; a final
  root-dead observation invalidated the instrument. `goal_met=true` in this historical file
  only expresses being under an observed 50 MB, not validity: it must be read as unknown for
  acceptance. It is kept unmodified.
- `rename-resources-normal-exit.json`: scenario gate PASS, tree peak 34239 KiB
  = 35.060736 MB (33.4365 MiB). Cycle peak 34159 KiB, steady 30531 KiB; close with a pending
  Apply peak 34239 KiB, steady 30415 KiB. Independent maximum Swap 64 KiB, included in
  each temporal sum. Maximum observed 14 FD/6 threads/1 process, complete PID/starttime cleanup
  after exit 0. Steady window [since,until) after creation: CPU median 0%,
  maximum 27.15%; **it does not prove settled idle**, it may include the listing reload.
  Close uses the preview window before Apply; CPU median/maximum 0% there.
- An empty/error-free terminal read is explicitly deferred until exit 0+cleanup are
  confirmed, without turning the memory of an unreadable live process into zero. Events
  are saved in terminal_exit_observations. Peaks/errors keep the whole run;
  stable_until only limits the medians/steady CPU. It does not guarantee complete lifetimes.
- `memory-10k.json`: new 0.0.3 base loaded 10000/1080p, sampled PASS with a warning
  about a child that exited during a read. Peak 36715 KiB = 35.8545 MiB = 37.59616 MB; steady 28335 KiB
  = 27.6709 MiB, PSS 26607 + Swap 1728. The 35 MiB goal **not met**; the 40 MiB threshold not
  exceeded in this run. Steady CPU 0%; at most 13 FD/5 threads/1 concurrent process
  observed, two identities during its life; the child's gap documented. It does not remove the
  earlier 0.0.2 FAIL nor prove the absence of peaks between samples.
- `startup-timing-only.json`: new 0.0.3 series **FAIL**, 8/10 <100 ms, external p50
  87.449927 ms/p95 and maximum 150.310623 ms; internal maximum 137.918 ms. The variation is not causally
  attributed to rename nor hidden behind the median. It is still pending.

Build/captures/resources/base/startup were serialized. SHA of the binary after
cleanup: `5c54617793ef7dc20a32bb9632b59fe82a0b6ca4707f2aa616f1289ca70cadec`.
The batch 2 scene does not cover 100k/8 MiB of simultaneous UI+Arc+planner+PreparedRename;
the latency and cancellation of that large batch remain pending. Regex/dates,
undoing a creation, a durable crash journal and migrating folder layouts are still
pending. The functional checkpoint **is not a global PASS**.

## 0.0.4 partial: Space text and the FLTK wait contract

Implementation 992fe04: Space/Preview with a single cancel/drain helper, only visible
text, selection by real width + a byte→display map for TAB/control characters, original CRLF/UTF-8,
plain code; Markdown/HTML are still source. 126 Rust tests, Clippy
workspace/release passed. Error/truncation/change A→B→C/a normal drained close have
framebuffer/internal handlers, without OS keys/clicks. Captures in evidence/0.0.4.
**Milestone not closed nor tagged**: the external clipboard is still FAIL because the fixture has no Wayland
focus, serial 0; the user keeps the CUA/jev restriction. Do not use wl-copy to simulate it.

- wait-contract.json: two readable pipes→raw 2/callbacks 2/errno 0. The Rust wrapper 1.5.23
  would reject ≥2; the local replacement of the C contract accepts ≥0, retries only on EINTR with a pause,
  keeps other errors and drains operations/helpers before exit 1.
- New captures record successful raw 4/3/2, so they confirm that the valid
  multi-FD case occurs in the corrected scene. The original lacked raw; no exact
  retrospective causality is attributed. The original FAILs are kept.
- text-ui.json: functional_gate FAIL/clipboard FAIL, the selected content exact
  Unicode+TAB+CRLF and app::copy invoked; wl-paste gets no offer. The compositor's active window
  differs from the fixture, the trace shows set_selection serial 0 without enter.
  Native focus is still pending, without improvising an alternative control. The rest of the fixtures
  and exit 0 after PREVIEW_DRAINED checked; they are not equivalent to an approved clipboard.
- text-resources.json: **resource UNVALIDATED and functional FAIL**. Observed peak
  34858 KiB = 35.694592 MB (34.0410 MiB), steady 34858 KiB; observed Swap 0. At most 17 FD,
  6 threads/2 processes observed; CPU of the truncated window median/maximum 0%. The readable surviving
  tree and the observed cleanup complete; helper_identities_verified
  empty, helpers too short-lived to identify/measure them in this run.
  It is not a complete preview budget nor helper=0. A child exiting during a read
  keeps memory unavailable. Requested cadence 5 ms, maximum interval 91.20 ms
  because of instrumentation/process/actions; the inherited 25 ms field corrected with a
  metadata note, without changing samples. This result is not used as a PASS.
- Build/capture/resource sequence separate. Main/helper hashes in the JSON after
  cleanup; the benchmark does not run wl-paste nor captures. No new 0.0.2 benchmarks.

Copy UI continues while focus/clipboard stays pending, per the user's
instruction. Missing: the full horizontal window for lines >4096 B, syntax highlighting,
Markdown/HTML rendering and the other previews; the scope of the current Space text is not inflated.

# Visible copy and reload (0.0.4 partial, 2026-10-26)

171 Rust tests (73 core/8 helper/90 UI), 36 Python, Clippy all-targets -Dwarnings and
release PASS. A reusable worker/generations/4 FS capacity, a selection snapshot
bounded to 8 MiB, consecutive reloads, a visible selection block while reloading,
rejection without loss and a retry when the batch is reduced. The suite was not serialized to
hide the original contention; earlier failures kept in copy-initial/.

Copy with F5/inspector, destination, eight pending/one active, pause/resume/cancel,
a conflict without overwrite, full bounded diagnostics/rescue and close with ACK.
Corrected capture copy-ui-1793025091272455448.json: internal handlers/real framebuffer,
not OS input. Source/bytes/SHA256/rwx/mtime/symlink and absence of staging,
active cancellation after 3120 files and a paused close before working. The first
capture had the toast over the controls and a copy conflict onto itself; it is kept.

Separate resource copy-resources.json: peak 34671 KiB (33.8584 MiB/35.503104 MB),
steady 32542 KiB (33.323008 MB), observed Swap 0, at most 19 FD/6 threads/1 readable
simultaneous process. Sampled PASS/50 MB goal for 16 MiB + a directory of 10k files + a symlink,
a 1000×650 logical window, not the 10k-row/1080p baseline. A known child identity exited
while smaps was being read: memory unavailable; complete only verifies survivors,
not lifetimes. 125 samples, maximum interval 104.188 ms against 10 ms requested.
Post-copy window CPU median 0/maximum 118.82%; not settled idle. Cancellation after
3280 files, the unpublished root cleaned, close 0 after ACK 5 and the observed tree reaped.

**New baseline FAIL**, not replaced by the copy PASS: copy-baseline-10k.json,
loaded 10000/complete metadata, 1080p verified; peak 55414 KiB (56.743936 MB),
steady 40484 KiB (41.455616 MB). The 40 MiB analysis threshold is exceeded, below the hard 60 MiB.
At the peak only the root 55382 PSS + 32 Swap, 5 threads/12 FD, ~407 ms from PID start; not attributed to the
helper. The later diagnostic acceptance=false copy-baseline-mappings.json has
another distribution: maximum 42001 KiB at 90 ms (anon 16016/memfd libdecor 14224), steady
30705 KiB (heap 12272/memfd 7112/anon 1952); initial geometry 854×1065→960×540. It does not
reproduce the gate nor prove cause/optimization/PASS. Non-simultaneous reads and a
different instrumentation cost; FLTK retention/transition remains a source hypothesis and
a research candidate, not a concluded causal diagnosis.

New startup without a sampler: external p50 90.644308 ms, p95/max 151.978267 ms, 7/10 <100,
**FAIL**. Internal maximum 120.429 ms. It is not the compositor's physical presentation;
global caches are not evicted. Evidence copy-startup-timing-only.json.

Separate throughput 64 MiB/1000 files/three repetitions and a 16 MiB smoke pass integrity;
see COPY-BENCH. No superiority is generalized nor is reflink turned into physical
throughput. files SHA d8f2934638a56d069428e55a2209948db1df1a562c1eef8a012dd010e4059c40.
No 0.0.4 tag nor global PASS: the Space clipboard FAIL/resources UNVALIDATED remain
preserved, and move/journal/resume, platforms and the rest of the ROADMAP are missing.
## Bounded images, checkpoint 0335e21

188 Rust tests, 36 Python, Clippy/release and release regressions text helper 8/du 6 PASS.
PNG/JPEG/WebP, alpha/EXIF 8 transformations, zoom/pan, the same single-flight cycle as
text. Framebuffer image-ui-1793027538323979457.json PASS; no OS input nor clipboard.

Resource image-resources-1793027605038038146.json **UNVALIDATED**: three helpers with
observed PSS and five of eight without a sample, 122 samples/max interval 92.070 ms. The lower bound
52440 KiB = 53.69856 MB **ALREADY exceeds the ideal 50 MB** (`observed_goal_exceeded=true`); the
incomplete coverage does not make that miss uncertain. At the peak 42152 PSS +
10288 Swap KiB in total; steady total 27800 KiB = 28.4672 MB, CPU 0 median/max over a 1 s window.
The independent maximums PSS 43478/Swap 24096 KiB are not added; source SHA/identities in the JSON.
Cleanup of the observed tree complete, but no total budget nor guaranteed lifetimes.

It is not repeated to hunt for a PASS, and the 32 MiB decoder limit is not claimed as PSS: the
image's JPEG input can be duplicated and coexist with decoded/source/output/wire/bitmap/viewport
plus the FLTK copy and Wayland buffers. P02 is still partial at 4 Mi pixels, 12/24 MP photos and a multi
LRU pending. A real IDCT is being prepared with the MIT/Apache jpeg-decoder, without adopting IJG. That preparation
without wiring does not count as photo support; see JPEG-SCALED-NOTES and IMAGE-PREVIEW.

## Partial rendered Markdown, after 16e6a7c

230 Rust (73 core/33 helper/124 UI), Clippy all targets -Dwarnings, release PASS. Scene
markdown-ui-1793032850488569491.json functional PASS: styles/tables/lists/Unicode
selection, resize, fresh revision/layout, relative links, symlink rejected, visible truncation
and close ACK. No OS input/clipboard. The original captures and sidecars preserve the false
positive of the first error assert; too-many was READY truncated, not a rejection.

Resources markdown-resources-1793032456170456020.json UNVALIDATED: 43.177984 MB peak/
37.692416 MB steady observed, PSS+Swap, Swap 0; 2/8 helpers sampled, 368 samples/max 74.676 ms,
19 FD 7 threads 2 proc, CPU 0/162.718% post-Ready, not idle. No rerun for a PASS. P04 partial:
embedded images and LRU pending. The cooperative 4 ms reflow had an 18.306 ms maximum in the resource run/
60.543 ms in the initial capture; the 201033-byte doc ~635 ms until the measured READY. See MARKDOWN-PREVIEW.
The 10k base 56.743936 MB and startup 151.978 ms FAILs, and the Space clipboard FAIL, remain in effect.

The earlier JPEG 16e6a7c: functional 12/24 MP baseline + EXIF PASS, resources 42.36288 MB sampled
with 2/2 helpers and 1 discovery gap, not complete lifetimes. Large progressive images rejected
because of the 32 MiB of coefficients, P02 still partial; do not turn the decoder limit into PSS. See IMAGE-PREVIEW.

## Markdown with embedded images/contextual navigation, resources suspended by the user

242 Rust (73/33/136), Clippy all targets -Dwarnings and release PASS.
markdown-embedded-ui-1793034469432638103.json functional PASS: real framebuffer
bitmaps, 5 occurrences/2 resources, capacity ledger 3164572 B; a large 320x320 bitmap with exactly 102400
pixels. Excess 4 MiB, remote/escape/symlink unavailable with a diagnostic; contextual
root/subdir/nested and returns/escape/close 0 ACK. Method handlers, no OS input/clipboard.

Embedded resources NOT_MEASURED: a new instruction of integration/capture only,
no new measurements/startup until it is lifted. An earlier attempt was blocked BEFORE launch
because of a competing build: markdown-resource-blocked-1793034332264805450 NOT_RUN/UNVALIDATED.
The unrelated build was neither suspended nor modified. Do not extrapolate the old 43.178 MB to the current binary,
nor confuse the 8 MiB auxiliary retained+scratch ledger with the tree's PSS. See MARKDOWN-PREVIEW.
No 0.0.4 tag/global PASS; P09 LRU across docs, HTML/video, base/startup and clipboard pending.


## Experimental visible HTML/CSS, resources suspended

252 Rust (73 core/33 helper/146 UI), Clippy workspace all-targets -Dwarnings and release
clean. html-ui-1793037143255536981.json functional PASS: the ephemeral native litehtml
engine really renders styles/CSS, a table and lists in Painter; visible UTF-8
selection, a 1000→800 media query with a fresh revision, scroll to the end, CSS/
images/script blocked, 32 KiB rejection, quick changes and exit 0 after the preview ACK.
Method: internal handlers + real framebuffer; no OS input nor clipboard. The
1000x650/800x500 logical capture does not prove the sharpness of each physical 2x pixel.

The adapter's metrics are approximate and CJK/cell clipping was observed; HTML links and
resources blocked, no selection preservation after reflow nor a bounded Gumbo allocator
beforehand. An optional native helper prepared next to the release,
portable packaging pending. P05 partial; see HTML-ENGINE-PROBE/DEPENDENCIES.

Resources/startup NOT_MEASURED because of the user's explicit phase. PSS was not read nor
was a benchmark launched. App/adapter hashes in the JSON; do not extrapolate a budget
from another binary. The baseline 56.743936 MB, startup 151.978 ms and clipboard
without focus FAILs remain, and the historical images/Markdown UNVALIDATED. No 0.0.4 tag/global PASS.


## Video: visible snapshots and seek, resource use pending

260 Rust (73 core/33 helper/154 UI), Clippy workspace all-targets -Dwarnings and release
clean. video-ui-1793037769557008693.json functional PASS: a Matroska FFV1 fixture
640×360/8 s, first frame red, seek 5 s blue, timeline click at 25% red, normalized EOF 7.8 s
blue, a quick change to the last seek, bad/symlink errors and close 0 after ACK. The helper's RGB
bytes and fresh revisions checked; real framebuffer images. The position
is the normalized request, not the frame's PTS. Method handlers, no OS input/clipboard.

P07 partial: no playback/audio/clip; rotation/SAR have pure tests but
no visual evidence. Large scenarios and a tree budget are missing. Resources
and startup NOT_MEASURED because of the explicit phase. RLIMIT_AS 768 MiB virtual and max_alloc 32 MiB
are NOT equivalent to PSS nor an authorization to exceed 150 MB. No global approval; the base/
startup/clipboard FAILs kept. Tool paths/versions/hashes and the app SHA in the JSON.


## Visible multi-page PDF (partial)

266 Rust (73 core/33 helper/160 UI), Clippy workspace all-targets -Dwarnings and release
clean. pdf-ui-1793038247910320945.json functional PASS: an own 2-page PDF,
RGB red/page 1→blue/page 2, zoom 2×/pan Y 60, the last changes coalesced,
malformed/symlink and close 0 after ACK. Real pdfinfo/PPM tools, hashes/version
and the app SHA in the JSON. Handlers + real framebuffer, no OS input nor PDF text selection.

FAIL pdf-ui-failure-1793038113834599772 is kept: the assert-pan handler was missing
in the verifier; the later revision implements it, and the FAIL was neither deleted nor changed.
The bounded PPM wire is validated with an exact EOF before exposing the bitmap, without a second RGB;
no budget before the header dimensions in the JSON supervisor. AS 96 MiB is not PSS. Complex/
password PDFs, extraction/selection, parser containment and other OSes pending.

Resources/startup NOT_MEASURED because of the explicit phase; the tree, new CPU/GPU
and startup were not measured. Close cancelled the job before the new recorded spawn; no claim of killing an active one.
The base/startup/clipboard FAILs and the historical UNVALIDATED remain, no 0.0.4 tag/global PASS.


## Video playback: functional, resource use not attributed

272 Rust (73/33/166), Clippy all-targets -Dwarnings and release PASS. video-play-ui-
1793038888536010466 functional PASS: real frames red→blue, pause ACK, resume/EOF
and close with the decoder observed alive (PID/starttime), confirmed absent afterwards.
Failure 1793038705143512044 kept: the verifier accepted an old sequence on resume;
resetting the sequence and requiring a fresh spawn/frame prevent that error. A bounded latest slot, no history.

The clip is 24 real framebuffers at ~4 fps assembled at 4 fps after exiting; not a continuous OS
recording nor a test of 12 fps UI. No OS input/audio/exhaustive media shaping nor benchmarks.
The main footer of 76.6 MiB under 24 in-process readbacks: **UNVALIDATED for attributing it
to the player**; no Swap/children/steady peak. capture_window adds buffers/readbacks
and confounds the component. The context sidecar keeps the figure, and a discriminating experiment is
pending, NOT run: playback without a clip and then captures alone when the user opens the phase.
Do not claim compliance or non-compliance for playback-only. Resource/startup NOT_MEASURED,
historical base/startup/clipboard FAILs intact, no 0.0.4 tag/global PASS.


## Audio: visible metadata

276 Rust tests (73/33/170), Clippy workspace all-targets -D warnings and release
clean. audio-ui-1793039367932469197.json functional PASS: synthetic FLAC/WAV,
real title/artist/album/duration/codec/rate/channels, Unicode selection,
clipped tags, quick changes, invalid/symlink and normal close with ACK.
Handlers and the real framebuffer; no playback nor clipboard/OS events tested.
External fields sanitized, no source code presented as metadata.

Resources/startup NOT_MEASURED because of the phase, with no new budget runs.
Do not extrapolate PSS from another binary nor from captures. P08 is still partial; pending
waveform/playback/cover art/archives/Office and the tree budget.


## Space for folders

279 Rust tests (73/35/171), Clippy workspace all-targets -D warnings and release
clean. folder-ui-1793040015107090730.json functional PASS: 8 B/5 entries,
symlinks not walked (a cycle and an own external 1 MiB file), an empty folder,
rejection of a file that replaces a listed folder, quick changes and a normal
close after ACK. The preview_directory protocol requires a directory; du keeps its API.
No filesystem I/O on the UI; internal methods/real framebuffer, not OS input.

Resources/startup NOT_MEASURED because of the phase. Do not extrapolate the earlier du measurement
to this binary. Offline mounts, treemap and a concurrent snapshot pending.


## ZIP: real read-only catalog

288 Rust tests (79/37/172), Clippy workspace all-targets -D warnings and release
clean. zip-ui-1793040518449068984.json functional PASS: Unicode, symlink,
traversal/drive and duplicates; raw non-UTF-8 escaped; empty; 2500 entries with
130846 B/1620 lines and a TRUNCATED notice; malformed and 10001 entries rejected;
quick changes and a normal exit after ACK. No extraction nor CRC/data checked.
No entry of the ZIP file created a file in the destination fixture.

Method: handlers and the real framebuffer, not OS input/clipboard. Closing before the
new recorded spawn is not equivalent to killing an active one. Resources/startup NOT_MEASURED
because of the phase; do not extrapolate a budget. Z01/P08/P09 are still partial.

## Standard window and menus — 0.0.4, 2026-10-26

window-ui-1793042474907145360.json bounded functional PASS. Three real instances:
normal by default (Hyprland floating=false/fullscreen 0), the fullscreen 2 choice
observed and saved, reopening fullscreen and back to/reopening normal 0. The TOML
fields effects=false/future.value 42 kept, three exits 0 after ACK/drain.
Changes from the centers of real hit-testing targets through internal handlers.
Mutually exclusive Preferences/Actions menus with Nerd, exclusion of every header
control, four corners and a free band; layouts 845×1061 and 1728×1117. Captures
of the FLTK framebuffer; they do not include the compositor's outer decoration nor prove
OS input. The native bridge is not called from the scene; gesture/cursors under OS input PENDING.

292 Rust tests (80 core/37 helper/175 UI), Clippy workspace all targets -Dwarnings and release
clean; logs in evidence/window. The C++ bridge compiles against the real headers of the pin
fltk-sys 1.5.23, FL_API 10405/ABI 10400. No registry/OS change nor an active fallback.
Resources/startup NOT_MEASURED because of the freeze, nothing inferred from the captures' PSS footer.

Failures kept: window-ui-failure-1793041926965059030 expected a maximization
that the real Omarchy rule suppress_event=maximize ignores (an FLTK flag does not prove the WM).
The extra mode was removed before delivery; only the supported normal/fullscreen are exposed.
window-ui-failure-1793042207114132046: the fixture validated the menu's targets during
close, when the UI already clears all its hits; the readiness condition now excludes
closing. Three new normal closes prove the fix; SIGTERM is not reused
as a PASS. The initial compile variant/config paths/Clippy failures are also preserved.
Functional/build/accessibility detail pending in WINDOW-MODES.md.

## Visible Trash and restore — 0.0.4, 2026-10-26

`trash-ui-1793045192155795906.json` bounded functional PASS on release SHA256
`eca3344bec10a7ae3aa0081fff9b14c8582fbc6467840f2616e6145497d7e82c`.
Actions/Delete and Undo run the Linux engine on the same filesystem; a borrowed
selection with a preflight of 64 items/256 KiB and a single worker. No permanent deletion
nor copy fallback. Four own objects were sent (two files, a non-empty
directory and a symlink); the symlink's external target intact. Metadata 0600 and the
Trash Info format verified. Undo restored three, kept the new occupant of the fourth,
showed verified paths; when that occupant was removed from the fixture, the retry restored the
exact bytes. A raw non-UTF-8 name `%FF` round-trip and a batch of 65 rejected before
mutation, without truncation nor loss. Normal close 0 after ACK and the marker drained.

Method: centers of real targets and internal handlers, a real isolated fixture filesystem
in `~/.cache/files-test`, FLTK framebuffer 845×1061. It includes an error panel
with horizontal pan to read the full `.trashinfo`. No OS input, clipboard nor
close during an active syscall tested in this scene; cancellation and draining
are also covered by worker tests. The personal trash is not read.

314 Rust tests (98 core/37 helper/179 UI), Clippy workspace/all-targets with
`-D warnings` and release clean. Regressions for external substitutions in both
directions do not move a foreign inode as a rollback; the original FDs keep the current
location for diagnostics. A confirmed restore that only fails path resolution
removes the already consumed token and emits a warning; non-retryable receipts move to
bounded manual recovery without a stale Undo. Logs in `evidence/trash/`.

Resources and startup **NOT_MEASURED** because of the freeze; do not use the capture's main PSS footer
as the tree's budget. Pending: per-volume trash/functional EXDEV,
a catalog of persisted receipts, redo, a durable/crash journal, other OSes and the
64-item budget. O04 is still partial. The first test run failed because a limit
fixture repeated the same name 64 times after duplicate rejection was added;
`initial-test-duplicate-fixture.log` is preserved, the fixture fixed to unique names.
The two earlier preserved scenes tested the normal flow with earlier builds;
they do not prove the final identity/receipt fixes.

## Visible regex in the renamer — 0.0.4, 2026-10-26

`rename-regex-ui-1793046339654903640.json` bounded functional PASS: selection 2,
before/after preview without touching bytes, named captures, a regex error and
an expansion over 4096 B rejected without a plan; Apply/Undo/Redo/Undo keep bytes,
an unselected file intact, empty staging and normal exit 0 after ACK/drain.
Method: internal handlers and the real framebuffer (normal, managed by the compositor),
not OS events/clipboard nor a mass-rename benchmark. Release SHA256
`5a9b4e38cf6f469efd92951b248376fba15d660b6741a43490725da97aa89d8d`.

324 Rust (106 core/37 helper/181 UI; last UI run 181), Clippy workspace all-targets
-Dwarnings and release PASS. The regex is compiled once per batch, NFA 1 MiB/nest 64/
captures 64/segments 256, the 4096 B output counted in slices before reserving. Only
std, without enabling the Unicode tables with an unauthorized license; literals/ranges/dot and
UTF-8 captures available, Unicode classes/properties/casefold show an error.
The selected tree and the audit in evidence/rename-regex, with no new exception.

The earlier capture `1793046051262606727` had a disk PASS but a visual FAIL:
FLTK multiline in a 28 px footer hid the error reason. Kept with a sidecar;
a single/bounded drawing summary fixed and the regression tested. Resources/startup
NOT_MEASURED because of the phase; the PSS footer is not a gate. Dates, advanced Unicode and the 100k
budget are still pending. The Cargo adaptation of the pure benchmark only py_compile,
without execution/measurements nor replacing its historical evidence.

## Properties and permissions — 0.0.4, 2026-10-26

341 Rust (115 core/37 helper/189 UI), Clippy workspace all-targets -Dwarnings and
release PASS. properties-ui-1793049068223979012 functional PASS: two processes,
real fixture modes/bytes, rejection of a stale observation, a read-only symlink,
Reload and close 0 after ACK. Closing right after submit cancelled before
chmod; do not claim an active syscall. Captures/internal handlers, not OS input.
Details and the initial failures preserved in FILE-PROPERTIES.md and evidence/properties.
READY now requires a repaint after the command; clipped controls do not
receive hits. Resources/startup NOT_MEASURED because of the freeze; no global gate nor tag.

## Direct open and close traceability — 0.0.4, 2026-10-26

345 Rust 115/37/193, Clippy workspace/release PASS, open_job 4 regressions PASS.
open-close-ui 1793056978667673268 bounded functional PASS in 3 instances: a missing host open
visible, a fake open with an exact argv without a shell and exit 7, a symlink does not assume a folder,
a directory navigates, FLTK callback→request→drain→exit 0. No real viewer nor Super+W validated.
CUA 0 windows/hyprctl own PID present; OS path pending. The old earlier installed
borderless binary and the empty unframed FLTK callback are static findings, not a reproduced cause.
The earlier clipped error summary preserved with a FAIL sidecar; the new capture shows the cause.
OPEN-AND-CLOSE details the contract/limits; freeze NOT_MEASURED, no global PASS.

## Space treemap — 0.0.4, 2026-10-26

370 Rust 121/44/205, Clippy workspace all targets -Dwarnings/release PASS. Helper CLI 9 cases
PASS; space-map-ui 1793057823344999138 functional PASS: proportion 8:4:2, selection
inspector, enter/back, empty/zero/error/partial, real resize through modes, cancelling
a 20k walk, List frees and the close callback with a live helper, 11 PID/start gone/root 0.
Own fixtures only, handlers/framebuffer; native input isolated by a fixture flag, not OS input.
Earlier CLI registration/validation/Clippy/finished-Cancel scene failures preserved;
the first scene with non-isolated ambient interaction failed; do not take its reads as acceptance.
SPACE-MAP details apparent bytes, limits 2048/256 KiB, unknowns without area and no
snapshot/hardlink dedup. Resources/startup NOT_MEASURED because of the freeze; the footer is not a gate.
V09 real UI delivered, adapters/raw root/offline mounts/budget pending. No tag.

## Global hidden files, Space v3 and context menu (2026-10-26, 0.0.4)

388 Rust (126 core/44 helper/218 UI), Clippy workspace/all-targets -Dwarnings and
release PASS. HIDDEN-CONTEXT-AND-STATUS.md details the contracts and preserved failures.
`evidence/hidden-context/hidden-context-ui-1793059768952742165.json`: functional
PASS, two processes read/persist the SAME global show_hidden (default false,
true→new instance→false); four views, selection/raw hits/operations pruned,
visible normalized areas and a different apparent total with hidden files. The batch/
background/keyboard context menu invokes existing forms without opening through B3. Empty Frame label;
the marquee with an injected clock changes only the bottom 32 px. Real captures 1000×560,
the current theme, internal handlers; no OS input nor a test of minimize/native gestures.
Close callback→request→drain→exit 0, two helper PID/start identities observed and reaped.
The first UI report without child logging has a sidecar: it does not prove cleanup of the tree.
The old smoke FAIL order.len==entries.len, the type mismatch and the fixture ETXTBSY are
kept. The open test fixture uses an installed interpreter; production unchanged.

Resource/startup freeze NOT_MEASURED. The main PSS visible in the footer is not a tree gate
nor does it include Swap. Baseline 10k 56.743936 MB/startup 151.978 ms/clipboard FAIL and
the other earlier UNVALIDATED remain in effect. No 0.0.4 tag/global PASS.

## Icon grid (2026-10-26, 0.0.4)

391 Rust 126/44/221, Clippy/release PASS. GRID-VIEW.md: virtualized Nerd icons,
button/Ctrl+5, zoom, selection/range/reveal, global hidden files and the context menu. Pure
100k/recorder 1000 tests do not replace the app 100k gate. grid-ui 1793060477438092576
functional PASS 162 visible + 2 hidden, eight captures 1000×560/800×500 because of the fixture's own
window constraints, folder/back, raw/toggle, callback drain exit 0. Internal handlers,
not OS events. No preview/du/treemap helper requested: root lifecycle,
not a tree test because of an empty set. Glyphs that were clipped before and the failures of the initial
repaint/fixture fullscreen constraints/du ETXTBSY kept. Resources/startup
NOT_MEASURED freeze; thumbnails/LRU/specific persistence pending, no global PASS.

## History and bounded omnibar (2026-10-26, 0.0.4)

399 Rust 129/44/226 + Clippy/release PASS. NAVIGATION-HISTORY: two Nerd buttons and Alt+arrows,
64 lossless 4096 B paths/total stack and no UI probes, a new branch clears Forward, a rejected
save does not move the history. A borrowed 4096 B paste preflight keeps the text/replace.
history-ui 1793061306527375025 functional PASS: real navigation through owned folders,
a missing error recovered with the Back button, a visible 1 MiB rejection/later Unicode,
six framebuffers 1000×560 and callback drain exit 0. No OS events/external clipboard,
no preview/du/treemap helpers requested (root lifecycle only). Tabs/sessions still
pending. The build shared the host with an unrelated OmaBridge Cargo build without touching it. Resource/startup
freeze NOT_MEASURED; no global PASS nor change to the historical FAILs.

## F1 help (2026-10-26, 0.0.4)

401 Rust 129/44/228, Clippy/release PASS. SHORTCUT-HELP: F1/Preferences with 38 static
rows, paging/Esc/close/resize, only implemented features. Scene
help-ui 1793061761953895411 functional PASS 4 captures 1000×560/800×500, no file hit
under the overlay, selection/path preserved and callback open/drain/exit 0. Internal
handlers/framebuffer, NOT OS input; main lifecycle without requested helpers.
AT-SPI/i18n/terminal/other shortcuts and resources/startup NOT_MEASURED are still pending;
earlier global failures intact. No 0.0.4 tag/global PASS.


## Selected media: identity and invalidation (2026-10-26, 0.0.4)

MEDIA-LIFECYCLE M1: 410 Rust 132/45/233, Clippy workspace/all-targets -Dwarnings and
release PASS. The checked image carries the expected revision down to the real FD and comes back
with the source; Ready/stream require generation+revision. A non-recursive watcher, serialized epoch
and token, attach before probe, 1 s reconciliation/re-attaching the parent,
a single reusable worker without I/O nor a syscall join on the UI. Own ACK/refresh invalidates.

media-lifecycle-ui 1793064138443049803 functional PASS: four framebuffers 1000×650,
red/blue/green patterns, same path same-size+mtime replaced, modify/delete/rename/
parent replacement, rapid A→B→C, video deleted during playback, hidden without a new
helper, Trash ACK and close 0/13 helpers PID start gone/readonly probe ACK true. Only
internal handlers + FS fixtures + framebuffer, NOT OS input, no lifetime coverage.
FAIL 1793064084657403710 preserved + sidecar because of the ACK disabled in the instrument and
the wrong Trash name. No product cache in the fixture's source/config/data;
Trash keeps the legitimate original. Resources/startup NOT_MEASURED freeze; the footer's
self PSS is not a gate. LRU/thumbnails/embedded/non-UTF-8 IPC/other OSes/offline/minimize
are still pending. Historical FAIL/UNVALIDATED intact, no 0.0.4 tag/global PASS.


## On-demand thumbnails and widget loss (2026-10-26, 0.0.4)

MEDIA-THUMBNAILS: 423 Rust 132/46/245, Clippy workspace/all-targets -Dwarnings and
release clean. Grid/inspector share a borrowed RGB LRU 4 MiB + ticket before decode,
32 entries/64 KiB keys and 64 KiB demand; a separate 76800 B scratch. JPEG 12/24 baseline IDCT
+ 160 px, PNG/WebP and a video frame from the existing engines; raw Unix byte IPC. Source
FD proof/epoch, invalidated by event/timer; no I/O in draw, no repeated decode if the revision
does not change, a single helper coordinated with Space/du/Space map. No PSS budget proven.

thumbnails-ui 1793069315038209135 functional scenario PASS, 3 serialized instances:
7 resources/JPEG 12 MP 160×120/filename 0xFF/inspector without cloning/Space/cache/replace/delete/
modify/Trash ACK/folder/callback with a live helper→exit 0; notify suppressed, FD reconcile→exit 0;
an own C++ widget destroyed with a live helper→guard/failure drain/expected exit 1 without a panic.
45/2/10 IDs PID start gone; live observed 1/0/1 is not lifetime coverage. 5 real
framebuffers 1000×650, internal handlers, NOT OS input. Resources/startup NOT_MEASURED freeze.
FAILs 6839306230828 (marker), 7329848136770 (ESRCH), 7614908477249 (first paint + panic)
intact + sidecars. The original WidgetTracker panic without a backtrace: cause open; an idle 20 s
non-repro does not resolve it. The new guards never use removed widgets and cancel/
drain; they do NOT test the compositor/Super+W nor a durable crash journal. M2 partial, global P09/
embedded/Space LRU/other OSes/HiDPI/minimization and the global FAILs remain pending.


## Opaque paths in CLI/Request/full image (2026-10-27, 0.0.4)

RAW-PATH-WIRE: 427 Rust 136/46/245 + Clippy workspace all targets -Dwarnings + release clean.
Request PathBuf as a legacy string/raw Unix array 4096 B/NUL rejected, relative without URL decoding nor
normalization; CLI args_os/--. The first SpaceMap test expected the earlier rejection and
failed, then it requires a round trip of id 1/path bytes. No limit removed nor a new
runtime dependency (core dev serde_json already in the lock). Core stays without UI.

raw-path-ui 1793071898429792393 functional PASS: argv root 0xFE + filename 0xFF, Grid/
inspector→purple Space, full RGB verified and the real source dev/ino/bytes; two
framebuffers 1000×650/callback drain exit 0/3 helpers PID start gone. Internal handlers only,
NOT OS input. The earlier report 1677948673706 preserved, the second scene adds a Grid capture,
no resources/startup. Other raw GUI commands/other OSes/omnibar editing and embedded signatures
are still pending; the original Tracker cause open. NOT_MEASURED freeze; no global PASS.

## 2026-10-27 — FD signature of Markdown resources

RELATIVE-SOURCE: 434 Rust 141/47/246, Clippy workspace all targets -Dwarnings and release
clean. Wire 1793074260681793818 PASS with a deterministic replacement between probe/open,
root/intermediate replaced, symlinks/traversal/raw. Wire FAIL 1793074121342824158
with a sidecar is kept (the helper's initial allowlist was incomplete).
UI 1793074268202431350 PASS: 5 occurrences/2 resources checked, 5 framebuffers 1000×650,
root→subdir→nested→subdir→root, callback/drain/exit 0. Capture without a sampler: cleanup
only proves the root, not the tree of children nor a budget. No OS input/clipboard.
Resources/startup NOT_MEASURED freeze, historical FAILs active, no 0.0.4 tag/global PASS.
Watcher/reconcile/post-publication for embedded resources still pending; see RELATIVE-SOURCE.

## 2026-10-27 — Publication and invalidation of Markdown media

DOCUMENT-MEDIA: 442 Rust 141/47/254, Clippy workspace -Dwarnings and release PASS.
UI 1793076116463397564 PASS 2 instances normal/without callbacks; same path/size/mtime
replace, in-place, delete/recreate, rename, root replace/recover, 5 captures 1000×650.
The Loading candidate is not visible until watches+manifest proof+epoch; the old RGB absent
from the framebuffer and the FD signatures match the fixture. Callback→helper ACK→exit 0, 26 helper
identities per instance absent; no sampling/lifetime coverage. Readonly stop false/true
recorded, do not claim the first probe drained/no UI join. Contextual MD regression
1793075764251781328 PASS. Source NOT_MEASURED resources/startup freeze, no OS input,
global P09 and the historical failures remain open; see the limits in DOCUMENT-MEDIA.

## 2026-10-27 — Reduced progressive JPEG

JPEG-DC-PREVIEW 445 Rust 141/50/254 + 10 vendor lib, Clippy workspace/release clean.
UI 1793078251643976150 PASS: 12 MP 500×375, 24 MP 750×500, EXIF 500×750, 33 colors and the FD signature,
6 framebuffers 1000×650 (compositor 2x, not 2x sharpness). Close pending/ACK/exit 0,
8 helper PID/start absent, Z not accepted; a helper alive at close NOT_VALIDATED.
No new PSS/startup/CPU because of the freeze. The 32 MiB coefficient cap did not increase: 1 i16/block,
AC entropy skipped/no complete integrity. Color matches the full codec at 1/8; the comparison
with Turbo 4:2:0 FAIL explained by chroma DCT 2 vs 1, the equivalent 4:4:4 PASS ≤3/255.
Archive/17 src/patch fuzz 0/licenses audited; the earlier formatting drift of 8 src
reproduced with rustfmt. Vendor doctests fail because of upstream images that are not included,
no false PASS for the command. See the JSON/logs in the jpeg-dc evidence; the P02 limits are still partial.

## 2026-10-27 — PNG reduced by rows

PNG-ROWS-PREVIEW: 453 Rust 141/58/254, Clippy workspace -Dwarnings/release PASS.
UI image-ui-1793081848018085943 functional PASS 9 framebuffers 1000×650; PNG 9/12/24 MP,
alpha/late EXIF/Adam7/zoom/corrupt, JPEG/WebP regression and the effective FD signature.
Compositor 2x/physical 2000×1300 does not prove the sharpness of every 2x pixel.
Close pending→ACK/exit 0, 12 helper PID/start absent or reused, Z not accepted;
a helper alive at close NOT_VALIDATED, no lifetime coverage nor OS input.
No next_frame/full-source RGB, decoder 32 MiB best effort is not PSS; RGB 3 MiB and a 64 KiB row.
Resources/startup NOT_MEASURED because of the freeze, historical baseline failures intact.
P02 partial (large WebP/APNG/ICC/quality) and global P09 pending.

## 2026-10-27 — Video buffers and large thumbnails

454 Rust 141/58/255 + Clippy workspace -Dwarnings/release PASS. VIDEO-PREVIEW
1793082933186610779 functional PASS 4 FB 1000×650, play/pause/resume EOF/close with an
active stream, 9 logged PID/start gone Z not accepted. The single Ready/Recycled slot receives the
RGB displaced by the UI; a 100-handoff test + State, no PSS/CPU savings claimed.
FAIL 1793082885016655179 preserved + sidecar: the fixture lost the entry, trigger
not established; the repeat isolates ambient input through an existing flag, NOT OS input.
Thumbnails 1793082861888227321 PASS PNG 24 MP/progressive JPEG 24 MP 160 px/FD signatures,
Grid-inspector/Space/raw/delete/replace/Trash/navigate/active close; 4 FB and 44 logged
helpers gone/exit 0, no lifetime coverage. Local budgets are not PSS.
Freeze in effect: resources/startup NOT_MEASURED; the old 76.6 MiB footer and the baseline failures intact.

## 2026-10-27 — Audio, effective FD and invalidation

458 Rust 141/58/259, Clippy workspace -Dwarnings and release PASS. AudioJob requires the
expected Revision before ffprobe and returns the actual FD proof after revalidating;
State keeps signature/generation. Deterministic probe A/open B/late Ready A
and a readonly gate without a fake ACK; they are not tests of real kernel D-state.
audio-ui 1793083931390323451 functional PASS 8 FB 1000×650 with notify suppressed:
same path/size/mtime replacement, in-place with mtime restored, delete/recreate,
rename/parent recovered; the new title and real FD stamp, deleting hides the metadata.
Root exit 0/ACK and 11 logged PID/start gone (Z with the same identity not accepted), no lifetime
coverage nor a live ffprobe at close tested. Audio is still metadata/WITHOUT playback.
Freeze NOT_MEASURED resources/startup; no OS input nor native 2x sharpness, P09 partial.

## 2026-10-27 — Audio playable with Pulse

463 Rust 141/58/264, Clippy workspace -Dwarnings and release PASS. AUDIO-PLAYBACK
UI 1793086873981565834 PASS 9 FB 1000×650, a silent 6 s source/the OS's Pulse output:
play/pause, seek by pointer keeps the selection, resume/EOF, hiding does not restart,
deleting cancels/hides and restoring revalidates. FLTK callback with the player PID/start alive
→ACK→exit 0, 14 logged identities gone/reused Z not accepted; no lifetime coverage.
No OS input/Super+W, audibility, PCM position/sample-exact seek nor native 2x sharpness.
The FFplay prototype native EOF timeout/dummy PASS, the stale Hypr signature and argv -infbuf 0 FAILs
preserved. Pulse AS 512 mmap FAIL and the 768 contract PASS are virtual containment, not PSS;
AS 768 like video/max_alloc 32 M, the 150 MB ceiling/50 MB ideal unchanged. No new resources/CPU/
startup/GPU because of the freeze, no 0.0.4 tag/global PASS; see AUDIO-PLAYBACK.

2026-10-27 after restarting Hyprland: native-inventory 1793087442753417655
confirms its own window mapped before/after, but the cua-driver MCP returns an empty
windows array. Normal auto-exit with ACK; no focus/key/click sent, no
content of other windows recorded. The jev-computer-use skill prevents a control
fallback; OS input/clipboard with focus/Super+W remain pending.

## 2026-10-27 — Own helpers on parent death

464 Rust 141/58/265 + Clippy workspace/release PASS. OWNED-HELPERS covers the common spawn,
the video stream, du and Space; a user open is independent. helper-crash
1793088449742352360 PASS 4 roles PID/start alive→root SIGKILL→adoption/reap 9 by a private test
subreaper. It may happen before progress; no group/manual helper kill in the PASS,
no active mutations/durable recovery/crash during Save/kernel D-state/grandchildren/PID 1 latency.
Relaunch with a private config/verified FD/exit 0 ACK, 3 FB 845×1071. Normal Pulse audio
1793088666411517841 PASS 9 FB and video 1793089019986522162 PASS 4 FB/active close, strict
logged PID/start cleanup. No PSS/Swap/CPU/startup because of the freeze nor OS input.

## 2026-10-27 — Video with opt-in audio (M11)

470 Rust 141/58/271, Clippy workspace/all-targets -Dwarnings and release PASS.
VIDEO-PREVIEW: a single ffmpeg RGB+Pulse stream, mute by default/M, real indices,
a typed source failure versus a recoverable output error. Native Pulse zero PCM
video-play-ui 1793091780741012397 PASS 5 FB 1000×650/9 logged helpers gone strict Z false/
hot-toggle ACK/pause/resume EOF/callback close 0 ACK. Recovery with the socket missing
1793091967178711219 PASS 6 FB/9 gone/visible error→mute→frames EOF/close 0 ACK.
Standalone audio 1793092217283190449 PASS 9 FB/14 gone/active close 0 ACK. Same app SHA
52cde7a05957ba702f5c50946f2eaa5130e01d76663d1e0117929bcc7029e820,
helper 187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.

Default arenas + AS 768 virtual Pulse mmap FAIL 1793091361377293413 preserved;
a child-only MALLOC_ARENA_MAX 2 allows the context with the same cap, functional contract
video-audio-arena-contract. No PSS/CPU/throughput claim. The fixture FAIL with PulseServer
set after Popen 1793091839762673007 kept + sidecar, fixed before the spawn.
Handlers/framebuffer, without OS input/audibility/PCM/A-V sync/native 2x sharpness.
No new resources/startup because of the freeze; old FAILs and P07/P09 partial, no global PASS.


## 2026-10-27 — PDF with effective revision (M12)

473 Rust 141/58/274, Clippy workspace/all-targets -Dwarnings and release PASS.
PDF-PREVIEW: a mandatory expected FD/metadata and render on the same FD/source proof in Ready,
the existing watch/token/epoch/1 s reconcile, a typed SourceUnavailable/new token.
pdf-ui 1793092913810767063 functional PASS 8 FB 1000×650 with notify suppressed: real RGB and
signature, a replace with the same size+mtime, in-place with mtime restored, delete/recreate,
rename file/parent recovery, hidden without a helper, pages/zoom/rapid/errors/close 0 ACK.
23 logged PID/start gone/reused strict Z false, no lifetime coverage nor a live helper at close.
The readonly_gate shared by audio/PDF keeps ReadOnly exit allowed ≠ worker ACK,
no FS join on the UI nor a kernel D-state test. No OS input/native 2x sharpness/PDF text selection.
App SHA 3dacd1b81f77cf85da634484cddc1640623dfcb3836433df160fcd456a74ab2b,
helper 187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.
Resources/startup NOT_MEASURED freeze, baseline failures intact, no 0.0.4 tag/global PASS.


## 2026-10-27 — Rendered HTML bound to the FD (M13)

476 Rust 141/58/277, Clippy workspace/all-targets -Dwarnings/release PASS.
HTML-ENGINE-PROBE: a mandatory expected FD, the adapter inherits that FD and uses pread,
the effective Revision after render/decode/revalidate, the existing watcher/token/epoch/reconcile
gate; readonly close permission ≠ ACK, no FS join on the UI nor new workers piling up.
Native build 1793093259627217246 and fd-contract 1793093345381869451 functional PASS;
the legacy CLI keeps compatibility, an invalid FD/FIFO/>32 KiB without partial output.
html-ui 1793093483630467339 PASS 10 FB/resize 800×500/notify suppressed: text/FD
signature after a replace with the same size+mtime/in-place with mtime restored/delete/recreate/file+
parent rename recovery, hidden without a helper; selection/CSS render/scroll/block/error/
rapid/close 0 ACK. 14 logged PID/start gone/reused strict Z false, no lifetime coverage
nor a live helper at close/OS input/clipboard/native 2x sharpness/real kernel D-state.
App SHA bb32f9725d40d6327c66897aa0a1595c7a2c113e28fd81206f41f0a83a83575c;
HTML 85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4;
helper 187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.
Approximate typographic metrics/visible clipping are still pending; NOT correct
shaping. Resources/startup NOT_MEASURED freeze, the footer's values are not a tree gate.
No 0.0.4 tag/global PASS, historical baseline failures kept.


## 2026-10-27 — Selectable extracted PDF text (M14)

480 Rust 141/58/281, Clippy workspace/all-targets -Dwarnings and release PASS.
PDF-PREVIEW: T/the Nerd button switches Page/Text of the same page, previous/next,
Ctrl+PgUp/Down and TextDocument selection; pdftotext uses the same expected/effective FD and Job,
mutually exclusive outputs/RGB freed, stdout 256 KiB/2000 LF/strict UTF-8 without NUL.
The String takes the Vec without a clone, caps before the helper/wire retention; no product cache/file.

pdf-ui 1793094189643854652 functional PASS 12 FB 1000×650/notify suppressed: ASCII text
pages 2/1, FD signature/pointer selection/Copy handler and back to Raster, plus the M12 lifecycle.
The PDF 256 MiB+1/HTML 32 KiB+1 source cap keeps the revision and no new children during the
reconcile interval: a stable capacity error NO longer forces a repeated source token.
29 logged PID/start gone/reused strict Z false/close 0 ACK, no lifetime coverage nor a live
helper at close. UTF-8/TAB/CRLF/caps and text latest/source gate tests, not a real Unicode PDF.
App 786c52d7c9afef200ad7c258514af96a531b4cf805be4e2b66f67614aad6be92;
helper 187c939eb981/HTML 85ea1b8009ef with no helper change. No OS input/clipboard/
selection on PDF geometry/native 2x sharpness. Resources/startup NOT_MEASURED freeze;
historical baseline failures intact, no 0.0.4 tag/global PASS.


## 2026-10-27 — Audio and video PCM on a private sink (M15)

481 Rust 141/58/282 + Clippy workspace/all-targets -Dwarnings/release PASS, 4 pure Python PASS.
AUDIO-PCM: the actual Pulse monitor over local Unix on a unique/own null sink, an explicit
device only for paired fixtures/ASCII 64 B, without a fallback nor a default change. Sources
WAV 96000 zero + 480000 tone→FLAC 440/video FFV1 + FLAC 880, 12 s metadata cross-checked.
audio-pcm-ui 1793096793305882328 functional PASS 2 FB 1000×650, tones 440/880 confirmed
and silence on pause/default mute RMS 0, the sink route by PID, callback with a live stream/root 0 ACK/
8 logged helpers gone/reused strict Z false, recorder reaped/module absent/default unchanged.
No physical speakers/A-V sync/PCM clock/exact seek/OS input/clipboard/native 2x/lifetime.
App b76ecfdb6c4f3d76d93f4ca6e02bbb8e14b044212cc570123bf70502e899749d.

AUDIO-PCM details 4 preserved harness FAILs, including the first failed module cleanup
recovered by nonce and an initial sink mismatch without an ID whose cause is NOT established.
Zero-route 6246104965614 zeros only; the initial PCM 6434340845151 with lavfi duration 10.042667
does not prove 12 s/preamble, fixed by a sidecar/an explicit final WAV. The history is not hidden.
ENOSPC .pyc and SIGINT 130 only in our own build, log preserved, cleaning the repo's incremental cache and
a retry without incremental 481 PASS; no global config/unrelated builds nor a causal claim.
Resources/startup NOT_MEASURED freeze, no 0.0.4 tag/global PASS nor PSS/CPU savings claimed.


## 2026-10-27 — On-demand GIF and inspector thumbnail (M16)

485 Rust 141/58/286 + Clippy workspace/all-targets -Dwarnings/release PASS. GIF-PREVIEW
the same video engine, profile 8 MiB/4 Mi pixels/300 s, exclusive gif codec/demuxer and an explicit
ignore_loop 1 on three paths; no new decoder/GPU/dependency nor product cache.
video-play-ui 1793098465429205562 functional PASS 6 FB 1000×650: paused red snapshot,
play red→blue/pause/resume EOF notice None despite NETSCAPE loopcount 0; Close Space no
new stream, inspector 160×90 first red/FD signature/pixel/capture. Active close 0 ACK,
10 logged PID/start gone/reused strict Z false, no lifetime coverage/OS input/native 2x.
App SHA e0a9e7f6c4a0e21aef2b7872518b4e6e275d76bf7c3a186b00df8c61e928f87a.
FAIL 1793098062479122223 preserved + harness correction: it counted ALL helpers after
ClosePreview even though the visible inspector was requesting a thumbnail; now it verifies stream+thumbnail
separately. EOF does not recognize a backend error as success. Alpha/disposal/PTS/zoom and grid GIF
not verified; resources/startup NOT_MEASURED freeze, no 0.0.4 tag/global PASS.


## 2026-10-27 — Video/GIF readonly close without a fake ACK (M17)

487 Rust 141/58/288 + Clippy workspace/all-targets -Dwarnings/release PASS.
VIDEO-SOURCE-CLOSE: snapshot/stream with a shared ReadOnly/Helper/Stopped gate,
the closing mutex against admission before each spawn, readonly permission ≠ Finished ACK;
Helper requires reap/try_wait Some at EOF. Selected items; the thumbnail lifecycle unchanged.
video-readonly-close 1793099666110053045 PASS 2 apps/2 FB 1000×650 with a SIMULATED 30 s hold:
snapshot and stream→callback→exit 0 with readonly pending true/drained false/no future child,
no late admission after the hold. Do not wait 30 s nor claim kernel D-state interrupted.

GIF regression video-play-ui 1793099692852727198 PASS 6 FB/red blue/pause/resume EOF/
inspector 160/source FD/active close ACK 0, 10 logged PID/start gone/reused strict Z false.
App SHA a7ab09cab0c7f89414fb6b650d586e1d46ee9d62155848373556bde5944ff2b8.
FS blocking during the Helper phase is still pending; no complete offline lifecycle/Super+W/
OS input/native 2x/lifetime coverage nor new resources/startup because of the freeze.
No 0.0.4 tag/global PASS. The intermediate E0061 arity error kept, final without warnings.

## M18 — video and GIF zoom/pan, 2026-10-27

VIDEO-ZOOM: the same Viewport 1–8×/Ctrl+wheel/+/-/0/drag, frames invalidate pixels without
resetting the transform; seek frees the RGB and keeps 3 floats, a new source/revision resets.
488 Rust 141/58/289 + Clippy workspace all targets -Dwarnings/release/fmt PASS. Scenes
video-play-ui 1793101116101351845 (GIF 11 FB/12 helpers) and 1793101152229152975 (MKV 9 FB/10)
PASS 1000×650/zoom 2 pan 48/36/kept across frames-paused-seek-resume EOF/reopen reset/
active close 0 ACK/logged PID/start gone Z false. Internal handlers, no OS input/clipboard/
2x sharpness/lifetimes, no photographic detail proven by uniform colors.
No 24-frame clip nor PSS/Swap/CPU/startup because of the freeze; P07/P09 still partial.

## M19 — visible preview navigation, 2026-10-27

PREVIEW-NAVIGATION: Nerd previous/next buttons + Alt↑/↓ share an Action,
visible order/hidden/filter; no UI I/O/preload. An end keeps state/revision;
a real change reuses selection/generation/cancel/drain and the last destination.
488 Rust 141/58/289 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-neighbor-ui 1793101665648168454 PASS 5 FB 1000×650/PNG-WebP-raw PNG 0xFF/
hidden excluded/FD source and pixels/real error + back/rapid last wins/callback 0 ACK/
7 logged helper IDs gone Z false. Hit target/accessible name verified; no OS input/
AT-SPI/clipboard/lifetime coverage/active helper close in this scene.
Resource/startup freeze NOT_MEASURED; no playlist/autoplay nor P09 completeness.

## M20 — non-destructive viewer rotation, 2026-10-27

PREVIEW-ROTATION: Nerd/R/Shift+R buttons only rotate the image/PDF/video/GIF view;
direct sampling of the original RGB, with no rotated copy nor modified files.
The numeric transform keeps the rotation across frames/seek and PDF pages/text-return;
a new source/revision/reset 0 removes it. F1 Help adds the real media shortcuts.
489 Rust 141/58/290 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-rotation-ui 1793102443334515413 PASS 14 FB 1000×650/asymmetric 4 quadrants with
checked framebuffer pixels/CW-CCW-reset rotation/video-GIF play/seek/PDF page 2 +
text return/original bytes intact/callback 0 ACK/21 logged helpers gone Z false.
No OS input/native 2x/AT-SPI/lifetimes/active helper close in this scene nor a test of
photographic color; 14 functional readbacks, NOT an isolated measurement of use.
Resources/startup NOT_MEASURED freeze; no global P02/P06/P07/P09 nor 0.0.4 tag.


## M21 DOCX — recovery and functional acceptance (2026-10-27)

500 Rust (141 core/67 helper/292 UI), Clippy workspace/all targets -Dwarnings, release and
fmt PASS. The 5 earlier ENOSPC fixtures run by the coordinator with the existing binary
PASS; the new full suite PASS, without skipping/relocating fixtures. FAIL logs intact,
the storage cause not inferred. A new XML regression reproduced RED and passed
with the fix; 10 real helper wire cases PASS. office-ui-1793105654289278359.json PASS 6 FB,
expected/effective FD revision and generation, raw Unix, a replace with the same size/mtime,
delete/recreate, CRC/DTD/cap/symlink errors, a Unicode 102 B selection, truncation at 256 KiB,
callback 0 ACK/9 logged helpers gone Z false. No helper alive at close/OS input/real clipboard/
faithful Word layout/other Office. Resources/startup NOT_MEASURED freeze; the incidental footer PSS
does not count. DOCX-PREVIEW details limits, review and evidence; no global PASS.


## M22 — tabs and state restoration within the instance (2026-10-27)

513 Rust (141 core/67 helper/305 UI), Clippy workspace/all-targets -Dwarnings, release and
fmt PASS. Final logs tabs-checkpoint-test, tabs-empty-clippy,
tabs-checkpoint-release and tabs-checkpoint-fmt; a copy in tabs-checkpoint-validation.json.
Scene tabs-ui-1793109776417107918 PASS: nine real logical framebuffers, A/B
with the same names and different content, independent state and changes without a pump,
scroll 120 kept, global hidden/empty pruning held after rebuild, raw 0xFF,
eight tabs and reopening with a new ID. Resize of a file/folder with metadata ready
800×500/440; CalculateSize omitted when it does not fit and present at 650 in the recorder.
The last tab exit 0 after ACK; one helper registered by PID/start disappears,
no zombies. No coverage of lifetimes/a helper alive exactly at close.

Three FAILs preserved: tabs-ui-1793108574031992206 (harness socket),
1793108595977296407 (scroll by a premature clamp) and 1793108996440627071 (rebuild
selected a folder after pruning). The regressions of paint during load/layout
and empty selection/recovery have tests; the original asserts kept.

Method: pointer handlers for visible targets; Actions for cycles and tabs
outside the strip. No OS events nor external clipboard. No test of native 2x sharpness.
Resources and startup NOT_MEASURED because of the freeze; the footer's incidental PSS does not count.
V10 partial/durable session pending; no global PASS nor 0.0.4 tag.


## M23 — durable session (2026-10-27)

527 Rust 141 core/67 helper/319 UI, Clippy workspace/all targets -Dwarnings, release/fmt
PASS; session-checkpoint-validation.json keeps the logs, including the fixture FAIL of a
dropped Receiver and two style Clippy FAILs. tabs-session-ui 1793111266679001110
PASS 6 FB/8 processes exit 0 after ACK. A real save→restore of A/B metadata,
closed raw 0xFF, an explicit argv neither applies nor autosaves, --no-session disabled; two
windows without takeover/stomp and invalid/future bytes preserved. Real Model
regressions against a late SessionOpened after selection/filter/nav/close;
a borrowed encode/decode cap, FIFO/symlink do not block, a replaced lock rejected,
a dir sync failure after rename returns Unsynced and a retry validates the real file.
SESSION-CONTRACT details reservations/limitations. Handlers/Actions, no OS input nor
clipboard; no resources/startup because of the freeze, no task recovery/global crash.
The three M22 visual FAILs intact. No global PASS/0.0.4 tag.


## M24 — native move (2026-10-27)

537 Rust 148/67/322, Clippy workspace all targets -Dwarnings, release/fmt PASS.
move-checkpoint-validation keeps the final logs and the syntax/type/lifetime FAILs,
a menu assert by index and the Clippy initializer; fixed without relaxing safety.
move-ui 1793113502303715506 PASS 6 FB, the filesystem checked before Confirm and intact
in Paused; then 4 roots inode/dev/mode/uid/gid/mtime/ns/bytes/xattrs, a literal symlink
and raw 0xFF. The Target keeps its selection, the source relists without the moved roots;
no-clobber and a real /proc EXDEV preflight without publishing nor copy/delete. A paused cancel/close
keeps the source, exit 0 after ACK. Core covers cancel after 1 root, changed IDs,
parent rename/replacement, a destination relocated inside the source, an uncertain
publication with a source hardlink and actors preserved. The queue counts the active FD/unread
ACK+pending; close cancels everything before waiting for the review.

MOVE-CONTRACT scope: native same FS only, 128 roots, other OSes Unsupported,
EXDEV transfer/journal/undo pending. No OS input/clipboard/resources/startup;
do not extrapolate the incidental PSS of captures nor promise inode-conditional Linux
protection against a hostile actor. No global PASS/0.0.4 tag.


## M25 — single-file EXDEV and visible recovery (2026-10-27)

586 Rust (193 core/67 helper/326 UI), three private/opt-in core ignored ones delimited,
Clippy workspace/all-targets -Dwarnings and release clean. Final logs
exdev-workspace-final-test, exdev-workspace-final-clippy, exdev-resize-release;
the first compile/FD assertion/Clippy FAILs are kept. Core real dev 63→75 and
three process pairs with a publish/delete/restore gap run separately (exit 73 without
Drop → new process 0); no SIGKILL/power loss/real fsync syscall failure.

exdev-ui-1793122272433112203 PASS: three cases dev 63→75, non-mutating review/pause,
exact bytes/metadata, tabs refreshed, explicit catalog recovery, normal
exit 0 after ACK and stages absent. Ten PNG IHDR match capture_dimensions.
The INITIAL 440×440 window proves separate/complete controls; long diagnostics and
title clipped, no full readability/OS clipboard. Handlers/Actions and the
FLTK framebuffer; no OS events. Scope: a regular nlink 1 file, not trees/batches.

Preserved: 1793121684715864566 FAIL stale Hyprland socket;
1793121723013735005 functional PASS with a sidecar that corrects an assumed resize to 440
(capture 1000×650); 1793122169103469951 FAIL the dimension guard detected an oscillation
440↔1000. The final scene uses an initial 440 instance; live resize UNVALIDATED, the
assertion was not weakened. No new resource/startup measurements: NOT_MEASURED because of the freeze,
the historical baseline/startup FAILs are still active; the incidental PSS footer is not a gate.
No global PASS nor 0.0.4 tag. New previews paused; features/UI continue.


## M26 — Columns/Miller (2026-10-27)

596 Rust 194 core/67 helper/335 UI and 3 delimited core ignored, Clippy workspace all targets
-Dwarnings, release/fmt PASS. columns-checkpoint-validation.json keeps the logs:
the initial build (match/borrow), a wrong budget fixture, the first
workspace WouldBlock recovery, a duplicated-FD regression RED and GREEN; an intermittent focused
FAIL of a missing input A without enough context, not reproduced exactly,
the fixture later with exclusive creation/an atomic ID/diagnostics and the final suite PASS.
No exact causal attribution invented. JournalDrop explicitly releases the flock
only in the creating PID; it does not drop the owner when a Request is rejected nor skip the IntentProof.

columns-ui-1793124416464738573 PASS 6 PNG IHDR and capture_dimensions match,
five 1000×650 + initial 800×440. Chain and A/B pruning, copy from the focused column
(A/nested versus the same name in B), exact bytes/original data intact,
filter/raw and the renamer payload; two normal 0 after ACK. Arrows and quick steps
through Model methods; rows/selector/confirmation through visible pointer handlers.
No OS events/external clipboard/live resize, nor full-chain sessions.
Additional unit regressions: 12 levels/at most 8 retained, focus before Done/
stale Stats, Ctrl/Shift on activation, a deferred + cancellable right-click menu, tag 3 migrates
without geometry and tag 5 saves with scroll. Long names/ancestors partially
clipped by scroll/clipping; do not claim full readability. Resources/startup
NOT_MEASURED freeze and the historical FAILs active; the footer's incidental PSS does not count.


## M27 — MC dual pane, 2026-10-27

DUAL-PANE-CONTRACT and `evidence/0.0.4/dual-checkpoint-validation.json`: 603 Rust
(194 core/67 helper/342 UI), 3 pre-existing ignored, Clippy/all-targets -Dwarnings,
release/fmt PASS. The last two changes are renderer labels only; the final framebuffer
confirms per-pane filters and the basename before the path context.
`dual-ui-1793126826929914048.json` functional PASS, eight PNG with exact IHDR
dimensions, 2 processes exit 0 after ACK. Copy/move raw 0xFF→path 0xFE with real bytes/names,
non-mutating review/paused, the job finishes after focus/a closed pane, B's selection/query
preserved. Unit tests for context menu/joint budget/rename+Undo in both listings and
a single worker with a blocked backend. No OS input/clipboard/live resize nor metrics.
V07 partial: persistence of both panes/other OSes/other views/gates pending.
Metrics NOT_MEASURED because of the freeze and the earlier FAILs kept; no global PASS.


M27 report correction: `dual-title-fmt.log` was a FAIL on a single line because of
wrapping, even though it had been described as PASS. Original preserved. Formatting applied
with no functional change and `dual-format-correction.log` records fmt --check exit 0.
The earlier Clippy/release/scene/installed hashes remain valid; M28 WIP is not rebuilt
or installed for this whitespace-only correction.


## M28 — internal clipboard, 2026-10-27

624 Rust 203/67/354 PASS (3 core ignored), Clippy all-targets -Dwarnings, release without
warnings and fmt exit 0 verified in clipboard-checkpoint-validation.json.
clipboard-ui 1793130549985863820 scoped functional PASS 8 FB exact IHDR/2 root 0 ACK.
Raw Copy, partial Cut 2/retry/cancel/replacement rejection/an old ACK over a new buffer,
menus 800×440 without a toast and an own-text callback immediately + after the event loop exact.
The canonical FLTK constant true versus the Rust CString false shows the cause of the earlier
preflight, not a loss of focus/selection. FAILs 1793129984528710931/1793130010623243148/
1793130118639376044/1793130164558745696 and the functional PASS 1793130287088020335 with the
Actions visual sidecar preserved. The context menu only left a toast strip
outside the rect without covering controls; suppressing it is optional cleanup.
Handlers/framebuffer, no OS events, compositor ownership/external MIME clipboard,
live resize nor an independent process tree. Resources/PSS+Swap/CPU/startup
NOT_MEASURED because of the freeze; the incidental main footer is NOT a budget. O06 partial,
no 0.0.4 tag/global PASS. Install only an app/helper/HTML matching this scene.


## M29a — partial native clipboard, 2026-10-27

637 Rust (208 core/67 helper/362 UI), 3 ignored, Clippy all-targets -Dwarnings, fmt and
release exit 0: native-transfer-checkpoint-validation.json. The first C++
reading_epoch FAIL preserved. Scene clipboard-ui 1793133638367792338 scoped PASS:
9 PNG with exact dimensions, 2 root exit 0 ACK. M28 regression raw/partial cut/retry/
cancel/replacement/old ACK/own text; the 800×440 menu shows Paste from the system and the
handler WITHOUT a real gesture rejects with no operation/fallback nor a generation change.

isolate-input keeps the bridge disabled: it does NOT test an active registry/data_device,
a real serial/focus, ownership, sending/receiving MIME between applications nor
DnD. The State unit test injects callbacks; the pipe is private. M29/O06 remain open,
KDE/multi-source/DnD pending. Other OSes/OS input/live resize not validated. Resources
and startup NOT_MEASURED because of the freeze, the incidental main footer is NOT a budget.


## M29b — partial DnD receiver, 2026-10-27

642 Rust (208 core/67 helper/367 UI), 3 ignored, Clippy workspace/all-targets
-Dwarnings, fmt --check and release exit 0. `native-dnd-receiver-final-test.log`
exercises the real C++ TU with simulated proxies: busy→source_actions, an obsolete
drop_epoch before the pipe, ACK false without finish and COPY finish. The test is not the OS protocol.
Rust covers the complete Lease before ACK, a changed/filtered target, cancel/close and
private pipe→AuxIo→Source→review→queue/byte copy without touching the source.

`clipboard-ui-1793135944759735086.json` scoped PASS: 12 PNG with real dimensions,
2 root exit 0 ACK; hover, review, cancel and a real copy of the raw fixture, selection
kept. An isolate-input scene with private handlers/pipe/ACK: the compositor's
bridge, serial, negotiation, external reception and OS input UNVALIDATED.
The pinned FLTK hook has its own source/patch/license and a strong guard by hash;
earlier integration FAILs preserved. Real DnD availability is not inferred
from the fixture. Sender, KDE, multi-parent, other OSes and O06 are still pending.
Resources/startup NOT_MEASURED because of the freeze; the incidental PSS footer is not a gate.


## M29c — partial DnD sender, 2026-10-27

A COPY sender on its own window: it keeps the permission of the original press, waits for an
8 px threshold, captures raw names on press and verifies the Source/encodes in
AuxIo. Before requesting `start_drag` it checks window/seat/epoch/original serial,
expiry and the left button; releasing or cancelling invalidates before
taking the Model. A multiple selection keeps its batch when dragging and a click
without a drag is resolved on release. The joint reservation includes the desired
clipboard, proof, payload and published source; it is held until ACK or cleanup
(also bounded by a deadline). The source announces only COPY; `finished` or
`cancelled` never delete files nor consume a Cut. The own drop reuses the
existing review and queue.

648 workspace tests (208 core, 67 helper, 373 UI; 3 ignored), Clippy/fmt/
release PASS. A C++ fixture uses simulated proxies: COPY/serial/surface in an
already admitted helper, permission/epoch/deadline rejections and `finished` marks cleanup; it does not
prove the compositor's grab/focus nor a complete teardown. Scene
`clipboard-ui-1793136958438850356.json` bounded PASS: 13 framebuffers, real Source
and AuxIo, a simulated final dispatch, Cut kept and two closes with ACK.
There is no OS input nor proven Wayland interop; O06, KDE, multi-parent and adapters
remain open. The first scene `clipboard-ui-1793136933634122245.json` FAIL:
pasting own text after the event loop did not arrive. The PASS repeat neither explains nor
erases the failure; a stable global clipboard is still not proven. Resources and
startup NOT_MEASURED (freeze); new previews paused.


## M29d — correlated KDE, 2026-10-27

The KDE import requests `text/uri-list` and `application/x-kde-cutselection`
from the same offer and epoch, with a single ACK after validating intent, Source and
destination. Two pipes are drained concurrently with a common 3 s deadline; the URI list is
limited to 1 MiB and the metadata to exactly one byte `0` or `1` with EOF. An error,
cancellation, limit or offer change rejects everything without turning Cut into Copy.
The bridge's per-phase maximum is 8 FDs: four sends plus four transient
ends; after opening, two readers remain, which are closed before capturing the
Source. Other workers keep independent reservations.

652 workspace tests (208 core, 67 helper, 377 UI; 3 ignored), Clippy/fmt/
release PASS. A C++ fixture with simulated proxies verifies two MIME types of one offer
and rejection when the selection changes between EOFs. Seven transport tests include
both orders, invalid metadata, the limit, the deadline and cancellation after the URI
EOF. Scene `clipboard-ui-1793137466717410666.json` bounded PASS: two private
pipes, a simulated ACK, Cut opens the Move review, cancel keeps the source, two
framebuffers and exit 0 after ACK. Real KDE input/interop are still UNVALIDATED;
multi-parent, real DnD and O06 remain open. The intermittent text FAIL
`clipboard-ui-1793136933634122245.json` remains unexplained. Resources and
startup NOT_MEASURED because of the freeze, new previews paused.

## lib-fltk-omarchy v0.0.1 theme — 2026-10-28

First debug build PASS with the `config.h` and `FL/fl_config.h` guard; the evidence
`docs/evidence/0.0.4/fltk-omarchy-first-build-headers.json` records hashes and a
normalized comparison (only installation paths are excluded). Cargo.lock
contains one copy of fltk and fltk-sys 1.5.23 and the toolkit git `50a42927`.

`toolkit-theme-ui-1793181381907414143.json`: bounded PASS, two FLTK captures
1000×650 and a count of background pixels before/after replacing the symlink,
reading the shell's font.base and square Corners retained; normal close. It does not
check OS input, nor the application of the typographic scale to Painter. 668
workspace tests PASS (222 core/67 helper/379 UI; 3 core ignored), Clippy/fmt and
release PASS. Resources and startup NOT_MEASURED. The remaining toolkit components,
global parity and multi-parent are not finished; the earlier clipboard FAIL and the
native Wayland limitation are kept.

Later adjustment: Files' existing watcher validates colors and shell before
invoking the toolkit v0.0.1 reload; the toolkit's validation and read
can race against a simultaneous write. Scene
`toolkit-theme-ui-1793181731296656632.json` PASS: three framebuffers, truncated
TOML rejected with the earlier color visible, the symlink changed and exit 0.
669 workspace tests PASS (223/67/379, 3 ignored), Clippy/fmt/release PASS.
[Issue #5](https://github.com/avillagran/lib-fltk-omarchy/issues/5) asks the
toolkit to validate and keep its own last valid theme. NOT_MEASURED
for resources/startup; no OS input.

## Bottom help ellipsis — 2026-10-28

The marquee's initial pause signals truncation with `…`, computing the
width with Painter and keeping UTF-8 boundaries; while scrolling, the full
text is kept. `toolkit-theme-ui-1793181920822762898.json` provides a
real 1000×650 framebuffer, without OS input. 670 workspace tests and Clippy/fmt/
release PASS in the repeat; 3 ignored. The first suite
`target/status-tip-ellipsis-test.log` failed in the existing test
`kde_active_cancel_after_uri_eof_closes_both_readers`: the worker returned
Interrupted but a write to the pipe still succeeded. The isolated test and the later
suite passed; cause not determined, that failure is not declared fixed.
Resources and startup NOT_MEASURED.

## lib-fltk-omarchy v0.0.2 pin — 2026-10-28

First build `target/fltk-omarchy-v002-first-build.log`: header guard PASS;
`fltk-omarchy-v002-headers.json` confirms one copy of fltk/fltk-sys and normalized
header hashes. Workspace: 670 tests PASS (223/67/380), three core
ignored, Clippy/fmt/release PASS. `toolkit-theme-ui-1793212312149703731.json`
validates theme/rename/partial rejection in Files' framebuffer, without OS input.
`toolkit-v002-focus-fixture.png` shows a ButtonGroup ring in an
isolated toolkit window through `focus::press`; it is not a ring in Files.
Files' router, its Tab, focus and real input tests remain open
according to `FLTK-OMARCHY-KEYBOARD-MIGRATION.md`; it does not replace the current behavior.
Resource/startup freeze NOT_MEASURED. Earlier failures kept.

## Toolkit v0.0.7 pin — 2026-10-28

Isolated dependency change: `3f5978a3836e42eca4940ae5007fe21ec66d9c3e`, GPU
off. First build/guard and `fltk-omarchy-v007-headers.json` PASS: normalized
headers and a single copy of fltk/fltk-sys 1.5.23. 670 tests (223/67/380),
3 ignored; Clippy/fmt/release PASS. Scene
`toolkit-theme-ui-1793227096202584809.json` PASS for the existing theme, the real
framebuffer and internal handlers; it does not verify PopupMenu, KeyboardPanel nor OS input.
Resources and startup NOT_MEASURED. The later migration and W02 are described in
`FLTK-OMARCHY-KEYBOARD-MIGRATION.md`.

## Toolkit v0.0.7 strict theme — 2026-10-28

Files uses `Theme::try_load` at startup and `reload`/`last_error` on changes;
its duplicated TOML validation was removed. Files' watcher keeps the
`FILES_THEME` override. Scene `toolkit-theme-ui-1793227227928691866.json` PASS: five
framebuffers, truncated colors, a missing colors file and a malformed shell
keep the last palette; `last_error` reports each rejection and a valid rename
recovers the theme. Normal close; internal handlers, not OS input.
669 tests (222 core/67 helper/380 UI; 3 ignored), Clippy/fmt/release and the header
guard PASS. One copy of FLTK/fltk-sys 1.5.23, toolkit `3f5978a`, GPU off;
resources and startup NOT_MEASURED. A test of the removed local validation
was deleted; its cases are covered by this real scene.

## 2026-10-30 — Local search / keyboard / directory-size slice

Scoped functional PASS: `evidence/usability/usability-ui-1793391860625293505.json`,
679 workspace tests (228/67/384, 3 existing ignored), Clippy all-targets -D warnings,
release workspace and fmt clean. Six framebuffer captures include 800×440 and
1000×650. Filters, Unicode/raw names, selection pruning for Copy, marking with
separate focus and directory size/count recalculation verified. The helper result
is apparent bytes by name, not allocated size or a filesystem snapshot. The
fixture changed its directory from 8 B/5 entries to 12 B/6; both results match.

Internal handlers/painted hits and real framebuffer only; no native input,
clipboard interoperability, startup or resource gate. Historical FAILs retained.
Installed checkpoint and exact five-artifact backup hashes are linked from
USABILITY.md. The new broad usability goal and remaining ROADMAP are not complete.

## 2026-10-30 — Recursive filename search

Scoped functional PASS: `evidence/search/recursive-search-ui-1793393137003734053.json`.
688 Rust (230/70/388), 3 existing ignored, Clippy all-targets -D warnings,
release/fmt clean. Eight captures include duplicate basenames/parents, raw result
location+selection, independent hidden scope, 500-hit explicit partial state,
invalid filter and a result directory. Both 1000×650 and 800×440 were checked.
Root callback/drain/exit0; eight logged helper PID/start identities absent/reused,
strict Z rejection. Cancellation before ACK observed, active OS helper exactly
at cancel/close not proven. Earlier scene PASS retained with finished-before-cancel.

Unit regression rejected delayed text paste after query revision change. Helper
symlink/cycle/invalid-root/raw/partial limits and bounded wire tests pass. No
runtime mount fixture, viewer/native input/clipboard or kernel-D acceptance;
resource/startup NOT_MEASURED. Full goal, historical FAILs and remaining S01
capabilities stay open. Detailed contract: RECURSIVE-SEARCH.md.

## 2026-10-30 — Queue and measured directory rows

697 Rust (230/72/395), three existing ignored, workspace/all-targets Clippy
-Dwarnings, release and fmt PASS. Final compact-inspector-only change follows the
suite, checked by Clippy/release plus final scene. `FOLDER-SIZES.md` records limits,
root/epoch invalidation, bounded stream grammar, cancelled/stale ACK rejection,
root replacement and progress sink errors. Final scene1793395687494116398 scoped
PASS six framebuffer, including 800×440; existing usability1793395692067448178
and recursive-search1793395693275772935 scenes PASS on the same final binary.
Earlier pre-compact scene observed positive progress; final scene did not catch
one, preserved without seeking a rerun. Counts are apparent observations, not a
snapshot; external deep changes require refresh. No native input/resource/startup
acceptance, global PASS or tag. Historical failures retained.

## 2026-10-30 — Pattern marking and operation focus

701 Rust (230/72/399), three existing core ignored; full workspace tests,
all-targets Clippy -Dwarnings pass. New semantic tests cover filter-to-mark/show-all
with exact raw copy names, empty selections that survive rebuild without F5
fallback, preserving focus under Select All, invalid pattern/partial-load guards,
and real background menu activation plus a bounded selection submenu at 440×440.
Tab refresh now finds a surviving marked focus after pruning the old focused name.

Visual inspection of earlier direct-action/copy scenes found an old painted
Actions fallback; subsequent wiring adds its compact selection submenu and fixes
post-background-action clearing. Original reports and menu-scope-sidecar.json
remain, as do initial compile/import and regression-fixture failures. No native
input/resource/startup acceptance or global PASS; preview expansion stays paused.

Final release/fmt and selection-tools-ui1793396623817639503 PASS10FB at1000×650/
800×440. Submenu painted-target and background All, exact raw marked copy to
opposite pane/bytes, empty no-F5 fallback and root0ACK verified. Same final binary
sizes1793396638238738889/search1793396640181618221 PASS. Native OS events, native
popup interaction, lifetime coverage and resource/startup NOT_MEASURED remain.

## 2026-10-30 — Keyboard navigation and viewport indexing

708Rust230/72/406 +3existing ignored; workspace all-targets Clippy-Dwarnings,
release/fmtPASS. KEYBOARD-NAVIGATION records shared key handling, Shift/Ctrl ranges,
Groups visual order, derived5endpoints and empty-frame invalidation, viewport-only
List/Groups row indexing, focus strips and partial-row reveal. Functional100k
Recorder bounds are not performance/resource evidence.

Final keyboard-nav-ui1793398675053039028PASS10FB1000×650/800×440, raw marked copy
review, age-section endpoints/ranges, Columns hierarchy, Grid/dual owner, editing
and empty guards, treemap single selection and root0ACK. Same app selection scene
1793398677117404890, sizes1793398679365350932 and search1793398681208364663PASS.
No native OS keys/clipboard/lifetime/active-helper-close/kernelD proof. Resource/
startup NOT_MEASURED freeze and all historical failures/global limits remain.

Original full-suite failures remain: space-map timeout PID-marker ENOENT and KDE
post-cancel writer accepted byte. Test-only proofs now use Rust-spawned PID plus
strict/proc absence, and parent reader FD identities respectively, with a new
deterministic pre-marker timeout case. No cause asserted, no deadlines increased,
no production transport changes or global test serialization. validation-notes.json
and target/keyboard-nav-* retain details.

## 2026-10-30 — Path text editing and compact header

719Rust231/72/416+3existingignored, Clippyworkspacealltargets-Dwarnings/release/fmt
PASS. PATH-EDITOR documents byte-preflighted scalar cursor/selection, text-only
undo/redo, byte-count IME replacement, bounded history/geometry, revision-targeted
late paste, exactly-once explicit local URI editing and literal ordinary paths.
Search transition drops editor geometry and invalidates prior callbacks.

Final path-edit-ui1793400883075229579PASS11FB1000×650/800×440/440×440, owned data
byte checks and callback0ACK; painted pointer/confirm/cancel/pencil and view-selector
controls. Native input/publication/IME/grapheme/bidi/filter-caret/context-menu and
resource/startup acceptance remain open. Same final app history1793400883822026213,
keyboard1793400885767519762, sizes1793400887737740092/search1793400889575792607PASS.
Initial width-type compile failure and pre-adaptive visual scopes preserved in
validation-notes.json. All historical FAILs remain; no global PASS/tag.

## 2026-10-30 — Recursive size/date filters

SEARCH-FACETS.md defines exact sizes (decimal/binary units, checked comparisons
and ranges) and inclusive-after/exclusive-before UTC modification boundaries.
Metadata reads are candidate-only and no-follow; size predicates include regular
files only, even under negation. Unknown metadata never matches a negative facet.
Ordinary name/ext/type searches report zero added metadata attempts. These are
functional/source properties, not performance or process-tree resource gates.
The UI requires the helper's metadata capability flag for metadata queries.

Workspace: 723 Rust tests (234 core / 73 helper / 416 UI), three existing core
ignored. Clippy workspace/all-targets -Dwarnings, release and fmt checks pass in
`target/search-facets-verified-*.log` and `target/search-facets-fmt.log`.
Resource/startup freeze and historical FAILs remain; no native input claim.

Functional scene `evidence/search/recursive-search-ui-1793401991577102975.json`:
PASS scoped, 11 real FLTK framebuffer images (1000×650 and 800×440), private
production actions, actual helper IPC. Known-size/mtime files, size exclusion,
invalid units (no running job), zero added metadata for names, raw locate,
local hidden scope, symlink cycle, 500 explicit partial, directory navigation,
source hashes and root exit0 after drain ACK. Ten logged helper PID/start
identities absent or reused (Z not accepted), not lifetime coverage. This run
finished before cancellation; active-helper cancellation/close is not established.
No native OS input/clipboard, filesystem snapshot, or consumption claim.

Same-binary folder-size calculator regression: `evidence/folder-sizes/folder-sizes-ui-1793402024183320126.json`, scoped PASS. Folder totals remain independent of apparent-file search predicates.


## 2026-10-30 — Visual search criteria

Four bounded minimum/maximum/since/before inputs combine with advanced queries.
F4/Tab/ShiftTab/Enter/Esc/CtrlF/CtrlA share policy between runtime and private
fixtures. Separate input epoch rejects a late text callback after focus changes
without invalidating an in-flight job merely on focus. Composed terms precede
advanced text to preserve an unfinished quoted name; bad ranges/dates/byte limits
retain drafts and never admit helpers. No new worker/dependency or filesystem I/O
in UI. SEARCH-FACETS.md documents UTC semantics and append-only input limitations.

727 Rust tests PASS (234 core / 73 helper / 420 UI), three pre-existing core
ignored; logs `target/search-controls-final-tests.log`. Last suite precedes only
the compact viewport drawing guard and fixture row-hit instrumentation. Final
Clippy workspace/all-targets -Dwarnings, release, fmt PASS:
`target/search-controls-viewport-{clippy,release,fmt}.log`. First Clippy misnamed
getter FAIL preserved in search-controls-clippy.log; renamed input_revision,
without suppressing warnings or changing helper generation semantics.

Final `evidence/search/search-controls-ui-1793402932050805344.json`: scoped PASS,
18 real FLTK framebuffers (1000×650/560,800×440,440×440), four painted field hits,
keyboard policy, invalid range without running helper, clear preserving query,
stale callback rejection, no clipped result targets while drawer hides retained
results. Includes previous recursive/raw/hidden/500-partial/no-match/error/locate
regression and byte hashes unchanged. Root exit0 after drain ACK;12logged helper
PID/start identities absent or reused, no Z accepted, not lifetime coverage.
Cancel-before-ACK observed, not proof of a live helper at cancel/close.

First functional PASS report1793402706606379401/17FB preserved with scoped visual
FAIL sidecar `controls-validation-notes.json`: help clipped to an unreadable8px
sliver at440. Final source omits help without54px and rows/targets without46px;
final scene verifies retained result invisibility and restoration. No native
input/OS clipboard or PSS/Swap/CPU/startup gate; freeze and prior FAILs intact.


## 2026-10-30 — Discoverable commands and existing tools

COMMANDS.md describes a 46-entry static catalog, Ctrl+Shift+P and Actions-menu
entry, independent-term fuzzy matching/aliases/shortcuts, explicit disabled reasons,
mouse/keyboard selection, context recapture and normal Action dispatch. Selection
proof is exact sparse indices/dense bits (64KiB vector budget);100k dense indices
use12504bytes of u64 backing in the structural unit fixture, NOT app memory proof.
Proof is checked on dispatch rather than per-paint; cached directory-presence
and virtual rows avoid new full-list painting scans. Query256B, source epochs and
owner/generation checks protect stale pointer/paste/context. No filesystem I/O,
new worker/timer/dependency/shell interpreter or persistent command history in this
layer. Existing operation preflight/review/queue/ACK remain authoritative.

Final735Rust PASS (234core/73helper/428UI),3pre-existing core ignored;
`target/command-palette-guard-tests.log`. Eight directed module cases include
same-count selection replacement, sparse high indices/dense100k/oversize preflight,
stale listing/context, disabled dispatch, modal file-route blocking, late paste
and reopened epochs, stale pointer activation, and notice arriving after opening.
Final Clippy workspace/all-targets-Dwarnings/release/fmt PASS in
`target/command-palette-guard-{clippy,release,fmt}.log`.

`evidence/commands/command-palette-ui-1793404858332671715.json` scoped functional
PASS:11real framebuffers1000×650/560,800×440,440×440; menu and shortcut policy,
46entries, disabled/no-match, page selection, stale context rejected until explicit
recapture, stale text callback, actual6B directory calculator, raw0xFF name and
raw0xFE destination review through the global dual-panel copy queue, empty target
before confirmation, bytes copied afterward and original byte hashes preserved,
command-to-search and root callback while catalog open→drainACK→exit0. One logged
measure_directory PID/start identity absent/reused, Znotaccepted; no lifetime
coverage or helper vivo exactly atclose. Native popup/inputSO/OSclipboard/AT-SPI
are unvalidated; this does not prove every command's end-to-end workflow.

First scene1793404366025571814/11FB preserved (functionalPASS), before final
presentation/notice-guard refinements; see validation-notes.json. Initial Clippy
FAIL and script patch exception remain in target/tool history; CAS checked epoch
and two style fixes suppress no warnings. No resource/startup measurements or
comparability claim; freeze, preview pause and historic failures remain intact.


## 2026-10-30 — Bounded live search counters

SEARCH-PROGRESS.md defines provisional counters, no early paths, final root proof
and compatibility.742Rust PASS (235core/74helper/433UI),3existing core ignored;
`target/search-progress-admission-tests.log`. Includes counter bounds/regressions,
fragmented framing/caps/EOF/summary consistency, actual helper callback/root replace,
UI latest-slot/cancel/ACK/new-intent and repeated-Enter policy. Final product suite
precedes only readonly pre-key phase logging in the private fixture; final Clippy
workspace/alltargets-Dwarnings/release/fmt pass in search-progress-keyphase-*.log.
Three release CLI contract cases PASS, hash bound in helper-search-cli-contract.json.
Legacy one-summary/raw path, streamed mode/metadata/final counts, request-command
mismatch and explicit query error are checked against the real executable.

`evidence/search/search-progress-ui-1793407162101821172.json` scopedPASS20real
FLTK framebuffers1000×650/560,800×440,440×440 with actual helper IPC.40k owned tiny
files force candidate metadata work; positive128visited observed while running,
reportNone/rowhits0 and live framebuffer. No percentage or performance timing gate.
Cancel-before-ACK→new query and raw locate, size/date/negation/hidden/500partial,
invalid/no-match, four-field controls and root0afterdrainACK all pass; byte hashes
unchanged.15loggedhelperPID/start identities absent/reused, Znotaccepted, not lifetime
coverage. Active child exactly atcancel/close is not proven. Enter phase in this
scene was completed0hits before the key, so it correctly refreshed; active Enter
not restarting is separately unit tested.

Preserved FAILs and causes in search-progress-validation-notes.json:
1.1793406487049744417: CLI whitelist omitted new command; standalone existing
release exit2/usage preserved in search-stream-admission-before-fix.json. Fixed
admission only; no raised AS96MiB/deadline/data budget. CLI contract then added.
2.1793406698673456631: positive progress/cancel worked; fixture appended ext:bin
to previous needlequery. Fixed explicit CtrlF/replace, not parser behavior.
3.1793406826248172232: positive progress then completed40622visited/0hits BEFORE
Enter, proven in preserved log. Fixture incorrectly required progress after that
key. Readonly before/after key phase logging now checks the correct branch.
Clippy manual-is-multiple-of FAIL logs preserved; fixed without allowances.

No native input/OSclipboard, snapshot of mutable tree, offline/kernel-D instant
termination, early result batches, CPU/PSS/Swap/startup acceptance or globalPASS.
Prior failures, preview pause and resource/startup freeze remain intact.

## 2026-10-30 — Batch pause and failed-only retry

748 Rust tests PASS (235 core / 74 helper / 439 UI; three pre-existing core ignored),
workspace/all-targets Clippy with warnings denied, release and fmt PASS. Logs:
`target/size-controls-footer-{tests,clippy,release}.log`. Tests include finishing
an active measurement while paused, old cancelled ACK admission barrier, retry
only errors preserving measured/cancelled rows, invalid preflight preserving pause,
navigation/Stop reset, normal Model dispatch and catalog availability, plus status
geometry at 440/800/1000. No new dependency, engine, queue or worker.

Final `evidence/folder-sizes/folder-sizes-ui-1793408838174586064.json` scoped PASS:
ten real framebuffers at 1000×650/560,800×440,440². The real batch is paused before
admission, resumes via catalog painted Execute; directory→file replacement remains
Error, restoring that directory then Retry starts exactly one new MeasureDirectory
helper. Other measured labels/selection remain; retry disables after success.
Raw filename, filtered hidden scope, 40k directory/positive partial sample, zero,
sort, Grid, refresh/recalculate, navigation and root0/drain ACK are checked. Nine
logged PID/start helper identities are absent/reused; zombie is not accepted.
This is not helper lifetime coverage or helper-alive-at-pause/close proof.

Initial `folder-sizes-ui-1793408604914973870.json` scoped functional PASS remains,
with visual FAIL because rotating help clipped paused status; original nine PNGs
and `size-controls-validation-notes.json` retained. Final suppresses help/secondary
free space while size status owns the bar, using a compact label below640px.
Earlier command regression `evidence/commands/command-palette-ui-1793408614103585218.json`
PASS11FB verifies 49 commands/disabled and modal context/raw dual copy/440 layout
on the binary before the footer-only correction; not claimed as final-binary run.
Final size scene rechecks catalog resume/retry plus their disabled states.

Handlers/painted targets and owned filesystem fixtures, not OS input/clipboard,
filesystem snapshot, kernel-D/offline, process-tree resources or startup proof.
Freeze/preview pause, historical FAILs and broader ROADMAP remain open.

## 2026-10-30 — Search input cursor, selection and bounded geometry

754Rust PASS (235core/74helper/445UI;3pre-existing core ignored), Clippy workspace
all-targets-Dwarnings, release and fmt PASS. Final logs target/query-editor-cached-
{tests,clippy}.log and query-editor-release.log. Tests cover Unicode middle edits,
selection/word deletion, per-field undo/redo, rejected1MiB preserving text/capacity/
selection/epoch/history, no-op deletion preserving results, live Fake worker not
cancelled by caret motion, delayed paste rejected on caret/result-focus change,
geometry pointer/scroll/stale stamp/cap/release and width-call reuse.

`evidence/search/search-editing-ui-1793409739193969692.json` scoped PASS24FB at
1000×650/560,800×440,440²: Unicode logical selection/Delete/undo/redo, query drag
from actual painted byte positions, private stale paste rejected, execute ext:rs
against actual helper, retained report while moving caret, Tab and Home/End input
vs result focus, criteria middle editing,4096B end/start viewport440, existing
filters/size/date/raw/hidden/500partial/locate/directory/cancel/reopen regressions.
Original fixture hashes unchanged, root0/drainACK;13logged helper PID/start absent
or reused, zombie not accepted. Not all-lifetimes/helper-alive-at-close proof.
No40k progress sampling this scene; live worker retention is a deterministic state
test, not a claim of observed native input during a real scan.

Exact draft cache and point table are bounded and release on close; fixed logical
font13 metrics reuse on unchanged draft/area and binary pointer lookup are unit
call-count/structural checks, not CPU/PSS savings. No engine/dependency/thread or
persistent text cache added. Search-only; browser filter still pending.

CJK sample has missing glyph coverage; UTF8 selected bytes are proven, native CJK
rendering is not. Auxiliary bottom shortcut text still clips in minimum window;
main fields/controls bounded. Notes in query-editing-validation-notes.json retain
these limits. No native input/clipboard/IME/grapheme/bidi/HiDPI/globalPASS/tag0.0.4.
Resource/startupNOT_MEASURED; freeze, preview pause and historical FAILs unchanged.

## 2026-10-30 — Browser filter focus and editing

758Rust PASS (235core/74helper/449UI;3pre-existing core ignored), workspace
all-targets Clippy-Dwarnings/release/fmt PASS. Final logs:
`target/filter-editor-focus-{tests,clippy,release}.log`. No serial test-thread flag.
New/updated cases: CtrlF retains query and selected text, path/treemap exit plus
type-to-filter→List, middle edit drives visible results, per-input undo/redo,
text-copy result not private-file copy, F5 suppression, huge paste preserves
capacity/epoch/selection, delayed cursor/blur paste reject, pane/tab/path focus
release, MC CtrlShiftA kept, Backspace text vs parent after Enter.

Final `evidence/usability/filter-editing-ui-1793411701021271474.json` scoped PASS:
12 real FLTK framebuffers1000×650/560,800×440,440². Owned fixture, painted byte-point
click/drag, Unicode ñ range/edit/undo/redo, private selected-text copy (no OS
publication), F2/F5/F6 no dialogs,1MiB rejection, stale caret/pane/tab paste rejected,
queries retained per pane/tab and by CtrlF, path pencil reachable with query,
4096B viewport start/end440, listing Backspace→owned parent then root return.
Existing name/raw/filter/view/MC selection and real helper sizes8B/5entries→12B/6
also pass. Original file hashes intact, root0 after drainACK;2logged helper
PID/start identities absent/reused, zombie not accepted. No helper lifetime or
helper-alive-at-close proof.

Same final binary recursive search regression
`evidence/search/search-editing-ui-1793411753809197070.json` PASS24FB,13logged
helper identities cleaned (same strict rule), actual name/size/date/raw/hidden/
500partial/locate/directory, input/criteria editing and delayed paste regressions.
No40k progress sampling or performance comparison.

Originals preserved with `filter-editing-validation-notes.json`:
-1793411028309635495 FAIL fixture expected oversize error cleared by SelectAll;
 explicit known-error state now checked and next valid edit must clear it.
-1793411079837629215 FAIL fixture omitted existing FILES_TEST_TABS opt-in;
 no unknown command accepted; flag enabled only for explicit editing fixture.
-1793411105881791432 FAIL old1000×650 dimension after explicit1000×560 resize;
 actual dimensions now asserted per invocation, no product resize changes.
-1793411146396526185 functionalPASS12FB with visual guide clipped at440; final
 short guide and contextual inspector hint inspected. Existing informational
 Panel cerrado toast still clips at minimum width; remains pending.

Initial workspace failure retained: CtrlF-erases-query assertion intentionally
replaced with preserve/select plus path/treemap assertions. Initial check warnings
(fetch_update deprecation, unused append) removed with existing CAS convention and
Cursor-backed retained preflight tests. No new dependency/engine/thread/timer or
session wire. Active editor bounds/history/owner/epochs in FILTER-EDITING.md.

Foreign Cargo observed during final unit run; own Clippy mistakenly launched after
printing a new competitor, finished clean. Mistake recorded, no foreign signal.
Final release uses an idle-snapshot launch guard. No resource/startup gates or
comparisons; timings incidental, freeze/new-preview pause and historical FAILs
remain. No native input/OS clipboard/IME/grapheme/bidi/HiDPI/kernelD/globalPASS/tag.

## 2026-10-30 — Visible folder-size queue manager

Actions, Ctrl+Alt+Space and the new `sizes-manager` command open a modal over the
same current-root queue. Mouse and keyboard select rows, pause/resume admission,
cancel/retry one attempt, stop pending work, retry only errors, or enqueue visible
directories. Wrapped diagnostic details expose available errors plus escaped raw
root/name. Closing this tool does not stop calculations. See SIZE-MANAGER.md.

Final logs `target/size-panel-wording-tests.log` (764 Rust:235 core/74 helper/455 UI,
3 core ignored), `size-panel-wording-clippy.log` (workspace/all-targets -D warnings),
`size-panel-wording-release.log` and fmt/check pass. Initial Clippy style failures
(collapsible_if, fixture after test module) preserved and fixed without suppressions.
Queue IDs are monotonic per model and change on each attempt. Unit tests cover
cancel→retry before old ACK, stale completion, old row/panel controls and context
invalidation; active filename clone removed. These are structural bounds, not a
measured resource saving. Error UTF8 retained512B with explicit ellipsis on truncation.

`evidence/folder-sizes/size-manager-ui-1793415512530551936.json` scoped PASS:
14 real FLTK framebuffers1000×650/560,800×440,440². Paused raw row cancelled/retried
with fresh ID, obsolete Cancel rejected, browser file focus retained; actual helper
error after directory replaced by a file, readable detail, restore/retry exactly
one error while good observations remain, successful retry disabled. Async refresh
invalidates row controls; explicit recapture plus a new paused batch rejects old
panel Stop. All enabled controls fit440. Original size/sort/Grid/refresh/navigation
regressions pass, root0 after drainACK.10 logged helper PID/start identities gone
or reused; original Z not accepted, not lifetime coverage. Positive partial progress
was not observed; active cancellation/ACK is unit proof, not this runtime scene.

Same final binary `evidence/commands/command-palette-ui-1793415519946934466.json`
scoped PASS11FB:50 commands, search/aliases/disabled/actions, raw marks, reviewed
copy, actual folder calculator, search transition and root0ACK. Prior manager scene
1793414496788583300 PASS14FB retained; final rerun follows error/count wording changes,
including generic helper directory error instead of misleading “vista previa”.
No original40k full-tree hash or helper-alive-exact-close claim. Private handlers,
owned filesystem fixtures and real framebuffer; no native input/OS clipboard,
AT-SPI/HiDPI/offline/kernelD/snapshot proof. No new helper/engine/dependency/thread,
cache/session wire/timer. Foreign Cargo launch refusal recorded; own final checks
admitted by idle-snapshot guard, not proof of no later overlap. Resource/startup
NOT_MEASURED freeze, new-preview pause and historical FAILs remain. No global PASS/tag.

Local source checkpoint `4eff560` installed with report
`evidence/0.0.4/installed-checkpoint-1793415566779715821.json`.
All five installed artifacts and five backups independently rehashed exact.
App `cd616a62c0f3bb829604d47284ded5f2b4f80aeb6a7bf95d4d909760f0c03826`,
helper `4d76b4843506ffb11b7baa6b503ea8d67b2029cd264862c18219997eed5dafed`,
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user authorized install only; no source publication, global configuration or
other windows controlled.

## 2026-10-31 — MC operation strip

`OPERATION-BAR.md`: Preferences/Ctrl+Alt+B/catalog toggle, persisted through the
existing atomic preference snapshot/ACK pipeline; compact browser default off.
Eight existing actions with F-key/Nerd labels, current owner/generation on click,
no captured filename payload. Disabled input clicks are handled before editor
blur; enabled Copy/Move still review/preflight through the existing queue.

`target/operation-bar-final-tests.log`:769 Rust (237core/74helper/458UI),3core
ignored, PASS. Follow-up `operation-bar-geometry-tests.log`3 PASS after stronger
end-focus assertions for List/Grid/dual and MC-notice clearing; no algorithm or
operation-engine change after that full suite. `operation-bar-clippy.log`
workspace/all-targets -Dwarnings PASS. No suppressed warnings. Preference tests
cover boolean/default, unrelated fields and in-flight false→true coalescing.
Stale pane/generation/invalid index and dim text-focus/no-selection controls cannot
launch; geometry test covers440/800/1000 and clipped entry targets above the strip.

`target/operation-bar-release.log` and fmt/check PASS. Final
`evidence/commands/operation-bar-ui-1793416379455845906.json` scoped PASS20 real FLTK
framebuffers1000×650/560,800×440,440². Eight painted buttons; no-selection Copy
explains without a form, text-focused Copy preserves query/caret/editor and cannot
launch, Rename/New-folder reviews cancel, Help opens/closes, minimum controls fit.
Dual Move reviews exact raw sources/destination and cancel leaves both intact;
bar Copy then commits expected bytes through the existing global queue. Existing
51-command catalog, disabled/stale/paste guards, calculator6B and search transition
regressions pass. Same owned config reopens in a second process with bar enabled,
then disabled persists; unrelated fixture field and hidden flag intact. Both roots
exit0 after ACK. One logged helper PID/start identity absent/reused, original Z
not accepted; no helper lifetime/helper-alive-exact-close proof.

Images inspected at1000/440 and text-focused state. No new preview backend or
feature expansion. Widget handler/pointer fixture, not native OS input/AT-SPI/
clipboard/HiDPI/interoperability. No additional thread/timer/dependency/session
wire or filename cache; fixed eight buttons per paint. Build idle-snapshot guard
admitted own serial Cargo; no global performance comparison. Resource/startup
NOT_MEASURED freeze and historical FAILs stay; no global PASS or release tag.

Source `c114f9a` installed via
`evidence/0.0.4/installed-checkpoint-1793416459957302416.json`; all five installed
artifacts and all five backups independently rehashed exact. App
`9c2d9a9b3b13af1d029098da64762f29988c382a2fd0e9d1670e16b482f7bdc0`, helper
`4d76b4843506ffb11b7baa6b503ea8d67b2029cd264862c18219997eed5dafed`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user install only, no source push/global config/unrelated window control.

## 2026-10-31 — Literal content search

CONTENT-SEARCH.md defines the UTF8/no-NUL regular-file scope, exact literal1024B,
1MiB/file and64MiB total reads,32KiB scratch/KMP prefix table, metadata from the
read FD and post-read FD/name revision. Binary prefix matches cannot publish.
Limits/time/read/change failures are partial; binaries are explicit scope
exclusions. Every hit carries first LF-line/byte, no snippets/index/cache. Same
SearchStream command/supervisor/progress/last-intent/reap admission, new optional
report counters/match fields checked against request mode. Name-only no-read path
unchanged. Guided content/name drafts and all six input editors are bounded.

`target/content-search-complete-tests.log`:780Rust (240/79/461;3core ignored) PASS.
Tests include quoted/Unicode queries, boundary-spanning UTF8/KMP overlap, late
binary/invalid UTF8 rejection, per-file/total/time bounds, real FD replacement/
in-place proof, metadata AND raw/hidden/link exclusions, forged report mode/
counters, independent guided draft/escaping/undo/preflight and latest-intent ACK.
`content-search-final-clippy.log` workspace/all-targets-Dwarnings PASS; final
release/fmt PASS. Last suite precedes only equivalent Clippy if-collapse and UI
labels/hint/scope presentation; no operation-engine changes after it.

Four real CLI/wire tests `evidence/search/content-helper-contract-1793418304103621516.json`
PASS on the exact final helper hash. Final
`evidence/search/content-search-ui-1793418723201526437.json` PASS24FB1000/800/440;
raw plain text with quotes/backslash, name+size AND, pointer field5, stale mode
paste rejection, oversize retention, binary2/limit1/line2-byte5, narrowed complete,
advanced raw hit and invalid empty criterion, all existing recursive/criteria
regressions. Original source hashes unchanged, root0 after drainACK;16 logged
PID/start helpers absent/reused, original Z not accepted. Runtime cancel finished
before request; no active-child cancellation/close or full lifetime proof.

`content-search-validation-notes.json` preserves initial E0283 test reader, missing
optional-field fixture, incorrect fake successor-ACK expectation and Clippy style
failures. First24FB functionalPASS report1793418314346868379 remains before legacy
minimum footer hint/Archivos count clarification. Final minimum footer inspected,
Nombres/UTF8 scope explicit; drawer with no visible rows has disabled result targets.
No native OS input/clipboard/AT-SPI/HiDPI/mounted-FS kernelD/other-platform/global
resource claim. No new dependency/service/thread per file or persistent content
cache. Cargo idle-snapshot admitted serial own checks; no performance comparison.
Resource/startup NOT_MEASURED freeze, new-preview pause and historical FAILs stay.

Source `fcfd7ce` installed through
`evidence/0.0.4/installed-checkpoint-1793418877381327408.json`. Five installed and
five backup hashes independently match. App
`eeeba274d8ef1c5e5c57368aeda6223ff9d40d7806e3d80a433300fa14b91f47`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install only; no source publication/global config/window control.


## 2026-10-31 — Favourites/places hub (scoped)

793Rust244core/79helper/470UI PASS,3 existing ignored. Workspace/all-targets
Clippy-Dwarnings/release/fmt PASS on final source. Final
`evidence/bookmarks/bookmarks-ui-1793422800552758666.json` PASS11FB/four normal
process exits0 after ACK: raw paths/addition merge/label expected tuple/remove
folder intact/new ID/old stamp/440 close resolution/nonblocking lock/captured
retry after navigation/current directory replaced by file/new-process restore/
future format preservation.11 actual PNG inspected, all IHDR dimensions match.
Same-binary command regression1793422810269958601 PASS11FB/53commands.
See BOOKMARKS.md and bookmarks-validation-notes.json for exact bounds/scope/logs.
Initial private scene marker expectation and subsequent actual stale-current-path
load defect remain FAIL with sidecars; fixed checker and product regression
respectively. CtrlAltD close and stale Edit controls regressions also PASS.
No real input/clipboard/native accessibility/kernelD/other-platform/lifetime or
powerloss acceptance; no measured PSS/Swap/CPU/startup. Freeze/new-preview pause,
historical failures and broad goal stay open. No new watcher/service/dependency
or claim of performance gains from source caps.

Source `1b448f0` installed through
`evidence/0.0.4/installed-checkpoint-1793422888499506241.json`. Five installed
artifacts and five backups independently rehashed exact. App
`12d5d44825a0ea647f96833595ec309bfc6cc3a1d098b4a7106557eca23a61ad`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user install authorized, no source publication/global config/window control.


## 2026-10-31 — Copy/Move destination editor

798Rust244/79/475+3ignored, workspace/all-targets Clippy-Dwarnings, release/fmt PASS
(`target/destination-editor-captured-*`). Final scene
`evidence/destination/destination-ui-1793423889017087207.json` scopedPASS9FB1000/440:
Unicode path edits/undo, stale caret paste rejection, pointer end click,1MiB
preflight, actual copy18B/source hash intact, Move review remains on caret motion
but edit discards, cancel/raw protection/full replacement,4096B viewport at440,
root0 after ACK. NinePNG inspected; regular informational lines can clip, action
controls complete. Details and exact proof limits in DESTINATION-EDITOR.md and
`evidence/destination/destination-validation-notes.json`. Earlier9FB PASS retained
on preceding binary; final source adds control escaping/raw-display cap tests.
No native input/clipboard/AT-SPI/HiDPI/lifetimes/helper-active-close/resource proof,
no new thread/dependency/operation engine. Move runtime is review/cancel only.
General conflict policies remain ROADMAP. Freeze/previews/historical FAIL intact.

Source `9e850a5` installed through
`evidence/0.0.4/installed-checkpoint-1793423949236404441.json`. Five installed
artifacts and five backups independently rehashed exact. App
`7cadf781a7a08c5032c918bf58e8b80008e85ce367ced456375b9eeeb6e8ab1a`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install; no source push/global config/window control.


## 2026-10-31 — Copy conflict Skip policy

806Rust249core/79helper/478UI+3ignored PASS (`copy-conflicts-report-tests.log`),
final Clippyworkspacealltargets-Dwarnings/release/fmt PASS (`copy-conflicts-notice-*`).
Full suite before only final compact geometry/wording. Core initial/late collision,
uncertain cleanup actor data preserved, source errors, no directory merge,
entry/detail caps; queue capacity fixture shares completed+skipped payload cap,
keeps4MiB/2MiB headroom. Report totals survive diagnostic truncation.

`evidence/conflicts/copy-conflicts-ui-1793425513683762662.json` PASS8FB1000/440,
default Stop1publish thenerror, explicit Skip2publish/2skip, raw copied bytes,
symlink/directory/actor sentinel intact, fresh defaultfalse, mousecheckbox and
all4skip/0publish at440, Move policy blocked, source hashes intact/no stage,
root0drainACK. All8 PNG inspected. Existing pan/scroll needed for long detail
paths. Original1793425188533528112 functionalPASS/compactvisualFAIL preserved
with sidecar; fix actual inspector visibility for toast width/counts-first.

Core-only publication races are real owned-FS fixtures, not native UI/protocol or
powerloss proof. E0425 initial edit corrected/log retained. Foreign Cargo guarded,
not touched. No new benchmark: resource/startup NOT_MEASURED; historical FAIL,
new-preview pause/global goal remain. COPY-CONFLICTS.md records limits/pending.

Source `fdc8efa` installed through
`evidence/0.0.4/installed-checkpoint-1793425631915265384.json`. Five installed
artifacts and five backups independently rehashed exact. App
`ae7461f282a338ec73b6a0dc00c3436e83a8315535f82fa3fbbd894f09cd473f`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install, no source push/global config/window control.


## 2026-10-31 — Alternate Copy target names

818Rust252core/79helper/487UI+3existingignored PASS; final workspace all-target
Clippy-Dwarnings/release/fmt PASS (target/copy-names-counter-* logs).
Selected Copy validates actual opened root FD against captured Source before
staging; root-pinned symlink read and data FD checks, per-root name relation.
Core tests reject source replaced before open and later root replaced during
first-root progress. One extra temporary root FD, no new dependency/thread.
Final evidence/copy-names/copy-names-ui-1793429532169892092.json PASS9FB,
1000x650/440x440: five remapped roots, unchanged source bytes, directory/link,
raw/Unicode/literal-backslash, duplicate-plan rejection, occupied preservation,
stale paste, private clipboard and same-folder duplicate, root0drainACK.
All nine images inspected; source paths may clip, editor scroll remains usable.
First functionalPASS/visualFAIL report and sidecar preserved: pending fifth
name missing from title, fixed/tested. Initial E0282/warnings logs retained.
Incidental footer53.7MiB original/44.4MiB final is NOT tree+Swap/resource proof;
no causal/comparison claim. Resources/startup NOT_MEASURED freeze; native input,
MIME clipboard, full O03 and broad goal remain open. See COPY-NAMES.md.

Source32c77fa installed via evidence/0.0.4/installed-checkpoint-1793429595820606375.json. Five installed+five backups independently rehashed exact; app25c0ec3bcfa9/helper7e7dd8402607/HTML85ea1b8009ef. Per-user authorized, no source push/global configuration.


## 2026-10-31 — Folder-size manager display order

820Rust252core/79helper/489UI PASS,3 existing core ignored; workspace tests log
size-order-workspace-tests.log. Clippy all-targets-Dwarnings/release/fmt PASS,
final fixture-only Home action compiled by size-order-final-clippy/release.
Button/S cycles Queue/Largest/Name; bounded128 presentation indices only, queue
never reordered; selected record stays fixed while navigation follows display.
Pure tests keep partial/error unknowns after complete totals (zero is measured),
ties raw names; model test Delete targets original record after sorting.
Final evidence/folder-sizes/size-manager-ui-1793429969089468901.json PASS17FB,
all5 displayed IDs/order asserted; Home selects largest40kB, next9B/8B; NameEnd
selects E7B and opens corresponding detail. Existing raw cancel/retry, stale
controls, recapture, error/retry-only, hidden/filter, Grid and recalculation pass.
Three final PNG inspected (largest Home, Name detail,440 controls); all17 have
framebuffer dimensions checked by script. root0drainACK,9logged helperPID/start
absent/reused, Z not accepted; no lifetime/active-close/native-input proof.
First16FBPASS retained, final adds Home evidence (no production behavior change).
Incidental final440 footer50.8MiB and previous51.0MiB are main-only, exceed ideal
at face value; NOT tree+Swap proof/comparison. Resources/startup NOT_MEASURED under
freeze. No new engine/thread/dependency; global goal and historical FAIL remain.

Sourcecca8f3f installed via evidence/0.0.4/installed-checkpoint-1793430001081242152.json;5installed+5backup independently rehashed exact. App51c2cdd4feb7/helper7e7dd8402607/HTML85ea1b8009ef. Authorized per-user, no global config/sourcepush.


## 2026-10-31 — Omnibar command/place catalogs

823Rust252core/79helper/492UI+3existingignored PASS, workspaceClippyalltargets
-Dwarnings/release/fmt PASS (target/omni-catalog-*). CtrlL path Enter/commit
routes >query to commands and :query to places, query256B/control preflight before
clone/end-edit. No implicit execute/jump; same contexts/disabled gates/AuxIo.
Normal paths untouched; ./ prefix names remain literal. End-edit invalidates late
paste. Unit tests cover no execution/navigation/selection change, empty query,
UTF8oversize/control rejection retaining editor, literal paths and stale paste.
Final evidence/omnibar-catalogs/omnibar-catalogs-ui-1793430313608194102.json
PASS5FB1000/440; all5 inspected: >grid catalog/no action then explicitview-grid,
257Breject/draft/root preserved, :Destino filtered existing place then explicit
jump, actual ./ >/: directories navigated, catalog440, sourcehash/root0drainACK.
Private place list belongs only to fixture; no nativeinput/clipboard/OSprotocol,
new scanner/worker/dependency or full lifetime proof. Resources/startupNOT_MEASURED
freeze, previewsPAUSA/globalFAIL/goal remain. See OMNIBAR-CATALOGS.md.

Sourcee8302e4 installed via evidence/0.0.4/installed-checkpoint-1793430353104088658.json.5installed+5backup independently rehashed exact; appfb0eefd6fc41/helper7e7dd8402607/HTML85ea1b8009ef. Authorized per-user, no global config/sourcepush.


## 2026-10-31 — Incremental search scanner foundation WIP

New borrowed cumulative-prefix API shares scanner; optional first-hit checkpoint
without waiting128 visits, subsequent bounded cadence; no paths cloned by API.
3newtests exercise first1visit hit/raw prefix consistency, consumer BrokenPipe
stop, root replaced after provisional prefix => terminal error/original preserved.
10scanner tests and82helper tests PASS before last wrapper-only wiring. Initial
ClippyPASS before eager split; final ClippyFAIL unusedAPI preserved, corrected by
routing old counters wrapper through API with eager=false, no suppression.
Final formatter/tests/Clippy rerun NOT_STARTED: guard observed foreignCargoPID
2652552/start10686550 cwd/tmp/relb alive, untouched. Current WIP NOT accepted,
no release/capture/install; installed e8302e4/c063165 unchanged.
SEARCH-BATCHES.md describes wire/reader/UI/memory/generation contracts still
pending. Counters remain only installed incremental feature; visible rows NOT
implemented. FreezePSSSwapCPUstartup and previewsPAUSA unchanged, no globalPASS.


## 2026-10-31 — Incremental search bounded candidates foundation

ForeignCargo exited; own guard admitted fmt/workspace tests/Clippy. Full suite
830PASS256core/82helper/492UI+3existingignored, plus6codec targeted PASS including
2cases added after full suite. Workspace alltargetsClippy-Dwarnings/fmt PASS.
Actual scanner first-hit/prefix/rootreplace/BrokenPipe tests now compiled with
corrected counter wrapper; originalClippyunusedAPI101 and guard75 preserved.
Codec32hits/500total/256KiB pathcapacity, offsets/duplicates/malformedpaths/content
bounds reject atomically; bounded serde does not materialize33rdHit. RawUnix
roundtrip, equal-count path/kind/content-position mismatches reject; exact final
reconciliation accounts sorting, consumes candidates, no hash-only acceptance.
No new wire command/reader integration/UI/release/captures/install. This is a
foundation, NOT visible incremental results nor resource/globalPASS. Installed
checkpoint remains e8302e4/c063165; freeze and previewsPAUSA unchanged. See
SEARCH-BATCHES.md, target/search-batch-* logs.


## 2026-10-31 — Incremental search visible

837Rust258core/82helper/497UI+3existingignored PASS, workspaceClippyalltargets
-Dwarnings/release/fmt PASS. Fullsuite before fixture rawJSON/final wording only;
finalClippy/release compile them. Source legacy request/counters preserved, new
SearchBatches v1/32-entry frames borrowed helper/strict Candidates exactfinal.
One latestArc prefix slot in same reader; revision/atomicerror hides beforeACK,
finalselection byrawpath; no new engine/thread/dependency/persistent cache.
Final evidence/search-batches/search-batches-ui-1793432390766040716.json PASS7FB,
1000/440 all7 inspected. Realhelper held by isolatedfixture gate<=5s, livePID/start
whileprovisional/reportNone,40final exact/selection, cancel/rootreplace/newquery,
rawcontent440/provisionalLocate, sourcehash/root0ACK/6loggedhelpersgone-reusedZno.
This is artificialhold proof, NOTnatural latency/kernelD/lifetime/native input.
Samebinary content-search-ui-1793432406201870889.json PASS17FB/14loggedhelpers
strictcleanup/root0ACK;4PNG inspected (partialUTF8/complete/rawLocate/500limit).
Originals1793431914206752394/1793432088913319951PASS priorwording retained;
FAIL1793431925772843277 visiblepaintpredicate and1793432050269332012 legacy-only
cleanupmatcher preserved+sidecars; full originalassertion now waited/no product
navigation change. HelperSerdecompileFAIL fixed by coreFrame, no newpackage.
ForeignCargo guards75 intact, no processcontrolled. Resources/startupNOT_MEASURED,
footerNOtreeproof, previewsPAUSA and historical/globalFAIL remain. SEARCH-BATCHES
records localpath retention<=1MiB UI plus256KiBhelper, arrays/wire extra; NOTPSSgate.

Source90169c2 installed via evidence/0.0.4/installed-checkpoint-1793432548607567862.json.5installed+5backup independently rehashed exact. Appddf4b8f65a15/helper5d9239389b82/HTML85ea1b8009ef. Per-user authorized, no source push/global configuration/window control.


## 2026-10-31 — Content-search fragments

843Rust260core/85helper/498UI+3existingignored, Clippyworkspacealltargets-Dwarnings,
release/fmt PASS (target/search-snippet-*). Header plus optional bounded256B
snippet/UTF8marks/ellipsisflags, same single KMP/UTF8whole bounded read and FD/name
postread proof; no full-fileString/reopen/newengine/thread/dependency/cache.
Helper1088B previous tail+256B raw/normalized; arrays/wire separate. Per500 hits
adds128000B snippet capacity/population,4 UI populations<=512000B on top of path
budgets. NOT measured process tree or idealbudget proof. Worst JSON escaping test
keeps existing2MiBline/4MiBtotal wire caps. Bad positions/control/oversize reject.
Samebinary evidence/search/content-search-ui-1793434023389723179.json PASS20FB:
quote/slash literal, binary/invalid/limit no snippet, expected bytes/line/text marks,
Unicode context1000/440,1024Bliteral fullmatch/clipped256Bfragment, raw Locate,
old filters/500cap/errors, sourcehash/root0ACK/16loggedhelpersgone-reusedZno.
4content PNG inspected. evidence/search-batches/search-batches-ui-1793434087267418740
PASS7FB: provisional raw snippet+mark verified before terminal under realhelper
fixturehold, cancel/rootreplace/latest/Locate,root0ACK/6loggedidentitiesstrictgone;
compactSnippet PNG inspected. All27 dimensions checked; not all PNG visually read.
Original E0282 Optioninference fixed explicitMatch, E0063 old fixture field supplied;
edit-script ValueError left UI unchanged before corrected patch; logs preserved.
No OSinput/clipboard/lifetimes/native2x/latency/kernelD/resource/startup proof.
Resources/startup NOT_MEASURED freeze, previewsPAUSA/globalFAIL/goal open.

Source04e11bb installed via evidence/0.0.4/installed-checkpoint-1793434441549030735.json.5installed+5backup independently rehashed exact. Appadcdaac71a20/helper36e9302c3ba1/HTML85ea1b8009ef. Authorized per-user, no global config/source push or other window control.

## 2026-10-31 — Copy numeric Keep Both

853 Rust PASS269core/85helper/499UI,3 existing core ignored; workspace/all-targets
Clippy-Dwarnings/release/fmt PASS. Logs target/copy-keep-both-final-*.log.
Core publication-race/cancel/source-change/exhaustion and name-budget tests PASS;
UI evidence copy-keep-both-ui1793436602618559696 PASS7FB1000×650/440², all7
inspected; conflicts regression1793436615203947340 PASS8FB same binary. Two
independent roots normal0 after queue drainACK, no native input or helper-lifetime
proof. Original keep-both1793436577392527746 FAIL preserved+sidecar (harness
dismiss command on toast with no such hit); corrected report path only, no
product rerun patch. Paths/details can need pan/scroll, controls/counts visible.
No PSS/Swap/CPU/startup experiments; historical failures remain unchanged.
See COPY-KEEP-BOTH.md for publication/source-check limits. Source8623abd installed
with five artifacts and five backups independently rehashed exact; report
installed-checkpoint-1793436673368581944.json. No global acceptance.

## 2026-10-31 — Open selected folder from sizes manager

855Rust269core/85helper/501UI PASS,3ignored; Clippyworkspace/alltargets-Dwarnings/
release/fmt PASS (target/sizes-open-* logs). Original directed7PASS/1FAIL retained:
fixture forgot reselect after recapture, corrected without weakening guard.
sizes-open-ui1793437301816876363 PASS5FB1000×650/440²,all5inspected; real helper
32/9/8/0B, keyboardlargest/rawmouseopen, historyback/stalecontrolsdisabled, hashes
intact/root0drainACK. Original1793437284794884475 FAIL+sidecar: missing Actions
menu before painted Sizes hit, fixed scene route only. No nativeinput/AT-SPI/
clipboard/HiDPI/helperidentity-lifetimes/activechildclose/globalacceptance.
PSS/Swap/CPU/startupNOT_MEASUREDfreeze; incidentalfooter not treegate.
Sourcef58652b installed, report1793437358813676667; five installed+fivebackups
independently rehashed exact. Scope/limits in SIZE-MANAGER.md.

## 2026-10-31 — Search display order

858Rust269core/85helper/504UI PASS,3ignored; Clippyworkspace/alltargets-Dwarnings/
release/fmt PASS (target/search-order-*.log). search-order1793437888618938167
PASS5FB1000×650/440²(all5inspected), provisionalName→final42selectedpath retained,
natural2before10/pathties/Kindgrouping/visualkeys/rawFF→Locate/root0ACK/1logged
helperPIDstartabsent-reused,Znotaccepted. Regressionbatches1793437899122910460
PASS7FB/6helpersgone/root0ACK;2regression440PNG inspected. No nativeinput/
protocol/clipboard/wholelifetimes/exactactiveclose/global/resources proof.
Original1793437832543422732 FAIL+sidecar retained: unpaintedstate accepted by
harness; waitrowhits+indexcount now,no product/deadlinechange. Naturalkeys extra
transient and cachedindices no PSS/speedclaim. Freeze intact. Source64e18d3 installed; report1793437954301097754, five installed
and five backups independently SHA256 rehashed exact.

## 2026-10-31 — SFTP foundation WIP, no visible feature

SFTP-MOUNT-CONTRACT.md and evidence/sftp-foundation/foundation.json: core278PASS
(+3ignored), 6target/3mounttablefixtures; Clippycorealltargets-Dwarnings/fmtPASS.
Own Linux mountinfo read-only example parsed31records, no paths/sources printed
or mounts changed. Exactlog/sourcehashes in evidence. Empty/truncated/duplicate/
stacked/substituted mount records rejected; source/user/port/argv bounded before
owned fields. NO process argv dispatch/SSH/FUSE/authentication/UI/nativeinput/
resource/startup/install acceptance. sshfs still absent; no global package/config/
keys/knownhosts/service changes. Installed SearchOrder64e18d3/e236e1a intact.
Worker/lifecycle/reviewUI/realfixture pending; N01 not complete.

## 2026-10-31 — SFTP preparation only

873Rust278core/85helper/510UI PASS(+3ignored), Clippyworkspacealltargets-Dwarnings/
release/fmt PASS; final logs target/sftp-form-cas-workspace-tests/final-clippy/
cas-fmt/release.log. Original helpSFTPban failure and deprecatedfetch_update
ClippyFAIL preserved; revisedhelp admitsPreparationonly, CASusednosuppression.
sftp-preparation-ui1793440044492647516 PASS5FB1000×650/440²(all5inspected),
actualAuxIo read-onlytool/FUSEaccessprobe/missingSSHFS/hostportvalidation/literal
path/cursorundo/stalepaste-oldstamp/Actions+catalog/reopenempty/sourceunchanged/
root0mutationdrainACK. mount_gateNOT_IMPLEMENTED; no SSH/auth/FUSE/keys/config/
knownhosts/nativeinput/clipboardbytes/readonlysyscalldrain/lifetimes/resources/
startup/globalacceptance. Freeze/previewsPAUSA unchanged. Source86bf3ff preparation installed, report
1793440285730101786;five installed+five backups independently SHA256exact.

## 2026-10-31 — Native SFTP laboratory, Files compatibility FAIL

SFTP-NATIVE-MATRIX.md +evidence/sftp-native/*: actual loopback OpenSSHinternal-
sftp/privatefixturekeys-knownhost/SSHFS3.7.6privatebuild/kernelFUSE. Native
mount/read-raw-link/externalcat/write/busy-unmount/unmountabsent/SSHFS0 PASS.
Final1793442486810260624 overallFAIL/exit1: CorecopyFAILerrno22/0published/source
intact/stageclean; directrenameNOREPLACEerrno22; linkexclusiverejectsoccupiedbut
virtualinodealiasesdiffer.6loggedPIDstartgone-reused strictZfalse,0identityerrors,
notwholelifetimes. Earlierreports retained; genericcopyUNSUPPORTEDclassification
refinedtoFAIL andstrictscriptexitfixedwithsidecar/documentation. No production
worker/UI/newrelease/install/globalpackage/config change or proofweakening. Core
alltargetsClippy-Dwarnings/fmt/examplesbuildPASS; resource/startupNOT_MEASURED.
Installedpreparation86bf3ff/f0df843 remains. NativeGUI/input/daemon/offline/kernelD/
powerloss/hostilewriters/fullN01/globalPASS pending.

### 2026-10-31 — Explicit visible SFTP copy adapter

Core VisibleExclusive opt-in retains partial destinations and reports acknowledged,
completed and uncertain roots; no rollback/source deletion or default-policy change.
Six new tests cover tree/link omission, cancellation, root substitution, numbered
collision and child collision. Native SSHFS report1793444281194564656:
visible_adapter PASS; default stagedcopy FAIL errno22 and overallFAIL/exit1 retained.
No GUI/mountworker/install, native input or resource/startup claim. See
SFTP-NATIVE-MATRIX for scope and explicit omission of unsafe symlink timestamps.

### 2026-10-31 — Mount transaction foundation, not Connect

Pure sftp_lifecycle single-flight ticket/immutable intent, admitted spawn,
late-cancel mount retained, foreground exit reconciled, unreadable table uncertain,
exact-identity explicit disconnect and absence-before-next-transaction. No process
worker/mountpoint provenance/restart journal/UI activation yet. Review retains
port+probe and recheck revokes it; presentation shows port. 885 workspace tests
PASS +5 final controller tests, Clippy/fmt; no new release/install/GUI/resource
acceptance. Full SFTP-MOUNT-CONTRACT and ROADMAP remain open.

### 2026-10-31 — Held SFTP mountpoint provenance

sftp_mount_point creates exclusive 128bit-random private directories under pinned
0700/euid parent, no symlink components/no chmod/no Drop cleanup. Five new tests:
empty/drop-retention, permissions/symlinks, point substitution, parent substitution,
nonempty/synthetic mount coverage. 890 Rust PASS294/85/511+3existingignored:
target/sftp-point-workspace-tests.log; Clippyalltargets-Dwarnings and finalfmt PASS.
Core example holds FDs through real SSHFS: native1793444992430070322 pointgatePASS,
empty→mountedinspectionreject→unmountedidentity/empty,7loggedidentitiesgone,
0readerrors/survivors. DefaultcopyFAIL/overallFAIL retained; visibleadapterPASS.
No processworker/UI integration/newrelease/install/resources/startup acceptance.

### 2026-10-31 — Actual SFTP process engine, not product activation

895Rust299core/85helper/511UI+3ignored PASS (sftp-connection-accepted-tests2.log),
all-targets Clippy/release/fmt PASS. Core Connection spawns SSHFS/ordinary unmount,
setsid/no Drop kill/unmount, sealed8KiB anonymous stderr/bounded4KiB visible text
with truncation, exact mount FD initial serving proof and explicit disconnect.
Native current-source679630 engine/busy/retry PASS;679659 owner-exit/new remote
file read PASS;679678 strict wrong-hostkey reject/no Ready/cleanup PASS.12/7/7
logged identities gone/0errors/survivors, no whole-lifetime guarantee. Default
copyFAIL/overallFAIL preserved. Original wrongkey namespace-onlyReady588098 FAIL
and cleanup-harness611859 FAIL remain, with scope sidecars. Header mount presence
was insufficient; now initial statx/getdents + pre/post identity and process recheck.
No durable store/restart/GUI/AuxIo/new install/inputSO/resources/startup acceptance.

### 2026-10-31 — Durable SFTP registry and cross-process observation

Private anchored single-transaction registry16KiB, lifetime advisory lock/read-only
second opener, raw paths+Target/port+provenance+generation+PID/start+mount signature.
Atomic publication distinguishes Unsynced/Uncertain; retry only syncs. Unknown/
invalid/future/FIFO/symlink/changed records preserved. Active identities retained;
Absent publish requires fresh own-namespace absence+original point+process reap.
908Rust312/85/511+3ignored,13registrytests/Clippy/release/fmtPASS; syncEIO injected,
not powerloss. Native1793450438576487018 registry observation PASS: owner alive
readonly→owner gone exact mount/process match, fresh server file readable; after
LAB unmount reader reportsNoMount/Missing without rewriting history.7loggedIDs
gone/0errors/survivors, no lifetime coverage. DefaultcopyFAIL/overallFAIL retained.
Still NO AuxIo/actual-window close/recovered disconnect controls/Connect UI/new
install/resources/startup gates. Full ROADMAP active; freeze/previewsPAUSA intact.

### 2026-10-31 — Explicit recovered SFTP disconnect

New core controller never signals/waits original SSHFS or adopts a Child. Current
registry/preimage/lease/privateparent/fullmount revalidated, durable intent before
ordinary unmount child, fresh check after sync; no force/lazy/delete. Read-only,
stale, live/zombie prior unmount and unknown PID-gap reject before child. Busy
retained with diagnostic; retry/completion preserve identity and reap guards.
No-op closed reconciliation avoids another journal write.913Rust317/85/511+3ignored/
5newtests/Clippy/release/fmtPASS. Native1793453782020109493 readonly denial +
new-owner busy→retry→durableAbsent PASS,14loggedgone/0errors/survivors; first
1793453058010388848 preserved. Pythonprivate reaper handled originalserver, not
controller; not PID1/GUI/crashgap/lifetime/inputSO/resourceproof. DefaultcopyFAIL/
overallFAIL retained. Still no shared executor/UI/install. Freeze/previews and
complete goal/ROADMAP remain active.


### 2026-10-31 — SFTP product executor and controls

Core Connection/Store/recovered controller now wired to a lazy fixed transport
worker with bounded1request/1reply and single flight, separate from AuxIo saves.
Explicit reviewed connect; fresh/recovered serving-proof Open; ordinary disconnect,
reconcile and sync buttons/keyboard; polling/reaping while modal hidden. Close
revokes future work and awaits mutation ACK, without claiming blocked readonly I/O
was drained. Registered OS mount survives window exit. Failed publication retains
child/registry and blocks automatic replay; known Absent allows another connection.
Inspection does not hold a writer lease. Missing host sshfs remains visible and no
automatic global install is performed. One registry slot, no connection catalog.

See SFTP-PREPARATION.md and evidence/sftp-native/ui-validation for current logs,
scoped native UI evidence and independent install verification. Native proof uses
actual loopback SSHFS/FUSE with private keys/known_hosts/config and product handlers;
no OS input or resources/startup experiment. Existing default staged-copy FAIL is
preserved; visible-exclusive copy policy needs an explicit product review before
wiring. N01/full ROADMAP and hard/ideal resource goals remain open; previews paused.

Validation:919Rust318core/85helper/516UI(+3existingignored), Clippyworkspace/alltargets-Dwarnings, release/fmt PASS. Native `sftp-loopback-1793455941407692094.json` has UI+freshConnect+mount PASS with8PNG1000×650/3root0;25loggedPID/start identities gone,0readerrors/survivors, not lifetimecoverage. Fixture parent reaped the old SSHFS, product only its own children. DefaultcopyFAIL/overallFAILexit1 are unchanged. Initial recovered UI mismatch1793455368964965907 and its sidecar preserve wrong Registered-vs-Retained expectation; no broadening to arbitrary states. Final negative-tool/440² scene1793455971563736333 has6PNG, no registry creation/source mutation/root0. No native input, clipboard or PSS/Swap/CPU/startup measurements. Long diagnostic lines can still clip; a full copyable transport report remains pending.

Final lifecycle follow-up:921Rust319core/85helper/517UI(+3ignored) and Clippy PASS. Closing increments a shared admission epoch; cancelling close resumes only NEW epoch requests, never revives a queued revoked command. Added deterministic test. Registry Lease now explicitly unlocks only in its acquiring PID, including construction-error paths: deterministic forked-FD test proves child cannot unlock parent, and owner Drop allows a new opener while child retains inherited FD. The prior concurrent new_owner.writable() failure remains in ui-validation/sftp-executor-close-epoch-tests.log; which child caused that historical failure is UNKNOWN. No serialization/relaxed assertions/rerun-only resolution. Current evidence references are listed in ui-validation/checks.json.

Final release UI acceptance: native `sftp-loopback-1793456384121839709.json`, product controls/freshConnect/mount PASS8PNG/3root0; negative-capability and440² `sftp-preparation-ui-1793456409825960118.json` PASS6PNG/root0.25loggedPID/start identities gone/0errors/survivors; not whole lifetimes. Native overall/defaultcopy FAIL remain. Source/log/binary hashes and scopes in ui-validation/checks.json; supersedes previous display-only scene1793455941407692094 for installation. All original reports retained.

Installed source1754f83 through `evidence/0.0.4/installed-checkpoint-1793456472288022118.json`;5installed+5backups independently rehashed exact. Appab1d438d58784c133eeb7bb7c30641bf75c5c0012996121b012a7fc34ef4d911/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. No sshfs global installation/config/source publication/tag; host capability still reports missing sshfs.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Opciones in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925Rust319/85/521+3existingignored, Clippyworkspace/alltargets-Dwarnings/release/fmt
PASS. Native1793457917645506527 UI/copy/mount PASS16framebuffers incl440² and
1000×560;4copiedroots(raw0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expectedFAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves57partial
files and original3000files intact.24loggedPID/start identities gone/0errors/survivors,
not lifetimes;3roots exit0 after normal callback. Native defaultcopy/overallFAIL
exit1 retained. Final copy-names UI1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visualFAIL preserved+sidecar;
explicit bounded lines fixed and finalframebuffer inspected. No source delete,
automatic removal of visible partials, powerloss/nativeinput/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.

Installed source `e58f576`, report `evidence/0.0.4/installed-checkpoint-1793458216633312563.json`;5installed+5backups independently SHA256 exact. Appfee00a4d1260bdd2163dd84798eafc6e1d3f3626d3de7feb86a812ecd53e611c/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Fixture rwx+mtime equality checked specifically a.txt with whole-second time; byte hashes also cover tree child/raw file, literal symlink payload and outside-target mtime. No nanosecond/ACL/xattr/ownership fidelity claim. No source push/tag/global sshfs install.


### 2026-10-31 — Captured local search calendar

F8/Nerd calendar: Today/Yesterday/today and the six previous days, process-local calendar resolved
once on existing AuxIo with reference captured on choice. Dates populate fixed
UTC bounds and require explicit Search. Date-only UTC syntax unchanged; strict
YYYY-MM-DDTHH:MM:SSZ extension, mixed bounds compare instants. Latest/epoch/close/
reopen/edit guards; readonly ACK separate from helper reap, no per-click thread.
Content editor preserved after ACK, stale paste and overflow reject without
changing criteria. No per-result DU or added metadata calls beyond existing lazy
metadata predicates. See SEARCH-CALENDAR.md for DST/error and platform scope.

937Rust324core/85helper/528UI(+3ignored), Clippyworkspace/alltargets-Dwarnings,release/
fmt PASS. Eight private TZ process cases: leap,23/25h, Santiago midnight gap/
25h yesterday, Havana fold, Apia skipped date,7dates/DST. UI
search-calendar-ui1793460920785323202 PASS26framebuffers incl440² and1000×560,
actual AuxIo+files-helper: inclusive start/exclusive end+subsecond/raw name,
metadata_reads7 for seven candidates, no automatic helper on preset, yesterday/
seven, dates+content/raw filename/editor, equal mixed bounds reject and existing
UTC controls regression. Root0 after callback/ACK,16loggedPID/start helpers gone/
reused strictZfalse, not lifetimes/helpervivoatclose/kernelD/OSinput/clipboard/
otherOS/resources/startup proof. Earlier name-only scene1793460239093706643 retained;
syntax FAIL wiring log preserved. Full goal/ROADMAP, freeze and previewsPAUSA open.

Installed source36b8a18 through `evidence/0.0.4/installed-checkpoint-1793461261514927889.json`;5installed+5backups independently SHA256 exact. App7654450b01fadcfbf004461820ecdcb12f678a07085a05d3a7834dd4f2e9134a/helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Source publication/tag/global timezone/config changes not performed.

### 2026-10-31 — Sizes subtotal and captured report

942 Rust PASS (324core/85helper/533UI;3 existing core ignored), workspace
Clippy all-targets-Dwarnings/release/fmt PASS. Final workspace suite precedes only
a stale-context help label; final Clippy/release/fmt and framebuffer scene compile
and cover that label. Logs/checks in `evidence/folder-sizes/report-validation/`.
Tests cover unknown vs measured zero, running not a total, checked overflow,
UTF8/raw/control escaping, explicit64KiB truncation, layout reuse/resize/4096line
cap, frozen capture across late completion, explicit refresh and stale context.

`size-manager-ui-1793462910855146190.json` scoped PASS:21real FLTK framebuffers,
real files-helper IPC, five rows including raw0xFF, error excluded from subtotal
40017B and retried-only result40024B, measured empty0B, paused unknown not0,
copy-request handler only, F5/Home/End/I/Escape, no extra report helper, stale
Copy/Refresh disabled and mouse Recapture restored;440² controls. Final minimum
and stale PNGs inspected. Root0 after callback/drain ACK;9logged PID/start helpers
absent/reused, Z rejected; not complete lifetime coverage or helper-alive exact
close. Native input/OS shortcut/AT-SPI/clipboard delivery remain UNVALIDATED.

First compile E0716 and first UI1793462686342799927 FAIL retained. The latter
correctly found missing mouse Recapture on a stale report; production footer now
offers Current context, without weakening guards/deadline. Sidecar records fix.
Interim UI1793462811407106105 PASS retained; final rerun follows only accurate
stale Escape wording. No repeated search for resource PASS.

PSS/Swap/CPU/startup NOT_MEASURED under freeze. Incidental main-process footer
55.2MiB at440² is above the ideal at face value and is not tree+Swap proof or an
accepted budget. No resource/speed improvement claimed from layout caching/caps.
Historical resource/startup/clipboard failures and full ROADMAP remain open;
new previews paused. No global PASS, release tag, publication or OS config change.

Size-report source1885b60 installed through
`evidence/0.0.4/installed-checkpoint-1793462954825746258.json`.
Five installed files and five backups independently rehashed exact in
`evidence/folder-sizes/report-validation/install-verification.json`.
App13935bb18583ace4e3f1c2a230ccf8915e08e463a6ff9a168326e1e3d44891ee,
helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd,
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Authorized local install only; no source push/tag or running-window control.

### 2026-10-31 — Background tabs from search

945Rust324/85/536(+3existingcoreignored), workspace Clippyalltargets-Dwarnings/
release/fmt PASS. Logs/checks in `evidence/search-order/background-validation`.
New preflight includes empty-history reserve; joint closed-tab/other-pane and
8tab limits reject without partial insertion. Model coverage includes source
filter/selection/generation, raw focus restore, provisional directory/no probe,
empty results/invalid leaf/path/close and notice lifecycle. Initial test falsely
expected empty History charge0; assertion and previously missing source history
reserve corrected, originalFAIL preserved with sidecar.

`search-background-ui-1793463564614078345.json` scopedPASS10real FLTK framebuffer
PNGs: actual search_batches, provisional source/query/result/helpercount unchanged
on admission, raw0xFF parent selection after activation440, directory CtrlEnter
opens empty directory, 8tabs cap error preserves search/active/tabs, source hashes
unchanged and sorting regression. Three new PNGs inspected. Root0 after drain ACK;
3logged PID/start search helpers absent/reused, Z rejected; no lifetime coverage.
Private painted handlers are not native input/OS shortcut/AT-SPI or clipboard
proof. PSS/Swap/CPU/startup NOT_MEASUREDfreeze; main-process footer incidental only.
No new preview, memory/speed claim or globalPASS. Full goal/ROADMAP still active.

Background-tab source1a425b3 installed through
`evidence/0.0.4/installed-checkpoint-1793463655854916537.json`.
Five installed files/five backups independently rehashed exact in
`evidence/search-order/background-validation/install-verification.json`.
App224d138405a32bac5246fdbe224dafe99fa4d96820f1f710ca0bbd3eafc54d85;
helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd;
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Local authorized install only, no source push/tag/global config or window control.

### 2026-10-31 — Search marks and atomic background-location batches

951Rust324core/85helper/542UI(+3existingcoreignored)PASS; full workspace suite
precedes only final compact acknowledgment/tooltip/help wording. Final workspace
Clippyalltargets-Dwarnings/release/fmt PASS compile it. Logs/checks in
`evidence/search-order/marks-validation`. Pure marks/range/bytes/count/raw/pruning
and Model old-epoch/atomicbatch/invalidlast/visualorder/budget tests pass.

`search-marks-ui-1793464566350502738.json` scopedPASS15real FLTK framebuffers,
actual search_batches, provisional checkbox→final stable path, oldepoch ignored,
ShiftDown3→atomic3tabs, Insert retains other marks, CtrlA42 rejects entire tab
batch, Ninguno mouse→explicit empty no fallback, raw440activation/emptydir and
8tab regression, source hashes unchanged. Three new PNGs inspected. Root0 ACK;
3logged PID/start helpers absent/reused, Z not accepted; no lifetime coverage.
E0502 fixture closure and Clippy too_many_arguments FAIL logs preserved+sidecar;
fixed scalar epoch capture and grouped modifier flags, no lint/guard disabled.

No native input/shortcut/clipboard, multi-origin mutation Source proof or global
PASS. PSS/Swap/CPU/startup NOT_MEASUREDfreeze, footer incidental only; no memory/
speed claim. FullROADMAP active, new previews paused. Search marks do not grant
mutation authority; Copy/Move integration is the next separate contract.

Search-marks source04af97e installed through
`evidence/0.0.4/installed-checkpoint-1793464690558751222.json`.
Five installed files/five backups independently rehashed exact in
`evidence/search-order/marks-validation/install-verification.json`.
Appd484fc75ccdee27e08c8b1f4d1cd1d56db025514fe637c224dfbbec603fe7f9b,
helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd,
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Local authorized install only; no source push/tag/global config or window control.

## Search-result Copy/Move — 2026-10-31

F5/Copiar and Shift+F6/Mover from marked search results now capture up to128
roots/32parents through AuxIo, validate effective FD proofs and open the existing
review/queue. Cancel retains marks; stale intent/changed root cannot redirect the
job. Global queue/FD/metadata accounting remains shared. Native multi-parent Move
reports effective observed locations and confirmed/pending/uncertain roots;
EXDEV remains one regular nlink1 only, not batch/tree/hardlink transfer.
964Rust327/85/552 PASS+3existingignored, Clippyalltargets-Dwarnings/release/fmtPASS.
Final scene1793470188336000116 scopedPASS27realFB,5loggedhelpersgone strictZno,
root0ACK; cancellation/paused/replacement/partial conflict/raw/copy5/move5 and
single EXDEV actual bytes/inodes verified. See SEARCH-TRANSFERS.md and
evidence/search-order/transfer-validation/checks.json. Original FAILs retained.
No inputSO/clipboard/lifetime/performance proof. Resource/startup NOT_MEASURED
freeze and newpreviewPAUSA intact; full ROADMAP/global gates remain open.

Search transfers source2af8224 installed with five installed/backup hashes independently
verified in evidence/search-order/transfer-validation/install-verification.json;
installation report evidence/0.0.4/installed-checkpoint-1793470237646603707.json.
App3922c3dd373c/helper59187351e32c/HTML85ea1b8009ef. Local only.

## Explicit pending Move retry — 2026-10-31

Incomplete multi-parent Move now offers Nerd **Reintentar pendientes / Ctrl+R**
in its own report. Same original token/ledger/destination, fresh ID, no recapture
or replay of Confirmed; Uncertain/unresolved cleanup blocks. Atomic queue
reservation transfer restores full outcome/proof on rejection. Repeated failures
retain one diagnostic, cancellation before execution keeps proof until ACK.
970Rust327/85/558+3ignored PASS, finalClippyalltargets-Dwarnings/release/fmt; full
suite before only title text/assertion, targeted report-title test also PASS.
Scene1793471063548051155 scopedPASS29FB/root0ACK/6loggedhelpersgone strictZno,
occupied retry then fixture occupant removal→CtrlR pending-only/sameinode/1root.
Previous command-prefix FAIL and wrong success-title capture preserved/sidecars.
See SEARCH-TRANSFERS.md and evidence/search-order/retry-validation/checks.json.
No nativeinput/clipboard/lifetimes/metrics. PSS/Swap/CPU/startup NOT_MEASUREDfreeze;
new previews paused, full ROADMAP and global acceptance remain open.

Pending retry source58604de installed: report evidence/0.0.4/installed-checkpoint-1793471107693651572.json, independent5installed+5backup exact hashes in evidence/search-order/retry-validation/install-verification.json. Appda5c24312ca1/helper59187351e32c/HTML85ea1b8009ef; local authorized only.

## Keyboard and mouse operation reports — 2026-10-31

Up/Down/PgUp/PgDn/Home/End and wheel share bounded, viewport-derived navigation.
Nerd first/previous/next/last buttons have no active target at unavailable edges;
line range/total and key hints stay visible. Paint measures/draws full visible
rows only, resize clamps a full last page, pan clamps actual visible overflow.
No report clones/newworker/decoder/dependency; no PSS/speed claim.975Rust327/85/563
(+3ignored)PASS, final2focusedtests/Clippyalltargets-Dwarnings/release/fmtPASS;
fullsuite before onlyteststyle/tooltiplabels. Scene1793471804962465697 scopedPASS
34FB440/1000, report10/7visible→full-last/widefitsall0, operationunchanged,
root0ACK/6loggedhelpersgone strictZno. DisabledEnd fixtureFAIL+sidecar and2Clippy
test-styleFAILs retained. See REPORT-NAVIGATION.md and
evidence/search-order/report-navigation-validation/checks.json.
Freeze resources/startup/newpreviewsPAUSA, nativeinput/clipboard/AT-SPI/lifetimes
and global/fullROADMAP acceptance remain open.

Report navigation source17597f4 installed: evidence/0.0.4/installed-checkpoint-1793471905931935042.json. Independent5installed+5backup exact hashes: evidence/search-order/report-navigation-validation/install-verification.json. App91e92f71edcb/helper59187351e32c/HTML85ea1b8009ef; local only.

## Saved/reusable search criteria — 2026-10-31

F2/Nerd catalogue and CtrlS named capture; readonly review, raw root, name/literal,
4size/UTCfields, localhidden/editor/order. Load stored/current search root fills
draft only; Enter executes. Metadata ACK never applies search fields and selects
a newly saved entry only for exact unsuperseded request/panel.32entries/128KiB,
2passcodec, regular-onlybounded reads, checkedatomic/CAS entry/creatorPIDflock;
existingAuxIo/sentACK/explicit retry-discard, published-unsynced distinct.
993Rust334/85/574+3ignored/Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
Scene1793475882305119370 scopedPASS4exit0ACK/14FB/6loggedhelpersgone strictZno,
load-nohelper then Enter/raw reopen/rename/remove/futurepreserve/compact440 fields
andcontentrows. Root fields32B and raw4096B/global128KiB are local bounds only.
Inheritedflock risk tested with2privateforks; original WouldBlock exactcause not
claimed. OriginalFAILs+sidecars andfirstPASS before new-row selection fix retained.
See SAVED-SEARCHES.md and evidence/search-order/saved-search-validation/checks.json.
No inputSO/OSclipboard/AT-SPI/lifetimes/powerloss/kernelD/resource/startup/global
acceptance; freeze/newpreviewPAUSA/fullROADMAP intact.

Saved searches source7ba2d57 installed: evidence/0.0.4/installed-checkpoint-1793476145647532183.json. Independent5installed+5backup exact hashes in evidence/search-order/saved-search-validation/install-verification.json; app0e7fed8a8883/helper59187351e32c/HTML85ea1b8009ef. Local authorized only.

## 2026-10-31 — Notice detail ownership

`notice-owner-validation/checks.json` under `docs/evidence/search-order` records 994 Rust PASS (334/85/575), three existing ignored, Clippy all-targets -Dwarnings, release and fmt PASS. Full-report painting and export share the open-notice operation-ID predicate; unrelated/closed/mismatched notices cannot export a retained old outcome. Retained outcome/proofs are preserved. Unit tests cover mismatched/closed/reopened ownership. Existing search scene extended with real transfer → unrelated saved-search validation diagnostic → explicit original-result reopening: scoped PASS, 39 framebuffer captures, four exit0 after ACK, ten logged search-helper identities gone/reused with Z rejected. No clipboard export invoked, native input/lifetime coverage unvalidated. Resources/startup NOT_MEASURED under existing freeze; no global PASS.

## 2026-10-31 — Selected folder-size recalculation

See SIZE-MANAGER and `docs/evidence/folder-sizes/recalculate-validation/checks.json`. F5/painted Recalculate refreshes only completed selected row through same queue/helper. Fresh attempt/context guard, other results and active ACK unchanged, old observation excluded from subtotal/browser size sort.997Rust workspacePASS+3existingignored; later sort/focus1PASS, final Clippy/release/fmtPASS. Real scene1793477083884390572 scopedPASS24FB/raw9→19→29/11loggedhelpersgone/root0ACK. Native input, lifetimes, OS clipboard and resources NOT validated; no global PASS. Original build/import and checker KeyError FAIL evidence retained.

## 2026-10-31 — Search elapsed-hour windows

SEARCH-CALENDAR and `docs/evidence/search/elapsed-validation/checks.json`:999Rust335/85/579+3existingignoredPASS; final Clippy/release/fmtPASS after label-only precision. Exact1/24/168h ends at captured floored reference; no auto execution. Real AuxIo/helper scene1793477561276508002 scopedPASS23FB/AND size+content/raw/inclusive-exclusive±1ns/future excluded, six metadata attempts per six candidates, root0ACK/loggedhelpersgone strictZno. Original three-button assert and script shadowing TypeError FAILs/sidecars retained. No native input/lifetimes/resource/startup/global PASS.

## 2026-10-31 — Retained search pages foundation, not UI

SEARCH-PAGES and `docs/evidence/search/pages-foundation-validation/checks.json`:1007Rust338/90/579+3existingignored, final Clippy/fmt/dev-helper-buildPASS. Seven CLI pipe tests PASS on dev helper:1207raw hits500/500/207, exact1000→empty terminal, exact credit/wrong ID-sequence/EOF, legacy regressions. Directory boundary descendants, root replace, session limits and wait-vs-active budget covered in Rust tests. UI/reader/history/progress/lifecycle/install pending; five installed artifact hashes unchanged. No PSS/Swap/CPU/startup experiment/globalPASS. Initial Clippy unnecessary_unwrap FAIL log preserved; if-let fix without lint suppression.

## 2026-10-31 — Search pages visible

SEARCH-PAGES and `docs/evidence/search-order/pages-validation/checks.json`:1023Rust341/91/591+3existingignored PASS; final Clippy/release/fmt PASS. Final scene1793482591192967537 scopedPASS6Apps/18FB/8loggedPID-startgone Zrejected, observed helper live before Stop and callback.1207,1000 empty-terminal,4507 eviction,500raw513B paths/Locate,602literal fragments440px, per-page marks/source review cancel/stale actions. Shared supervisor preserves child reap ACK; Stop keeps valid accepted data/cached navigation and ignores late finished Stop. Two real red tests preserve opaque-path false budget rejection and late-stop metadata erase; other fixtures/style errors/older scoped scenes retained. Same-SHA release helper seven pipe cases PASS. No native input/lifetimes/resource/startup/global acceptance.

### 2026-10-31 — SFTP profile catalogue

See SFTP-PROFILES.md and evidence/sftp/profiles-validation/checks.json.
1040Rust349core/91helper/600UI+3existingignored PASS, final Clippyworkspace
alltargets-Dwarnings/release/fmt PASS. Full suite precedes caption-only shortening.
`sftp-profiles-ui-1793484790015975923.json` scopedPASS:3apps/7PNG, save/reopen/load
explicit without review/network; Unicode/port spelling, duplicate,0600, real
interprocess lock failure+exact retry, future file preserved and3root0ACK.
Handlers/private pipe/real FLTK framebuffer; NOinputSO/clipboardinterop/network
or active storage-close/lifetimes/powerloss proof. Local caps are not tree PSS.
Preserved initial clipped caption positive+sidecar and unit/compile/layout FAILs;
dialogue no-spawn-marker original cause remains OPEN, now exact error assertion
and normal concurrency PASS. PSS/Swap/CPU/startup NOT_MEASURED underfreeze; no
new previews/tag/globalPASS. Installation report/hashes recorded separately.

Profiles source6f2eab1 INSTALLED: `docs/evidence/0.0.4/installed-checkpoint-1793484872289248556.json`; independent5+5 hashes PASS in
`evidence/sftp/profiles-validation/install-verification.json`, app `ec5ab99217fc`. Existing helper/HTML coherent.

### 2026-10-31 — SFTP readable capture/export request

See SFTP-REPORT.md/evidence/sftp/report-validation/checks.json.1050Rust349/91/610
+3existingignored PASS, final Clippyworkspace/alltargets-Dwarnings/release/fmtPASS.
Report1793486210147321426 scopedPASS2apps7PNG: long own diagnostic all pages,
Unicode/control escaping, fixed vs later status, refresh ID/stalecopy/readonlypaste,
form+Review intact,440 layouts/2root0ACK. Copy request handler only; OS bytes
UNVALIDATED. Profile regression1793486235803344634 PASS3apps7PNG sameSHA.
No real network/host/mount/inputSO/clipboard-receiver/lifetimes/activehelper-close
or PSS/Swap/CPU/startup experiment.24KiB text/range metadata are not PSS proof.
Original scalar-wordwrap positive+sidecar and compile/count/Python fixture FAILs
preserved; final whitespace wrapping byte-lossless. Freeze/new-preview pause and
historical global FAILs remain. Installation/hashes recorded separately.

SFTP report source d1345fb INSTALLED, app cfaa8f3ac1ac; `docs/evidence/0.0.4/installed-checkpoint-1793486330919943223.json` and independent5+5 SHA256 PASS in `evidence/sftp/report-validation/install-verification.json`.

### 2026-10-31 — Archive foundation source-only

See ARCHIVE-OPERATIONS-CONTRACT.md/evidence/archives/foundation-validation.
1068Rust367/91/610+3ignored PASS; Clippyworkspace/alltargets-Dwarnings/release/
fmt PASS.18new pure tests +30actual releasehelper Office wire cases generated
with Python ZIPstored/DEFLATE/descriptor cover preserved responses/source stamps.
No payload-tree extraction/destination FS/GUI/queue/publication/newinstall proof.
Installed5files independently unchanged cfaa8f3ac1ac checkpoint. Initial missing
test helper compileFAIL and wrong legacy-empty-stream comparison retained;
legacy also rejects it, no old-bug claim. Locks/packages unchanged except2core
depedges.8/32KiBchunks/quotas/nodecaps not PSS; freeze/new-preview pause/oldFAILs
unchanged. CRC/output success never permits publication absent separate proofs.


### 2026-10-31 — Archive execution source-only

See ARCHIVE-OPERATIONS-CONTRACT.md and evidence/archives/execution-validation.
1088Rust387/91/610 +3existingignored; final workspace Clippy/all-targets-Dwarnings,
release and fmt PASS.20 filesystem cases;12 independent Python zipfile/core-example
child contracts PASS (stored/DEFLATE/descriptors/raw/modes/umask0777/CRC/cancel/
unsafe paths/empty/occupied destination). This is not queue/UI/native-input proof.
Publication uncertainty and incomplete cleanup stay explicit; same-UID namespace
races and powerloss are not solved by metadata checks. Sync fault injection is
synthetic before syscall. Capacity/FD reservations are admission APIs, not PSS.
No new installation; installed SFTP-report checkpoint remains unchanged. Resource/
startup freeze, preview pause, full goal and prior FAILs remain open.


### 2026-10-31 — ZIP review/Apply and shared queue

1093Rust387core/91helper/615UI+3existingignored; finalClippyworkspacealltargets-Dwarnings,
release/fmtPASS; evidence/archives/ui-validation/checks.json. Five queue/UI tests
cover success, source replacement while paused, fixed outcome reservation and next
copy after ACK, over-budget reject without staging, source/destination intent and
stale buttons/replies. No report-layout assumption, no shared-budget increase.
Final archive-extract-ui1793491842976117966 PASS scoped:7actualPNG1000x650/440x440,
painted pointer handlers; review/cancel/paused no mutation, raw/bytes/modes/source
hashes, other-tab selection, occupied/traversal/CRC and stage absence. Own callback
with paused active job→worker ACK→root0; not live syscall/kernelD or lifetime proof.
Original1793491193816772458 passed filesystem but source filename absent in440
visual review: original+7PNGs+FAIL sidecar preserved. Final visual review passes.
No native input/clipboard delivery/powerloss/resource/startup measurements.1source
per review; ZIP32 regular/dir only; large plans and dedicated retry UI remain open.
Foreign Cargo ttfx observed; own admission returned75 without launching, revalidated
gone then final checks ran serially; competitor untouched. Installation separate.

Installed ZIP review sourcee0dd847; record installed-checkpoint1793492160790962056.
Independent five installed +five backup SHA256 PASS in archive ui-validation/
install-verification.json. App7e264dd0c3c9/helper63a05ffd3fcd/HTML85ea1b8009ef.
No new resource/startup gate and no overall0.0.4 tag/PASS.


### 2026-10-31 — Folder direct-count semantics and size ordering

1100Rust388core/91helper/621UI+3existingignored. Final full suite, Clippyalltargets
-Dwarnings/release/fmtPASS, evidence/folder-sizes/summary-validation/checks.json.
7newregressions: bounded iterator/error/cancel, no child metadata/descent, core and
inactive known-vs-unknown/metadata-vs-total ordering, late summary across panefocus.
Count iterator cap1024, fixedrecord<=32B and no automaticbyte field are source/API
contracts, not process-memory/kernel-I/O/latency experiments. No new worker/helper.
Final direct-summary-ui1793493877395472428 scopedPASS4PNG1000x650/440x440: direct3,
limited>=1024,emptycountnotTotal; explicit8B/0B recursive facts, unknownlast bothsort
orders, own callback→root0ACK,2loggedPID/start helper identities gone/reused, strictZ
not accepted. Two sourcefile hashes unchanged; no fulltree/lifetime/kernelD proof.
Initial fullsuite existing space_map ETXTBSY then mutexpoisonFAIL retained; tests
not altered or serialized. First GUI assumed ascending Size first; actual default
descending was correct, originalFAIL+2PNG+sidecar retained. Combined inspector hint
was clipped; final no-follow line and Calculate shortcut are legible. Native input/
clipboard receiver/resource/startup/allocateddisk/snapshot/globalgates unvalidated.

Installed folder summary source7983c0f/app43a09343ee99. Record1793494152752973446;
5installed+5backups independently rehashed exact, summary-validation/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. Resource/startup freeze remains.

### 2026-10-31 — Explicit ZIP retained-result recovery

1104Rust390core/91helper/623UI+3existingignored, finalworkspace/Clippyalltargets
-Dwarnings/release/fmtPASS. Same worker/controller budgets; atomic reviewed recovery
before pending jobs, no token in ordinary pending queue. Rejected admission returns
original capability/outcome; resultID+freshstamp guards controls. Cancelled cleanup
retains removal ledger; sync checks binding before/after actual parent fsync.
GUI1793496524673844308 scopedPASS5PNG1000/440: foreign retained/not deleted,
startpausedcancel/reviewBack/queue8reject/Clear+staleconfirm, cleanup0publishedroots,
sourceZIPremoved before sync, replacedpublishedname rejected then restoredinode
confirmed. Callback→root0ACK. No helper/lifetime/nativeinput/resource/startup proof.
Synthetic observer failure BEFORE fsync, NOT failed syscall/powerloss. FirstGUIFAIL
checker required modal for success; original/2PNG/sidecar intact. Initialtestcompile
andClippy failures retained in archives/recovery-validation. Durable restart recovery,
creation/ZIP64/TAR/links/otherOS/globalgates remain open; freeze/previews pause intact.

Installed ZIP recovery source61f93c6/app4143d6b8de54. Record1793496618085702365;
5installed+5backups independently rehashed exact in recovery-validation/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. No source push/tag/globalPASS.

### 2026-10-31 — Reviewed streaming ZIP32 creation

1114Rust398core/91helper/625UI+3existingignored; final workspace, Clippyalltargets
-Dwarnings, release andfmtPASS. Separate CreateZIP Action/context/command catalogue,
existing modal/AuxIo/shared queue, fresh review token/epochs, no source reinterpretation.
Read-only FD inventory; files/dirs/literal links, openat2 anchored/no-follow/no-XDEV,
parentdev/inode/statxMountID pre/postsync; no hostile mount topology proof.128roots/
4096entries/512KiBnames/depth32/3GiBregular bytes; controller scratch/outcome/FD charged,
no memory/CPUclaim. DEFLATE64KiB+CRC, exclusive stage/outputsync/no-clobber/parentsync;
ordinaryrwx, UTC DOS2s/clamp, raw Unix names. No ACL/xattrs/ownership/hardlink identity.

Scene1793498375771482261 scopedPASS7PNG1000/440: review/Back nooutput, pausedapply,
other-tab completion preserves selection, independent Python zipfile CRC/content/
raw/link/exec/empty/UTC checks and UIlinks1, occupiedrejection, activepositiveprogress
fixture→cancel cleans ownstage, changedsource preventsoutput, pausedclose→root0ACK.
Source file-byte/rwx/link-target fingerprints unchanged; not full metadata/snapshot.
Core also rejects a substituted stage without deleting occupant. No nativeinput/
clipboard/helperlifetimes/kernelD/powerloss or resource/startup gate. New previews paused.

Original six core Openat2 O_PATH|NONBLOCK failures preserved and fixed; initial UI
compile/style/deadcode failures preserved. FirstGUI1793497597931207464 checker stale
state TypeError, sidecar; second1793497610130038637 functionalPASS has visualFAIL
sidecar (folderlabel and clippedpolicy440), originals retained. Final labels/policy/
namepreview/cancel diagnostics legible. Creation durablejournal/recoveryUI/ZIP64/TAR/
otherOS and global0.0.4 acceptance remain open. Evidence archives/create-validation.

Installed creation sourcee5970ff/app20db1571eb91, record1793498441702024461.
Five installed+five backup hashes independently exact in create-validation/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. No source push/tag/globalPASS.

### 2026-10-31 — Search root navigation

1121Rust398core/91helper/632UI+3existingignored; fullworkspace/Clippyalltargets
-Dwarnings/release/fmtPASS. Root caption Nerd menu and Alt↑parent/Alt→resultfolder/
Alt←previous root. Same criteria, no automatic scan, eight paths/32KiB without old
result snapshots or new thread/helper. Rootepoch distinct from result epoch; menu
result captured/revoked on refresh, stale root controls rejected. Cancels active
search but retains it until ACK, prevents late rows. Pending date blocks scope
change instead of silently discarding criteria. Close/reopen clears root history.

Final1793499783655547501 scopedPASS8PNG1000/440/3apps: query/rawroot0xFE, key policy
and painted menu buttons, no spawn until explicit Enter, content-mode header,
actual old helper PID/start stateS before Root change; new report expected prefixed
paths and zero old unprefixed needle rows. Callbackroot0ACK;6loggedidentities gone/
reused, strictZnotaccepted. No wholelifetimes/overlap/kernelD/inputSO/AT-SPI or budgets.
Initial test compile Fake-scope error preserved; previousGUI1793499682385206785
PASS retained, final strengthens absence assertion over all old needle rows. Source
fixture hashes intact. PSS/Swap/CPU/startup NOT_MEASURED freeze; previews remain paused.

Installed search scope sourceb405554/app35c2c4003224, record1793499893189005044;
five installed+five backups independently exact in search-scope/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. Overall goal active; no source push/tag.

### 2026-10-31 — Forget directory-size observations without browser refresh

1128Rust398core/91helper/639UI+3existingignored, fullworkspace/Clippyalltargets
-Dwarnings/release/fmtPASS. O: terminal selected row only, with active-ACK guard;
Info Nerd button. ShiftO/report Nerd button: immutable completed IDs of that frozen
report, validate-all before removal, preserve later measurements/errors/queued and
pause. No FS deletion, worker, scan or queue-epoch reset; frees retained record/name/
error slots without ID reuse. Fresh panelstamp, nearest display neighbor, same
browser generation/selection and both-pane Size-order invalidation after forgetting.

Real scene1793501340564075091 scopedPASS4PNG1000/440:8B/zero/raw, removeRaw observation
and frozen3measurements, report unknown not0B, readmit4paused with fresh IDs and no
extra helper/refresh. Whole own fixture tree fingerprint unchanged, callbackroot0
postACK;4loggedmeasure_directory identities gone/reused, strictZnotaccepted. Native
input/clipboard/lifetimes/kernelD/process budgets not inferred. Core/unit128budget,
stale/atomic/newer results, cancelledactive ACK and inactive order separate proof.
Initial stamp-shadow, tuple and Arc marker compile errors retained. Two tests first
paused an already drained queue and expectedtrue; corrected with pending work,
originalFAILs retained, production pause unchanged. Freeze/newpreviewpause intact.

Installed forget-observations source8347b65/app18cb22c21e8c, record1793501593425632782;
five installed+five backups independently exact in size-forget/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. No source push/tag/globalPASS.

## Live dual-pane size ordering — 2026-11-01

Preserved RED model-pump reproduction uses an injected completed Job: measured
20B accepted but quiet inactive Size order stayed stale. Three new regressions
cover valid completion and zero/error/stale ACKs plus browser/manager invalidation.
1131Rust (398core/91helper/642UI),3existingignored; workspace, Clippy all-targets
-Dwarnings, release and fmt PASS. No test serialization or new process/thread.
Evidence: `docs/evidence/size-sync/`. First native-helper scene FAIL is a checker
error (requested-set cardinality is not pending count), preserved with sidecar.
PSS/Swap/CPU/startup NOT_MEASURED; native OS input and global acceptance unchanged.

A later final suite preserved ETXTBSY before a fresh Python fixture script
entered its protocol (641UI PASS/1FAIL). Only registered test scripts now launch
through stable `/usr/bin/python3`, leaving production child spawning unchanged.
Full workspace1131 PASS again, same parallelism/no retries/ignores added. Second
scene FAIL preserves manager pause sent as a browser Action, blocked by modal
routing; fixture uses the manager P handler after F5 instead.

Real-helper scene `size-sync-ui-1793502610961034290.json` scoped PASS:
5 actual1000×650 FLTK framebuffers, completed20→1→16B updates and paused
browser/manager invalidation, active Name order and inactive selection/id/path
stable.4 logged helper PID/start identities absent/reused (Z not accepted),
root exit0 after ACK. Owned actor changes Z only; A source bytes unchanged.
Handlers/fixture keys, NOT native OS input/live resize/lifetime/resource proof.

Installed source3ff50b9 appd84418d722df/helper63a05ffd3fcd/HTML85ea1b8009ef.
Record1793502631105837393; independent5installed+5backup hashes exact.
No source push/tag/global PASS; resource/startup freeze unchanged.

## Sort discoverability — 2026-11-01

1133Rust398core/91helper/644UI+3existingignored, workspace and Clippy all-targets
-Dwarnings PASS. Two new regressions cover inactive Size/Modified/active Name
transitions, retained size observations/owner/generation, stale/refreshing guards,
and six disjoint targets at multiple widths. Original first imports/test API
compile failures preserved. Epoch assertion initially captured before enqueue
started the dataset; corrected capture after enqueue/finish, not weakened epoch
behavior. Resource/startup NOT_MEASURED; native input/AT-SPI remain unvalidated.

First sort scene1793502992179193770 scopedPASS6FB; extended scene
1793503025776201051 scopedPASS7FB adds single-list440. Visual review exposed
folder-size/date touching: folder-size cell now leaves6px gutter like file units.
Those originals and sidecar remain. Full suite precedes only this cell-width
adjustment; final Clippy/release/functional framebuffer scene follow. No logic
change, source mutation, test ignores or timing tolerance added.

Final sort-controls-ui1793503127353685479 scoped PASS7 actual PNG1000×650/440:
Name/Size/Modified header targets and direction toggles, inactive sort preserves
selection/owner/generation and20B measurement, date command keyboard dispatch,
compact dual/single lists,6px size/date gutter.1logged DU PID/start gone/reused
(strictZ not accepted); root0 after ACK. Source A bytes unchanged, date actor
only sets owned directory timestamps. No native input/AT-SPI/full-lifetime/budget
claim. Clippy workspace all-targets-Dwarnings/release/fmt final PASS.

Installed source39abf7b app6ac0be467e75/helper63a05ffd3fcd/HTML85ea1b8009ef;
record1793503143821263359, independent5installed+5backup hashes exact.
No sourcepush/tag/globalacceptance; resource/startup freeze unchanged.

## Pane filters and explicit zero selection — 2026-11-01

RED `red-exact.log` reproduces ClearFilter losing explicit zero-selection intent.
The first --exact command omitted the module prefix and ran0tests; it is preserved
and is not reproduction evidence. Direct exact qualified test then ran1FAIL.
Original added diagnostic used Debug on View (not implemented); compileFAIL log
preserved; changed only diagnostic formatting.
1138Rust398/91/649+3existingignored workspacePASS;5new tests cover Clear/Escape,
no-copy in four views, default focus for no-match, inactive owner/DU preservation
and stale/refresh guards, disjoint compact controls. Clippyalltargets-DwarningsPASS.
Resource/startup NOT_MEASURED; previewsPAUSA; no global/native input acceptance.

Visual review of prior scopedPASS7FB1793503735454209243 preserved with sidecar:
compact pattern recut and notice/counters overlapping. Compact header now omits
redundant Filtro label; counters use short form. Visible compact notice reserves
48px of browser area and moves above MC strip; shared visibility predicate and
geometry test validate footer/notice/row/strip nonoverlap440/800/1000, with/without
MC bar, full-modal/closing exclusion.

Workspace-layout-cancel-fail.log preserves646UI PASS/3FAIL: cancellation fixture
timed out waiting for PID marker, two existing PROCESS_TEST mutex poison failures.
Audit found script registry was thread-local although cancellation transfers
execution to a worker thread. Registry is now test-only shared set with brief
registration lookup locks released before spawn; production executable path and
process supervision unchanged. A deterministic thread-handoff/remove regression
checks registration. Original timeout's unique cause not proven; no deadline
increase/ignore/test-lifetime serialization added.

Final workspace1140Rust398/91/651+3existingignored PASS after compact labels,
notice reservation and test registry handoff fix; Clippyalltargets-DwarningsPASS.
Seven new tests relative to installed checkpoint, no ignored tests added.

Final pane-filter-ui1793504132191949858 scopedPASS8actualFB1000×650/440×440:
independent patterns/edit+clear targets, inactive clear keeps active owner/query/
generation/marks/20B DU, active clear retains marks; explicit zero survives clear
and Escape and F5 opens no copy form (real notice dismissed normally). Compact
pattern/counters visible, notice and MC strip separate. One logged real DU
PID/start gone/reused, strict Znotaccepted; root0 after ACK. Whole owned fixture
structure/content/symlink fingerprint unchanged, not a full metadata snapshot.
No nativeinput/clipboard/AT-SPI/lifetimes/PSS/Swap/CPU/startup proof. Release/fmt PASS.

Installed source4bf0b6a app41bec759247b/helper63a05ffd3fcd/HTML85ea1b8009ef;
record1793504185198676049, independent5installed+5backup hashes exact.
No sourcepush/tag/globalPASS; goal remains active and resource/startup freeze.

## Terminal request integration — 2026-11-01

Terminal raw-directory prototype was not sent to the real UWSM UTF8-only daemon.
Primary installed-source audit led to bounded ASCII hex + user-service helper
exec. Existing open unit contracts retained. First Clippy lint failure and menu
cast parse failure are preserved; no tolerance/test ignore/parallelism changes.
Functional/native report and final check totals follow after actual execution.
No resource/startup measurements; previews remain paused; no global acceptance.

Final unit/Clippy stage:1151Rust401/94/656+3existingignored PASS,11newtests.
Added absolute PATH binding before helper cwd switch, relative/non-executable/
oversize PATH cases; media caps unchanged. Compact labels retain private
clipboard (files) wording and full names/tooltips; text-width invariant added.
First functional/native scene1793506806484779429 PASS9FB confirmed actual mapped
Alacritty/PTY/exact raw cwd and independent lifetime. Its visual scope is fallback
font only: private PATH hid fc-match, not a host font failure. Sidecar preserved.
Final fixture adds only fc-match, keeping missing uwsm isolated; final Nerd scene
and install are pending below. No resource/startup/global gate claim.

Terminal final `terminal-ui-1793507918398541254.json` scoped PASS9FB, real Nerd
fonts, Actions/context/palette and all21 controls440. Actual compiled helper
executes raw cwd/argv after ASCII broker transport; native configured Alacritty
mapped with owned PID/start/class, writer tty0/tty1 true and exact raw cwd, stays
alive after Files exit0ACK then exits on release. Private fake launcher likewise
not killed by Files close; harness alone adopts/reaps it. Two root0ACK runs.
Independent cleanup:6registered identities absent/reused, same-start Znotaccepted,
owned unit inactive. Source9B file unchanged; not whole-tree/metadata/lifetime proof.
No native keyboard/clipboard/focus-control/otherOS/PSS/Swap/CPU/startup acceptance.

Installed source1a266e5 app8deae3545aac/helpera9a2c8dcde68/HTML85ea1b8009ef;
record1793508497934499916. Independent5installed+5backup hashes exact.
No sourcepush/tag/globalPASS; full goal active, metrics/startup freeze intact.


## 2026-11-01 · V12 folder-view storage foundation only

`folder_views` adds bounded exact-raw-path settings and cooperative atomic storage
under config, without writes into saved folders. 410 core tests PASS / 3 existing
ignored, final focused 9 PASS, core all-targets Clippy -Dwarnings and fmt PASS.
Initial two Clippy style errors are preserved in the evidence directory. Final
focused tests include corrected duplicate-key byte fixture and final source.
Evidence: `docs/evidence/folder-views/foundation.json` and copied logs.

No UI/controller integration, restoration, framebuffer scene or new installation
is claimed. Terminal2a4d018 remains installed. Whole-workspace/release validation
was not rerun for this isolated unused core module. See
`FOLDER-VIEWS-CONTRACT.md` for lifecycle/intent constraints still to implement.
PSS/Swap/CPU/startup NOT_MEASURED by freeze; previews remain paused and all historic
FAILs remain in force. No global PASS or 0.0.4 tag.
