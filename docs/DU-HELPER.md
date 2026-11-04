# Recursive apparent-size helper

`files-helper du` accepts one JSON request on stdin and exits after one response:

```json
{"command":"du","id":1,"path":"/absolute/directory"}
```

Success retains the protocol's `usage` response with `id`, `bytes`, and `entries`.
Traversal failure returns `status: error`, the request ID, and an error message;
it never returns a partial total as success. Malformed/oversized input exits 2.
This single-request lifecycle replaces the earlier unsupported persistent stub;
the inspector now uses this lifecycle. Treemap integration is pending.

`bytes` means **apparent file length per directory entry**, not allocated disk
blocks. Hard-link aliases count separately. Directory/symlink/special-file sizes
are excluded; descendant entries of every type count in `entries`. A non-directory
root counts as one entry. Symlinks are not followed, including a symlink root.
Concurrent changes can produce errors; the result is not a filesystem snapshot.

Linux pins each traversed directory with an open descriptor and opens child
folders with `O_NOFOLLOW`. Enumerating via `/proc/self/fd` avoids following a
replaced parent name. Procfs is required. Other platforms currently return an
explicit Unsupported error; their adapters remain pending.

Limits: 1 million entries, 128 directory levels, 15 seconds checked between
filesystem calls, 64 KiB request, 30 seconds waiting for the request, and 96 MiB
virtual-address-space limit on Linux. Traversal stores only the active directory
stack, not all discovered paths. No file contents are read. Blocking filesystem
syscalls can outlast the cooperative deadline; the future UI supervisor must
terminate the child on cancellation/timeout, including when output is blocked.
The virtual-memory limit is not evidence of measured total PSS or the app's
150 MB ceiling. Resource integration measurements remain pending.

Validation: `cargo test -p files-helper`,
`cargo clippy -p files-helper --all-targets -- -D warnings`,
`cargo build -p files-helper`, then `python3 scripts/helper-check.py`.
The script covers complete input, input spanning multiple reads, EOF termination,
structured errors, oversized input, and exit while the parent keeps stdin open.

## Inspector integration

The inspector now offers **Calcular tamaño** for a selected directory and
**Cancelar cálculo** while its job runs. One supervisor thread owns one child;
selection changes, navigation, hiding the inspector and closing cancel the job.
Closing waits asynchronously for the child's termination/reap. Replies are
bounded to 8 KiB and accepted only for the current request. Errors remain visible
in the inspector; the user can retry. The helper must sit beside the app binary.

Evidence: `docs/evidence/du/inspector.png` is a real FLTK framebuffer using an
internal scene action, not native input. The compositor chose a different window
size from the requested 1000x650; this is not the 1080p baseline gate.
`memory-100000.json` samples the app plus identified helper while calculating
100,000 files (60 MB apparent): sampled PSS+Swap peak **35.108 MB**, helper exit
observed, cleanup complete. Swap was present (up to 3168 KiB separately sampled);
peak already includes swap at that instant. This is a lower-bound sampled feature
check, not exhaustive lifetime coverage or a cross-platform validation.
`memory-10000.json` stays UNVALIDATED because the short-lived helper identity was
not captured. A failed fixture setup in /tmp preceded the larger test; fixtures
now live under ~/.cache/files-test and are removed after each run.

UI validation: 16 tests passed including response-ID checking and cancellation/
reap; UI Clippy with warnings denied passed; release built. The internal scene
and capture passed separately from the resource run. Native user input remains
unverified. The treemap is still pending.
