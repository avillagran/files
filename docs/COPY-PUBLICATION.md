# Explicit copy publication review

Copies keep their default publication: each root remains in staging until its
successful no-clobber rename. A backend rejecting that operation produces an error;
it never triggers another publication method automatically.

F3 / **Opciones** inside F5/copy or Copy/Paste review opens a separate options view.
Two independent, per-dialog choices:

- **Destino visible durante la copia** (1): exclusive final names are created from
  the start. Cancel/error retains partial destinations, without rollback/delete.
  This is the existing `VisibleExclusive` backend, not a second copy engine.
- **Conservar fechas de enlaces** (2): on by default. Explicitly switching it off
  omits symlink atime/mtime; regular files/directories keep their requested times.
  Visible mode rejects preserved symlink times rather than silently omitting them.

A no-follow symlink timestamp setter is not validated on FUSE (known SSHFS can
follow the target despite AT_SYMLINK_NOFOLLOW). The core now checks fstatfs on the
actual pinned destination parent before creating a symlink in staged mode: FUSE
with requested symlink timestamps rejects, never calls that unsafe setter. This
conservatively covers unvalidated FUSE backends. No new allocation/parser/capability
probe is introduced. Explicit omission allows the copy without touching a link's
existing target. It is not universal FUSE metadata support.

F3/Enter/Esc returns from options to review, not to the queue. Publication and
omission remain visible in review; only **Encolar copia [visible]** submits the
captured source/destination/name plan and owned CopyOptions. Moves have neither
these controls nor these policies; EXDEV delete authority is unchanged. Every new
copy dialog resets defaults; policy does not persist across jobs. Queue rejection
retains the dialog options. The same controls cover clipboard Copy (not Cut).

Opening options revokes old text receipts and touches destination/name epochs.
Text/pointer editing is blocked there; an old paste cannot become current after
return. Direct submit while options are open rejects without mutation. Choices
inside a new dialog cannot modify options already owned by a queued request.

Reports reuse existing bounded visible-started/completed/uncertain, omission and
verified rescue details. A partial directory/file is not a completed root. No
silent skip/overwrite/fallback, source delete, automatic cleanup of visible partials,
atomic whole-batch claim, crash journal or complete remote-filesystem support.

## Validation

Current release evidence/log/binary hashes and preserved failures are in
`evidence/sftp-native/copy-policy-validation/checks.json`. The optional existing
loopback scene `--ui-control --ui-copy` uses the real product form/options/queue
on a real SSHFS mount with fixture-only PATH authentication wrapper. Includes
expected staged failure, FUSE link-date rejection, visible link-date rejection
without omission, reviewed/paused no mutation,4 roots/raw filename/perms/mtime,
unchanged outside target, occupied destination, active cancel with retained partial
and original3000-file source, existing SFTP disconnect/close lifecycle.

It uses injected painted handlers and FLTK framebuffer, not native OS input.
Memory/CPU/startup remain NOT_MEASURED under freeze, footer incidental is not a
resource gate. The harness overall/default staged-copy FAIL is preserved even
when the explicit adapter/UI gates PASS. N01/full ROADMAP/global budgets remain
open. New previews remain paused.

Installed source `e58f576`, report `evidence/0.0.4/installed-checkpoint-1793458216633312563.json`;5installed+5backups independently SHA256 exact. Appfee00a4d1260bdd2163dd84798eafc6e1d3f3626d3de7feb86a812ecd53e611c/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Fixture rwx+mtime equality checked specifically a.txt with whole-second time; byte hashes also cover tree child/raw file, literal symlink payload and outside-target mtime. No nanosecond/ACL/xattr/ownership fidelity claim. No source push/tag/global sshfs install.
