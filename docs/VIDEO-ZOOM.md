# Video and GIF: zoom and framing — 0.0.4

M18 reuses `image_preview_view::Viewport`, with no new decoder or cache.
Ctrl+wheel or +/- zooms from 1× to 8×; dragging pans the framing and 0 fits it
again. Up/down pan vertically; left/right keep the
5 s seek. The timeline takes priority over the drag gesture.
Videos are not preloaded and do not play on open.

A new frame invalidates only the resampled pixels, without resetting
zoom/pan or freeing the single viewport buffer. Pause/resume keeps the same
viewer. Seeking frees the previous RGB and keeps only three numbers; the
new snapshot restores that transform. Reopening the file, changing the source or receiving
an invalidation of its revision resets the framing and discards the old generation.
The framing is clamped to the image bounds during paint, without I/O.

The sampling raster stays bounded to the viewport (at most 1160×800×3 bytes), even
at 8× zoom. Its capacity may be kept when the window shrinks; this is not evidence of the
process/helper/allocator use nor of the global budget. RAM/pipes, no
on-disk product cache or frame history. The transform belongs to the
session of the selected source; it is not persisted per file.

488 Rust (141 core/58 helper/289 UI), Clippy workspace/all-targets -Dwarnings,
release and fmt clean. A sampling test verifies new RGB without stale pixels,
reused capacity, a preserved transform and a distinct reset. State tests for
stream/seek/coalescing check that the transform persists and resets per file.

`video-play-ui-1793101116101351845.json` GIF and
`video-play-ui-1793101152229152975.json` FFV1/MKV: functional PASS, 11/9 real
framebuffers 1000×650. The shared Action zooms 2×, pointer handlers pan 48/36;
asserts after red→blue/pause/seek 2 s/resume EOF keep those values. Reopening
resets to 1×/0/0. Closing with the stream observed alive ends 0 after ACK and 12/10
registered PID/start identities are no longer alive (Z not accepted), with no lifetime coverage.
The GIF scene includes the existing 160 px inspector thumbnail; no new decoder.
Captures of both profiles were inspected; fields/controls with no evident
overlap. Solid red/blue does not prove photographic detail or color accuracy.

No 24-frame clip was generated and no memory is inferred from the incidental footer. No OS input and no
2× sharpness test. Resources/CPU/startup NOT_MEASURED because of the freeze; the historical FAIL/UNVALIDATED
results remain and P07/P09 are still partial (VFR/PTS, color, A/V sync,
large codecs and other OSes).

Checkpoint 59e348c installed automatically with backup `/home/avillagran/.cache/files-test/installed-backups/1793101224102815320`.
files `8cb2c80c87045b60f9a8ea02010f1831be0d61b1323dab7f2c124d948b5a9b7f`; helper/HTML consistent,
verified in `docs/evidence/0.0.4/installed-checkpoint-1793101224257101226.json`. Existing windows were not controlled.
