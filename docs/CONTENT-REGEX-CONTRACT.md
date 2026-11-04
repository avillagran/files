# Content regex search

Status: advanced backend0a1be7c plus installed guided sourcef2b22e2. Current
record1793532994109325152 verifies five installed files and five backup hashes.
The full explorer goal and the remaining S02 capabilities stay active.

## Query and matching

Use `regex:"(?m)^error[0-9]+$"` in the main search query. Combine it with name,
extension, type, size and date clauses. Exactly one positive `content:` or
`regex:` criterion is allowed. `content:` and its separate F6 field retain exact
literal semantics; regex is an explicit advanced-query clause. `name:regex:x`
continues to search a filename. Local folder filtering rejects content criteria.

Quoted patterns use the existing conventional string escapes, so `regex:"\\z"`
and unquoted `regex:\z` express EOF. Empty criteria reject; `regex:"^$"` can match
an empty file. Invalid/oversize patterns reject before a helper is admitted. The
bounded compiler reason is shown on one line rather than below an error heading.

Regex sees the complete allowed UTF-8 file, not separate truncated lines. First
match includes zero-width anchors, word boundaries and EOF, including empty files.
Positions are UTF-8 byte offsets and one-based lines. Inline multiline/dotall
flags work. Only the first match per file is retained, as for literal search.

The pinned std-only regex engine is unchanged. Unicode literals/scalars work;
Unicode property tables/case folding/backreferences/lookaround are unsupported
and fail explicitly. Use ASCII ranges or (?-u:...) classes when appropriate. No
new dependency or license change. Other encodings remain unsupported.

## Bounded read and observation

Pattern max1024B, NFA limit1MiB, DFA setting256KiB, nesting64, diagnostic1024B.
Core Matcher rejects supplied text over1MiB. Regex helper scanning reserves one
buffer of at most1MiB, reads blocks of32KiB within the shared64MiB search budget,
and checks deadline before reads and before/after matching. NUL/invalid UTF-8
files have no accepted match, even if their prefix matched. Oversize/read-limit/
changed-revision/deadline outcomes remain explicit; none proves absence.

The literal KMP scanner stays streaming. Regex compilation happens once per
helper scan through Filter; it is not repeated per file. Both paths use the same
NOFOLLOW/nonblocking checked descriptor, mount/metadata policy and final revision
validation before accepting a match. Regex requires complete input validation
before acceptance. There is no file reopen for matching or snippets.

Regex may buffer a full allowed file. These input/compiled-state bounds are not
whole-process PSS/Swap/CPU or timing claims. Checks cannot interrupt a single regex
engine operation internally, nor an already-blocked filesystem syscall. Helper
supervision/cancellation and its reap ACK remain separate from result publication.

## Wire, pages and UI

Stats.regex is optional/default false and omitted in literal reports. Match's
optional regex_end is omitted in literal mode. Regex requires
`byte <= regex_end <= total bytes` and end<=MAX_FILE. Literal strict < bounds
remain. Mode must match the exact composed query's expectation in final reports,
candidate batches, page ledgers/readers and the UI stream decoder.

Zero-width matches have regex_end==byte and no snippet; validators reject an
invented highlighted character. UI marks them with ∅ beside the line. Positive
matches use existing normalized <=256B snippets, with explicit clipping flags.
The strict nonempty Snippet contract is unchanged. Every provisional span/path/
snippet is reconciled exactly with the final report. Page mode/counters stay
coherent through credits and terminal pages. Both wire modes fit existing
2MiB/frame and4MiB/response budgets at500 worst-case hits, without expansion.

Advanced query clauses already participate in saved criteria and fingerprints
without a format change. Guided content interpretation adds the explicit v2 flag
documented below; literal-only catalogs still encode as v1.
Guided Literal/Regex controls and preset-specific v1/v2 runtime acceptance are
now documented in GUIDED-REGEX-CONTRACT.md. Literal F6 and advanced filename-query
syntax remain separate from the explicit content interpretation.

## Evidence history and acceptance

Matcher foundationfb9958f:429core/3ignored then4 focused, core Clippy/fmt PASS.
Wire foundation16e09c6:1224workspace433/95/696+3ignored/Clippy/fmt PASS; later
literal+regex500-hit wire budget PASS. See content-regex-foundation and
content-regex-wire evidence. Initial decoder-callback and initial-progress test
fixture failures were corrected and preserved. Historical folder-view WouldBlock
reappeared in one run; normal-concurrency retry passed, cause unconfirmed.

Current integration acceptance is recorded separately in content-regex-integration.
No native OS input/clipboard/AT-SPI/2× or whole-process/resource/startup evidence
is implied. PSS/Swap/CPU/startup experiments and new content previews stay frozen.


## Integrated acceptance on NODE (2026-11-01)

Current-source1228 Rust PASS434core/98helper/696UI+3 historical ignored;
workspace/all-target Clippy -D warnings, fmt and release PASS. CLI10 tests PASS
including literal legacy, streamed/batched modes, invalid patterns, full-file
cross-chunk spans, binary/invalid/oversize rejection, empty EOF,503 empty files
across credited pages, and existing1207/exact1000/wrong-credit page regressions.

Wayland receipt1793531765299825055 and X11 receipt1793531779602361024 PASS:
28 captures/one drained root0/23 logged search helper identities gone or reused,
including search_pages, with no zombie accepted. Regex partial counters, UTF-8
multiline across32KiB, literal contrast, empty/EOF no highlight, visible invalid
reason before helper, terminal paged EOF mode and exact raw Locate pass. The
existing literal/snippet/name/size/date/cancel regression and fixture hashes pass.
These are owned fixtures/internal handlers and framebuffer, not native input or
whole-helper-lifetime/resource evidence. Guided regex controls/preset-specific runtime coverage are covered by the later
GUIDED-REGEX-CONTRACT.md; advanced query access remains functional.

Initial GUI1793531461186329110 FAIL: compile began before the final error-caption
edit, so its binary1cc4a8b8d0611534826931a3e737f3bc9fbcf0c638db5a47734bd00c3ac6423a
retained the multiline diagnostic. Final tests/Clippy/release ran after edits;
the failure is preserved. Intermediate PASS1793531672382874868 predated a harness
fix: F9 starts pages rather than toggling mode. Its paged caption is not proof of
paged empty/EOF results; final receipts explicitly wait for mode/current/terminal
state and check all23 helper identities. No runtime backend fix is attributed to
that fixture correction.

Accepted app SHA256 a09057f8ed129ca38e9cec0b8269438504e21266b2029adf5f7dfd0c1509dd68;
helper ec246445e70aba1558771d43cf1bc9144d34fabbc6d279c8297f79c94f6a0609.
HTML helper remains04aa7953202d26c3602f2dd5ef6df1107a9e652f26357068541e59e0776694cd.


Installed Wayland receipt1793531932464138170 PASS28 captures/one drained root0/
23 logged batch/page helpers gone or reused, no zombie accepted. Installed CLI10
cases PASS including503 empty credited pages and literal regressions. App/helper
hashes match the accepted release; HTML helper unchanged. See install-verification
and cli-installed in evidence/content-regex-integration. No source push or tag.
