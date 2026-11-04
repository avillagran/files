# SFTP controls

## Current integration — 2026-10-31

Acciones → SFTP · montaje and command catalog `sftp` now use the actual core
Connection/registry/recovered controller through one lazy fixed transport worker.
Capability discovery remains on AuxIo; potentially blocking remote serving reads
cannot stall preference saves on that worker. No per-gesture threads or Files daemon.
Requests and responses each have capacity one; one operation in flight. Normal
polling is at most once per500ms after ACK, including while the form is hidden.
This is a local scheduling bound, not measured CPU/memory or a kernel I/O deadline.

Enter validates immutable Target+probe; CtrlEnter explicitly connects. AltO opens,
AltD disconnects, AltR reconciles and AltS retries sync only. Every operation has a
Nerd button, including Sync. Fields retain Ctrl1–4/AltUpDown/editor semantics.
Closing the form clears its draft, not the transport. Late replies update transport
accounting; opening a browser from a reply additionally requires the original form
stamp and no application close. Inspection creates no mount directory or writer
lease held between actions. A required missing sshfs is visible and Connect performs
no registry/filesystem mutation in that case. No package is installed automatically.

Connect publishes Prepared before spawn and immediately accounts PID/start before
remote proof. A postspawn publication failure retains the Connection and blocks
further side effects; polling still reaps its own children without replaying the
failed write. Record/pinned mount identity remains available for explicit recovery.
A new connection can replace only a durably Absent transaction. One active registry
slot is supported, not a connection catalog. Failed/unknown PID gaps remain explicit.

Explicit Open of a recovered mount requires current record/root/namespace/process,
a fresh remote directory FD with statx mount-id/device and bounded getdents, and a
second matching record/namespace/process check. It does not adopt/wait/signal the old
server or change its durable transaction. The read may block off-UI; no deadline is
claimed. Passive metadata observation never grants Open by itself.

Window close revokes future admission immediately, then awaits outstanding mutating
request ACKs. A queued/active read-only request may remain pending; close permission
is not a fabricated drain ACK or a joined blocked syscall. A registered SSHFS server
is intentionally not killed or unmounted on Drop. Ordinary disconnect only, durable
intent/PID/ACK/reap and namespace reconciliation retain the existing core guards.
Errors are bounded to4096 UTF8 bytes with explicit truncation. UI state labels are
Spanish; exact backend phases remain in the private test observations.

Functional evidence and scope are recorded in `evidence/sftp-native/ui-validation/`.
Native UI extension exercises fresh Connect/Open/Disconnect and recovered busy,
close-retained, retry/relaunch with actual SSHFS, production handlers and framebuffer;
no OS input, physical focus/clipboard, resource/startup or general SFTP acceptance.
The private fresh-connect wrapper adds only fixture `-F` authentication to actual
SSHFS; it is not a shipped product override or adopted dependency. Default staged
copy remains FAIL on this SSHFS; the visible-exclusive adapter still needs an
explicit UI review policy. No fallback or source deletion is inferred.

## Historical preparation-only checkpoint

The following records the previously installed preparation UI. Its absence claims
apply only to that old checkpoint; evidence and original failures remain intact.


Acciones → SFTP · preparación, or command catalog `sftp`, opens a single modal.
This is a capability diagnostic and target review, not N01 completion. There is
no Connect/mount dispatch button in this checkpoint. The installed transport must
not be inferred from a valid destination or present executables.

## Editor

Four fields: SSH host/alias, optional user, optional port, remote directory.
Ctrl1–4 focus fields; AltUp/Down changes field; mouse click/drag selects bytes at
UTF8 scalar boundaries using cached line geometry. Existing cursor/word motion,
Home/End/Shift selection, CtrlA/C/X/Z/Y and bounded text-paste gate are reused.
There is no new custom Tab field router. Native focus/IME/clipboard-byte acceptance
is separate. Enter/Revisar validates via core Target and caches a presentation
source string. No shell/SSH process is called. F5 rechecks local capability;
Escape closes. Field edits invalidate review; a rejected edit preserves exact
draft/cursor/history/review. A new review can clear the error for unchanged text.

