# Video/GIF: closing during a readonly probe — 0.0.4 (M17)

The selected snapshot and stream reuse the readonly_gate from audio/PDF/HTML.
Before opening/verifying the file they are in ReadOnly; admitting each child
shares a mutex with cancel/closing and moves to Helper. Close may allow exiting with
readonly still without ACK, because that same gate forbids every later child.
No join on the UI and no new thread for each change while waiting for a response: the
intent remains latest-wins and single-flight.

Helper does NOT grant that permission: it requires the supervisor's kill/wait. Snapshot returns
to ReadOnly after the common supervisor (reap on every result); between
ffprobe and ffmpeg it is admitted again under the gate. Stream returns to ReadOnly after
try_wait Some(status) at EOF or after the final cleanup wait. Only then can the
final readonly checks run and grant permission. WorkerStopped/channel result
are kept separate: exit_allowed is NOT is_drained, nor a promise of a fake reap.

When closing with a retained worker the app emits FILES_PREVIEW_EXIT_ALLOWED drained=false,
not FILES_PREVIEW_DRAINED. The closing flag prevents late processes even if a
readonly call returns after the permission. The existing Linux PDEATH bridge keeps
its scope for owned children. The thumbnail path keeps its earlier
supervisor; this unit applies the new gate to the selected preview.

**Open limit:** a syscall blocked during the Helper phase (for example
revalidate between frames while the child exists) still requires cancellation/
reap. No claim is made about interrupting kernel D-state or immediately terminating a group
with a D-state thread; I/O isolation, a real offline filesystem and mount recovery are still
pending. Hiding live helpers behind a fake ACK is not allowed.

487 Rust (141 core/58 helper/288 UI), Clippy workspace/all-targets -Dwarnings/release PASS.
`python3 scripts/video-readonly-close-check.py`: report 1793099666110053045 PASS,
two apps/framebuffers 1000×650, snapshot/stream roles with a SIMULATED 30 s hold in the
readonly worker, no mutex held. Shared callback→exit 0 with readonly_pending=true/
drained=false/no_future_helper=true, no child admitted for that role after the hold.
The harness does not wait 30 s: no join. It is not a real FUSE mount, D-state, OS close or measured
latency. The hook only exists with the pair FILES_TEST_PREVIEW=1 + fixture role, and is
normally inert. It simulates a pending call; it does not cancel a real syscall nor claim an ACK.

Regression video-play-ui 1793099692852727198 PASS 6 FB 1000×650, GIF snapshot/red-blue/
pause/resume EOF without error/inspector thumbnail and close with the stream observed alive→ACK 0;
10 logged PID/start gone/reused strict Z false, no lifetime coverage. Same app/helper SHAs.
No OS input/Super+W/clipboard/native 2x sharpness/PSS/CPU/startup: NOT_MEASURED freeze.
An intermediate E0061 build stayed in target/video-readonly-test.log; final build clean.
