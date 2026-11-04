# Copy and Move from marked search results

In the results area, **F5 / Copiar** captures the chosen roots and opens the
existing destination review. **Shift+F6 / Mover** does the same for Move; F6 still
switches the content-search editor. The footer provides Nerd mouse buttons at
440px. Criteria editors keep their own shortcuts. No explicit marks uses the
focused result; explicit empty/pruned marks never silently substitute focus.

## Observation -> captured proof -> review -> queue

Search paths are observations, not permission to modify an inode. Borrowed
multi_origin planning rejects empty/over-limit/ambiguous inputs as a whole.
Capture uses the existing **AuxIo** worker, not filesystem opens/stat on the UI
and not a worker per gesture. It pins each parent/root and records the existing
root/topology identity. Response admission checks owner(panel,tab), browser
generation, search root/edit intent and the same raw marked paths. A changed or
closed intent cancels/discards the response; no old response opens a new review.
Current explicit marks can survive an appended/final search batch by raw path.

Only the captured proof reaches the dialog. The search remains underneath;
cancelling the destination retains the search/marks. The dialog owns rendering,
keyboard and paste precedence. Opening it invalidates pending search-text paste;
a destination paste cannot later write into the search on cancellation.

Changing destination or target basenames does not recapture origins. Copy supports
the ordinary name plan, conflict policy and explicit publication/symlink-time
options. All source/destination/name lengths, global target-name uniqueness,
request count and aggregate path bytes are checked before building requests.
Raw Unix names and captured opaque destinations stay PathBuf/OsString; display
escapes are never reparsed. The ordinary queue/worker owns each confirmed job,
with pause/cancel/ACK and captured paths independent of later tabs or search.
This neither consumes the private files clipboard nor claims OS MIME ownership.

## Effective checks and partial outcomes

Copy first verifies every captured group, after its pause checkpoint, then
compares each **actual engine-opened parent/root FD** to the original group proof.
It does not re-capture a replacement at the same path. If a later root changes
after initial verification, previously completed roots remain reported, while
the wrong root is rejected before publication. Standard bounded cleanup/rescue
reporting and source-preserving copy behavior remain. These checks are root
binding/metadata evidence, not a frozen snapshot of all descendant content.

A single-parent Move reuses the existing verified native/EXDEV adapter with a
verification-only lease; it is never published or ACKed through the clipboard.
The existing EXDEV allowance remains **one regular file, nlink1**, with journal
and recovery. Multi-parent Move uses the existing prepared native batch/ledger:
all group tokens are reviewed before confirm, then executed sequentially with
confirmed/pending/uncertain root states. No global rollback and no permission to
delete from CopyReport.completed. Cross-filesystem batches/trees/hardlinks remain
pending and reject explicitly rather than copy/delete implicitly.

The complete move report merges each actual engine report: effective FD-observed
source/destination parents and confirmed destination paths, including a failed
group. Requested paths remain explicitly labelled in the ledger; each root state
appears first so a long path cannot hide CONFIRMED/PENDING/UNCERTAIN. Batch failure
retains this aggregate report, token, ledger, requested paths and cause
for the bounded result panel. Explicit pending-ledger retry is described below; durable multi-batch
recovery remains separate pending work; an uncertain publication is never replayed
automatically. Existing tabs relist on activation and active/other-pane refresh
uses captured source/destination/affected parents, not current file selection.

## Bounds, cancellation and liveness

Up to128 roots/32parents,4096B per path; core plan also rejects duplicate target
basenames, lexical overlap, physical aliases and ambiguous mount/topology cases.
Those limits are visible errors, never partial import. A dynamic borrowed capture
reservation uses the **same core estimator** as the actual capture, plus retained
input capacities/relative-path snapshot/slack. The queue's4MiB accounting and2MiB
report reserve are unchanged; source capture/review checks available metadata and
its own bounded scope. These are local accounting bounds, not app/PSS guarantees.

