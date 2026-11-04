# Bounded raster image helper

`files-helper preview_image` reads one JSON request and exits after one response:

```json
{"command":"preview_image","id":1,"path":"/absolute/image.png"}
```

Success is a **single JSON line**, then exactly `byte_len` raw RGB8 bytes, then
EOF. There is no base64, JSON byte array, padding or trailing newline after pixels:

```json
{"status":"image","id":1,"width":800,"height":600,"byte_len":1440000,"source_width":800,"source_height":600,"format":"png"}
```

`width`/`height` describe the oriented output, `source_width`/`source_height` the
unrotated encoded source, and `format` is `png`, `jpeg`, or `webp`. Errors use the
existing single-line `error` response and have **no pixel payload**. The consumer
must validate request id, dimensions (1..1024), `byte_len == width*height*3`, and
exact payload length/EOF before exposing pixels. Reject extra/truncated data.
The request cap stays 64 KiB; image output cap is 3 MiB plus a small JSON header.
The existing du/text response contracts remain unchanged.

Input is detected from content, not extension. PNG (including 16-bit samples),
JPEG, and **static** lossless/lossy WebP are supported. EXIF orientations 1–8 are
applied while sampling into the output, avoiding a second rotated source image.
Output uses nearest-neighbor reduction, preserves aspect ratio within integer
rounding, does not upscale, and composites alpha over a dark 16-pixel checkerboard.
This is a bounded preview, not an export or a color-managed image editor: embedded
ICC profiles are not applied, animation/playback is unsupported, and zoom cannot
recover detail beyond the 1024-pixel output. APNG uses the codec's static image.

## Bounds and filesystem behavior

- Encoded input: at most **8 MiB**. An opened-file metadata size is checked before
  an exact allocation; a one-byte sentinel rejects growth. Shrink/read errors fail.
- Source: nonzero dimensions, at most **8192 per axis**, **4,194,304 pixels**, and
  **16 MiB** decoded samples. Thus ordinary **12/24 MP camera photographs currently
  fail explicitly**. P01/P02 remain partial; JPEG decoder-level downsampling before
  full decode is a required follow-up, not silently treated as supported.
- Header dimensions are parsed without allocation before constructing a codec.
  WebP's inner VP8/VP8L frame dimensions are also checked against its VP8X canvas
  before decoding; animation/multiple frames and contradictory dimensions fail.
- Decoder `Limits.max_alloc`: **32 MiB**, in addition to strict dimension limits.
  Upstream documents allocation limits as **best effort** (some codec internals
  may ignore them); this is not a hard allocator guarantee or PSS measurement.
  The Linux helper retains its existing **96 MiB RLIMIT_AS** as a separate last
  resort. RLIMIT_AS is virtual address space, **not PSS** or an app-tree budget.
  A codec allocation failure can terminate the child; the supervisor must surface
  that failure and reap it. No budget was increased for this implementation.
- Output: one RGB Vec, at most **1024×1024×3 = 3,145,728 bytes**, no global cache,
  thread pool, GPU, thumbnails or background scanning. The decoded source and
  compressed bytes remain resident while this output is produced. JPEG's image
  adapter also keeps its own compressed input; these limits are not an assertion
  that the sum equals real process PSS.
- Unix uses `O_NOFOLLOW|O_NONBLOCK` followed by descriptor metadata validation;
  final symlinks, FIFOs, devices and directories are rejected. Parent path symlinks
  may resolve normally. Same-size concurrent writes are not a snapshot. The file
  descriptor is retained through the bounded read. Filesystem syscalls may still
  stall on remote mounts: parent supervisor timeout/cancel must kill and reap the
  **process**, not accumulate abandoned threads. There is no cooperative callback
  inside codec decode. Non-Unix symlink checks are best effort and the race-free
  Windows/macOS adapter remains unverified.

## Dependencies and licenses

Pinned `image = 0.25.10`, `default-features = false`, features only `png`, `jpeg`,
`webp`. No Rayon, AVIF, EXR, GIF, TIFF, network or GPU feature is enabled. These
are helper-only Rust dependencies; the main UI process does not decode images.

Reviewed package manifests and original licenses in the local Cargo registry:

| Package group | Permitted license alternative used |
| --- | --- |
| image, image-webp, png, flate2, fdeflate, crc32fast, cfg-if, num-traits, quick-error | MIT / Apache-2.0 |
| zune-core, zune-jpeg | MIT / Apache-2.0 (not their Zlib alternative) |
| bytemuck, miniz_oxide, byteorder-lite, adler2 | MIT / Apache-2.0 where offered; byteorder-lite MIT |
| simd-adler32 | MIT |
| moxcms, pxfm | BSD-3-Clause / Apache-2.0 |
| autocfg (build), bitflags (existing) | MIT / Apache-2.0 |

Cargo.lock can contain **unselected optional** `zlib-rs` (Zlib-only), brought into
lock resolution by png/flate2's optional backend. It is **not enabled, compiled or
linked** by this helper; `cargo tree -p files-helper -e normal,build` is the actual
selected closure. No Zlib-only license exception is assumed or requested. Reject
any later feature change that selects that backend without user authorization.

