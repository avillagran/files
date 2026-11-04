# Explicit ZIP recovery UI

Final scene: ../../0.0.4/archive-recovery-ui-1793496524673844308.json
Five actual FLTK framebuffers at1000×650 and440×440; internal painted-target
handlers, not OS input. No resource/startup sampling (freeze).

The scene creates a retained staging failure by pausing after positive progress,
placing an independently owned foreign file in the private stage and cancelling.
Cleanup refuses it. Moving that file outside the stage preserves its inode/bytes;
a start-paused cancellation retains capability. Queue8 rejection restores the
original outcome; explicit Clear invalidates the old confirmation stamp, and
cleanup then succeeds with zero published roots.

A private publish-observer error occurs BEFORE real parent fsync. It is not a
failed syscall/powerloss simulation. The source ZIP is then removed. A substituted
published directory is rejected without touching its occupant; restoring the
original inode permits sync-only retry, with bytes/inode preserved and no extraction.

The first GUI FAIL1793496508849879679 is retained with its two PNG and sidecar:
cleanup succeeded, but the checker incorrectly required an open error notice for
success (product uses compact success toast). No product relaxation. Initial test
compile double-Box and Clippy large-enum failures are retained; final Work boxes the
large extraction token, leaving ownership/accounting conservative.

Durable recovery across app restart, archive creation, ZIP64/TAR, link extraction,
native input and resource/startup/global acceptance remain open.
