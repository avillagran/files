> Current product integration: see [ALLOCATION-UI.md](ALLOCATION-UI.md). Earlier source-only milestones below are historical.

# Allocated folder usage wire, version 1

Status: source helper/core integration, now connected to the typed UI worker and
per-attempt queue; see ALLOCATION-QUEUE-CONTRACT.md. User actions, report
presentation and installation are still pending. The installed application
continues using apparent usage.

## Opt-in request

Run `files-helper measure_directory_allocated`, sending one newline-terminated
JSON request. The command argument and request variant must agree:

```json
{"command":"measure_directory_allocated","id":41,"version":1,"path":"/absolute/private-fixture"}
```

Version is required; only 1 and positive request IDs are accepted. Invalid version
or zero ID produces an error before traversal. Paths use the common bounded
`path_wire` representation: Unicode string or raw byte array, at most4096B and
no NUL. The traversal also rejects relative roots and final symlinks. All existing
held-descriptor, mount, parent-symlink, deadline, depth and entry policies remain
those documented in ALLOCATION-CONTRACT.md. No second scan is performed.

The command emits the existing apparent `usage_progress` frames, starting at
zero. Allocation is never emitted as partial progress. On success, the final
progress has exactly the same apparent bytes and entries as this sole terminal:

```json
{"status":"allocated_usage","id":41,"report":{"version":1,"bytes":7,"entries":2,"allocation":{"by_name":{"state":"bytes","value":8192},"by_identity":{"state":"bytes","value":4096}}}}
```

Both values describe **regular-file blocks** in512B units. `by_name` counts
hardlink aliases repeatedly; `by_identity` counts each observed device/inode
once within this traversal. Neither includes directory/link inode blocks or
proves physical ownership, space freed by deletion or a union across roots.

Unavailability is a typed value, never zero or a partial successful total:

```json
{"state":"unavailable","value":"identity_limit"}
```

Reasons are `overflow`, `identity_limit`, `changed_identity`. The name sum can
only overflow. The unique sum can have any of the three reasons. Empty successful
scopes contain measured zero for both values. Overflow states are independent;
the name sum can overflow while the identity sum remains valid.

Errors use `error` and unsupported platforms use `unsupported`, with the request
ID and diagnostic. No final successful report follows a traversal error. Helper
process failure or cancellation is not a successful final report.

## Shared validating reader

`files_core::folder_usage::{Report, Frame, Ledger, Reader}` define the strict
vocabulary and reader intended for the UI worker. JSON parsing is supplied by
the caller, avoiding a new non-test JSON dependency in core.

- At most4096 payload bytes per newline-delimited frame, enforced before push.
- Arbitrary input chunk boundaries are supported; truncated EOF is rejected.
- A positive expected ID is bound for the entire stream. Legacy `usage` finals,
  other result types, unknown fields/reasons and duplicate JSON fields reject.
- Initial progress must be zero; subsequent apparent counters must be monotonic
  with at most1,000,000 entries. An allocated final requires a preceding progress
  with exactly matching counters, a valid v1 report, and no previous terminal.
- Finite block values must be multiples of512. Unique cannot exceed name sum
  when both are available. Unique overflow cannot coexist with a finite name
  sum. Identity-limit requires more than65,536 entries; conflicting identity
  requires at least two entries. An empty result requires all measured zeros.
- Diagnostics must contain1–512UTF8 bytes. Data after a terminal, parse errors,
  overlong frames, semantic errors or missing final results fail closed. Reader
  and ledger failures are sticky even if a caller tries to continue feeding.

The worker must still require successful process exit, cancellation handling,
pipe drain and reap. A valid terminal alone does not prove helper lifecycle
completion. The typed source worker now uses this reader; the installed UI is unchanged.

## Compatibility and acceptance

`du`, `preview_directory`, `measure_directory`, `Usage` and `UsageProgress`
retain their request/result shapes and apparent per-name semantics. New types
do not introduce optional allocation fields into legacy frames. Unit tests
assert their exact serialization and strict mode rejection.

`scripts/test-helper-allocation.py --output <receipt> --identity-limit` exercises
the actual release helper against owned sparse/hardlink/raw-path/empty fixtures,
bad request/version/command boundaries, unchanged legacy output, and the full
65,536-identity boundary including an alias at the cap and a new identity beyond
it. Filesystem allocation expectations come from each fixture's own metadata,
without assuming that a filesystem allocates a particular number of blocks.

This contract does not establish X11/Wayland behavior, native input, clipboard,
AT-SPI, full application lifetime or resource/startup acceptance. No installation
is authorized by a CLI result alone; complete the UI integration and scoped
functional acceptance first.

Source acceptance on NODE: 1249 workspace tests (442core/105helper/702UI),
3historical ignored, Clippy and recursive core/helper rustfmt PASS. The release
helper passed all7 CLI cases including the full identity-boundary fixture.
Evidence: `docs/evidence/allocation-wire/assessment.json`, `helper-cli.json`,
raw validation logs (including the corrected initial compile error). Source
helper SHA256: `3945c77c0d011e8dd93a3c0917a6397ff693d39e8f12ad38f9b00ac4a6fddbbd`.
All five installed hashes still match the prior marked-report installation.
