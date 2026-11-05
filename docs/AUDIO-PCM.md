# PCM on a private sink — 0.0.4 (functional verification)

`python3 scripts/audio-pcm-ui-check.py` is a functional scene, not a benchmark.
It uses the user's Pulse over a local Unix socket, a null sink with a random name
`files_test_*` and its explicit monitor. It does not change the default device or the
desktop configuration. The product's only output override requires
`FILES_TEST_PREVIEW=1` and `FILES_TEST_AUDIO_SINK=files_test_*`, ASCII 64 B, no paths,
server or other names. `-device` is passed to ffmpeg: if the sink is missing, it errors without
falling back to the speakers. It does not drive graphical interfaces or need CUA focus.

It generates a sample-defined WAV: 96000 zero samples + 480000 tone samples at 48 kHz/mono/s16, then
FLAC 440 Hz and an FFV1 video + FLAC 880 Hz without time filters; 12 s is checked in the metadata
of both. Private cache fixtures, never user files.
The parec monitor at 48 kHz/stereo/s16 uses a RAM pipe bounded to 1 MiB per window, with no
PCM file. It checks a non-null signal and the energy of the matching tone, silence when paused
and initially muted video after the preamble, then 880 Hz when audio is enabled
after pause/seek 0 in the same video child.
The sink-input is validated by that ffmpeg's PID and the own sink before
analyzing samples. Signal, samples and energy are audio-content criteria,
NOT PSS/CPU/latency/performance nor a test of physical audibility/A-V sync/PCM clock.

Pause/close callback uses the app's shared handlers; the real framebuffer does not
prove OS input. Afterwards it verifies root 0/State ACK and the absence of registered helper
identities (same PID/start in Z state fails), reaps the recorder, revalidates
ownership of the module before removing it and confirms the default is unchanged. Only that
module and those own processes are cleaned up; no other sink is modified.
SIGTERM/errors save partial evidence and run the finally block; fixture
cleanup is not guaranteed if the supervisor is killed with SIGKILL, nor with the server offline.
The product does not create those modules: they are temporary test infrastructure.

481 Rust (141 core/58 helper/282 UI), Clippy workspace/all-targets -Dwarnings/release PASS;
4 pure Python tests PASS, telling apart tones/silence/incomplete data. Final capture
`audio-pcm-ui-1793096793305882328.json` functional PASS 2 FB 1000×650: 440 and 880 tones,
pause/default mute RMS 0, routes by PID on the own sink confirmed. Callback with
stream PID/start alive→State ACK→root 0; 8 logged helpers gone/reused, Z not accepted.
Recorder reaped/own module absent/default unchanged at the end. No lifetime coverage.

Historical failures kept: 1793095899920132809 module JSON without index prevented
cleanup; the single module was recovered by type+nonce+short ID and its removal verified.
1793095941649589586 the hyprctl parent used an obsolete signature even though the child was correct;
1793096007907294698 an explicit audio Action was missing. 1793096114698595496 initial route
mismatch without recording its ID: cause still OPEN; it is not claimed that it was PA_INVALID_INDEX,
nor is physical audibility claimed. PA_INVALID_INDEX is now pending,
distinct real IDs are still rejected and target.object/private ID is recorded.
Zero-route 1793096246104965614 PASS only for total silence. PCM 1793096434340845151 historical
PASS with an lavfi generator whose duration 10.042667 differed from the intended 12; the sidecar
withdraws the accuracy/preamble claim. An explicit WAV is used in the final report, same app SHA.

The initial build hit ENOSPC on a .pyc and our own cancellation 130 is preserved; cleaning only the repo's
incremental cache and retrying with CARGO_INCREMENTAL=0 passed. Do not attribute PSS/CPU savings nor a
single cause of slow builds to this comparison; unrelated builds were not touched.
Capture/close, cleanup or content failures are never turned into PASS. Resources/startup
NOT_MEASURED because of the freeze; FFmpeg virtual limits do not prove the 150 MB ceiling.
