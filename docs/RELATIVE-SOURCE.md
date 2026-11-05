# Embedded resources bound to the descriptor — 0.0.4 (2026-10-27)

This unit closes the race between the probe of a Markdown resource and the FD of the
helper that actually decodes it. It does not yet close its invalidation after
the document is published: watching every parent, reconciliation and immediate
removal of the embedded bitmap are still pending. Full P09 is not claimed.

`relative_source::Opened` keeps the root FD and the file FD. The final root requires
O_DIRECTORY/O_NOFOLLOW/CLOEXEC; each relative component is opened with openat and
O_NOFOLLOW, intermediates O_PATH/DIRECTORY, the file NONBLOCK/regular-only.
Parents above the root may follow symlinks; it does not limit mountpoints or
hardlinks, it is not a sandbox. Root dev/ino and file dev/ino/size/mtime/ctime/ns form
`Proof`. It is not a content hash nor an atomic filesystem snapshot.

The text helper now returns the effective identity of the root and the revision of the
FD it read, and verifies FD/name after reading. Direct reading keeps its
symlink-parent semantics; relative resources keep the existing, stricter policy.
The prefix reader does not silently load a truncated image:
it asks for cap+1 and the shared codec rejects compressed input over 8 MiB before reading the
header. Text keeps 256 KiB/2000 LF and explicit truncation.

Markdown passes the text's effective root to ProbeRelativeFile. This probe opens the
resource without reading its content, requires the same root and returns a Proof. After
reap, PreviewRelativeImageChecked starts with that expected signature. The helper
compares against its own FDs BEFORE reading, reuses the existing decoder/IDCT and
verifies after decode: root FD, the root occupying the current name, file FD and the
file reachable through every relative component. Replacing an intermediate
folder also invalidates the result. Response::Image returns the
effective signature and the file's Revision; the UI requires both to equal the expected ones before
accepting the RGB. The controller generation still discards responses from an
earlier document when navigating/closing. No parallel decoder or helper is added.

Per document, at most 8 normalized resources: a manifest of paths <=4096 B and Proof,
plus Vec/tuple metadata, separate from RGB 4 MiB + a 4 MiB transient reservation. Arc
shares occurrences without duplicating RGB; the capacity of the drained wire is still charged.
The probe and decoder run sequentially with the document's remaining 10 s time
budget; close/cancellation uses the existing supervisor and reap before replacement.
No I/O is awaited in draw/UI. There is no new persistent cache and no product files
next to the original. Verified on Linux; other OSes return Unsupported, not dev/ino 0.

There is a window between the last name check and publishing,
and later changes do not yet invalidate the embedded resources. The signature manifest
prepares the next watcher/reconcile unit; it does not replace it. A bounded
close of a mount syscall in an uninterruptible state is not claimed either.

434 Rust 141 core/47 helper/246 UI, Clippy workspace all targets -Dwarnings and release
clean. `scripts/relative-source-check.py` real release PASS in
`evidence/relative-source/relative-source-wire-1793074260681793818.json`:
text/effective root, A→B same path/size/mtime before open, new B accepted,
root rename/replacement, intermediate parent replaced, symlinks/traversal and a raw
0xFF name. FAIL 1793074121342824158 is kept: the new commands were missing from
the initial CLI allowlist, fixed before the PASS evidence.

`evidence/0.0.4/markdown-embedded-ui-1793074268202431350.json` functional PASS:
5 occurrences/2 unique resources checked, retained RGB 3165524 B (ledger capacity and
metadata; NOT PSS), navigation root→subdir→nested→subdir→root,
escapes blocked, 5 captures 1000×650 and callback→preview drain→exit 0. Internal
handlers + framebuffer, NOT OS input. Cleanup in this capture only proves root exit 0,
not coverage of child lifetimes: there was no sampler. Diagnostic timings already
emitted by the harness are not used as a performance or startup gate.
Resources and startup
NOT_MEASURED because of the explicit freeze. The historical global FAILs remain active;
no 0.0.4 tag or global PASS.

d1dbd02 installed with backup and consistent hashes, report
`evidence/0.0.4/installed-checkpoint-1793074346922882547.json`:
files e40bd5c13646007e44874b4eb27dd84ab1e94ee07b975486d427d32122dae576;
helper fcf3a913c39b5173137a904d435a166bb7ae3c39052c17e382905786e3ca9fa7;
HTML a5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.

M4 follow-up: DOCUMENT-MEDIA adds hidden candidate→watch/final probe→Ready,
invalidation/reconcile of intents and resources, root recovery and UI tests.
The post-publication pending item described above belongs to checkpoint d1dbd02;
see DOCUMENT-MEDIA for current coverage, limits and failures that are still open.
