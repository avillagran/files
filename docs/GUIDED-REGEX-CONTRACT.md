# Guided literal and regex content search

Status: installed sourcef2b22e2. Record1793532994109325152 verifies five
installed files and five backup hashes; advanced regex backend remains unchanged.

F6 selects the separate content editor; its Literal/Regex button is visible beside
the scope controls. Ctrl+F6 switches interpretation and focuses that editor.
The text, name query, size/date criteria, root and hidden policy are retained.
Switching is explicit, revokes old results/marks/page controls, cancels the old
intent through the existing ACK path and invalidates stale text-paste receipts.
It does not start a helper. Enter or Buscar is still required.

The editor holds the raw literal or regex pattern. Query composition escapes it
exactly once using the existing string contract, with content: or regex: selected
by the explicit mode. A visible backslash is never decoded into a path. Focusing
the name query does not silently change the content's interpretation; a retained
pattern keeps its mode. Reopening a fresh search resets to literal. Advanced
regex: clauses in the main query remain supported. Two content predicates reject
rather than silently rewriting an advanced query or dropping a criterion.

The mode button fits the existing scope-control row, including440×440 and page
navigation. The existing content input/pointer/text-history implementation stays
shared. This does not claim physical keyboard/mouse, clipboard MIME, IME,
AT-SPI or2× coverage.

## Saved criteria compatibility

Criteria stores content_regex independently of content-editor focus. Equality,
deduplication and current-criteria identity therefore distinguish literal and
regex using the same text. Save captures this state; Load restores it without
starting a search. Saved review labels the content as Regex or Texto literal.

FILESSCH v1 flags stay hidden(1)/content-focus(2), with implicit literal content.
V2 adds regex(4) in the same bounded record shape. V1 with the regex bit rejects;
v2 reads both kinds. Unknown versions/bits still reject and preserve the file.
The encoder emits v1 for literal-only documents, and v2 only when at least one
row requires regex. Removing all regex criteria allows a literal-only v1 output.
No quota expansion, implicit rewrite on read or automatic search on load.
Older app versions cannot read v2; their existing incompatible-file behavior
preserves the file. Changes require an explicit new regex Save.

## Validation

Core codec tests cover v1/v2 mixed roundtrip, mode-distinct identity, invalid v1
regex flags and return to literal-only v1. State test verifies raw pattern and
quoted/backslash composition, explicit mode switch, stale receipt epoch,
criterion capture/restore and no automatic helper. Workspace1230PASS
435core/98helper/697UI+3 historical ignored, Clippy all-targets/fmt PASS.
Initial compile hit the JSON fixture macro recursion limit; the fixture metadata
was split into smaller inserts, not a crate-wide recursion-limit change. Log kept
in evidence/guided-regex/validation. Final release/runtime receipts follow there.


## Functional acceptance on NODE

Wayland1793532804833634378 and X111793532832682703956 PASS37 captures each,
one drained root0 and27 logged batch/page helpers gone or identity reused, no
zombie accepted. The same app1163b1f94bfe24700883183a0eead43ca189f417c642b04bd9fb6129baf832af
and helper ec246445e70aba1558771d43cf1bc9144d34fabbc6d279c8297f79c94f6a0609
were exercised. Helper behavior/HTML helper are unchanged from content regex.

The fixture seeds a private v1 literal catalog, switches with the painted selector
and Ctrl+F6, rejects a stale private paste, checks unchanged raw text/no helper on
switch, and verifies selector/peer rectangles do not overlap at440, including
pages. Real AuxIo Save produces v2 with both rows; Reload/Load restores old literal
and new regex independently, without a search. Explicit Enter yields their
separate expected results. Mode/captured criteria labels were visually inspected.
The existing advanced regex/literal/raw Locate/partial-limit regression passes,
and all original owned source hashes remain unchanged.

Initial functional1793532704370210150 FAIL tested hit geometry after an async
result but before its repaint. Its captured frame already shows the selector.
The harness now waits for the painted search header as well as the finished
report, retaining the original failure. No product rendering fix is attributed
to this fixture correction. Evidence is in guided-regex/validation and search.
Guided controls and preset-specific runtime acceptance supersede those previous
pending items in CONTENT-REGEX-CONTRACT.md. Resource/native/global gates stay open.


Installed Wayland1793533005797097317 PASS37 captures/one drained root0/27logged
batch+page helpers gone or identity reused, no zombie accepted. Same accepted
app/helper hashes and actual v1/v2 Save/Reload/Load behavior. See guided-regex/
install-verification.json and the installed receipt. No source push/tag/global PASS.
