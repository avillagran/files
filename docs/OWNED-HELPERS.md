# Lifetime of owned helpers — 0.0.4 (Linux)

owned_child::arm adds PR_SET_PDEATHSIG=SIGKILL before exec. It checks the parent's
PID before/after prctl to reject the parent-death race. The
post-fork hook does not allocate memory, take locks or do I/O, and it is added to the existing
FD/rlimit hooks. On Linux the signal belongs to the creating thread: each
supervisor keeps that worker until reap.

It applies only to the common supervisor for text/image/Markdown/HTML/PDF/ffprobe/
audio, video streaming, du and the Space report. Apps the user opens
with open keep their own independent lifetime; there is no global process interceptor.
Normal cancel/kill/wait and ACK before replacing/exiting are still mandatory;
this signal covers the creator's abrupt termination. It does not create a daemon/reaper in
the product, does not interrupt kernel D-state, and does not cover grandchildren/setuid exec. Windows/macOS
equivalents are pending. It is not a durable journal/rollback for file tasks.

## Tests

464 Rust 141 core/58 helper/265 UI, Clippy workspace -Dwarnings and release PASS. A
prctl test after exec confirms SIGKILL and the parent PID, and that the previously
configured umask is still in effect (the earlier pre_exec is not overwritten).

`helper-crash-1793088449742352360.json` PASS: own fixtures, 20k empty
descendants and two synthetic media files; silent audio/Pulse, video, du and Space. In
each case the PID/start identity was alive right before killing ONLY the
root PID; no group kill and no manual signal to the helper. The script's PRIVATE subreaper
adopts/reaps all four and verifies WTERMSIG=9/identity gone.
It does not prove that PID 1/T3 reaps orphans with the same latency: the product does not install a
subreaper. The kill may happen after spawn and before useful progress.

Three framebuffers 845×1071 (normal tiled, not 1080p size nor 2x sharpness) show
its own window before the kill and relaunched afterwards. The hashes of both media files and the count of
20k descendants are preserved; no hash of each descendant/attribute. The relaunch
reuses the private config, gets verified media and closes 0 with ACK. No file-writing
operation was active; it does not test copy/rename recovery.

Normal regression: Pulse audio 1793088666411517841 PASS 9 FB/14 logged helpers gone, and video
1793089019986522162 PASS 4 FB/9 logged helpers gone, play/pause/seek/EOF/active close.
A FAIL is never rescued by manual cleanup; the script only uses it after a failure and does not
turn it into a PASS. OS input/audibility/PCM/kernel D-state/offline/grandchildren still
have no acceptance. Resources/startup NOT_MEASURED because of the freeze; the PSS footer is not used and
the 50 MB ideal/150 MB ceiling are not changed.
