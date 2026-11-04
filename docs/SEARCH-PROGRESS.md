# Observable recursive search

Current GUI uses `search_batches` and provisional rows; see SEARCH-BATCHES.md.
The counters-only behavior below documents the retained legacy SearchStream
command/checkpoint, not the current GUI behavior.

## Legacy counters-only mode

Search now shows provisional counters while the read-only traversal runs: visited
entries, possible matches and errors. There is no percentage or estimated finish
time because the total tree size is unknown. Visited includes examined entries,
including ones later excluded by hidden/name/type rules. Metadata remains lazy.

Only the counters are streamed. No paths or clickable rows are published before
the normal final report, root FD/path/mount revalidation and helper exit/reap.
Errors still mean incomplete/failed work, never an invented zero or “not found”.
Open/Locate are dimmed without hit targets until a valid result exists. Stop is
dimmed when no work remains. Enter while the current intent is in progress waits;
the explicit Restart button can restart it. Enter after a completed empty query
refreshes it. Enter in the criteria drawer still submits that form.

## Protocol and retention

The existing `search` CLI/Request retains its one-summary wire. New
`search_stream` / Request::SearchStream emits NDJSON SearchProgress and a final
Search/Error. Each process handles one request. Initial counters are zero; the
first positive checkpoint is at128 visits, further checkpoints are requested at
100ms intervals on128-visit boundaries, and a final counter snapshot precedes
root revalidation. Blocking filesystem I/O can delay any checkpoint. Counts must
be monotonic, within scan budgets, and agree exactly with the final report.
Progress is provisional even if it already counts matching names.

The existing supervisor retains nonblocking pipes, owned-child PDEATHSIG on Linux,
15s external deadline/cancel checks,4MiB total output and background kill/reap.
The scanner still uses its10s cooperative traversal budget,250k visits,64depth,
500results/256KiB path capacity and same-mount/no-follow policies. These are not
instant kernel-D/offline guarantees; the GUI does not join the reader thread.
No resource/startup gate was measured or relaxed.

The incremental decoder caps each line at2MiB and256 messages, checks request ID,
rejects regressions, malformed/truncated/duplicate/trailing/summary-free streams
and mismatched metadata mode or summary counters. Vector growth is explicitly
capped; the terminal raw line is released immediately after decoding. Final
path deserialization keeps the existing count/path budgets. An Error may arrive
before initial progress; its retained message is clipped to512UTF8 bytes.

There is one Arc<Mutex<Option<Progress>>> latest slot (fiveu64 counters), not a
queue of updates, and one terminal result channel. UI reads use try_lock. Cancel,
new criteria, navigation or closing clear the displayed snapshot; old revisions
cannot restore it. A replacement still waits for the prior helper's reap ACK,
and does not spawn on the same pump that consumes that ACK. No second helper,
extra worker/timer/dependency, persistent index/cache or early result clone is
introduced. These are component bounds, not measured process-tree memory savings.

## Evidence and remaining scope

See VALIDATION.md and evidence/search/search-progress-validation-notes.json.
A40k-file private fixture observed positive progress before a final report and
captured the live UI. Cancellation/next intent and final raw-path/filter/criteria
regressions use actual files-helper IPC and FLTK framebuffer. Private keyboard
policy logs distinguish an active Enter from Enter after completion; they do not
prove native input. CLI contract tests cover both wire versions and mismatches.

Incremental result batches, pagination/index/content search, full native-input
and cross-platform/offline acceptance, and overall performance/resource goals
remain open. New media expansion and resource/startup measurement freeze remain.
