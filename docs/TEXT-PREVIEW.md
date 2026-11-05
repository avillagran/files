# Bounded text preview helper

`files-helper preview_text` accepts a single JSON request:

```json
{"command":"preview_text","id":1,"path":"/absolute/file.txt"}
```

The `text` response contains `id`, `text`, `encoding`, and `truncated`. Errors use
the existing `error` response. The subcommand must match the request variant.
The process exits after its response, like `du`.

The reader consumes at most 256 KiB plus one sentinel byte, even for sparse files
many gigabytes long. It displays at most 2000 LF-delimited lines, preserving CRLF.
It supports UTF-8 (optional BOM) and UTF-16LE/BE with BOM. A code point split only
by the byte limit is dropped and truncation remains explicit; invalid encodings
are rejected instead of silently replaced. Binary control characters other than
TAB/CR/LF are rejected. Other encodings and symlink targets are pending.

Unix opens use O_NONBLOCK/O_NOFOLLOW, then validate the opened descriptor is a
regular file before reading, so FIFO replacement cannot block open. Blocking
filesystem operations on remote mounts still need the future preview supervisor's
hard deadline and cancellation. The existing Linux helper address-space limit
remains in effect. Text JSON escaping can expand the bounded text: consumers must
allow up to 2 MiB for this response, while retaining the 64 KiB request limit.
The du supervisor's 8 KiB response limit must not be reused unchanged for text.

This is text/source preview plumbing. Space overlay, syntax highlighting,
selection/copy, keyboard navigation, rendered Markdown/HTML, image/video/PDF
backends, integrated PSS and rendered UI acceptance are still pending. It must
not be advertised as Markdown or HTML rendering.

Validation commands: `cargo test -p files-helper`, helper Clippy with warnings
denied, `cargo build -p files-helper`, and `python3 scripts/text-preview-check.py`.

Verified locally: 8 helper Rust tests (5 text + 3 du), helper Clippy with warnings
denied, 8 text protocol cases and all 6 existing du protocol checks passed.
`text-preview-memory.py` measured the debug helper alone with a sparse 4 GiB file
and a 256 KiB response, deliberately holding stdout for 500 ms: sampled peak
PSS+Swap **1.3824 MB**, complete readable samples and cleanup. This tests output
backpressure and bounded allocation, not UI totals or preview latency. Evidence
is `docs/evidence/text-preview/helper-memory.json`.

## Space UI (0.0.4 in validation)

Space or the inspector's Preview opens the text in its own UI. A single controller
keeps the last requested destination; it cancels and reaps the previous helper before
replacing it, discards stale responses even when reopening the same path and frees
the document on close. Navigating, changing selection and closing the window cancel; exit
waits for `is_drained`, also when spatial preferences are discarded.

Only visible lines; selection by pointer/Shift+arrows/Ctrl+A and CopyPreview.
`Painter::literal_text` and `text_width` use the same font; each visible line
expands TAB to 8 scalar columns and control characters to ^X, with a source→display map. The
original UTF-8/CRLF/TAB bytes are copied, not the transformation. The gutter has its own clip;
an empty selection only paints the caret. Columns are not graphemes. To bound shaping,
the first 4096 bytes of each line are drawn and the cut is labeled: the rest is not yet
visually accessible, although Ctrl+A/copy keeps the full source within the limit.
Alt+↑/↓ changes file, Shift+wheel scrolls horizontally. Plain text/code;
Markdown and HTML are shown as **source**; their real renderers are still pending.

126 workspace tests (73 core/8 helper/45 UI), Clippy and release passed. New tests
for width/tab/control/gutter/empty selection, controller and supervisor. The verifier
`scripts/text-ui-check.py --capture` checks Unicode+TAB+CRLF source by pointer,
A→B→C, binary errors, explicit truncation and close with a pending helper. Real
captures in evidence/0.0.4, internal handlers; no native key/pointer events.

The external clipboard is still **FAIL; the milestone is not closed**: wl-paste receives no offer
when the fixture is not the active window and FLTK publishes with serial 0; the trace records
set_selection and no keyboard_enter. Wayland focus for that fixture and an exact
repetition after CopyPreview are missing. It is not replaced with wl-copy and earlier contents are not read.
The initial loop failure was fixed separately: the 1.5.23 wrapper rejected
poll counts >=2. Two pipes test raw 2/callbacks 2; the new scene records raw 4/3 successfully.
The raw value of the original failure is unknown, so the exact cause of
that event is not claimed. The local wrapper keeps C Fl_wait_for, immediate errno and retries only on EINTR;
a real error returns exit 1 after cancelling/draining mutations/preview/du.

Integrated Space metrics are still pending, separate from capture/clipboard.
The earlier isolated helper figures are not the app tree's budget.
