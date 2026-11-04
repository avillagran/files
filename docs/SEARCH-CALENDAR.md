# Captured calendar and elapsed-hour presets

Inside recursive search, **calendar / F8** offers **Hoy (1)**, **Ayer (2)** and
**7 días (3)**. Seven days means today and the previous six local calendar dates,
not a rolling168hours. The choices replace only the guided date fields; query,
size and content terms remain AND-combined. They do not run a search automatically.

The reference SystemTime is captured when choosing a preset. One bounded pending
choice/latest epoch and one admitted request use the existing AuxIo worker. No
per-click thread or timezone conversion/filesystem I/O occurs on the graphical
thread or per search result. The worker uses the process's libc local calendar;
UTC boundaries are materialized once and remain fixed when Enter starts/restarts
the scan. Press F8 and choose again to refresh a period after midnight. The fields
explicitly show the resulting UTC values.

The existing `after:YYYY-MM-DD` inclusive and `before:YYYY-MM-DD` exclusive UTC
midnights are unchanged. Both also accept strict `YYYY-MM-DDTHH:MM:SSZ`, without
fractions, leap seconds or other offsets. Mixed date-only/timestamp guided bounds
compare instants, not string lengths. Input/query bounds stay32B per field and
4096B/32terms per composed query. Oversized composition preserves previous fields.
No helper protocol shape or dependency changes: SearchBatches receives the same
query string, metadata remains lazy and follows the existing held-parent-FD rules.
An older helper rejecting the new syntax reports an error, not fake empty results.

Local resolution uses checked proleptic Gregorian dates1–9999 and libc mktime/
localtime_r off-UI. Standard/DST candidates select the earliest valid day start;
the preceding second must belong to a different local date. This handles tested
23/25hour days, midnight folds and forward gaps. A skipped date, ambiguous start,
out-of-range or non-POSIX UTC roundtrip rejects and preserves the prior criteria.
No universal historical-timezone transition/other-OS or atomic timezone-change
snapshot claim. Process timezone is never modified by product code. Tests set TZ
only on private child processes. Non-Unix local conversion reports unsupported.

Closing/editing/reopening or a newer choice revokes the old epoch. Replies release
only their own admitted slot. Preset/clock/wire bounds must agree before dates
apply. A closed chooser never admits its pending request; late results do not
change the new form. Old text-paste receipts are invalidated, and chooser text/
hidden result activation is blocked. Escape/F8/Enter returns to criteria, retaining
them; clicking another input closes the chooser normally.

A timezone lookup can still block the shared I/O worker on the host filesystem;
there is no kernel-I/O deadline, new replacement worker or UI join. Read-only
calendar ACK is separate from search child reap; exit permission does not fabricate
an ACK for a blocked conversion. Existing mutation/config save close rules persist.

## Evidence

Validation summaries, source/binary/log hashes and original failures:
`evidence/search/calendar-validation/checks.json`. Core fixtures cover UTC leap,
New York spring/fall, Santiago midnight gap/25hour yesterday, Havana repeated
midnight, skipped Apia date, negative epochs, strict syntax and forged overflow.
State fixtures cover latest-only admission, stale ID/clock, close/reopen/edit,
composition rollback, equal mixed bounds and no automatic search. Recorder and
real framebuffer validate440² chooser geometry. Native scene extends existing
`recursive-search-ui-check.py --calendar` using private TZ=America/Santiago,
actual AuxIo conversion + files-helper IPC + painted handler/keyboard paths;
no OS input or full clipboard/focus/accessibility acceptance.

PSS/Swap/CPU/startup NOT_MEASURED under freeze. No content DU, index, new media
preview, semantic-model download, filesystem snapshot or mutation authority is
introduced. Full search/size/tools/ROADMAP and global budgets remain open.


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
otherOS/resources/startup proof. Earlier name-only scene1793460239093706643 retained;
syntax FAIL wiring log preserved. Full goal/ROADMAP, freeze and previewsPAUSA open.

Installed source36b8a18 through `evidence/0.0.4/installed-checkpoint-1793461261514927889.json`;5installed+5backups independently SHA256 exact. App7654450b01fadcfbf004461820ecdcb12f678a07085a05d3a7834dd4f2e9134a/helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Source publication/tag/global timezone/config changes not performed.

## Elapsed-hour choices (2026-10-31)

F8 also offers **1 h (4)**, **24 h (5)** and **168 h (6)**. These are elapsed
3600/86400/604800 seconds, independent of local midnight/DST; the original
Hoy/Ayer/seven-local-dates choices remain unchanged. The clock is captured once
on choice and rounded down to a whole Unix second. Bounds are `[reference-span,
reference)`; the current incomplete second and future timestamps are excluded.
Both UTC fields are visible and fixed until explicitly choosing again. Saved
searches store the fixed fields, not a timer that advances behind the user's back.

Same AuxIo request/epoch/close protocol; no helper starts on choice, and existing
query/size/literal-content/hidden criteria remain AND-combined. The existing helper
receives ordinary strict UTC after/before terms, without a new command, metadata
call per result, worker, dependency, index or clock polling. Matching a reply for
an elapsed preset requires the exact duration/end/reference, rejecting forged
self-consistent but shifted UTC boundaries. Conversion remains in the existing
Unix date adapter; non-Unix support is still pending. No resource/startup claim.

Validation:999 Rust335core/85helper/579UI PASS, three existing ignored. Core
fixed/negative/subsecond clocks and forged bounds; existing private TZ cases also
contrast exact elapsed spans with23/25h local calendar days. State keeps size/
name/content criteria without auto-search. Six choices fit440²/800×500/1000×650
without overlapping each other or the footer. Final Clippy all-targets-Dwarnings,
release and fmt PASS after label-only precision. The original three-choice assert
FAIL remains with sidecar; no disabled/serialized tests.

`evidence/search/elapsed-validation/checks.json` records scoped real AuxIo/helper
scene `search-calendar-ui-1793477561276508002`:23 real framebuffers, all three
elapsed spans, key4/key6 and painted24h, raw filename, inclusive start/exclusive
end with ±1ns, future timestamps excluded, size AND literal content retained;
six metadata attempts per six candidate files. No automatic helper on choice.
Root0 after callback/drain ACK; logged PID/start identities absent/reused with
strict Z rejection, not lifetime coverage. First script TypeError (loop variable
shadowing command function) preserved with sidecar; corrected variable only.
No native input/OS clipboard/kernel-I/O/other-platform or resource/startup proof.

Source `a26865b` installed with five artifacts and five independently rehashed backups matching `evidence/0.0.4/installed-checkpoint-1793477612519723520.json`; app4310c38d4bcc/helper59187351e32c/HTML85ea1b8009ef. Authorized per-user installation, no source push/tag/global configuration.
