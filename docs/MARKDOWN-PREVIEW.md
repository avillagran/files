# Native Markdown, 0.0.4 in development

Space recognizes `.md` and `.markdown`. The text is read with the bounded helper,
the child is reaped and it is parsed/projected on the same supervisor thread. The UI receives
visible text with styles, without copying delimiters or the code block's language
metadata into the selection. No WebView, server, scripts or network are involved.

The own renderer draws headings, bold, emphasis, strikethrough, ordered and
nested lists, tasks, quotes, code, rules and tables with per-column alignment.
The styles use the regular/bold/italic faces of the same Nerd Font resolved
for Omarchy. The Cairo/FLTK backend interprets their suffixes through Pango;
outside this host the visual evidence for the faces is pending.

The layout reflows when the width changes and keeps the selection by UTF-8 offsets of the
visible text. Each step yields after roughly 4 ms between bounded
fragments. A shaping call cannot be interrupted; `step_max_us` records
the largest observed step, without promising 4 ms as the worst case. The tick stops requesting
redraws when complete. Only the fragments that intersect the viewport are painted.
There are anchors for empty/final rows for the caret and navigation.

Tabs expand to 8 columns and control characters to a visible notation, with the same
map for drawing and measuring; the selected text keeps its bytes. Movement
by scalars and prefix widths **does not prove grapheme navigation nor correct ZWJ
shaping**. The fixture keeps complete combining/ZWJ sequences in the checked
selection, without turning it into a general shaping test.

## Links and resources

Ctrl+click on an allowed link navigates inside Space. Fragments
look up heading slugs (lowercase Unicode alphanumerics, spaces to hyphens).
Relative Markdown paths are opened through `preview_relative_text`: a root opened
in the helper, components through `openat` without symlinks, a regular file, a bounded prefix
and the shared UTF-8/UTF-16 decoder. Each request pins its root FD during the
read; reopening the root in another request is not a permanent snapshot.

There is no automatic opening of another app nor of URLs. External and absolute destinations,
authorities, control characters, queries and lexical escapes are blocked. The policy now keeps pending parents when parsing and resolves them against
the current document relative to its root BEFORE the helper. `../` can go back
from a subfolder, but crossing the root is rejected without launching a load.
There is no second URL decoding; the helper still rejects a raw ParentDir. Mountpoints/hardlinks and in-place writes are not
confined; this resource policy is not a process sandbox.

The first block 7dcc037 showed alt/status without a bitmap. The later block
integrates local bitmaps as described in the embedded section below; P04 still has no
validated budget nor an LRU across documents. Embedded HTML stays literal and does not implement
the P05 HTML view. Image resources, LRU, fonts/rendering on
other OSes and accessibility of the rich document are pending.

## Limits

- Markdown input 256 KiB; helper 2000 LF and UTF-8/UTF-16 with explicit truncation.
- IR 8192 events, depth 64, columns 64, destinations 64 KiB in total.
- Visible text 384 KiB/2001 lines, 16384 spans, 32768 layout fragments.
- Measurement fragments up to 259 UTF-8 bytes, with tabs/control characters expanded locally.
- A table that is too wide produces a visible error, not hidden inaccessible columns.
- One preview helper at a time; changes keep only the last destination and drain
  the previous one. Close keeps pumping until ACK; Drop only cancels.

These limits bound the retained representations; they are not a total PSS limit
nor a parser limit. Measure main+helpers+Swap separately from the build/captures.

## Reproduction and evidence

`python3 scripts/markdown-ui-check.py --capture` uses its own fixtures and internal
handlers with the real framebuffer. It does not test OS input nor the Wayland clipboard. Each
READY requires a fresh revision and a finished layout at the checked width; resize waits for
the new width before capturing. New captures have unique names.

`python3 scripts/markdown-ui-check.py` measures another process without captures/building.
The gate requires PID/start+exe identity and the memory of every registered helper,
complete samples and normal cleanup of the observed tree. If readings are missing,
UNVALIDATED; an observed peak above 50 MB still shows the ideal is not met.
150 MB is the scenario's ceiling; it does not replace the 10k baseline gate nor the 50 MB goal.

The capture/resource status is added when the run completes. The earlier 10k baseline and
startup FAILs, the Space clipboard FAIL without focus and partial progressive P02
remain open; this document is not a global PASS.

## Verified result of the first visible block

230 Rust tests (73 core/33 helper/124 UI), Clippy workspace/all-targets with -Dwarnings and
release without warnings: logs in `docs/evidence/0.0.4/markdown-logs/`.

`markdown-ui-1793032850488569491.json`: functional scenario PASS, fresh revision and
width, headings/styles/tables/lists, 54 Unicode bytes selected, resize to 800×500
keeps the selection, local guide/return, a real symlink error, a truncated READY at
2000 LF, a 201033-byte document and a normal drained close. Framebuffer 1000×650/800×500,
scale 1. OS input, general ZWJ shaping and clipboard NOT validated. Literal HTML and
alt-text images are not equivalent to their respective full renderers.

