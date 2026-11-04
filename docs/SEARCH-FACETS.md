# Search size and modification filters

Ctrl+Shift+F searches recursively in the captured directory. Combine terms with
spaces (AND), for example:

- `ext:pdf size:>=10MiB after:2026-01-01`
- `size:1MB..50MB before:2027-01-01 !draft`
- `type:file !size:<1KiB`

`size:` accepts exact bytes, `=`, `<`, `<=`, `>`, `>=`, inclusive `a..b`,
`..b` and `a..`. Units B/KB/MB/GB/TB use 1000; KiB/MiB/GiB/TiB use 1024,
case insensitive. Up to three decimal places are accepted only when the result
is an integer number of bytes. Invalid units, overflow and reversed ranges are
errors before helper admission.

Size means **apparent regular-file bytes**. A size term, including a negated
one, excludes directories, symlinks and special files. It never runs recursive
DU per search entry. Folder content sizes remain the separate Ctrl+Space /
Ctrl+Shift+Space calculator.

`after:YYYY-MM-DD` includes midnight on that date; `before:YYYY-MM-DD` excludes
midnight on that date. These are **UTC modification-time boundaries**, not local
calendar dates or creation times. Dates are validated, including leap years.
Date-only filters can observe directory or symlink metadata; links are never
followed to inspect their targets. Strict UTC timestamps `YYYY-MM-DDTHH:MM:SSZ`
are also accepted, without fractional seconds/leap seconds/other offsets. F8 now
provides captured local-calendar Today/Yesterday/today-and-previous-six-days presets;
see SEARCH-CALENDAR.md. They populate fixed UTC bounds, preserving this syntax's
UTC meaning. F8 also captures elapsed1/24/168hour windows ending at the last complete second; see SEARCH-CALENDAR. Arbitrary natural-language/rolling-age query expressions remain pending.

The parser is borrowed and bounded to 4096 query bytes / 32 terms. Ordinary
name/ext/type search adds no metadata calls. Metadata queries first test cheap
name/type predicates, then read no-follow metadata only for candidate entries
through the held parent FD path. Errors and type replacement produce partial
results, not fabricated zero sizes or dates. The report exposes metadata-read
attempts. A metadata query requires a compatible helper response even when empty.
Results are observations of a changing tree, never filesystem snapshots or
mutation authorizations. Existing mount/depth/visit/time/result/path budgets,
cancel/reap and latest-intent rules remain in force.

The local Ctrl+F filter rejects these reserved facets with a hint to open search;
`name:size:...` and quoted names still match filenames rather than metadata.
No file content reads, index, persistent cache, extra thread or dependency is
introduced. No resource/startup measurements are claimed while the freeze holds.

## Visual criteria

In search, the Nerd `Filtros` button or F4 expands four fields: minimum and maximum
apparent regular-file size (inclusive), modified since (UTC, inclusive), and
modified before (UTC, exclusive). For sizes enter `10MiB` or `1.5MB`; dates use
`YYYY-MM-DD`. Empty fields add no restriction. Tab / Shift+Tab cycle the four
fields and the name/query field. Ctrl+F returns to the query; Ctrl+A replaces the
focused input. Backspace edits its trailing Unicode scalar. F4 or the first Esc
collapses the drawer; a second Esc closes search. Enter / Buscar validates, queues
one search, collapses the form and keeps all criteria. The button count indicates
retained visual criteria; reopen it to inspect/change them.

Visual criteria AND together with all advanced terms, including negated terms.
They are compiled *before* the advanced query, so an unfinished quoted name
cannot absorb generated predicates. Quitar removes only the four visual criteria,
leaving the advanced query and local hidden scope unchanged. Opening a fresh
search resets the visual criteria. No global preferences are changed.

Each visual value has a 32-byte preflight; the total compiled query still has a
4096-byte / 32-term limit before helper admission. Invalid dates, invalid units,
minimum above maximum, or since on/after before are visible errors; draft inputs
are retained. A large paste is rejected before copying or scanning its contents.
Editing a criterion cancels stale results through the existing generation/ACK
flow. Merely changing input focus does not discard an in-flight result. A separate
input epoch rejects a delayed text-paste callback after focus or selection changes.
This protects app routing; it does not prove OS clipboard interoperability.

Four columns at >=800px and two below keep fields accessible in small windows.
The drawer collapses on successful submission to restore result space. Full
caret/selection editing, IME/preedit, units/date pickers, relative/local dates,
accessibility and native keyboard/mouse validation remain pending. Existing file
metadata/helper/mount budgets are reused; no timer, thread or dependency added.


### 2026-10-31 — Captured local search calendar

F8/Nerd calendar: Hoy/Ayer/hoy y seis días previos, process-local calendar resolved
once on existing AuxIo with reference captured on choice. Dates populate fixed
UTC bounds and require explicit Search. Date-only UTC syntax unchanged; strict
YYYY-MM-DDTHH:MM:SSZ extension, mixed bounds compare instants. Latest/epoch/close/
reopen/edit guards; readonly ACK separate from helper reap, no per-click thread.
Content editor preserved after ACK, stale paste and overflow reject without
changing criteria. No per-result DU or added metadata calls beyond existing lazy
metadata predicates. See SEARCH-CALENDAR.md for DST/error and platform scope.

937Rust324core/85helper/528UI(+3ignored), Clippyworkspace/alltargets-Dwarnings,release/
fmt PASS. Eight private TZ process cases: leap,23/25h, Santiago midnight gap/
25h yesterday, Havana fold, Apia skipped date,7dates/DST. UI
search-calendar-ui1793460920785323202 PASS26framebuffers incl440² and1000×560,
actual AuxIo+files-helper: inclusive start/exclusive end+subsecond/raw name,
metadata_reads7 for seven candidates, no automatic helper on preset, yesterday/
seven, dates+content/raw filename/editor, equal mixed bounds reject and existing
UTC controls regression. Root0 after callback/ACK,16loggedPID/start helpers gone/
reused strictZfalse, not lifetimes/helpervivoatclose/kernelD/OSinput/clipboard/
otherOS/resources/startup proof. Earlier name-only scene239093706643 retained;
syntax FAIL wiring log preserved. Full goal/ROADMAP, freeze and previewsPAUSA open.
