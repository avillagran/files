# Audio playback — 0.0.4 (partial)

The current ffmpeg/Pulse output is functionally verified on Linux.
Resources/startup NOT_MEASURED; audibility and PCM position not validated.

## Kept prototype (historical)

ffplay prototype: real P/Play/Pause/seek controls, no autoplay,
expected source FD and the single gate/helper of AUDIO-PREVIEW. Document/selection
kept when seeking; cancellation/ACK before replacement, hiding frees the child.
The common supervisor has a spawn clock and strict stderr 8 KiB/diagnostic 4 KiB:
ffplay can return exit 0 on audio-open-failed (SDL disk fixture with a
nonexistent private parent, without generating audio on disk). That state is not accepted as EOF.

463 Rust 141/58/264, Clippy workspace -Dwarnings/release PASS.
UI 1793085963942149325 PASS only with the SDL dummy/silence: 14 logged helpers gone, play/pause/
seek requested/EOF/delete/active callback. It does NOT prove audible audio or PCM position.
Native system 1793085882916399311 FAIL: play/pause/seek arrive, EOF after seek hits the
timeout. The difference with the dummy does not by itself identify the cause in SDL/the driver.
This unit is NOT installed nor presented as complete native playback.
The installed one is still metadata FD ca8e41fd10be (1c06920).

FAIL 1793085547229903243 because of an inherited obsolete Hyprland signature, kept + sidecar;
the fixture selects the single instance for its WAYLAND_DISPLAY, without focus/global config.
FAIL 1793085633746085621: the argv -infbuf 0 treated 0 as a file; fixed to -noinfbuf.
Both are historical/images immutable.

The next adaptation uses the Pulse output of the system ffmpeg already required for
video, keeping the supervisor/FD/controls. No new linked decoder, daemon,
cache file or installed package. Resources/startup NOT_MEASURED freeze.

## Current output: ffmpeg → Pulse

Production keeps the controller and only replaces the prototype's output
with a direct ffmpeg, using the OS's Pulse (compatible with PipeWire-Pulse on Linux).
It does not use ffplay/SDL nor create another window. No own daemon, new Cargo
dependency, shell or on-disk PCM cache. Pinned/expected FD source, allowed
demuxer/codec, MOV without external references, forced Pulse output, stereo s16/48 kHz
and volume 0.5. It does not accept arbitrary file destinations. The null muxer exists
only with two explicit fixture flags; it is not a product fallback.

Pause/seek cancels and reaps before restarting from the requested position; they are
not control of a PCM clock nor a sample-exact seek. The timeline does not steal the selection.
Pause/EOF keeps the metadata, a device error allows retrying; an
invalid source destroys the previous metadata. Closing/hiding cancels, no autoplay when
returning. The UI clock starts at spawn and is estimated; it does not prove audible sound.

stderr error-only 8 KiB and sanitized text 4 KiB are kept, excess is drained and EOF is required
on both pipes plus reap. No exit 0 with a diagnostic is presented as success.
Child CPU 2× remaining+10 s (up to 1 day), wall remaining+10 s, input 8 GiB, max_alloc 32 MiB,
virtual AS 768 MiB like video. A silent functional probe at 512 MiB fails creating the
Pulse/mmap context and 768 MiB accepts it; that is NOT PSS and does not change the tree's 50 MB ideal/150 MB
ceiling. No new PSS/Swap/CPU/startup because of the freeze; resource acceptance
is pending. Do not accept a long playback that exceeds the ceiling once measured.

Waveform, cover art, UI volume, PCM position/latency, other formats and OSes,
crash/kernel D-state/offline and global P09 are still pending.

## Evidence for the current output

463 Rust 141 core/58 helper/264 UI, Clippy workspace -Dwarnings/release PASS.
`audio-ui-1793086873981565834.json` PASS with the system's real Pulse output and a
6 s null PCM source: 9 framebuffers 1000×650, metadata/UTF-8/cut/errors/rapid,
P/play-pause requested through the Action, seek through the pointer handler without stealing the
selection, resume and EOF after the seek. There are no OS keyboard/pointer events,
no audible recording and no reading of samples at the output. UI position estimated.

Hiding reaps the player and does not restart; deleting during playback hides the metadata and
cancels/reaps; restoring gets a new FD proof and allows playing. Own FLTK
callback with the player's PID/start observed alive→ACK→exit 0; 14 registered
identities absent/reused, Z with the same identity not accepted. No coverage of
full lifetimes and no test of Super+W, kernel D-state/offline or native 2x sharpness.

FAIL Pulse 1793086437393143500 keeps the OOM/mmap with AS 512; the silent contract
512/768 and the sidecar record a virtual change, not an increase of the real budget.
The prototype's native ffplay FAIL and dummy PASS remain immutable as background;
they are not reused to accept the current backend. No new measurement of memory,
CPU, startup or GPU; all the project's historical failures remain.


2026-10-27 M11: Info/codec validation and the Pulse output construction shared
with opt-in video, without duplicating the player. Standalone regression
audio-ui-1793092217283190449 PASS 9 FB 1000×650/14 logged helpers gone strict Z false/
active player callback 0 ACK, SHA app 52cde7a05957/helper 187c939eb981. Silent source:
no audibility/PCM/resources. See VIDEO-PREVIEW for the two-output gate.


M15 2026-10-27: AUDIO-PCM adds real-signal evidence (not only zero PCM):
1793096793305882328 PASS private local Unix sink/440 Hz, pause RMS 0 and opt-in video 880,
2 FB/8 logged helpers gone/live stream callback 0 ACK/recorder reaped/module absent/default unchanged.
481 Rust + 4 Python + Clippy/release PASS. Physical speakers/A-V sync/PCM clock not verified;
historical failures, including the unknown initial route ID, preserved. Freeze NOT_MEASURED.
