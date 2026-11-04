# Retained-walk search pagination

Status: core/helper and UI implemented with scoped functional evidence. Default
incremental search remains available; **F9** explicitly starts a paged walk.
Resources/startup NOT_MEASURED; full ROADMAP/global gates and new-preview pause
remain open. Local installation is recorded below after verification.

## Engine and transport implemented

`files-helper search_pages` accepts `Request::SearchPages{id,path,query,hidden}`.
Version1 page lines contain id, zero-based sequence, cumulative found_total,
more, and one ordinary bounded search Report. Page paths are observations,
never mutation authority. The typed core Ledger binds one request ID, expected
metadata/content modes, consecutive sequence, exact cumulative hit count and
non-regressing visited/error/mount/metadata/content counters. Invalid frames leave
its previous state intact; no frame is accepted after terminal.

One FD-anchored depth-first walk remains in the same child while waiting for the
consumer. Each window retains at most500 hits/256KiB path capacities; it may fill
earlier to leave room for the next4096-byte path. It serializes the borrowed
Report directly, then waits for a credit line with exact `{version:1,id,sequence}`.
Credit is capped256B before JSON decoding, rejects unknown fields/old identity,
and does not prefetch another line. EOF/bad credit aborts the child. The initial
request rejects pipelined bytes observed after its newline. Credits are a
read-only flow-control contract, not a native gesture/authorization proof.

On credit the old hits are dropped and their Vec capacity reused. The walk does
not restart or collect all hits. FD frames retain the existing depth64 bound
(one File and one ReadDir per frame); root proof and ordinary pipe/runtime FDs
are additional. A current matching directory is still descended after a page
boundary. Root dev/inode/mount ID is revalidated before publishing a page and
after waiting, without following a final root symlink. Replacing the root while
waiting fails before another page. This is not a whole-tree snapshot, global
change watcher, mount-hostility proof or inode-bound authorization to mutate.
Readdir under concurrent changes can omit/repeat paths; consumers must present
that limitation and capture fresh operation proofs as existing operations do.

Visits250000, content64MiB and active elapsed10s are **session-wide**, not renewed
per page. Depth/mount/path/content rules stay unchanged. Time spent in the page
callback (serialization/pipe/credit wait) is excluded from the active elapsed
budget; syscall/kernel I/O still has no hard deadline. Waiting can retain the
bounded helper and FDs indefinitely until credit/EOF/cancellation. There is no
replacement worker, timeout-induced ACK or UI join.

`more=true` means a page boundary with the end still **unproven**. It never claims
completion. A full last page needs one more credit to discover EOF: the terminal
page may be empty. A consumer must keep the preceding nonempty page visible and
mark end rather than erase its results in that case. Terminal more=false can
also carry an explicitly partial Report because of visit/time/content/errors.
Ordering is by raw relative path inside each discovery window, not a global sort
across the whole tree. No index, repeated scan or persistent result cache added.

## User interface and bounded history

**F9 / Nerd page button** validates current captured criteria and starts a new
paged walk. Enter repeats that mode in this drawer; opening a fresh drawer uses
the ordinary incremental/batch search. A normal search reaching500 now points to
F9. **Ctrl+PgUp/PgDown** or the two painted arrows navigate previous retained /
next pages. F9 always restarts from the first page of a new observation, not a
promise of replaying an unchanged filesystem.

The process-local history retains at most eight pages and2MiB of accounted
Report/path/snippet/deque storage, including the current Report. An incoming
512KiB page and metadata reservation evicts oldest history before sending credit;
the decoder's2MiB scratch, one incoming slot and child memory are separate bounds,
not a total-process PSS guarantee. Reports are moved, never deep-cloned per visit.
The footer names the actual retained range; Previous stops at its oldest page.
There is no persistent result cache or arbitrary-access index. Sorting is local
to each discovery window; global sorting across all pages remains pending.

Marks are explicitly **page-local** (button: Marcar página) and clear on switching
windows. Copy/Move uses the existing captured-origin/review contract for the
visible selection, never hidden marks from another window. Cached positions and
scroll restore on return. A new page arriving while reading an older page does
not steal that view. Async arrival preserves an editor's focus and paste epoch;
explicit page navigation focuses results. Old result-epoch controls cannot send
credit to a different page. Saved searches retain criteria, not page position or
history, and storage ACK still does not execute a search.

**Parar** cancels future scanning/credits and retains already accepted valid
pages/counters for reading and cached navigation. It remains distinct from an
internal scope reset. A late Stop on a confirmed finished search is a no-op.
Protocol-invalid observations are removed before cancellation can change the
revision, so Stop cannot resurrect them. Editing/restarting/closing invalidates
history through the existing generation rules. A stopped task is still active
until its owned child has been killed/reaped; no false ACK or UI join.

