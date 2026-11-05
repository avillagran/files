# Local properties and permissions

Partial U04 block: Actions → Properties or Alt+Enter opens a selected object.
It does not interpret the path as a command nor read the content. It shows type, apparent
size, allocated blocks (`st_blocks * 512`, not exclusive physical use), the number
of hardlinks, UID/GID, mode, dev/inode identity and the access,
modification and inode-change timestamps. **ctime is not called a creation date**. A
directory's size is not its recursive size. Dates are shown as Unix seconds and
nanoseconds in this first dialog. The requested path is historical; the
verified path is resolved in the worker and may differ after the parent is renamed.
Links show their escaped raw target without following or opening it.

The form lets you prepare the r/w/x bits for owner, group and others;
Apply permissions explicitly confirms the non-recursive change. Linux only, with
`fchmodat2(AT_EMPTY_PATH)` available, own files/directories, without existing special
bits. Other objects, symlinks, different owners and
setuid/setgid/sticky stay read-only. No chown, escalation, ACL, xattrs or
recursion. Changing an inode affects all its hardlinks; the dialog warns about it.
This unit offers no permission Undo: a confirmed change replaces the
previous history so an Undo of another kind is not presented as if it reverted chmod.
Reading properties, or failing before the change, keeps the previous history.

## Identity, cancellation and limits

Capture and change run on the existing mutation worker: no other thread,
helper or daemon. A snapshot keeps two FDs (parent and object `O_PATH|O_NOFOLLOW`).
The final symlink is pinned as the object and read with `readlinkat` on that FD;
raw reads and paths are limited to 4096 B. The change uses the pinned FD with
`fchmodat2`, without reopening by path nor a symlink-following fallback. If the kernel/FS does not
support it, the capability is reported as unavailable. The `/` path and multi-selection
properties are not implemented in this block.

The object/entry identity, UID, GID and the observed mode are checked right before
confirming. An external substitution or change requires Reload, keeps the actor
and does not apply the prepared mode. An own change followed by a valid observation
updates the snapshot for the next edit. There is no conditional syscall
that ties entry identity to chmod: an actor with the same UID can move the
inode after the check. The FD never switches to its replacement. No content
snapshot, hostile isolation or atomicity against a crash is claimed.

Cancellation is checked before the syscall; it does not interrupt it. A confirmed
commit is still a success even if the later diagnostic fails: a warning is reported,
a new edit without a valid read is disabled and a retry is not offered
as if it had not happened. Messages are at most 1024 B and
eight warnings, and property snapshots/history do not pile up. Closing
during a change cancels and waits for the ACK before exiting; Drop does not replace the drain.
Reads or syscalls stuck on mounts still have no effective deadline and are not
presented as recovered offline.

The controls are own Painter controls, with keyboard focus and panel scroll; no
native toolkit dialogs are opened. The requested/verified paths and the raw link target are laid out in
measured lines and can be walked with the wheel, arrows, Home/End and PageUp/PageDown.
The full escaped text is kept; other metadata may still be clipped.
Select/copy/full export of the properties report is pending. AT-SPI accessibility and physical input still have no compatible evidence.

## Verification

341 Rust tests (115 core/37 helper/189 UI), Clippy workspace/all-targets
`-D warnings` and release PASS. `properties-ui-1793049068223979012.json` functional
PASS in two processes: real read, chmod 0600→0640 with bytes intact, an external
change to 0600 rejected while keeping the 0644 request, Reload, a read-only symlink
without modifying its target, a directory and normal close 0 after ACK. The second close
was requested right after enqueuing Apply and cancelled before the commit;
it does not test cancellation inside the syscall. Method: visible target handlers,
real FLTK framebuffer and modes/bytes of own fixtures; no OS events.

App SHA256 `3b40ad35bc1aaad3bbc86764d7b00aa456d8c8edf0e686e760729320ebba6554`.
The failed scenes 1793048241761101633 and 1793048903461152694 are preserved:
a short tiled window left a clipped control without a hit (correct); afterwards a
READY marker earlier than the repaint was identified. The fixture now uses the same
Tab/reveal as the form and waits for a drawing revision after the command.
The earlier success toast is also hidden while a form is open and cleared
before a new change, so it does not contradict the current rejection. Logs of the initial and final
build/Clippy failures are kept in evidence/properties.

No chown and no changes to personal data. Resources/startup NOT_MEASURED because of the
explicit freeze; FD/byte bounds do not prove a process budget. U04 is still
partial (multi-selection, copy report, ACL/xattrs/links, other OSes).


## NODE: full paths and minimum window (2026-11-01)

The panel reserves the global footer. The nine checkboxes and the three actions fit
the available width at 440×440. Tab reveals the focused permission; Home/End and the
page keys walk the content. Paths use the common escaped labels
without rebuilding targets from the text. The layout uses real widths and
UTF-8 scalar boundaries, with a cache per width/path/target. It does not promise grapheme
segmentation, native input, AT-SPI or 2× sharpness.

The snapshots and the per-descriptor authorization do not change. Renaming the object
correctly invalidates the capture; moving its parent directory allows resolving the new
verified path. The first test mixed up both cases and failed; the failure
is kept and the fixture was fixed, without relaxing the backend validation.


NODE validation: 1214 Rust tests (425 core/95 helper/694 UI), three historical
ignored; Clippy workspace/all-targets -D warnings, format and release PASS.
X11 `1793529310535401451` and Wayland `1793529319603368419` flows: 31 captures
each, three normal exits and seven registered helpers terminated/reused.
The long fixture contains invalid bytes and control characters: full text in the cache,
PageDown/Home, nine targets, three buttons, chmod 0600→0640 on the same inode,
a read-only symlink and the content preserved. Properties regression
`1793529331818288088` PASS: read, rejection after an external change, Reload,
directory and close during a request. The intermediate IN_PROGRESS receipt is kept.
App SHA256 `ced5d60bdb1386d3e6927d7a1f00588f036a648e6bffaa858e2f4f7d4e084dc7`.
These are fixtures/handlers and framebuffer, with no test of OS input, clipboard or AT-SPI;
PSS/Swap/CPU/startup not measured. The historical X11 anomaly still has no confirmed cause.


Installed checkpoint source `d2dfee1`, receipt `1793529424770438069`: five
installed hashes and five backups verified. Installed Wayland repeat
`1793529436364969476` PASS: 31 captures/3 exits 0/6 duplicate helpers and 1 search
helper terminated or identity reused, no zombie. Fixture permissions and
original bytes verified. Helper SHA/HTML keep their earlier hashes.
