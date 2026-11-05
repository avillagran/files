# Rotating the view without modifying files — 0.0.4

M20 adds two Nerd Font buttons in the image/PDF/video/GIF header.
R rotates 90° clockwise and Shift+R counterclockwise; the accessible names make clear
that only the view is rotated. 0/Fit returns to 1× zoom, pan 0 and rotation 0. Rotating keeps
the zoom and sets pan 0 so it does not drag the crop from the previous orientation.

Sampling maps viewport coordinates directly to the original RGB, with
width/height swapped for 90/270°. No rotated copy of the source is allocated,
and the file, EXIF or PDF is not modified. The single visible raster stays bounded to
1160×800×3 bytes; the content key includes the rotation. Roundings are applied to the
oriented image. It is not destructive editing nor a persisted preference.

The numeric transform (3 floats and 1 byte of rotation) survives new frames and
seeks. PDF keeps zoom/pan/rotation across pages and when going through extracted text;
the new raster clamps the pan to its bounds. Another file/revision and closing
reset it. No RGB history is kept and the rotation is not shared with thumbnails.
The EXIF/metadata orientation already applied by the decoder is kept as the base;
this manual rotation is added afterwards.

F1 Help adds the navigation, zoom, rotation, playback, audio and PDF shortcuts
that already have handlers. Text/code/HTML/Markdown do not show rotation buttons.

A test with an asymmetric 3×2 matrix checks the 4 mappings and the length of the single
raster; State tests keep the rotation across frames/seek/pages and reset on source change.
489 Rust (141 core/58 helper/290 UI), Clippy workspace/all-targets -Dwarnings,
release and fmt PASS. An intermediate Clippy run rejected the overly complex key tuple;
it was replaced by SampleKey with explicit fields, without silencing the lint. The initial
and corrected logs are kept in target/preview-rotation-*.log.

`preview-rotation-ui-1793102443334515413.json`: functional PASS, 14 framebuffers
1000×650 of image, video, GIF and PDF with 4 asymmetric quadrants. Real
framebuffer pixels (tolerance 8/255) are checked against the expected order: clockwise/
counterclockwise/reset rotation, playback/seek frames, page 2 and returning from the PDF
text. Reopening another source resets the rotation. The byte hashes of all originals are
identical before/after. Root exit 0 after callback/ACK; 21 registered helper PID/start
identities absent/reused and Z not accepted. No helper observed alive
on close, and no lifetime/AT-SPI/OS input/clipboard/native 2× coverage.
Image 0/90°, video during playback and PDF page 2 were inspected; Nerd buttons
visible, with no evident tofu/header collision. It is not a test of photographs,
color accuracy or playback performance.

This scene makes 14 functional in-process readbacks, not an isolated memory
experiment or clip. Do not infer resource use from the incidental footer. Resources/startup NOT_MEASURED because of the freeze;
no local limit is equivalent to global PSS nor resolves the historical FAILs.

Checkpoint e537ce1 installed with backup `/home/avillagran/.cache/files-test/installed-backups/1793102536282364022`,
app/helper/HTML hashes verified in `docs/evidence/0.0.4/installed-checkpoint-1793102536363937562.json`.
files `a682485cf39a00fe2695a4b020338c55d156277f4b5ef08ba01d3964fb73fc3a`. Existing windows not controlled.
