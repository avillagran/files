# On-demand GIF — 0.0.4 (partial)

Space opens an initial snapshot, without autoplay. P/Play uses the same software
video engine, pause/seek/EOF and single-flight; no other linked decoder,
package, daemon or on-disk cache. When it ends it keeps the last frame and lets you
replay by explicit choice. `-ignore_loop 1` is forced in ffprobe, snapshot and
stream: the file's infinite loop does not create a permanent job.

Profile shared by snapshot/thumbnail/stream: regular GIF ≤8 MiB compressed,
4 Mi pixels per frame, finite duration ≤300 s; output 960×540 RGB24 or a 160 px thumbnail.
Forced `gif` format/codec, exclusive whitelist (it does not widen codecs for other
containers); file/pipe protocols, byte cap before probe, pixel cap passed to the decoder
and verified in metadata. Expected/effective FD, generation and watch/reconcile of
existing media. Symlink parents/mounts still follow OS navigation, not confinement,
an atomic snapshot or a sandbox. Local limits/AS 768 MiB are NOT equivalent to the tree's PSS.

RGB is opaque; transparency/complex disposal or fidelity for every
GIF is not promised. The 12 fps cadence is a setting, position is estimated by sequence, not PTS
nor exact playback of delays. Large/irregular GIFs and zoom are still pending.
The visible inspector thumbnail is a static first frame: when Space closes it
may request a snapshot; that is NOT restarting the hidden stream. Grid uses the same
selector/helper/cache, but a GIF in the grid was not captured in this unit.

485 Rust (141 core/58 helper/286 UI), Clippy workspace/all-targets -Dwarnings/release PASS.
`python3 scripts/video-play-ui-check.py --gif --no-clip` creates an 8 s 640×360 GIF,
red→blue, with the NETSCAPE2.0 extension and loopcount 0 checked in the bytes.
video-play-ui 1793098465429205562 functional PASS 6 framebuffers 1000×650:
paused/red snapshot, play/red→blue/pause/resume EOF without backend notice, no stream
after closing Space, red 160×90 inspector with a checked FD signature; new play and
close with stream PID/start alive→ACK→exit 0, 10 logged helpers gone/reused, Z not accepted.
No lifetime coverage, OS input/clipboard, native 2x sharpness, performance or budget.
No 24 readbacks/clip; resources/startup NOT_MEASURED because of the freeze, no 0.0.4 tag/global PASS.

FAIL 1793098062479122223 is kept: it checked that NO helper existed after
ClosePreview, even though the inspector was still visible and requesting a thumbnail. The log only
showed new probe/frame, not stream; fixed to count streams and to validate the visible
thumbnail separately with snapshot FD/dimensions/pixel and a real capture.
The EOF check now requires notice=None in addition to pause/drain; a backend error
cannot count as a normal end. The intermediate build with a missing import/unused
Path is kept in target/gif-final-test.log/gif-test.log; the final build is clean.

## M18 — interactive framing, 2026-10-27

`VIDEO-ZOOM.md` extends the same viewer to video/GIF: zoom 1–8×, drag,
Ctrl+wheel and +/-/0. The framing is kept across frames/pause/seek; only three numbers
survive the new snapshot, not a copy of the raster. Changing source or revision
resets it. The timeline keeps priority and the arrows seek 5 s.
This replaces the earlier pending-zoom limitation; VFR/PTS accuracy,
exhaustive rotation, GIF alpha/disposal/delay and resources are still pending.
Resource/startup freeze in effect.