The first capture `markdown-ui-1793032403245429423.json` and the resource report
contain an original functional PASS whose error assert was too broad:
`too-many.md` was READY truncated, not a rejection. Both originals stay byte for byte;
their `*-correction.json` sidecars invalidate that claim. The corrected capture
checks the real behavior; memory was not repeated to hunt for a PASS.

`markdown-resources-1793032456170456020.json`: **UNVALIDATED**, only 2/8 registered helpers
have a memory+exe sample. Observed peak 42166 KiB = 41.177734 MiB =
43.177984 MB; post-Ready median 36809 KiB = 35.946289 MiB = 37.692416 MB, observed Swap 0.
368 samples, maximum interval 74.675593 ms, 19 FD/7 threads/2 processes; post-Ready CPU
median 0/max 162.718%, idle not assured. 3070 exit events correspond to 10 identities,
not 3070 children. Cleanup of the observed identities complete. No budget for full
lifetimes nor GPU. The 50 MB ideal is not proven because of incomplete coverage.

Observed parse/layout ready for the 201033-byte document: 634.649 ms in the resource
run, 908.578 ms in the corrected capture. Incremental reflow keeps callbacks;
a single step reached 18.306 ms (resources), 27.564 ms (corrected capture), 60.543 ms
in the preserved first capture. No 4 ms guarantee. First internal paint 92.329 ms in
resources/193.599 ms in capture; NOT an external cold-start series nor a replacement for the earlier
p95/max 151.978 ms FAIL. App SHA 240ce31875b282dfc25321c4982b11eabf66433d28637da0494f284b11327417,
helper SHA 9f225bf85dbb3a264f961dbcde7db07751565a3993112978f3dfc5b530c5ae86.

## Embedded images and contextual navigation, second block

482393f/7504a93 (helper relative_text/image) and 3676855 (contextual resolver) are integrated.
Local PNG/JPEG/WebP use the same codec limits already documented. The
helpers run sequentially in the document's supervisor: one is reaped
before the next, with cancellation and a 10 s cap for the whole set (each child up to 5 s).

Up to 8 local references are processed; external destinations/fragments/escapes,
symlinks, errors and limits are shown next to the alt text. The footer counts **unavailable**
images, not loads that are still waiting. The selection copies the stable alt
text, not a 'pending' text that would become false once the bitmap completes.

Retained RGB: 4 MiB in total, charging `Vec::capacity`, `String::capacity`,
Bitmap and Arc. Normalized paths inside one document share Arc/RGB; a
bitmap is not cloned per occurrence. Before a decode the ledger reserves
another 4 MiB at most for transient wire/RGB, and new loads are skipped if it is already
full. This ledger does NOT include the whole process/helper and does NOT prove its PSS.
A single software buffer of at most 640×320 is reused across visible images,
invalidates its key when the bitmap changes and records its real capacity. Not N buffers per image.

No LRU across documents yet: it is read again on navigation to avoid a
cache without safe invalidation. P09 is still pending. The current-root path is opened
by FD on each request; no snapshot nor confinement against hardlinks/
mountpoints is claimed. Contextual escapes are rejected before the helper syscall.

242 Rust (73 core/33 helper/136 UI), Clippy all targets -Dwarnings and release without warnings.
`markdown-embedded-ui-1793034469432638103.json` functional scenario PASS:
5 occurrences/2 retained resources, 3164572 ledger bytes; three real stripes in the
framebuffer (5792/5611/5611 pixels), an admitted 320×320 bitmap with exactly 102400
pixels; another bitmap rejected because of the 4 MiB. Navigation root→subdir→nested→subdir→root,
fragments, three escapes blocked without a new helper/revision and close 0 after ACK.
The earlier captures 1793034242503041636 are intact; final labels unavailable.

**Resources NOT_MEASURED by the user's explicit instruction: only integration
and capture in this phase; no more resources/startup until the phase is lifted.**
Attempt 1793034332264805450 was blocked in preflight before launching the app because
compilers were running; `markdown-resource-blocked-1793034332264805450.json` has
UNVALIDATED/NOT_RUN, with no new memory readings. The footer's PSS is not used
as a budget. The earlier MD run 43.177984 MB UNVALIDATED does not measure this binary.

App SHA 16cd4565e50b40f20ed21eed3665cfaf085726f75f65d38f704b2a42f8dc1d80,
helper SHA 9f225bf85dbb3a264f961dbcde7db07751565a3993112978f3dfc5b530c5ae86.
No OS input/clipboard/general grapheme shaping, no 0.0.4 tag nor global PASS.
Next: real P05 HTML, then video/the rest of the ROADMAP; do not count the HTML probe as a feature.
