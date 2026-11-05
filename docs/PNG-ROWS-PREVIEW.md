# PNG downscaled by rows — 0.0.4 (2026-10-27)

Functional unit verified on Linux/Wayland; P02 is still partial. The adapter uses png 0.18.1, already a transitive dependency of image and
MIT/Apache licensed; only the direct edge in helper/Cargo.lock is added, with no
new package. No native engine or GPU. The production PNG path reuses this
same row decoder; WebP keeps the earlier path limited to 4 Mi pixels.

8 MiB compressed, 8192 per axis/40 Mi source pixels, 1 MiB of metadata and 65536 chunks.
Decoder::Limits 32 MiB is kept as a best effort for its own reservations,
NOT a cap on the allocator/total PSS. next_frame is never called and the full source
raster is never reserved for decode/deinterlace. RGB output <=1024²/3 MiB and a
sourceY→destination-axis map <=8192 u16/16 KiB. The reader only receives one row/scratch; the largest raw
row is 8192×4×2=64 KiB. The compressed input is lent to a Cursor, not duplicated.

RGB/gray/alpha 8 and 16 bit, palette/tRNS and sub-8 gray are expanded before sampling.
16 bit is rounded to 8 bit with the same scale as image; it does not simply drop the
low byte. The checkerboard compositing and the
orientation/fit functions are shared with the earlier path. The eight orientations write directly
to their final position, without a second RGB. Adam7 compares each pass descriptor
with the expected schedule; empty/narrow rows and selected samples are
covered without building a full-frame deinterlace buffer. Every output pixel must be
filled exactly before Preview is returned.

Preflight looks for eXIf even after IDAT to know the orientation before
sampling. Decode validates CRC, Adler32 and ancillary CRC errors; finish validates
the end before publishing. Text/ICC are skipped (no full color management),
raw metadata limited to 1 MiB. APNG is explicitly rejected as pending; no hidden
frames are launched. Trailing padding after IEND keeps the earlier compatibility
under the total 8 MiB limit; it is not presented as validated PNG content.

Expected/effective source FD, resource root, epochs, latest-wins, cancel/kill/reap,
LRU and pipe are the existing ones. No I/O/decode in draw/UI and no persistent cache.
The local caps do NOT prove the tree's 50 MB ideal/150 MB ceiling. Helpers and viewport/
backend have separate costs; the resource/startup phase is still frozen.

## Tests

8 PNG regressions: exact match with the earlier full reference for 8/16-bit types,
palettes/tRNS/sub-8 gray, eight orientations, Adam7 odd/narrow and late eXIf,
CRC/truncation/large metadata/APNG, a generated 12 MP image decoded by rows,
source/output cap. Whole workspace 453 = 141 core/58 helper/254 UI and Clippy -Dwarnings
PASS. The old bomb test had a truncated IHDR and expected the 4 Mip text; now a
complete 8192² IHDR rejects 40 Mip before CRC/reservation. The receiver accepts a large PNG
only with a bounded RGB wire, like JPEG; a large WebP is still rejected.
Image/PDF/video presentation and embedded bitmaps now share the
8 px CPU radius of thumbnails, as addendum H required: before, the general paint() passed 0.
The letterbox test checks both paths; it does not depend on window corners.
A single sampling reservation is kept, with no new image for the mask.
The footer switches to Reduced view/Original resolution based on the bitmap's pixels,
not the earlier technical text PNG/WebP≤4 Mip, which no longer describes the PNG path.
Release clean and scene image-ui-1793081848018085943.json functional PASS:
9 framebuffers 1000×650; PNG 9 MP→1024², 12 MP→1024×768, 24 MP→1024×682,
late EXIF 6→682×1024 and Adam7 with zoom 2. The effective FD signature and
at least 4 bitmap colors were checked; alpha, corrupt, small JPEG/WebP, quick change and
zoom keep their tests. The compositor scales 2/physical window 2000×1300, but the capture is
1000×650: it does NOT prove native 2x sharpness. A visual review of 12 MP and oriented 24 MP shows
checker/colors, rounded borders and controls without overlap.

Normal close 0 after the preview ACK/drain; the 12 registered PID/start identities
are absent or reused; Z for the same PID/start is never accepted. This is NOT
coverage of every lifetime. Close was requested with a pending preview;
the source gate blocked that helper from starting, so closing DURING
an active PNG decode is still NOT_VALIDATED. It does not test OS input.

Resources/startup NOT_MEASURED because of the freeze; the footer's PSS does not approve a budget.
No new tree gate, no 0.0.4 tag/global PASS. Large WebP/APNG, ICC color,
downscaling quality, global P09 across formats and other OSes are still pending.
SHA files 3dc74314523f4862f919d9f66779855fde502fb0006b6ed5cb2886e9ef4628f0;
helper 187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.
