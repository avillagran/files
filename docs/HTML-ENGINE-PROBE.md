# Experimental HTML engine with a visible view

2026-10-26. P05 partial: Space shows real HTML/CSS layout through an ephemeral
native litehtml adapter and a Painter display list. There is no browser nor
JavaScript. The optional adapter is built and prepared next to the release build, without
a global install nor a new Cargo dependency. It does not complete P05: approximate
metrics, blocked resources/links, budget still NOT_MEASURED.

## Immutable sources and licenses

litehtml is pinned to commit `6fd7aaa63833c913cf1a0336e46fb2e027e4c656`
(upstream commit date: 2026-10-25). This pinned archive was downloaded, verified and built:

| Artifact | SHA256 |
| --- | --- |
| [Commit archive](https://codeload.github.com/litehtml/litehtml/tar.gz/6fd7aaa63833c913cf1a0336e46fb2e027e4c656) | `82cfdc8347ebb61fbf308e29ebcb6ea5f82e894dd6a013c928deacc0446a471e` |
| [litehtml LICENSE](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/LICENSE), BSD-3-Clause | `71bd488e77ee821bf38d89e2b8a9c5ebe687553fcae9cefa00a96499792a3e23` |
| [Bundled Gumbo LICENSE](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/src/gumbo/LICENSE), Apache-2.0 | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |
| [Gumbo VERSION.txt](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/src/gumbo/VERSION.txt), `0.14.1 (2026-09-27)` | `2490fbe7cb742a7ae9334d17bef9c0026e5d64b498dd198fd96da9b532241652` |
| [document_container.h](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/include/litehtml/document_container.h) | `fcfe5c2d82d1f45c787a950c40732177564b942c4e2ed68dce8c94c9974515af` |

The archive contains 241 regular files and 4,146,028 expanded bytes. Gumbo
is part of the same pinned tree: the build does not need to look for another version.
Digest of that subtree:
`e0a132ae902189e6972b8d8256951edd5bb7327119fe88b04d7f738dbb33b5f0`.
To reproduce it, sort the regular files under `src/gumbo` by relative POSIX
path and feed SHA256 with the path, NUL and the 32 bytes of the SHA256 of its
content. It identifies the bundled copy, even if it differs from upstream Gumbo.

The [pinned CMake configuration](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/CMakeLists.txt)
builds both engines. Its testing option is disabled because it uses another
repository through ExternalProject. The probe adds no Cairo, GTK, Qt,
FreeType, IJG nor codecs. FTL is not assumed to be equivalent to an authorized
BSD license.

The top-level license is not enough to classify every bundled notice:

- `src/strtod.cpp` contains a UC/Sun permission that differs from the authorized standard
  MIT/Apache/BSD text. **It is not built**: the runner creates another copy in
  the cache, applies `exclude-strtod.patch` with `--fuzz=0` and adds our own
  MIT `strtod_bridge.cpp`, which calls `std::strtod`. It keeps the originals and
  their notices. CMake rejects a source list that still includes the original
  file. The ephemeral executable sets `LC_NUMERIC=C` before the parser, without
  changing the desktop configuration nor the main files process.
- `src/gumbo/src/utf8.c` has an Apache-2.0 header and an internal Hoehrmann
  notice that lacks MIT's warranty paragraph. The
  [author's primary source](https://bjoern.hoehrmann.de/utf-8/decoder/dfa/) was consulted:
  it publishes the full MIT license. Its improved variant matches the 364
  entries of `utf8d` and the body of `decode` bundled in Gumbo, ignoring
  only whitespace in the body. The full permission is preserved in
  `scripts/fixtures/html-engine/THIRD-PARTY.txt`. The hash and comparison
  evidence is in `hoehrmann-source.json`; no new exception is invented.
  Gumbo's surrounding code keeps Apache-2.0.

The numeric replacement checks fixtures at the start of each process for
decimals, sign, exponent, precision, `endptr`, negative zero, invalid input,
overflow and underflow/ERANGE. Equivalence with the whole upstream numeric
parser is not ensured: special values, hexadecimal notation and hard CSS cases
need review. The bridge is only valid with the numeric C locale
controlled by the helper; it must not impose a locale on the UI process.

## Concrete adapter and output

`scripts/fixtures/html-engine/probe.cpp` implements the abstract interface of the
pinned version. It hands HTML to `document::createFromString`, calls
`render(width)` and records `document::draw` callbacks for the whole height of the
document, finite and up to 1,000,000 px. The visible width and the real height of the
viewport still feed CSS and media queries; the drawing clip uses the
full height so the host can scroll without relaunching the helper. The
layout comes from litehtml; HTML is not converted to Markdown and text
extraction is not considered a rendered preview.

The current API uses `create_font(const font_description&, const document*,
font_metrics*)`, floating `pixel_t` values, separate callbacks for image,
solid fill and three gradients, and a clip stack. Another tag requires
reviewing the overrides; an old example does not prove compatibility.

The output is a bounded JSON list of boxed text with RGBA, size and style;
solid rectangles; and clip push/pop. litehtml computes flow, CSS spacing,
tables, headings and media queries. The approximations are counted explicitly:
simplified solid borders, rectangular clips, undrawn gradients and
blocked images with a placeholder. A probe PASS would not complete these
features.

The runner only queries `fc-match monospace` to get the configured family,
with a readable `monospace` fallback, without enumerating or rasterizing the catalog.
**The metrics are approximate**: an advance of 0.6 × size per Unicode scalar,
including combining marks and ZWJ. It forces the configured family and limits sizes
to 8–64 px. It does not prove shaping, grapheme segmentation, glyph coverage,
absence of tofu, selection nor a rasterized image. The final integration must
use Nerd Font measurement consistent with Painter, not these advances, for hit tests.

## Host policy and limits

The input is a regular, no-follow, nonblocking local file of up to 32 KiB.
Its size is checked before allocating the buffer and during the read in case it
grows. NUL and invalid UTF-8 are rejected. The limit is experimental, not a
final decision on the HTML documents files will accept.

The callbacks contain no network I/O nor resource opening. `import_css`
empties the requested sheet; CSS links, images, base changes and navigation
are counted and rejected. Images have a fixed placeholder size. There
is no JS execution engine and no external browser is opened. The fixture
includes a script, an iframe, an active URL, a remote image and imported CSS. The app's
functional scene verified the blocking counters and the absence of the JS sentinel;
it does not prove an operating system sandbox nor every variant of hostile HTML.

| Resource | Probe limit / scope |
| --- | --- |
| HTML | 32 KiB before the parser |
| Fonts | 64 live handles before inserting |
| Callback text | 4096 UTF-8 bytes before expanding JSON |
| Element creation | 4096 callbacks; it does not count every internal text node |
| Commands | 8192; 2 MiB of total text reserved once |
| Clips | depth 32; balanced stack at the end |
| Viewport | width 128–1600, height 128–1200 |
| Coordinates and extent | finite; absolute value ≤1,000,000 |
| Pipes | stdout ≤2 MiB + 16 KiB; stderr ≤64 KiB |
| Process | monotonic timeout 5 s; CPU 3 s; virtual space 64 MiB from the UI (128 MiB in the isolated runner); 32 FDs; no core dump |

**document_container provides no allocation budget for the parser.**
In [document.cpp](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/src/document.cpp),
`createFromString` uses Gumbo with default options and allocator before
invoking the element callbacks. The host limit does not prevent allocating that
initial tree; an exception may keep intermediate allocations until
the process exits. CSS and layout do not have a proven aggregate cap either.
Input, time and virtual space limits contain part of the risk, but
**RLIMIT_AS is not PSS** and does not verify files' budget. Production
needs allocator/node/depth limits in the parser or a restricted mode
with explicitly measured scope and resource use.

No URL in this experiment opens local files. Future resources must be
resolved once against an authorized root and opened by components without
symlinks through the existing helper policy. `<base>`, redirects, `url()`, fonts,
data URIs, imports and refresh also have to be defined. The lexical
filter is not a sandbox; hardlinks and mountpoints need an explicit scope.

## Integration and functional reproduction

The first build failed because it compared `pixel_t` with an integer.
`docs/evidence/html-engine/probe-1793035151738003920.json` is kept; `.value()>0.0f`
fixes the contract of the pinned version. A later CMake Release build compiles and
links without warnings. The UI has run the real adapter, including its numeric startup
fixtures, through the common supervisor.

```sh
cargo build --release
make html-helper
python3 scripts/html-ui-check.py
```

`make html-helper` verifies/builds the immutable sources in the cache and prepares
`target/release/files-html-helper`, its license and stamp. It does not install globally.
`stage-html-helper.py` rejects a source fingerprint or binary SHA256 that
does not match. Without that executable, Space shows the capability as unavailable.
Automatic distribution/installation of the adapter and other OSes are pending.

The `--prepare/--build/--run` runner remains available for isolated work;
`--run` was NOT used in this checkpoint. It keeps the original sources and failures,
builds one CMake job and writes evidence with an exclusive timestamp. No
resource/startup command will run while the user's explicit
integration/captures-only phase continues.

Rust validates the wire before building the TextDocument: 2 MiB+16 KiB, a pre-count of
commands and JSON structures, schema/status, the exact requested viewport, up to
8192 commands, balanced clips ≤32, finite coordinates, fonts 8–64 and text
384 KiB/2001 lines. Cancellation/timeout reaps the child before publishing; State
keeps a single job and only the last revision. Resize cancels the previous one and
recomputes CSS at the new width; the selection is not preserved across those layouts.

The scene uses internal handlers and the real framebuffer, not OS events/clipboard:
CSS/table/styles, visible selection, a 1000→800 media query, blocked resources,
scroll down to FINAL_HTML_VISIBLE, 32 KiB rejection, quick changes and close 0 after
ACK. 252 Rust tests (73 core/33 helper/146 UI), Clippy workspace all-targets
-Dwarnings and release clean. Evidence and app/adapter hashes in
`docs/evidence/0.0.4/html-ui-*.json` and the `html-*.png` captures.

**Visible limits confirmed:** the 0.6×size advance per scalar differs from the
real font: CJK glyphs and the end of some cells are cut off (`archivo.md`).
Selection is a projection of commands in drawing order, not DOM nor shaping.
Black text adapts to the theme without analyzing every CSS background; documents
with explicit light backgrounds may lose contrast. Borders/alpha/clips are
approximations; there are no gradients, HTML images nor working links.
Arbitrary parser safety and PSS compliance are not claimed. Resources and
startup NOT_MEASURED because of the phase; the historical baseline/startup/clipboard FAILs remain.

## Possible C ABI contract

Proposal, still without exported symbols: an opaque document and no STL
crossing the boundary.

```c
typedef struct files_html_document files_html_document;
typedef int (*files_html_measure)(void *user, const unsigned char *utf8,
    size_t bytes, unsigned font_id, float *advance);
typedef int (*files_html_emit)(void *user, const files_html_command *command);
int files_html_create(const unsigned char *html, size_t bytes,
    const files_html_limits *limits, const files_html_host *host,
    files_html_document **out);
int files_html_layout(files_html_document *, unsigned width, unsigned height);
int files_html_draw(files_html_document *, float scroll_y);
void files_html_destroy(files_html_document *);
```

Pointers borrowed for the duration of the call, errors as integers, exceptions
contained in C++, an explicit length for each text and callbacks that can
reject work. The helper must be single-flight, cancellable and reaped.
Selection and hit tests need ranges of the rendered text and stable IDs,
separate from the offsets of the source HTML.

Do not block the UI through cross-process measurement callbacks. The helper
needs an allowed and audited font backend that matches Painter,
or bounded reusable metrics/shaping. The probe's approximate metrics cannot
silently become the final contract.

Still pending: real metrics/shaping, full CSS contrast, preserving the
selection after resize, local links/resources, a parser allocator and tests of
deep or malformed HTML/CSS, crashes and
measurement of the main+helper tree: steady/sampled-peak PSS+SwapPss, CPU,
latency and startup. Complete HTML and an approved budget are not declared.


## Effective identity of the selected HTML — 2026-10-27 (M13)

The Rust worker receives the expected Revision from the existing watcher/probe, opens the final
regular file NOFOLLOW/NONBLOCK and requires a match before rendering. Its 32 KiB
limit comes before spawn/parser; no content is read on the UI. The adapter now accepts
`--source-fd N WIDTH HEIGHT FONT`, with N 3..255 validated before reading. It inherits exactly
the opened FD, without dup to a fixed index nor reopening the path. The earlier path mode
is kept for CLI fixtures, but files only uses the FD. pread does not change the shared
offset; fstat checks regular/cap before the 32 KiB+1 buffer. No new dependency.

After reap and decoding the display list, the worker revalidates the same FD and name;
Ready carries the effective Revision, State requires the expected signature and the current generation.
Dirty/deletion hides the earlier list and selection, cancels, re-arms the token and waits for a
new stable probe. Watch before probe/monotonic epoch/1 s reconcile are reused,
with no per-file watcher nor new threads piling up. A typed source error is separate
from a parser failure. The readonly_gate shared with audio/PDF: the helper requires reap, a readonly
close may allow exiting without a syscall join nor faking an ACK. It does not test kernel D-state.

O_NOFOLLOW only protects the last component; parent symlinks/mounts follow OS
navigation. Neither revisions nor the watcher provide an atomic snapshot, content hash,
sandbox nor guaranteed instant detection. Scripts/network/CSS/images/external
navigation are still blocked. The approximate scalar monospaced metrics and the
CJK/cell clipping are still visible; this unit does not declare correct shaping.

476 Rust (141 core/58 helper/277 UI), Clippy workspace/all-targets -Dwarnings/release PASS.
Native build probe 1793093259627217246 records a new binary fingerprint;
fd-contract 1793093345381869451 PASS: the FD keeps A after the name is replaced
by B, the shared offset intact, the legacy CLI reads B, FD/FIFO/>32 KiB rejections without
partial output. Rust tests Verified A/open B/finish A after replace and a late Ready A
after Dirty/B, plus readonly admission and the cap before the child.

html-ui 1793093483630467339 functional PASS 10 FB 1000×650 and resize 800×500 with notify
suppressed: FD signature and effective text of a replacement with the same size/mtime, in-place with
mtime restored, delete/recreate, rename file/parent recovery, hidden without a helper;
selection/styles/table/scroll/blocked resources/32 KiB error/symlink/rapid/close 0 ACK.
14 logged PID/start gone/reused, Z not accepted; no lifetime coverage nor a live helper at close.
App SHA bb32f9725d40d6327c66897aa0a1595c7a2c113e28fd81206f41f0a83a83575c;
HTML 85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Internal handlers/real framebuffer, no OS input/clipboard/native 2x sharpness.
Resources/startup NOT_MEASURED freeze; the footer's incidental values are not a gate
nor a measurement of the tree. P05/P09 stay partial; no 0.0.4 tag/global PASS.
