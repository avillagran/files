# Multi-page PDF (0.0.4 partial)

Space renders pages through the OS's optional pdfinfo/pdftoppm, without
linking/distributing the reader's libraries nor a new crate. Controls
Previous/Next, ←/→ and PageUp/PageDown, Home/End; Ctrl+wheel or +/- zoom,
drag and 0 to fit. Only one RGB page or extracted text and one visible viewport, with no history cache.

Linux opens in the worker a final FD O_NOFOLLOW|NONBLOCK, regular, ≤256 MiB, and shares that
inode with both direct, sequential processes; render is only launched after the info
process is reaped. No shell/child helper/grandchildren. Changing page cancels and drains
before the last request; closing keeps the main process until ACK. In-place
changes are not isolated; parents/mounts follow OS navigation, not a sandbox.

Metadata 64 KiB, 1..100000 pages, C locale. pdftoppm rasterizes a single page with
-scale-to 960. PPM output limited to 960²×3+4096 B before storing; P6 header/dimensions/
maxval 255 and exact payload/EOF before exposing the bitmap. The same Vec is moved after
drain(header), with no second RGB. The common supervisor only understands a one-line JSON
header: the PPM is validated at the end inside the hard wire limit, not before keeping the snapshot.
Nothing is allocated from unvalidated dimensions. Selection on the original PDF geometry is pending; the extracted-text mode is described in M14.

CPU 3 s/process, wall 5 s, AS 96 MiB virtual, bounded FDs, core 0, OMP/OPENBLAS 1. This is NOT
PSS nor does it allow exceeding the 150 MB ceiling. Complex/protected PDFs may fail; the Poppler
parser is not a filesystem/network sandbox. The host does not invoke JavaScript/URLs/actions
nor open external resources requested by the UI; PDF vulnerabilities/references
need additional containment to handle arbitrary hostile content.

`python3 scripts/pdf-ui-check.py` creates a synthetic 2-page red/blue PDF with
text, verifies RGB and count/page/revision, zoom/drag, quick last destination,
malformed/symlink, close 0 ACK. Handlers + real framebuffer, no OS events. Resources
and startup NOT_MEASURED because of the user's phase; selection/text/cache, complex/
password documents/other platforms and the tree budget are pending.

Current verification: 266 Rust (73/33/160), Clippy/release PASS. Scene pdf-ui-
1793038247910320945 functional PASS with RGB/revision and pan Y 60 after 2× zoom. First
attempt 1793038113834599772 FAIL because the instrumental assert-pan handler was missing;
it is kept. Close ACK before the new spawn does not prove killing a running child.


## Source review and publication — 2026-10-27 (M12)

A selected PDF joins the same media watcher/token/epoch/1 s reconcile;
no per-page watcher nor I/O in paint/UI is added. A stable probe authorizes a
mandatory Revision before the render worker's open. The real FD must match
it and be a non-empty regular file ≤256 MiB; pdfinfo and pdftoppm inherit that same FD.
FD+name are revalidated after metadata and after the PPM decode. The bitmap carries
the effective signature; State only publishes with the expected revision/generation. Changing
the source resets to page 1 and the count, cancels and frees the previous RGB. Modifying,
replacing or deleting hides the old pixels and requires a new verification.

Source failures have a typed flag: invalidation before the ACK and a new token,
not a decision based on Poppler strings. O_NOFOLLOW only on the final component;
parents/mounts follow OS semantics, not confinement/snapshot. Changes after
the last stat are detected through an event or reconciliation, not guaranteed to be instant.

readonly_gate extracts the contract already used by audio. Helper requires reap before
ACK/exit; close during ReadOnly forbids future helpers and allows exiting without a
join on a syscall that cannot be cancelled. That authorization is NOT worker DRAINED:
State keeps exit_allowed/is_drained separate. It does not test real kernel D-state nor
interrupting an offline FS. No new thread when navigating until the previous one is drained.

473 Rust (141 core/58 helper/274 UI), Clippy workspace/all-targets -Dwarnings and release
PASS. pdf-ui-1793092913810767063 functional PASS with --source-lifecycle --drop-events:
8 framebuffers 1000×650, real RGB and effective signature; replacement with the same size+mtime,
in-place modification restoring mtime, delete/recreate, renaming file/parent,
recovery, hidden without a new helper, quick last destination, errors and close 0 ACK.
23 registered PID/start gone/reused, Z not accepted; no lifetime coverage nor a live helper
at close shown. Deterministic tests Verified A/open B and late Ready A/Dirty/B.
Internal handlers/real framebuffer, no OS input nor native 2x sharpness. Helper with
no SHA change. Resources/startup NOT_MEASURED because of the freeze; no values taken from the
footer. Text/selection, hostile parser/other OSes/global LRU are still pending.


## On-demand extracted text — 2026-10-27 (M14)

The Nerd Text/Page button and T toggle between the rendered page and the text extracted from
THAT page. Previous/Next and Ctrl+PgUp/PgDn change page in both modes;
in text, the arrows/Shift/Home/End/Ctrl+A/C reuse TextDocument's selection and scroll.
The text is explicitly labeled as extracted, not a selection
on the original layout nor OCR. Scanned pages may be empty: a
notice is shown, without inserting fake content into the document/clipboard.

It reuses the PDF Job and the expected/effective FD. In text mode it runs pdfinfo and pdftotext
sequentially, does NOT rasterize nor keep a simultaneous bitmap. FD input and stdout output,
`-f/-l` one page, UTF-8/layout/nopgbrk; no temporary/cache files next to the source.
Changing mode/page cancels and waits for the ACK before the last request. SourceDirty frees the
text, resets to Page mode and requires verification. Same readonly/helper gate and
parent-death of the existing supervisor, with no other engine/daemon/Cargo dependency.

Output ≤256 KiB before keeping the wire, ≤2000 LF (2001 lines with the last one empty), strict
UTF-8 and no NUL; the String takes the same Vec, TextDocument owns it without cloning.
Excess is visibly REJECTED, not silent partial text. If pdftotext is missing, the
capability is shown as unavailable and Page lets you go back to the raster. Local limits/AS 96 MiB
are NOT total PSS nor quality/compatibility for arbitrary documents.

The size gate is also fixed: a stable empty/>256 MiB PDF or HTML >32 KiB
is a terminal error for that revision, not SourceUnavailable that would re-arm the token
and retry in a loop. A real replacement still invalidates. Tests without source I/O
check rejection before open/child and that the reprobe flag stays intact.

480 Rust (141 core/58 helper/281 UI), Clippy workspace/all-targets -Dwarnings/release PASS.
pdf-ui 1793094189643854652 functional PASS 12 FB 1000×650/notify suppressed: Text modes
pages 2/1, FD signature, pointer selection equal to the extracted ASCII text, Copy handler
invoked and return to Page; PDF/HTML source caps keep the revision with no child during
a reconciliation interval. It repeats the scoped M12 lifecycle/raster, errors,
rapid/hidden/close 0 ACK and 29 logged PID/start gone/reused strict Z false. No lifetime
coverage/live helper at close/OS input/clipboard/Unicode PDF font coverage/native 2x sharpness.
UTF-8+TAB+CRLF and limits are covered in pure tests; that is not equivalent to a real Unicode PDF.
App signature 786c52d7c9afef200ad7c258514af96a531b4cf805be4e2b66f67614aad6be92.
Resources/startup NOT_MEASURED because of the freeze; P06 partial, no 0.0.4 tag/global PASS.

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
