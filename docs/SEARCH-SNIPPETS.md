# Content-search fragments

Content results include a short observed fragment around the first literal match,
in addition to filename/parent/line. The match is highlighted. Ellipses indicate
omitted source context or additional UI cropping; a long literal is still matched
in full even when its displayed portion is clipped. Tabs, newlines and whitespace/
controls are normalized to spaces in the fragment only, never in the matcher or
source file. This is a search result, not a document preview or live file snapshot.

The existing exact UTF8 KMP scan reads/validates the whole bounded file, including
bytes AFTER an early match. A binary/invalid/limited/timed-out/changed file does
not publish an accepted fragment. Existing FD/name/revision post-read validation
still gates BOTH position and fragment. There is no second read/reopen, full-file
String, new helper/thread, dependency, persistent cache or preview engine.

## Capture and wire bounds

Helper retains1088 bytes of rolling previous-block history, up to256 raw context
bytes and up to256 normalized UTF8 bytes, plus its existing32KiB+3 read buffer/KMP
failure table. It copies only the previous-block tail, not a whole source raster.
The first match captures up to64 preceding source bytes and the remaining256-byte
window afterward. UTF8 edges are trimmed to character boundaries; normalized
highlight start/end are byte offsets into fragment text. Long patterns (up to1024B)
can set match_clipped; this does not change the full match result.

Hit.snippet is optional for compatibility. Its bounded256B text deserializer and
Report/Candidates validators enforce capacity, no controls and nonempty highlight
on UTF8 boundaries. Names-only hits cannot carry snippets. Exact final Hit equality
binds the fragment/positions/flags as well as raw path/kind/content coordinates.
A maximum500 hits means<=128000 bytes snippet capacity per retained population.
The decoder/visible/unread/building snapshots or final report can account for up
to4 populations: <=512000 snippet bytes in addition to existing<=1MiB path capacity.
Helper retention is separate. Hit/Arc/JSON/pipe/allocator overhead is not included.
These are local bounds, not measured PSS/Swap or a global resource PASS.

Existing2MiB line and4MiB total output caps remain. A pure worst-case encoding test
combines500 paths with maximally escaped control bytes,256-byte fragments requiring
JSON escaping, progress allowance and borrowed batches+final envelope within those
caps. This is protocol-size evidence, not a throughput or memory benchmark.

## Presentation

Rows with fragments use66px; names-only rows stay46px. Page movement and hit targets
use the same height. Context is cropped near the match in narrow windows. Literal
Painter text avoids interpreting fragment data as formatting; accent/background
roles mark the match. Read-only rendering does no filesystem I/O and creates no
per-file widgets. It does not add a text editor, selection/clipboard, OCR, regex,
case folding, alternate encodings or full grapheme/HiDPI coverage.

Validation:843 Rust (260 core/85 helper/498 UI;3 existing ignored),
workspace Clippy-Dwarnings PASS. Initial E0282 type inference fixed with explicit
Match option; E0063 old navigation fixture supplied optional field. Edit-script
ValueError left UI unchanged before corrected patch; logs retained. Tests cover
UTF8 chunk boundaries/one-byte reads, normalization, long-literal clipping, rejection
after later invalid/binary/file limits and literal compact highlight geometry.
Release/fmt PASS. Same-binary functional scenes:
- content-search-ui-1793434023389723179.json PASS20FB,16logged helper identities
  gone/reused/root0drainACK. Four new/content PNG inspected: ordinary quoted
  match, Unicode leading/trailing context1000/440, full1024B literal with256B
  clipped fragment. Wire text/UTF8-byte marks/flags/positions asserted, no controls;
  binary/invalid/limited files excluded. Sources retain their hashes.
- search-batches-ui-1793434087267418740.json PASS7FB,6logged helper identities
  gone/reused/root0ACK. Provisional raw-content fragment text/mark verified before
  final while helper held by the established isolated fixture gate; its compact
  PNG inspected. Cancel/root replacement/latest query and normal Locate regressions
  pass. This hold is not natural latency/kernelD evidence.
No native input/clipboard/full lifetimes/HiDPI acceptance. Five PNG inspected;
all27 framebuffers' dimensions checked by their scripts. Initial compilation/edit
failures retained; final image/semantic scenes passed on first execution.
Freeze resource/startup and new-preview pause remain; global goal still open.