Primary references: [image manifest](https://docs.rs/crate/image/0.25.10/source/Cargo.toml),
[image limits](https://docs.rs/image/0.25.10/image/struct.Limits.html),
[ImageReader](https://docs.rs/image/0.25.10/image/struct.ImageReader.html),
[orientation](https://docs.rs/image/0.25.10/image/metadata/enum.Orientation.html),
[png](https://github.com/image-rs/image-png),
[image-webp](https://github.com/image-rs/image-webp),
[zune-image](https://github.com/etemesi254/zune-image),
[flate2](https://github.com/rust-lang/flate2-rs).

## Validation boundary

Unit fixtures cover PNG alpha/RGB16, JPEG and WebP content detection, tiny WebP,
real JPEG EXIF orientation, eight asymmetric orientation maps against image's
reference transform, source/dimension bombs before codec creation, corrupt
compressed data, contradictory WebP frame/canvas, animation rejection, bounded
resampling, symlinks/FIFO/nonregular files and sparse oversized input. Fixtures
use `~/.cache/files-test`, never personal data or `/tmp`.

Commands (run Cargo serially):

```sh
cargo test -p files-helper
cargo clippy -p files-helper --all-targets -- -D warnings
cargo build -p files-helper
cargo tree -p files-helper -e normal,build
```

This document describes helper plumbing. Space image UI, zoom/pan, cancellation
through the integrated supervisor, visual evidence, main+helper PSS/Swap peaks,
release timing and the full performance gate are **not verified by helper tests**.

## Space integration (partial0.0.4)

Space now opens PNG/JPEG/WebP through the same single-flight lifecycle as text.
A text→image→text change cancels/drains the prior task before spawning a replacement;
closing releases the bitmap and viewport buffer and waits for direct-child reap.
Only one displayed bitmap is retained; a multi-entry LRU and thumbnails are pending.

The shared supervisor limits the JSON header to4096bytes and validates id/format,
source/output dimensions and checked RGB length BEFORE reserving the payload.
EOF must match exactly; trailing/short RGB is rejected. Removing the header drains
the same Vec rather than allocating a second RGB copy. Tests split headers byte by
byte, newline at4096, invalid headers before payload retention, earlyEOF, extra
payload and cancellation during a partial binary response with kill/reap.

The UI supports fit/reset, +/−/Ctrl-wheel zoom, arrow/drag/wheel pan. It resamples
only the visible viewport (at most1160×800×3bytes), never an enlarged full image;
FLTK's own bitmap copy and Wayland buffers remain additional process costs.
The viewport cache belongs to the displayed image and is dropped on replacement;
background palette changes invalidate it. Sampling is nearest-neighbor, not a
full-resolution photo editor. The4Mipixel cap is a partial capability: ordinary
12/24MP photographs still require real reduced decoding, not a higher allocation
limit or rejection presented as completed image support.

188 Rust tests (73core/17helper/98UI), Clippyworkspace all-targets -Dwarnings and
release passed. Existing text protocol8cases and du6cases passed with the release
helper after adding codecs;36Python tests passed. image-ui-1793027538323979457.json
PASS: PNG alpha, JPEG EXIF6 landscape→portrait, WebP1×1, zoom/pan, rapid replacement,
corrupt/validoversizedPNG errors, normal close while another request is pending.
Framebuffer and in-process handlers only; no native OS input or clipboard claim.

Resource run image-resources-1793027605038038146.json is **UNVALIDATED**, not PASS:
pico observed52440KiB=51.21094MiB=53.69856MB treePSS+Swap, above the ideal50MB.
At that sample: main24386PSS+10288SwapKiB, helper17766PSSKiB. Stable total median
27800KiB=28.4672MB; independent stablePSS/Swap medians must not be added to infer a
simultaneous snapshot. MaxSwap separately24096KiB,7threads/18FD/2processes.
122samples, maxinterval92.070ms vs5ms requested; post-previewCPU median/max0% in
its own one-second window. Three helpers had memory/executable identities verified;
five other logged short helpers had no sample. Cleanup of observed PID/starttime
identities passed; known misses are not counted as zero or hidden behind the goal.

files hashac7a03743957a933d9f3723bb6d8c4c60934c365e1e4f72d0b5637d2c859ac6e;
helper5a448d062eef3052fb29890a356ab4fb8ab2381ebdffd85e47f26ee0600eb55e.
No tag0.0.4 or globalPASS: copy baseline56.743936MB and startup151.978msFAIL,
Space text clipboardFAIL, missing helper lifetime coverage and full photos/LRU remain.

For large JPEG research, the installed libjpeg-turbo wrapper is BSD but the full
library also requires IJG; it was NOT adopted under the current exceptions.
A separate MIT/Apache jpeg-decoder IDCT scaling candidate is under review, including
progressive coefficient allocation and worker queue bounds before any wiring.


JPEG upgrade: helper now uses the audited MIT/Apache jpeg-decoder IDCT path, with
Immediate workers and pre-allocation output/progressive-coefficient caps. Baseline
12/24MP really decodes to1000×750/750×500 under the existing96MiBAS limit; scoped
release tree peak42.36288MB (both helpers observed), with discovery/exit warnings.
Large progressive32MiBcoeff rejection does not completeP02. PNG/WebP remain4Mipix;
initial mixed-run53.69856MB ideal miss and missing helpers are not superseded.
See JPEG-SCALED-NOTES for exact scene, hashes, tests and evidence.
