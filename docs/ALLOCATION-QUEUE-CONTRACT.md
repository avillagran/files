> Current product integration: see [ALLOCATION-UI.md](ALLOCATION-UI.md). Earlier source-only milestones below are historical.

# Allocation worker and queue integration

Status: source integration, no new user action or installation yet. Report and
manager presentation still need integration and functional acceptance.

## Typed worker result

`folder_usage::Mode` selects apparent or allocated measurement. A
`Measurement` contains apparent bytes/entries and optional allocation:

- `None` means an apparent-only request, never an allocated total of zero.
- `Some(Observation)` means a completed allocated request, including any typed
  `Unavailable` values. These do not discard a valid apparent total.
- Report conversion validates version and metrics before removing the wire
  envelope. A future-version report cannot become a valid in-process result.
- Validation binds allocation presence to the requested mode and checks the
  existing million-entry limit. The queue validates again before publication.

`du_job::Job<T>` retains `(u64,u64)` as its default type for directory previews.
The model's tracked size job uses `Job<Measurement>`. Its request builder sends
the legacy command for apparent mode and opt-in v1 command for allocated mode;
there is no fallback between modes.

The allocated branch uses the shared strict `folder_usage::Reader` with JSON
parsing. Only apparent progress counters are published to the progress mutex.
Both branches use `space_map_job::run_helper_stream`:64KiB request/4MiB total
response limits,20-second worker deadline, cancellation checks per read, pipe
drain, process exit and reap. The terminal alone never completes a live child.
The result channel is filled only after the transport succeeds and decoder
finishes. A nonzero exit invalidates an otherwise valid final frame.

## Per-attempt queue state

Each record stores its requested mode and optional completed allocation.
Admission `enqueue_mode` copies the requested mode to every selected record;
the existing `enqueue` chooses apparent mode. Both use the same whole-batch
preflight and128-row/64KiB-name/path budgets. Failure before admission preserves
IDs, modes, observations, pause, active slot and other records.

Admission replaces a selected record with a fresh attempt ID and clears its old
allocation. Unselected observations remain intact. Dispatch returns a path and
mode and captures the mode in the active attempt. Retry/recalculate retain the
intended mode, allocate new IDs and clear old allocation. Cancellation clears
pending allocation and marks the old active attempt invalid; its slot remains
occupied until the old helper's ACK releases it. Replacing the mode while a
helper is active cannot admit another helper before that ACK.

Completion requires the active epoch/attempt/mode to match. A stale result only
releases its active slot. A mode mismatch or invalid metric produces a failed
record with no measured sort key and no cached allocation. A valid allocated
result retains `Some`, including unavailable metrics, while its apparent total
continues to drive existing list sorting. Captured-report IDs and scoped
forgetting continue to use attempt IDs; no result text becomes an operation
target or proof of freeable space.

## Remaining product work

Add explicit actions for measuring blocks of focused and marked directories,
show requested mode and completed regular-file block observations in the size
manager and captured reports, and explain that sums across roots can repeat
shared identities. Preserve keyboard/mouse access,440px layout, immutable report
capture, no automatic helper from report browsing, and scoped forgetting.
Then run source/installed X11/Wayland functional acceptance before installation.

No resource/startup experiments, native input acceptance, physical ownership,
space-freeing estimate, filesystem snapshot or cross-root union is established
by these worker/queue tests.

Source acceptance on NODE:1256 workspace tests (443core/105helper/708UI),
3historical ignored; Clippy, recursive rustfmt of all four crate roots and
release workspace PASS. Seven actual helper CLI cases PASS, including the full
identity boundary. Existing apparent/marked report regression PASS in Wayland
(`1793538269598837581`) and X11 (`1793538282445360863`):15framebuffers and
19logged helper identities gone/reused per run, normal root0+drain. This is
legacy-flow regression, not allocated-mode UI acceptance.

Evidence: `docs/evidence/allocation-queue/assessment.json` and raw logs/receipts.
The initial workspace attempt repeated the historical folder-view WouldBlock
failure; it is preserved with cause unconfirmed, and no folder-view fix claimed.
All five installed hashes remain unchanged. New user controls and allocation
report presentation remain the next product work; do not install this phase.