One reader worker uses the existing owned helper supervisor. UI sends at most one
credit token through a bounded channel; only the worker writes the nonblocking
pipe. Responses are bounded4MiB per admitted turn and2MiB per JSON page, with
one pending-page slot. Terminal pages publish only after successful process exit
and reap; earlier full windows are observations, not completion. Decode failure
invalidates targets before reap. Controlled stderr retains at most8KiB and UI
errors512B, using the same worker. Waiting for the consumer pauses the supervisor's
15s elapsed accounting; no blocked-kernel deadline or instantaneous cleanup claim.
The default incremental/batch mode keeps its provisional rows and progress;
paged mode currently publishes whole windows rather than intra-page candidates.

A valid500-hit page of opaque513-byte paths exposed receiver Vec spare capacity
inflating the budget after decoding. A preserved red test reproduces rejection.
Bounded path payloads now retain exact capacity via boxed byte/string storage;
bytes, Unicode and path interpretation are unchanged. No URL/path redecoding,
new dependency, scanner or per-result metadata/DU work is introduced.

## UI delivery evidence

`evidence/search-order/pages-validation/checks.json` records1023 Rust
(341core/91helper/591UI) PASS, three existing ignored; final Clippy workspace
all-targets-Dwarnings, release and fmt PASS. Reader/credit/phase tests cover
fragmentation, unsolicited frames, missing terminal, bounded stderr/control,
consumer idle/cancellation/reap, byte/row eviction, cached position, late replies,
editor focus, terminal empty, valid/invalid Stop and late Stop on finished state.

Final `search-pages-ui-1793482591192967537`: six owned Apps,18 real framebuffer
captures, normal exit0 after callback/drain ACK; eight logged PID/start helpers
absent/reused with strict Z rejection, not full lifetime coverage. Actual helper:
1207=500/500/207 raw union, cached navigation without extra credit, current-row
copy review/cancel, stale epoch rejection,1000→empty terminal retaining500,
4507 history eviction,500 opaque513B paths and byte-exact Locate,602 literal hits
as500/102 with line2/byte7/snippet,440² counters/controls, observed live helper
before Stop and before callback close. Stop preserves a500-hit window and no new
helper until explicit restart. No native input/clipboard delivery/HiDPI/global
ordering/snapshot/resources or universal offline/filesystem proof is claimed.

Seven actual release CLI pipe cases remain PASS with the **same helper SHA** as
this final scene (`evidence/search/pages-release-contract.json`), including legacy
regressions, exact credit, raw paths and EOF/error termination. Earlier failures
and narrower positive scenes remain intact with sidecars; see pages-validation.

Remaining: intra-page progress for this mode, arbitrary-access/durable history,
global ordering/index, native input/accessibility/other OS and resource gates.

## Foundation evidence

`docs/evidence/search/pages-foundation-validation/checks.json` and
`docs/evidence/search/pages-helper-foundation.json` contain exact scope.
1007 Rust338core/90helper/579UI PASS (+3 existing ignored); final Clippy workspace
all-targets-Dwarnings and fmt PASS, dev helper build PASS. Tests cover1207 raw hits
as500/500/207, directory boundary descent, root replacement during wait, shared
visit/content budgets, cancellation callback, wait excluded from active deadline,
transactional sequence/mode/counter guards and bounded exact credit decoding.

Seven actual CLI pipe tests on the **dev helper**, including legacy regressions:
1207 exact-once raw hits, no next frame observed before explicit credit, exact1000
as500/500/empty terminal, wrong request/sequence credit and EOF abort with no next
page. The pipe observation is functional flow-control evidence, not latency/PSS/
CPU/startup proof. The initial unnecessary_unwrap Clippy FAIL is retained; changed
the checked Option branch to if-let without disabling a lint. No UI/native input,
OS clipboard, resource acceptance, release/install or global PASS is claimed.

Additional targeted boundary test PASS:80 paths under14×240-byte directory components fill the256KiB window before500 hits; all80 delivered, every page below path capacity. Test-only addition after full1007 suite; helper all-targets Clippy-Dwarnings PASS again. Logs path-budget-test/clippy in foundation-validation.

UI source `647527c` installed with five artifacts and five independently rehashed backups matching `evidence/0.0.4/installed-checkpoint-1793482998895302861.json`; app e72bb4cc2e14 / helper05736746b394 / HTML85ea1b8009ef. Per-user installation only; no source push/tag/global configuration or other windows controlled.