Caps253/64/5/4096B per field; excess/control input rejects before history clone.
No hidden trim/URL decode, no password field. Geometry and bounded undo states
are additional retained memory while the form is open, not PSS proof. Target
source obeys the existing4096B combined cap, IPv6 bracket and optional SSH config
inheritance contract. See SFTP-MOUNT-CONTRACT.md for accepted syntax/restrictions.
Closing drops drafts/history/geometry/review; no profiles/credentials are saved.

## Read-only capability lifecycle

One latest pending probe and at most one admitted probe on existing AuxIo. No new
worker or per-navigation thread. Recheck/close/reopen have independent monotonic
IDs; stale replies release only their own active slot and never overwrite a newer
form. Close clears future pending admission; an active read-only probe keeps its
identity until response. This is not an acknowledgement that a blocked syscall
was drained. There is no UI join. Shared I/O can still stall on offline PATH
metadata and delay unrelated queued I/O; no real kernelD/deadline guarantee.

Linux lookup uses bounded PATH<=4096B/32 entries, absolute directories only,
regular executable metadata/access checks and /dev/fuse R/W access. No binary
execution/version probe/device open/network/authentication/mount. Paths may
change after this observation; future transport must revalidate invocation.
Other platforms report unavailable, rather than pretending Linux code was tested.
Tools present is not FUSE success. Missing sshfs is visible on current host.

The modal blocks browser selection/file actions/native transfer/window gestures
using existing guards. Text paste has a distinct SFTP edit-epoch target plus
owner/generation, protecting against late reply after cursor/field/reopen intent.
Capability replies are independent of browser row context: this form creates no
operation over the browser's captured selection. Native and painted Actions menus
and the command catalog route to the same Action. Search remains the catalog's
default first item. F1 advertises preparation only, not connection/mount support.

## Scoped verification — 2026-10-31

Final workspace873Rust278core/85helper/510UI PASS(+3existingignored) in
target/sftp-form-cas-workspace-tests.log. Earlier final-workspace log is retained. Original workspace had one help guard
failure: it prohibited any SFTP label. Revised guard permits only Preparation
and still rejects Connect/Mount advertisements. Original Clippy deprecated
fetch_update FAIL retained; switched to existing CAS loop without suppression.
Final workspace after CAS change, release, Clippy
workspace/alltargets-Dwarnings/fmt PASS in sftp-form-final-clippy/cas-fmt.log
and sftp-form-release.log.

`evidence/sftp-preparation/sftp-preparation-ui-1793440044492647516.json`
scoped PASS, mount_gate NOT_IMPLEMENTED: five real FLTK PNGs1000×650/440², all
five inspected. Actual shared AuxIo metadata/access probe reports missing sshfs;
empty host/invalid65536 port reject; alias/user/port/Unicode-percent-shell literal
path reviewed, middle edit/undo, stale text callback across field and reopen
rejected, old form stamp ignored, F5, Actions/catalog access, reopened drafts empty.
Source bytes/names unchanged; normal root0 after mutation drainACK. This does not
prove drain of a blocked read-only capability probe, helper PID lifetimes, OS
clipboard contents or input. No new SFTP child is implemented/requested.

Source `86bf3ff` installed through
`evidence/0.0.4/installed-checkpoint-1793440285730101786.json`.
Five installed artifacts and five backups independently SHA256 rehashed exact.
App `5cb60e564cc3f2e9347a6685819b87d303ca758f8a993f718a5457b4c8ebf72a`,
helper `d6bc5579a3072be862d3f62c3ebf88e6ac29ca85c25b520faf071fa2256ebe82`,
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install of preparation only; no source push/tag/global config
or package changes, no window control. Native SFTP mount still not implemented.