Capture and pending review FD reservations remain in the common256-FD scheduler
admission even after cancelling/hiding the dialog until its exact reply. Pending
review includes source proof and engine/scratch reservations; an admitted batch
shares the same proof by Arc. A rejected queue admission preserves its prepared
review token. Close cancels, admits no future capture and waits for sent capture/
review ACK. No UI join or replacement worker around stuck I/O; kernel-D/offline
completion/instant close is **not** guaranteed. Reply-channel loss is a failure,
clears only the readonly capture/review slot and logs LOST; it is not an ACK.

No new preview, decoder, thread engine, dependency or persistent media cache.
Copy staging and EXDEV journals are the existing operation machinery. Resource/
startup experiments remain frozen and native input/clipboard/lifetimes/otherOS/
full ROADMAP remain separate open gates.

## Search-result Copy/Move — 2026-10-31

F5/Copiar and Shift+F6/Mover from marked search results now capture up to128
roots/32parents through AuxIo, validate effective FD proofs and open the existing
review/queue. Cancel retains marks; stale intent/changed root cannot redirect the
job. Global queue/FD/metadata accounting remains shared. Native multi-parent Move
reports effective observed locations and confirmed/pending/uncertain roots;
EXDEV remains one regular nlink1 only, not batch/tree/hardlink transfer.
964Rust327/85/552 PASS+3existingignored, Clippyalltargets-Dwarnings/release/fmtPASS.
Final scene1793470188336000116 scopedPASS27realFB,5loggedhelpersgone strictZno,
root0ACK; cancellation/paused/replacement/partial conflict/raw/copy5/move5 and
single EXDEV actual bytes/inodes verified. See SEARCH-TRANSFERS.md and
evidence/search-order/transfer-validation/checks.json. Original FAILs retained.
No inputSO/clipboard/lifetime/performance proof. Resource/startup NOT_MEASURED
freeze and newpreviewPAUSA intact; full ROADMAP/global gates remain open.

## Pending-only retry

The incomplete batch notice offers **Reintentar pendientes / Ctrl+R** only for a retained
ledger with at least one Pending, no Uncertain root and no unresolved rescue or
cleanup diagnostics. This pure UI check grants no filesystem authority: the sole
operation worker revalidates the original pending proof and destination tokens.
Confirmed roots are skipped even if someone creates another file at their old
source path. A changed pending origin is rejected, never recaptured.

The original destination/selection review remains binding; retry does not allow
editing paths or silently selecting fresh search results. Each explicit attempt
gets a fresh increasing operation ID. Report counters describe this execution,
not historical totals; failed attempts retain the complete per-root ledger.

Queue re-admission atomically transfers the retained batch FD/metadata reservation
to the new request. A full queue, close or budget rejection restores the previous
reservation and complete outcome with original tokens/report/diagnostics. No pump
or worker dispatch runs inside this transition. Accepted retry consumes the old
result; it is not a durable history. Copy its report first if needed. Closing and
explicitly discarding/cancelling pending jobs keep their ordinary semantics.
Multi-batch durable recovery and uncertain-result resolution remain pending.

Validation2026-10-31:970Rust327/85/558+3existingignored PASS; final suite before
only title text/assertion, targeted title test PASS. Clippyalltargets-Dwarnings,
release/fmtPASS. Final scene1793471063548051155 scopedPASS29FB incl440: occupied
target retry keeps Confirmed inode/occupant; explicit private occupant removal
then Ctrl+R moves only Pending,1root in this attempt. Success report title Move
verified; root0ACK/6loggedPIDstartgone/reused strictZno, not lifetime coverage.
Unit tests cover pending substitution/no recapture, queue-full ownership restore,
bounded repeated diagnostic, cancel-before-execution ACK and unrelated notice.
First unknown-command FAIL and first PASS's inherited wrong copy title preserved
with sidecars. See evidence/search-order/retry-validation/checks.json.
No inputSO/clipboard/metric/global acceptance. Durable/uncertain recovery pending.
