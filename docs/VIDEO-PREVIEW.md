# Video: frame, duration and seek

P07 partial, 0.0.4. Space on MP4/MOV/MKV/WebM/AVI/MPEG opens a view with a
frame and duration, a clickable timeline and five-second steps.
The initial delivery was a snapshot; the playback checkpoint adds
software play/pause and opt-in audio through the same process. It promises no UI frame
rate, A/V sync nor budget.

## Execution

It uses the system's ffprobe and ffmpeg if available, according to DESIGN J.
Those libraries/binaries are neither linked nor redistributed; no crate,
codec, GPU or daemon is added to the main process. If a tool is missing, the
capability is shown as unavailable. The job launches direct children sequentially and reaps each
one before the next; it never uses a shell nor a helper that launches grandchildren.

Linux opens in the worker a final regular file O_NOFOLLOW/O_NONBLOCK, up to 8 GiB,
absolute path 4096 B; it inherits only that descriptor and uses /proc/self/fd/N. It keeps the
inode when the path is renamed/replaced; it is not a snapshot of in-place changes. Symlink
parents and mounts keep OS navigation, not a folder sandbox.

The demuxer is forced by extension (mov, matroska, avi, mpegts, mpeg), protocols
file,pipe; no playlists/HLS/concat, external MOV references disabled.
Codec allowlist, max 64 Mi pixels/16384 px, finite duration ≤7 d, metadata 64 KiB.
Orthogonal rotations/bounded SAR are accepted through the display computation; exhaustive
visual validation of rotated/anamorphic videos is missing. Software FFmpeg
-hwaccel none, one decode/filter thread, exact RGB output ≤960×540×3. Only one
RGB wire allocation is moved into the Bitmap; no frame history.

State keeps only the last seek per revision; it cancels and drains before launching a
replacement. It frees the previous frame before decoding another. Close reaps the ACK
before exiting. A tool/file error is shown in the view.

## Limits and acceptance

Timeout 4 s ffprobe/8 s ffmpeg, CPU 8 s/process, bounded FDs and core 0. RLIMIT_AS 768 MiB
contains the virtual space of system libraries; it **does NOT allow that PSS** nor
prove the 150 MB ceiling. -max_alloc 32 MiB limits each allocation, not the sum nor
process memory. Decoders and containers are not an OS sandbox.
Total app+CLI+Swap use is pending: NOT_MEASURED because of the user's explicit phase.
Do not run new resources/startup until the phase is lifted; do not extrapolate other
previews. 4K/8K scenarios, complex codecs and playback still have no acceptance.

`python3 scripts/video-ui-check.py` creates a synthetic 640×360 Matroska FFV1, red
0–4 s and blue 4–8 s. It captures the first frame, seek 5 s and a timeline click at 2 s, verifies the
real decoded RGB and a fresh revision, quick change to the last destination, bad/
symlink errors and close 0 after ACK. Only handlers + framebuffer of a real window; no OS input
nor clipboard. Own fixtures in ~/.cache/files-test, no personal data.

Seeking to EOF keeps one frame interval according to avg_frame_rate/r_frame_rate;
if there is no valid rate it uses a 1 s margin. The displayed position is the effective
normalized request, not the frame's tested PTS. VFR/last PTS need more evidence.

## Software playback without audio

P/Play/Pause uses a single direct ffmpeg, -re input and fps 12 output.
One partial buffer and one replaceable last-frame slot in the worker; the UI
keeps only its current bitmap and reuses one viewport. No unbounded history.
The worker's 2-buffer cap does NOT include UI/viewport/FLTK copy/Wayland buffers nor
the child's decoder memory. Frames not shown are replaced, not queued.

Pause cancels/reaps the decoder; resuming opens another from the estimated position. Seek
pauses playback and goes back to a snapshot; P resumes. Changing file/closing drains
before dispatching a new one. The frame counter restarts for each decoder: a retained
Ready does not prove the new process. Position = seek + sequence/12, estimated,
NOT a PTS timestamp/A-V sync. 12 fps is a setting, not measured performance.

First frame/stall 5 s, duration+10 s and tight pipe quotas; stderr retained 64 KiB
with the excess drained, a partial EOF rejected, kill+wait before Finished. CPU
2× remaining+10 s (max 1 day), AS 768 MiB virtual/max_alloc 32 MiB, not PSS approval. Sparse/
VFR material and demanding codecs may fail; audio and hardware are still pending.

