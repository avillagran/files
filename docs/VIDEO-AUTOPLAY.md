# Video preview: autoplay, global mute, source quality

2026-11-04. User request: Space on a video plays immediately; mute is a
global choice (left muted, every later preview starts muted) and sound is
on by default; the open preview shows the ORIGINAL video, not a lower
quality.

## Behaviour

- **Autoplay.** The first decoded frame appears, and the next pump starts
  playback with options computed from that frame (the snapshot is never
  re-run). A seek while playing resumes from the new position; a seek while
  paused stays paused. EOF pauses as before.
- **Global mute.** `config.toml` `preview_muted` (absent = sound on). M or
  the Audio/Silencio button toggles it, saves it through the existing
  async config writer, and it survives closing, opening other files and
  restarting. Audio-only files keep their explicit play control.
- **Audio output unavailable.** The video keeps playing silently for that
  file ("Sin audio"); the saved preference is unchanged. If the silent
  retry also fails, the cause was not audio: the label is cleared and the
  error shown.
- **Source quality.** Frames are decoded at the source's own size, reduced
  only to the device pixels of the preview area (never upscaled; 4K cap so a
  frame fits the decoders' 32MiB allocation cap), with Lanczos/full-chroma
  scaling, at the source's declared frame rate (max 60, 30 if unknown). The
  viewer samples and draws in device pixels (`Painter::image_scaled`), so a
  2× output gets 2× pixels. Before: 960×540 cap, bilinear, 12fps, logical
  pixels, nearest-neighbour downscale.
- Decoding uses up to 8 threads (one before); CPU and virtual address limits
  scale with them. Still `-hwaccel none`, no new dependency.

## Bugs found on the way

- fltk-rs `Window::pixels_per_unit` ignores the Wayland output scale: the
  Retina M2 reported 1.0 (`m2/m2-source-native-scale-*.json`, retained).
  Toolkit a967007 adds `native::pixels_per_unit` with `fl_wl_buffer_scale`;
  M2 now reports 2.0.
- The stream reader slept a fixed 2ms after draining the 64KiB pipe: about
  32MB/s, 6fps at native size (failed report retained). It now polls both
  pipes and asks for a 1MiB pipe.
- Multi-threaded scaling hit EAGAIN under the old 768MiB virtual cap
  (retained); the first fallback wrongly blamed audio, now corrected.

## Validation

- Unit: autoplay + seek-resume, global mute across close/open, silent
  fallback; legacy lifecycle tests keep the paused/muted contract
  explicitly. Files 1625 Rust (708/112/805 + 3 historical ignored), Clippy
  `-D warnings`, rustfmt PASS.
- `scripts/video-autoplay-ui-check.py`: H.264 1920×1080 30fps + AAC zero PCM
  (nothing audible; audibility NOT validated), hidden workspace, internal
  commands. Plays with no play command, stream size = fit to device box,
  declared 30/1, ~34fps observed over 2s, sound on by default, mute saved,
  reopen and restart start muted, unmute saved, scripted mode unchanged.
  PASS NODE source/installed (twice) and M2 source (native scale 2 → 1884×1060
  for a 1124×530-unit area; FLTK 2× → 1920×1080 exactly) and installed.
- Legacy `video-play-ui-check.py` (5 variants) and `video-ui-check.py` PASS
  after updating the script for source-rate output and the silent Pulse
  fallback; media-lifecycle, preview-neighbor, video-readonly-close PASS.
  audio-pcm, helper-crash and thumbnails checks fail identically with the
  previously installed binary (pre-existing harness failures, not fixed
  here). Glass check PASS again on both machines.
- Scripted checks (`--ui-smoke`) keep the old paused/muted start unless
  `FILES_TEST_VIDEO_AUTOPLAY=1`.

Observed, not a gate: footer PSS 81.8 MiB while playing 1080p at 2× on NODE
(frames are now ~6MB instead of ~1.5MB). Below the 150MB hard limit, above
the 50MB goal; resources/startup NOT_MEASURED.

## Installed

NODE `files` 18c7b5212dcb (receipt
`docs/evidence/0.0.4/installed-checkpoint-1793819090179150445.json`), M2
a260b4ced38f (`docs/evidence/video-autoplay-20261104/m2/m2-installed.json`).
Five backups and hashes verified on both, user windows and settings
untouched; reopen Files to load it.