No SSH/FUSE/auth/keys/known_hosts/config changes, no mount/recovery/offline/native
input/resources/startup/global acceptance. Freeze remains, previews paused.
Next: actual owned mount worker/lifecycle/private FUSE fixture and dependency
availability, keeping the full SFTP scope and ROADMAP open.

Validation:919Rust318core/85helper/516UI(+3existingignored), Clippyworkspace/alltargets-Dwarnings, release/fmt PASS. Native `sftp-loopback-1793455941407692094.json` has UI+freshConnect+mount PASS with8PNG1000×650/3root0;25loggedPID/start identities gone,0readerrors/survivors, not lifetimecoverage. Fixture parent reaped the old SSHFS, product only its own children. DefaultcopyFAIL/overallFAILexit1 are unchanged. Initial recovered UI mismatch1793455368964965907 and its sidecar preserve wrong Registered-vs-Retained expectation; no broadening to arbitrary states. Final negative-tool/440² scene1793455971563736333 has6PNG, no registry creation/source mutation/root0. No native input, clipboard or PSS/Swap/CPU/startup measurements. Long diagnostic lines can still clip; a full copyable transport report remains pending.

Final lifecycle follow-up:921Rust319core/85helper/517UI(+3ignored) and Clippy PASS. Closing increments a shared admission epoch; cancelling close resumes only NEW epoch requests, never revives a queued revoked command. Added deterministic test. Registry Lease now explicitly unlocks only in its acquiring PID, including construction-error paths: deterministic forked-FD test proves child cannot unlock parent, and owner Drop allows a new opener while child retains inherited FD. The prior concurrent new_owner.writable() failure remains in ui-validation/sftp-executor-close-epoch-tests.log; which child caused that historical failure is UNKNOWN. No serialization/relaxed assertions/rerun-only resolution. Current evidence references are listed in ui-validation/checks.json.

Final release UI acceptance: native `sftp-loopback-1793456384121839709.json`, product controls/freshConnect/mount PASS8PNG/3root0; negative-capability and440² `sftp-preparation-ui-1793456409825960118.json` PASS6PNG/root0.25loggedPID/start identities gone/0errors/survivors; not whole lifetimes. Native overall/defaultcopy FAIL remain. Source/log/binary hashes and scopes in ui-validation/checks.json; supersedes previous display-only scene1793455941407692094 for installation. All original reports retained.

Installed source1754f83 through `evidence/0.0.4/installed-checkpoint-1793456472288022118.json`;5installed+5backups independently rehashed exact. Appab1d438d58784c133eeb7bb7c30641bf75c5c0012996121b012a7fc34ef4d911/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. No sshfs global installation/config/source publication/tag; host capability still reports missing sshfs.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Opciones in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925Rust319/85/521+3existingignored, Clippyworkspace/alltargets-Dwarnings/release/fmt
PASS. Native1793457917645506527 UI/copy/mount PASS16framebuffers incl440² and
1000×560;4copiedroots(raw0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expectedFAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves57partial
files and original3000files intact.24loggedPID/start identities gone/0errors/survivors,
not lifetimes;3roots exit0 after normal callback. Native defaultcopy/overallFAIL
exit1 retained. Final copy-names UI1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visualFAIL preserved+sidecar;
explicit bounded lines fixed and finalframebuffer inspected. No source delete,
automatic removal of visible partials, powerloss/nativeinput/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.

### 2026-10-31 — Connection profiles

The earlier no-profiles statements describe the preparation checkpoint.
SFTP-PROFILES.md now documents the local connection-draft catalogue and current
validation status. Mount registry remains one slot; profile metadata does not
authorize connection, probe, navigation, mount recovery or credentials.

### 2026-10-31 — Readable captured report

SFTP-REPORT.md supersedes the one-line-only diagnostic limitation: F4/Nerd opens a
fixed bounded readable capture, measured word wrap/pages/keyboard/wheel and copy
request. F5 captures only current held observations; no transport/FS probe. Esc
preserves the form/Review. OS clipboard delivery remains UNVALIDATED.