`python3 scripts/video-play-ui-check.py` verifies real frames, red→blue,
pause/drain, resume/EOF and close while the process is observed alive (PID+starttime).
The clip is 24 framebuffer snapshots taken at roughly 4 fps and assembled after
closing the app, not a continuous OS recording nor a test of 12 fps/CPU/latency. Resources/
startup are still NOT_MEASURED because of the user's phase, without replicating benchmarks.

272 Rust/Clippy/release PASS; video-play-ui 1793038888536010466 functional PASS
with the PID/starttime of a live ffmpeg and its disappearance after pause/EOF/close. The first FAIL
1793038705143512044, caused by an earlier sequence accepted on resume, is kept; the
sequence restarts and a new frame/spawn is required.

The capture video-playing-blue 1793038886785315859 shows a main footer of 76.6 MiB
after 24 in-process readbacks. That is a harness observation, UNVALIDATED for attributing it
to the player or deciding the tree's PSS+Swap budget. Playback without a clip or
captures alone was not measured; experiment only after the resource suspension ends. Do not attribute
+70 MB to RGB/Painter without an experiment; per-frame allocations are bounded.

## Returning UI buffers — 2026-10-27

The UI hands the RGB Vec it has just displaced back to the same job. The single slot
alternates Ready(frame) or Recycled(rgb); the reader consumes both as the next
assembly allocation. There is no additional queue/channel nor history. A new frame
that won the race is never replaced by recycled RGB; in that case
the returned buffer is discarded. The expected len and the maximum capacity are verified
before keeping it. Recycled does not publish Ready nor advance position/sequence.

This ownership improvement does not prove PSS/CPU savings nor resolve the footer
figure under captures. Freeze intact, no new measurements. The contract
stays partial: audio/VFR/PTS/global cache/other platforms pending.

454 Rust 141 core/58 helper/255 UI, Clippy workspace -Dwarnings and release PASS.
A model test of 100 handoffs keeps three pointers across assembler/slot/display;
State checks the return of the displaced bitmap. It is not equivalent to counting
allocations across the whole app nor measuring savings/latency.
video-play-ui-1793082933186610779 functional PASS with --no-clip: 4 framebuffers
1000×650, red→blue, pause/drain, resume/EOF and close with the stream observed alive.
9 registered helper PID/start identities absent/reused after exit 0, Z with the same identity
not accepted; no lifetime coverage. The 24-readback clip is omitted only for this
functional scene: NOT a discriminating memory experiment, no reading of the footer.
FAIL 1793082885016655179 remains with a sidecar: a command did not find the entry
and then showed a folder; the trigger was not established. The repeat enables the already
existing ambient-input isolation for fixtures, same binary/handlers; it does not prove the
cause of that failure nor OS input. The old 76.6 MiB footer is still UNVALIDATED.


## Opt-in audio in video — 2026-10-27 (M11)

The view opens muted; Audio/M enables a compatible track by explicit
choice. Changing during playback cancels and reaps the earlier process
before replacing it. Pause, seek, source change and close keep that gate.
It does not create a second player: the same ffmpeg delivers RGB through a pipe and PCM audio to
Pulse. They share the output formatter, validation and the selected audio
allowlist; there is no PCM/cache on disk nor a new linked dependency.

ffprobe inspects up to 32 tracks with unique absolute indices 0..255 within
64 KiB of metadata. The first video that is not attached_pic and the first
audio track are selected; if the latter is not compatible the capability is shown as unavailable,
without silently jumping to another one. The two outputs use their real indices,
they do not assume video 0/audio 1. Manual track selection is still pending.

A source failure marks a typed state from the worker, hides the RGB before
the ACK and requires a new token/watch/probe. A device error with a valid
source keeps the verified frame and shows a notice; disabling audio
allows retrying muted playback. Nothing is decided by parsing error
strings. The visible notice is sanitized to 512 B, the full diagnostic bounded in State.

The child process sets MALLOC_ARENA_MAX=2; it does not modify the global environment. The
functional AV contract with Pulse failed mmap with the default arenas and passed
with 2 under the SAME AS 768 MiB. See video-audio-arena-contract.json and FAIL
1793091361377293413 preserved. This only allows creating the context under the current
virtual containment: it does NOT prove lower PSS, CPU savings or the 150 MB gate.

470 Rust (141 core/58 helper/271 UI), Clippy workspace/all-targets -Dwarnings and release
PASS. video-play-ui-1793091780741012397 functional PASS with an 8 s synthetic source,
FLAC audio at index 0 and FFV1 video at index 1: initially muted, hot-toggle with
ACK/earlier PID absent, red→blue, pause/resume/EOF and close with an active stream.
5 framebuffers 1000×650; 9 registered PID/start gone/reused, Z not accepted.
The native Pulse output uses zero PCM: NO audibility, PCM capture, A/V sync nor
sample-exact seek tested. 2x geometry does not prove native 2x sharpness.

