# JPEG scaled preview preparation

The initial image helper can decode at most 4 Mipixels. This rejects common 12/24MP
camera photos and is explicitly partial P02. Preparation in
`crates/helper/src/jpeg_scaled.rs` plus `vendor/jpeg-decoder` addresses baseline
JPEG using **decoder-level scaled IDCT**, without a full-resolution RGB image.
It is now wired into files-helper and Space, replacing image's full JPEG adapter. Standalone and workspace tests passed; scoped UI/resource evidence follows below.

## Rejected native candidate

Local libjpeg-turbo 3.2 offers useful DCT scaling, scan/pixel and intermediate-memory
limits. It was **not adopted**: the primary
[license](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/3.2.0/LICENSE.md)
states the overall TurboJPEG wrapper is subject to **both IJG and BSD-3-Clause**,
not BSD alone. IJG is outside the exact current license exceptions. No permission
was inferred and no FFI, package install, system edit or native decoding was done.

## Permissive Rust candidate and required patch

`jpeg-decoder = 0.3.2` is MIT OR Apache-2.0. Its
[scale API](https://docs.rs/jpeg-decoder/0.3.2/jpeg_decoder/struct.Decoder.html#method.scale)
uses true 1/8, 1/4, 1/2 or 1 IDCT sizes. The local vendored patch is documented in
[vendor/jpeg-decoder/PATCHES.md](../vendor/jpeg-decoder/PATCHES.md), with pinned
source archive SHA256 and original licenses. Defaults are off; no Rayon dependency
is selected. Disabling Rayon alone was insufficient: upstream still creates ordinary
per-component workers backed by unbounded mpsc queues. The new explicit immediate
policy also disables parallel color conversion.

Upstream's output buffer limit is checked **after** decoding and does not cap its
full-source progressive coefficient vectors. The patch checks output size at SOS
before worker allocation, and adds a separate checked coefficient payload sum
**before** the progressive allocations. Neither API is a whole-process memory cap.

Helper-only API:

```rust
pub fn decode(data: &[u8], max_dimension: u32) -> std::io::Result<ScaledJpeg>;
// ScaledJpeg { width, height, source_width, source_height, rgb, orientation }
```

Input bytes are borrowed via `Cursor<&[u8]>`, avoiding the compressed-input duplicate
in image's zune JPEG adapter. `width`/`height` describe **unrotated reduced RGB**;
`source_*` keep original encoded dimensions, and `orientation` is EXIF1–8 for the
host to apply once. The existing image wire header can remain unchanged after the
host applies orientation. It must not use original-source dimensions to index the
reduced RGB buffer.

The wrapper preflights all markers without allocations, before decoder metadata:

- encoded input <=8MiB; source <=8192 each axis and <=40Mipixels;
- one Huffman DCT frame, 8-bit RGB/YCbCr or grayscale; CMYK/lossless/arithmetic pending;
- <=64scans, <=1MiB APP/COM metadata; malformed/trailing/truncated structures rejected;
- progressive coefficient payload <=32MiB, using padded MCU sampling geometry;
- select the highest supported DCT output that fits max_dimension<=1024 on both
  axes, validate actual scale result and RGB byte count **before** decode;
- final RGB <=3MiB, no upscale, no worker threads, no full-source RGB or cache.

Examples: 4000×3000 baseline becomes 1000×750 (1/4), and 6000×4000 baseline becomes
750×500 (1/8). Unsupported exact output sizes are not invented; max_dimension below
the minimum 1/8 size fails explicitly. Small images keep full resolution.

**Progressive 12/24MP 4:2:0 still fail explicitly**: padded coefficients alone need
about36/72MB, above the32MiB coefficient cap. 4:4:4 needs more. Small progressive
images can be decoded within the cap. No existing96MiB helper RLIMIT_AS or150MB
app-tree ceiling was raised. Disk-backed progressive coefficient spooling or another
bounded decoder remains a later task; P02 cannot be declared globally complete.

Prepared tests generate neutral YCbCr JPEG bitstreams directly with tiny fixed
Huffman tables. A24MP baseline fixture is only~140KiB and does **not** allocate72MB
source RGB. Tests cover12/24MP baseline scaled output, small progressive decoding,
large-progressive preflight rejection, panorama factor choice, EXIF host handoff,
malformed data, metadata and scan limits. Standalone compilation and tests were run serially outside app measurements:
9vendor tests (4patch +5upstream) and5wrapper tests **PASS**. The12/24MP baseline
fixtures decoded to expected dimensions and neutral RGB bytes; small progressive
also decoded, and large progressive inputs failed at the explicit coefficient cap.
Evidence: `docs/evidence/image-preview/jpeg-scaled-standalone.json`. This establishes
codec behavior in the Linux host, not helper RLIMIT_AS success, UI behavior, PSS,
startup or throughput. No app, Cargo, memory/latency benchmark or native program was
run for this check.

Cancellation remains parent process timeout/kill/reap; codec internals are not
cooperatively interrupted. Input/source caps and decoder budget checks complement
RLIMIT_AS but do not replace app+helper PSS/Swap, CPU, latency and cleanup measurement.


## Integrated release scenario (partial P02)

194Rust tests (73core/22helper/99UI),73Python, Clippyworkspace all-targets-Dwarnings
and release PASS. Vendor's9standalone tests remain separately recorded; the vendor
is excluded from app workspace version numbers. image uses onlyPNG/WebP now;
zuneJPEG is no longer selected for release. Compressed input stays borrowed, a
no-orientation result moves its RGB directly, and rotation indexes reduced decoded
dimensions while keeping original source metadata in the wire header.

jpeg-large-ui-1793029181093424664.json functionalPASS: real baseline patternedJPEG
12MP→1000×750 and24MP→750×500; EXIF24MP→500×750; real progressive12/24MP rejected
before32MiBcoeff. Cancel/close pending drained normally. Framebuffer/handlers only;
no native input and no claim that progressive photos are supported.

Separate resource run jpeg-large-resources-1793030084337467650.json **PASS scoped,
sampled with warnings**:41370KiB=40.4004MiB=42.36288MB peak treePSS+Swap,38324KiB=
39.243776MB stable; Swap observed0. Both logged JPEG helper PID/start/executable
identities had memory samples (2/2), cleanup complete. Max20FD/7threads/2processes,
204samples/maxinterval69.090ms (requested5ms). One lostTID/children discovery gap;
no lifetime guarantee.708exit observation records refer to FOUR identities, not708
children; raw events remain and derived identity/phase summary is attached.
Post-Ready CPU median0/max269.72% can include first software paint, not settled idle.

New optional sampler children mode is separately unit tested, with all TID and
owner identity checks; prior scan-mode reports remain intact. Different sampling
coverage and scenes mean this is not a controlled before/after memory reduction.
Initial mixed PNG/JPEG/WebP run's observed53.69856MB/UNVALIDATED is preserved and
still proves its ideal50MB violation. Baseline10k56.743936MB and startup151.978ms
FAIL also remain. No globalPASS or0.0.4tag.

JPEG resource scene decoded only two baseline12/24MP files; progressive rejection
and orientation/cancel were verified in the separate functional capture. Five
fixtures were listed. Encoded files are synthetic geometric patterns, not a general
camera-photo throughput benchmark. No global memory budget was raised.

files121d88b93d75dfefea29108b9e2e78c63876e54b95f6c5ff4acf4ada7ae17068;
helperb826e2eca4c14628adcccd02dd4769c57f0ed2695b7c3da42dc27ae21b7368e0.
P02 still needs bounded large progressive support, multi-entryLRU, thumbnails and
broader codec/security/platform/performance coverage. Next functional priority:P04
rendered Markdown, then HTML/video while preserving the complete ROADMAP.

## 2026-10-27 — Progressive DC preview follow-up

JPEG-DC-PREVIEW supersedes the earlier large-progressive rejection path:12/24MP
now render a bounded1/8 bitmap via opt-in DC-only coefficient storage, same32MiB
cap. AC entropy is deliberately not validated in that path; not full integrity
or full-resolution support.445Rust+10vendorlib, Clippy/release and scoped
UI1793078251643976150PASS. Read fidelity/reference/chroma limitations there.
A tiny installed native reference probe was used to explain component-scale
differences, not adopted/linked as a files/helper dependency. NewPSS/startup
remain NOT_MEASURED, prior baseline/partial resource results unchanged.
