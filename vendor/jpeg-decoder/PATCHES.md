# files bounded preview patch

Upstream `jpeg-decoder` **0.3.2**, MIT OR Apache-2.0. Source was copied from the
local Cargo registry, not edited there. Original archive URL/SHA256, upstream git
commit, and original source-file hashes are in `SOURCE.json`. Both original
licenses and README are preserved. `UPSTREAM-Cargo.toml` is the original manifest;
this local manifest omits examples/bench/dev dependencies and disables default
Rayon. No root workspace/helper manifest or lock wiring is part of this preparation.

`files-bounded.patch` records the source changes against the original archive:

1. `Decoder::set_parallel_decoding(false)` selects `ImmediateWorker` for all
   per-component IDCT work and serial color conversion, including when Rayon is
   present through future feature unification. Upstream defaults remain true for
   other callers. This avoids upstream's unbounded std mpsc MCU-coefficient queue
   (disabling Rayon alone still selects ordinary worker threads).
2. `Decoder::set_max_coefficient_buffer_size(bytes)` checks the checked sum of
   **all padded progressive coefficient Vec payloads** before any allocation of
   those Vecs. It excludes allocator overhead, output/planes/row state, metadata,
   compressed input, stack and libraries. The default stays unlimited upstream;
   files must explicitly set it. This is not a PSS cap.
3. Existing `set_max_decoding_buffer_size` is also enforced at SOS **before**
   worker planes/rows and progressive coefficients, after `scale()` has selected
   the reduced output. The original late validation is retained via the same
   check. Accounting includes two bytes per sample for precision >8.

The patch leaves Huffman parsing, IDCT kernels, SIMD, upsampling and colorspace
conversion unchanged. Four added unit tests cover exact coefficient payload limit,
forced immediate workers, scaled output accounting, and rejection before coefficient
or plane allocation. Standalone `rustc` compilation of this exact patched source and all9 unit tests
(4patch +5upstream) passed. The wrapper's5 tests also passed, including actual
12/24MP baseline decode to reduced RGB. No Cargo wiring, app, PSS/Swap or
performance gate was run; the test framework's runtime is not a benchmark.

Upstream primary sources:

- [Cargo manifest/license](https://github.com/image-rs/jpeg-decoder/blob/v0.3.2/Cargo.toml)
- [Decoder API](https://docs.rs/jpeg-decoder/0.3.2/jpeg_decoder/struct.Decoder.html)
- [decoder allocation/scale implementation](https://docs.rs/crate/jpeg-decoder/0.3.2/source/src/decoder.rs)
- [worker selection](https://docs.rs/crate/jpeg-decoder/0.3.2/source/src/worker/mod.rs)

Before integration: check diff against the
pinned archive, add it as a helper-only path dependency without default features,
exclude the vendor from files' versioned workspace members if needed, and measure
real release helper+UI PSS/Swap under the existing limits. Do not report this
preparation as image UI support or a passing 12/24MP resource gate.

Integration note: helper-only dependency, default features off. Modern Cargo manifest declares historical asmjs cfg as expected without editing audited source. Integrated helper/UI tests and measured JPEG scenario are documented in docs/JPEG-SCALED-NOTES.md.

## 2026-10-27 — Opt-in progressive 1/8 DC preview

`Decoder::set_progressive_dc_preview(true)` is a deliberately limited preview
mode, disabled by default. It requires1/8 IDCT for progressive frames, stores
one i16 per padded block and uses the SAME coefficient byte limit. DC initial
and refinement scan ordering are checked. Existing DC Huffman parsing, IDCT,
worker/upsampling/color kernels are reused; AC entropy is skipped to the next
structural marker. Ignored AC entropy is NOT validated: this is not an image
integrity checker or a full-frequency decoder. Missing DC components/duplicate
initial scans/wrong refinement order and wrong scale are errors. Default valid
image behavior stays unchanged; unknown marker now errors instead of unwrap panic.

At EOI bounded MCU rows expand DC into temporary64-coefficient blocks for the
existing1×1 IDCT worker; no full-image64-coefficient allocation. files enables
this fallback only when progressive full coefficients would exceed32MiB. The
limit remains32MiB, parallel decoding stays off, output/source/input limits and
parent cancellation/kill/reap remain required. This is NOT a total allocator/PSS
claim. Resource/startup measurement remains frozen by explicit user instruction.

10vendor --lib tests and50helper tests pass, including real4:2:0 color/refinement
byte-for-byte equality with the full decoder at1/8,4:4:4 external color reference,
12/24MP neutral fixtures, wrong scale and duplicate DC initial scans. A vendor
`cargo test -p jpeg-decoder` attempt also ran upstream doctests which expect
unbundled sample image files; those doctests failed, logged separately. Do not
call that command globally PASS. Appworkspace tests do not run excluded vendor
doctests. No license files or upstream README edits.

`files-bounded.patch` now uses zero context (no patch-context trailing spaces).
Its input archive/source hashes are pinned in SOURCE.json; applying with fuzz0
and comparing ALL17sourcefiles reproduced current source exactly, evidence in
`docs/evidence/jpeg-dc/vendor-source-review.json`. Earlier reconstruction evidence
is historical and unchanged. Native libjpeg-turbo was used only as an installed
reference tool/probe on synthetic fixture data, never linked to files/helper or
added to Cargo. Equal global1/8 scale is not equal chroma sampling: Turbo chooses
2×2 IDCT for4:2:0 chroma while this Rust codec uses1×1. That existing difference
is recorded, not hidden by increasing a reference tolerance.

Integration:445workspace tests+10vendorlib, Clippy/release and functionalUI
1793078251643976150PASS. Source audit also identified preexisting formatting drift
between old patch and8tracked sources in53998a5; rustfmt reproduces it exactly.
The combined patch now reconstructs all17sources; new algorithm change vs53998a5
is only decoder.rs. No new PSS/startup result, see JPEG-DC-PREVIEW.