video-play-ui-1793091967178711219 functional PASS: private Pulse socket missing,
visible error with the frame retained, muted retry/frames/EOF/close 0 ACK;
6 framebuffers and 9 logged helpers gone. FAIL 1793091839762673007 kept: the fixture
set PULSE_SERVER after Popen; fixed before, same binary.
Separate audio regression audio-ui-1793092217283190449 PASS 9 framebuffers/
14 logged helpers gone/active player close 0 ACK, same app/helper SHA.

Scenes through internal handlers and the real FLTK framebuffer, NOT OS input nor clipboard.
No 24-readback clip, no memory experiment. Resources/startup NOT_MEASURED
because of the freeze. P07/P09 stay partial: VFR/PTS, large codecs/rotation,
platforms, the global cache policy and the total gate pending. The baseline FAIL and the
historical 76.6 MiB footer intact; no 0.0.4 tag nor global PASS.


M15 AUDIO-PCM: a sample-defined WAV source→FLAC 880/12 s video, a private monitor confirms
880 when enabling audio after pause/seek 0; default mute checked after the preamble.
1793096793305882328 PASS 2 FB/8 logged helpers gone/root 0 ACK/own sink removed/default unchanged.
No A-V sync/PCM clock/physical sound nor hot-toggle with PCM proven by this scene (hot-toggle
with zero PCM already in M11); resources/startup NOT_MEASURED freeze. 481 Rust/Clippy/release and 4 Python.


M16 GIF-PREVIEW: GIF profile 8 MiB/4 Mi pixels/300 s, exclusive gif demuxer/codec and ignore_loop 1
in snapshot/stream/thumbnail; no new engine. 485 Rust/Clippy/release PASS and
video-play-ui 1793098465429205562 PASS 6 FB/10 helpers gone/active close 0 ACK/single pass,
inspector 160×90 first frame FD; grid GIF/alpha/exact delay/zoom and budget pending.
Harness FAIL 1793098062479122223 kept: Closing Space allows a visible thumbnail,
not a stream. The EOF instrument now requires notice None, not a backend error as a normal EOF.
Freeze NOT_MEASURED, no PSS savings nor native 2x sharpness/OS input.


M17 VIDEO-SOURCE-CLOSE separates readonly exit permission and Finished ACK in snapshot/
stream; child admission shares a mutex with close, Helper requires reap. 487 Rust/Clippy/
release and two roles with a simulated hold 1793099666110053045 PASS; normal GIF regression
1793099692852727198 PASS 6 FB/10 helpers gone/active close ACK 0. No real kernel D-state nor guarantee
about a syscall inside the Helper phase. No UI join, no fake DRAINED nor new metrics.

## M18 — interactive framing, 2026-10-27

`VIDEO-ZOOM.md` extends the same viewer to video/GIF: zoom 1–8×, drag,
Ctrl+wheel and +/-/0. The framing is kept across frames/pause/seek; only three numbers
survive the new snapshot, not a copy of the raster. Changing source or revision
resets it. The timeline keeps priority and the arrows seek 5 s.
This replaces the earlier pending-zoom limitation; VFR/PTS accuracy,
exhaustive rotation, GIF alpha/disposal/delay and resources are still pending.
Resource/startup freeze in effect.

## M20 — non-destructive viewer rotation, 2026-10-27

PREVIEW-ROTATION: Nerd/R/Shift+R buttons only rotate the image/PDF/video/GIF view;
direct sampling of the original RGB, with no rotated copy nor modified files.
The numeric transform keeps the rotation across frames/seek and PDF pages/text-return;
a new source/revision/reset 0 removes it. F1 Help adds the real media shortcuts.
489 Rust 141/58/290 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-rotation-ui 1793102443334515413 PASS 14 FB 1000×650/asymmetric 4 quadrants with
checked framebuffer pixels/CW-CCW-reset rotation/video-GIF play/seek/PDF page 2 +
text return/original bytes intact/callback 0 ACK/21 logged helpers gone Z false.
No OS input/native 2x/AT-SPI/lifetimes/active helper close in this scene nor a test of
photographic color; 14 functional readbacks, NOT an isolated measurement of use.
Resources/startup NOT_MEASURED freeze; no global P02/P06/P07/P09 nor 0.0.4 tag.

## 2026-11-04 — autoplay, global mute and source quality

See `VIDEO-AUTOPLAY.md`: it replaces the paused/muted start, the
960×540 cap, the bilinear scaling and the fixed 12 fps output described above.
