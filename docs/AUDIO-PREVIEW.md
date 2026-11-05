# Audio metadata in Space

0.0.4, partial. Space on FLAC, WAV and other supported formats shows title,
artist, album, duration, codec, sample rate, channels and declared bit rate.
The view lets you select the text and play/pause/seek with ffmpeg/Pulse
on demand; see AUDIO-PLAYBACK for evidence and limits. The Wayland clipboard
is still pending validation with focus.

## Process and limits

It uses the system's optional ffprobe as a direct child. Its library is not linked
or distributed; there is no daemon and no decoder in the UI. The worker
opens a final regular file with O_NOFOLLOW/O_NONBLOCK and keeps its FD
across exec. Absolute path up to 4096 B, source up to 8 GiB.
Parents and mounts follow normal OS navigation; it is not a sandbox nor
a snapshot if the content changes in place.

Demuxers forced by extension: mp3, wav, flac, ogg/opus, mov for m4a, aac
and aiff. Only file/pipe protocols and allowed audio codecs; external
MOV references disabled. JSON metadata up to 64 KiB before parsing;
finite duration up to seven days, sample rate up to 384 kHz and up to 32 channels.

Each tag is cut to 2048 B at a UTF-8 boundary and control/bidi characters are sanitized before
inserting a row. Document up to 16 KiB and 256 lines, with a visible cut.
HTML is not interpreted and no URL from the tags is opened. A tag that
exceeds the whole wire can cause a rejection before the view is produced.

5 s timeout, 3 s CPU, 256 MiB virtual AS and bounded FDs. These limits
do not prove PSS nor authorize exceeding the 150 MB ceiling. State cancels and reaps
the previous job before launching the latest destination. Close distinguishes the phase with a
child (kill/reap mandatory) from read-only I/O still without ACK: see the following
update. The supervisor's timeout does not interrupt a stuck FS syscall.

## Evidence and pending work

276 Rust tests (73 core, 33 helper, 170 UI), Clippy workspace all-targets with
-D warnings and release clean. audio-ui-1793039367932469197.json passes the functional
scene: own FLAC/WAV files, real fields and Unicode selection, long tag
cut, quick changes, invalid file/symlink and normal close after ACK.
Method: internal handlers and the framebuffer of a real window, without OS input.
Closing before the recorded new spawn does not prove cancellation of a live ffprobe.

Resources and startup NOT_MEASURED because of the user's explicit phase. Budgets
from other previews are not extrapolated. Playback, waveform, cover art,
other formats and platforms are still pending; P08 and P09 are not complete.

## Identity and publication — 2026-10-27

The existing media watcher now also covers selected audio, in addition to images
and video. It attaches the directory before verifying, token/epoch invalidate stale callbacks
and results, and a periodic visible reconciliation detects lost events.
There is no per-file watcher and no I/O in draw/UI. Audio stays out of thumbnails:
the image/video predicate is separated from the set of watched sources.

AudioJob requires an expected Revision; it opens the regular FD through media_source::open,
compares before ffprobe, inherits that FD, and revalidates descriptor and name after
parsing. It returns the effective Revision together with the document. State only accepts
with a matching active generation and expected signature; Ready keeps that signature.
Identity is never attributed to a path checked in another open.

Replacement/modification/deletion/rename hide the metadata and cancel the job before
publishing again. It is not a hash nor an atomic snapshot, nor total confinement:
the final O_NOFOLLOW does not prevent symlinks in parents/mounts. Parent recovery uses
watcher/reprobe; instant invalidation is not promised without an event or while the
app is off. There is no persistent audio cache and no product temporaries.

AudioJob has a ReadOnly/Helper/Stopped gate. Close sets closing under the same
mutex that admits spawns; permission to leave ReadOnly prevents any future
child, while Helper requires reap. State.exit_allowed does not change is_drained:
if the ACK is missing it emits PREVIEW_EXIT_ALLOWED drained=false, never PREVIEW_DRAINED.
No join on an FS syscall in the UI; instant termination of kernel D-state is not promised.
Controlled phase/controller tests do not replace a truly stuck mount,
which is still pending. Changing selection keeps single-flight until ACK, without
piling up threads or launching a replacement while the previous one is blocked.

458 Rust 141/58/259, Clippy workspace -Dwarnings and release PASS. Deterministic tests:
A→replace B before opening, name changed after probe, a late Ready A and
Ready A under generation B, readonly close without a fake ACK and a gate that rejects spawn.

`audio-ui-1793083931390323451.json` functional PASS with notify callbacks suppressed:
8 framebuffers 1000×650; FLAC/WAV/Unicode/selection/limits/errors/rapid and a source
with the same path+size+mtime replaced, a modification with mtime restored, delete/recreate,
rename away/back and a replaced parent. The effective signature is compared with the fixture's
stat outside the UI; on delete the metadata disappears from the framebuffer.
A closed preview launches no helpers while waiting for reconciliation. Root exit 0/ACK,
11 registered helper PID/start identities absent/reused, Z with the same identity rejected;
no lifetime coverage and no cancellation of a live ffprobe shown at close.
Internal handlers/framebuffer only; no OS input and no native 2x sharpness test.
Resources/startup NOT_MEASURED because of the freeze; footer not used. Audio still has NO
playback, and global P08/P09/other OSes/clipboard and the baseline failures are still pending.
