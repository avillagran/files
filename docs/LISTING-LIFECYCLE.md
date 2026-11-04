# Listing lifecycle

Four permits bound concurrent filesystem work, not the number of idle threads.
Each Model owns one reusable listing thread. restart replaces a single pending
path, clears bounded pending stat jobs and advances the internal generation.
Both jobs and responses carry that generation; stale entry/stat indices cannot
reach the new Model listing. Idle workers release their permit and sleep on a
condition variable. Capacity wait is visible and retries without a fictitious Done.

No filesystem operation or join runs while the UI holds the request lock. A mount
syscall that blocks still holds one permit and that Model's sole worker: navigating
again updates only its pending path and does not create replacement threads. Other
Models can use the remaining permits. Interrupting stuck syscalls/recovering that
same Model before the syscall returns still requires process isolation; not solved
by cancellation flags. Idle threads scale with Models; multi-tab budget remains a
separate unverified scenario.

A first batch/Done precedes the free-space probe. Rounding is cached per worker and
redelivered by generation. Four deterministic tests exercise six workers/capacity4,
latest-only50/100restarts with one thread ID, stale metadata and a blocked free-space
probe. They simulate backend stalls, not a real disconnected SFTP mount.

Initial copy UI tests failed when the old global counter included idle workers and
new refreshes started before old workers exited. Evidence remains in
`evidence/0.0.4/copy-initial/refresh-fail.log`; no test-only serialization was used.
Current integration passes171 Rust tests (73 core/8 helper/90 UI), Clippy and release.
