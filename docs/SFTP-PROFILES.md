# SFTP connection profiles

Status: source6f2eab1 validated and installed locally. No global PASS or0.0.4 tag.
This supplements SFTP-PREPARATION and SFTP-MOUNT-CONTRACT. A profile is a saved
connection **draft**, not a mount, SSH grant or promise that a host is reachable.

## Interaction

Open SFTP from Actions/the command catalogue, then **F2 / Nerd Perfiles**.
The catalogue accepts mouse selection, wheel, Up/Down and PageUp/PageDown.
**Enter / Cargar** fills the existing four fields and invalidates the prior Review.
It never connects, probes the selected host, navigates or reuses a connection proof.
Back in the form, review with Enter and explicitly connect with CtrlEnter as before.
Opening the catalogue/restarting Files does not restore a draft automatically.

To save, complete the four connection fields, open Perfiles, edit the displayed
name, then **CtrlS / Guardar**. The name is at most96 UTF8 bytes. CtrlA, cursor
selection, mouse selection/drag, cut/copy, undo/redo and the existing epoch-bound
text-paste route operate on the label rather than on connection fields.
Save captures the exact four fields plus label before admission. Subsequent draft
edits/modal closure/storage ACK do not apply or alter a connection draft.
Duplicate exact targets return the existing catalogue unchanged, including its name.
Renaming/replacing an existing profile is not exposed in this first UI.

**Supr / Quitar** removes the selected local profile only, using stable ID and exact
Entry proof; it never disconnects, removes remote data or deletes SSH settings.
**F5** reloads, or retries the exact failed operation. A publication whose directory
sync failed retries **Sync only**, not Add/Remove again. **F8 / Omitir** discards an
unadmitted/retry intent without rolling back anything already published.
ACK never selects a different connection or steals label editing focus. Old modal
stamps, catalogue revision and stable profile IDs reject stale load/remove actions
even if an externally recreated catalogue reuses the old ID.

## Storage and bounds

- `sftp-profiles.bin`, FILESSFP version1; portable UTF8 field format. Original host,
  user, port spelling (including0022), directory, percent and shell-like literals
  are preserved, not URL-decoded, trimmed or normalized.
- At most32 profiles and128KiB wire. Label96B, fields253/64/5/4096B and existing
  Target combined-source validation (4096B). ID/duplicate/bounds/full-format checks
  run on borrowed wire before allocating the catalogue. Target::validate shares
  exactly the borrowed validation used by Target::parse.
- Only connection fields/display label; no password, private key, token, credential,
  arbitrary SSH argument, process identity, mount authorization or agent secret.
  Host/user/path metadata remains potentially private local configuration.
- Missing catalogue read does not create directories. Regular-only, NOFOLLOW,
  NONBLOCK, bounded prefix128KiB+1 and effective FD/path revision checks. Future,
  corrupt, oversized, FIFO and symlink files are rejected and preserved.
- Nonblocking interprocess `sftp-profiles.lock`, fixed file identity rechecked before
  atomic checked publication. Newly created data/lock files0600. Creator-only
  explicit flock unlock avoids an inherited child extending/releasing ownership.
- Atomic file replacement with file/directory sync uses the existing persistence
  implementation. Published-but-unsynced is distinct from a failed publication.
  Error detail is bounded512B in UI. Storage never invokes the SFTP transport.

One pending/active catalogue operation with exact request-ID ACK and bounded retained
intent uses **existing AuxIo**, no new thread or network helper. Document is Arc
shared; visible rows paint borrowed labels/hosts rather than cloning the catalogue.
These local caps do not validate total PSS/Swap or a memory/performance improvement.
A storage operation already running must ACK before close; offline/kernelD/configFS
blocking has no new deadline guarantee. Read errors do not trigger automatic retries.

## Validation scope

See `docs/evidence/sftp/profiles-validation/` and the final scene referenced below
once complete. Unit tests include symmetric row/wire/ID/target checks, all partial
frames, Unicode/literal roundtrip, exact-entry removal, interprocess ownership,
invalid-file preservation, ACK/retry/close, stale modal/profile identity and no
automatic application of a saved draft. Synthetic unsynced state tests establish
controller behavior, not physical power-loss or an actual failing fsync syscall.

Resources/startup **NOT_MEASURED** under the existing freeze. No new previews,
OS input, real-host authentication, clipboard interoperability or global PASS.

Final scene: `evidence/sftp-preparation/sftp-profiles-ui-1793484790015975923.json`,
scoped functional **PASS**: three independent apps, seven real FLTK PNGs
(1000×650 and initial440×440), save/duplicate/load/no-auto-review-connect, restart,
lock conflict preserves bytes/exact retry, future version bytes preserved,
label editing/stale paste and normal root0 after callback/drain ACK. No active
storage syscall observed at close, native input or host/network exercise.

1040 Rust PASS (349 core/91 helper/600 UI),3 existing ignored; final Clippy
workspace/all-targets-Dwarnings, release and fmt PASS. Full suite precedes only
the shorter explanatory caption; final Clippy/release/fmt/scene include that caption.
Original positive scene1793484676038671057 and sidecar retain the clipped minimum
subtitle; final caption fits at440px. Compile/layout/fixture failures remain in
profiles-validation. An older dialogue test failed without a spawned-PID marker;
its actual result error was not retained and cause remains OPEN. The test now
requires the exact bounded-control rejection before checking reap; it passes with
normal concurrency. No supervisor runtime change or global serialization.

Installation: `docs/evidence/0.0.4/installed-checkpoint-1793484872289248556.json`. Independent five-installed/five-backup
SHA256 rereads PASS in `evidence/sftp/profiles-validation/install-verification.json`.
App `ec5ab99217fc50bd3df4c75c62e27a3528d4e3c5db054d974c6f909204db0576`;
helper `05736746b394413d59df5ecc169341592f66b44d9b895d5388d1b4b4f9e61967`;
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Existing running windows were not controlled.
