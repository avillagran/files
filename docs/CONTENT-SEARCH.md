# Literal content search

Recursive search accepts one positive `content:"literal phrase"` criterion ANDed
with name/type/extension/size/date filters. `Texto`/F6 switches the main input between separate name and literal-content
drafts, retaining existing filters. The content field accepts quotes/backslashes
directly; composition escapes them before serialization. Typing the advanced
syntax in the name field also works. CtrlF returns to the current primary field;
F4/Tab cycle metadata fields and the selected primary input. Blank content mode
requires text or a return to Name before searching; a nonempty text criterion
remains active when editing the name, indicated by Texto+. Exact,
case-sensitive UTF8 bytes, not regex/case folding/semantic search. Quoted values
support \", \\, \n, \r and \t escapes; Unicode can be typed directly. Empty,
negative/duplicate or >1024B decoded content criteria fail before helper launch.
Local browser filtering rejects content criteria and points to recursive search.

Only regular UTF8 files without NUL bytes are in scope. The helper opens with
NOFOLLOW/NONBLOCK, verifies regular-file/mount identity and uses metadata from the
same FD for size/date facets. Every accepted match requires full bounded text
validation and unchanged FD/name revision after reading. A matching prefix of a
binary, truncated, changed or unreadable file is not published. Symlinks/FIFOs/
devices are not read. Same root/no-follow/mount traversal, raw paths and500-hit
bounds remain; observations are not mutation authorization or a tree snapshot.

Limits:accepted files up to1MiB; the reader allows at most1MiB+1 to detect
growth despite metadata (the extra byte can never authorize a match).64MiB total
actual reads include that sentinel. Scratch is32KiB+3 for UTF8 carry, plus <=1024B
needle/KMP table and UTF8 carry. Limits/time/change/read failures make results
partial; binary files are explicitly counted scope exclusions. First match line
and byte offset are reported, no file snippets/persistent index/cache. Existing
single helper, latest intent/cancel/reap/ACK and progress/report validator stay.
Resource/startup freeze and new-preview pause remain. Scoped functional evidence is recorded below; this does not close the full search
ROADMAP or resource/native acceptance.

Match lines are1-based LF counts (CRLF counts once); offsets are0-based original
UTF8 byte offsets. The first exact match is shown per file, not every occurrence.
Binary exclusion includes invalid UTF8/UTF16, NUL-bearing text and incomplete UTF8;
encoding detection/conversion, regex/case folding, snippets, pagination/indexes and
semantic search remain open. Content counters count examined regular candidates,
returned read bytes, binary scope exclusions, limited and changed files. Name-only
search never enters the content-open/read branch. The caps are not PSS guarantees.

Guided content uses its own <=1024B buffer/editor/geometry (six input editors in
total); it does not overwrite the name query. Escaping and combined4096B query
preflight happen before composing the helper request. Editing data cancels the
previous intent; changing input focus/mode does not restart the helper. Delayed
paste checks the same edit epoch and cannot cross between name and text fields.
Results wait for final root proof and helper ACK, as before. When the drawer
leaves less than one visible row, result buttons also have no action target.

## Functional evidence

780 Rust tests (240core/79helper/461UI;3core ignored), workspace/all-target Clippy
-Dwarnings, release/fmt PASS. Final UI presentation follows the suite; final
Clippy/release and framebuffer inspection cover it. Four real release CLI/wire
tests PASS in `evidence/search/content-helper-contract-1793418304103621516.json`.

`evidence/search/content-search-ui-1793418723201526437.json` scoped PASS24 real
framebuffers1000×650/560,800×440,440²: literal field retains name+size, raw quote/
backslash and pointer selection, stale paste rejected after mode switch, oversize
input retains draft, binary/file-limit partial vs narrowed complete, L2/byte5,
advanced escaped content and raw filenames, empty criterion rejects. Existing
four-criteria/names/raw-locate/hidden/500partial/invalid/latest-intent regression
passes. Root0 after ACK;16 logged PID/start helper identities absent/reused, Z
not accepted. Runtime cancel arrived after completion; active cancel/old ACK/
replacement is unit evidence, not this scene. No lifetime coverage, native input,
OS clipboard or helper-alive-exact-close claim.

Original24FB report1793418314346868379 kept with validation-notes sidecar: functional
PASS before correcting a narrow footer hint and ambiguous Texto candidate count.
Final images use Archivos and explicit Nombres/UTF8 summary scope; minimum-width
result buttons disable when no row is visible. Initial compiler/style/fixture
failures and their explanations remain in the notes/logs. No reruns to seek
active-child/progress/resource PASS. Resource/startup NOT_MEASURED, historical
FAILs and preview-expansion pause remain.
