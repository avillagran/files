# SFTP as an OS mount — work in progress

N01 product mount integration remains incomplete. The core process engine is now
lab-tested with the durable registry and recovered disconnect controller below;
shared executor and UI wiring remain pending. Preparation is
covered separately in SFTP-PREPARATION.md; it reviews syntax and local capability
without connecting. This document separates completed pure contracts from the
still-required process/FUSE work. No SFTP mount feature PASS.

## Current host capacity — 2026-10-31

Read-only lookup: ssh/sftp/fusermount3/sshd present; sshfs absent. /dev/fuse exists
and access(R_OK|W_OK) succeeds. This is not a successful device-open/FUSE mount
proof. No package installed, server started, connection attempted, key created,
known_hosts modified or global configuration changed. Capacity evidence is
`evidence/sftp-foundation/host-capacity.json`; not a resource/startup measurement.

Upstream documents SSHFS as SFTP-backed filesystem, the source argument syntax,
foreground `-f` and Linux unmount with fusermount3:
[SSHFS manual](https://github.com/libfuse/sshfs/blob/master/sshfs.rst),
[README](https://github.com/libfuse/sshfs/blob/master/README.md).
No source from SSHFS or new Cargo/native package is incorporated here.

## Implemented pure target contract

core/sftp_target.rs validates borrowed fields before materializing owned strings.
Host is a bounded ASCII SSH alias/name (253B) or bracketed numeric IPv6. User is
optional ASCII token<=64B; port optional decimal1–65535. Empty user/port inherit
OpenSSH configuration instead of overriding it with guessed user/port22. Empty
remote directory means remote home. Directory is UTF8, no controls; entire source
argument<=4096B. No silent trim/URL/percent decoding. IPv6 zones, IDNA/Unicode
aliases/users, arbitrary options, passwords/URI credentials and remote raw-text
entry are not supported by this initial form contract. Mounted raw filenames
must later remain usable through existing raw path filesystem APIs.

`foreground_args` is a lab/prototype argv builder, NOT the product mount worker.
It returns separate OsStrings: fixed -f, BatchMode/strict host key/no password
prompts/connect/keepalive options, optional numeric-p, one remote source, one
absolute local mountpoint<=4096B/NUL rejected. No shell, custom ssh_command,
password_stdin, allow_other, reconnect, host-key bypass or credential file.
Mountpoint ownership/emptiness/identity is expressly NOT established by this API.
Literal-argument tests do not prove how an actual SSHFS/server version interprets
all remote path bytes. That needs real isolated SFTP/FUSE tests.

## Implemented pure mount-table observation

core/mount_table.rs is Linux-only. Caller supplies bytes; there is no file read,
stat or mount operation in the parser. Limits1MiB input/4096records/16KiBline/
64fields/4096B decoded path-source/64B filesystem; unknown/truncated escapes,
empty table, invalid numeric fields, missing separators, duplicate IDs or incomplete trailing
record reject the entire table. Linux space/tab/newline/backslash escapes decode
once, preserving raw bytes. No silent partial table or unreadable=>empty success.

Unique SSHFS lookup requires exactly one observed record at the mountpoint and
exact filesystem/source marker. Stacked mounts or source/type substitution reject.
Returned id/parent/device/root/path/source form an observation, NOT mount ownership
and NOT an atomic right to unmount a pathname later. A hostile same-user writer
can race separate checks and unmount; do not claim otherwise. Future worker must
combine private mountpoint provenance, expected identity and fresh observations.

## Required worker/UI lifecycle — not implemented yet

- Capability/read-only checks run through existing off-UI I/O. Present missing
  capability honestly. No auto-install, hidden password prompt or config rewrite.
- User reviews host/user/port/directory, then explicitly connects. Private local
  mountpoints are created exclusively in an owned0700 parent; no user-chosen
  pre-existing occupied path is silently adopted. One mutation in flight.
- States: Checking/Mounting/Verifying/Ready/Disconnecting/Uncertain/Error, each
  tied to request identity and generation. Spawn/exit0 is not Ready: verify real
  matching mountinfo. Late success after cancellation must be reconciled/recovered,
  never dropped as though no mount could remain. No new admission until accounted.
- Product intent is a native OS mount usable by other apps. Closing a browser
  window should not silently kill that filesystem. Persistent system SSHFS mount
  lifecycle and daemon PID/start/resource association must be designed and tested
  before shipping this behavior; foreground prototype is not that solution.
  No new Files background daemon or root service is authorized by this contract.
- Disconnect is explicit, exact-mount only, ordinary unmount without lazy/force or
  all-mount operations. Busy/error leaves a visible retained connection. Re-read
  identity immediately before the operation, report uncertainty conservatively;
  do not claim inode-conditional atomic unmount. Never remove mountpoint storage
  while its mount identity or contents are uncertain.
- Existing navigation, listing, selection, copy/move queues and raw filesystem
  paths should consume the resulting local path; no second virtual file browser.
  Actual SSHFS no-clobber/rename/FUSE/error/offline semantics need a real operation
  matrix. Do not infer local transactional guarantees from generic POSIX names.
- PID-tree accounting must include explicitly associated SSHFS/SSH daemons even
  if reparented outside Files. Existing ideal50MB/hard150MB is not enlarged or
  evaded by classifying those processes as unrelated. Freeze still prevents new
  PSS/Swap/CPU/startup experiments; no resource/global acceptance in this unit.

## Pure foundation verification (before preparation UI)

Six target tests PASS plus three mount-table fixtures. Full core suite278PASS,
3existingignored: target/sftp-core-final3-tests.log; earlier target-only6PASS at
sftp-target-tests.log. Core all-targets Clippy-Dwarnings/fmt PASS; logs
sftp-foundation-clippy.log and sftp-foundation-fmt.log. Read-only real `/proc/self/mountinfo` probe parsed31 records successfully;
`target/sftp-mount-table-probe.log`. No path/source list emitted, no mount changed.
No full workspace/release,
GUI, subprocess, protocol, FUSE, authentication, offline, resource or installation
acceptance. No new screenshot: previous checkpoint images are not SFTP evidence.

Capability diagnostics/review form are now implemented in SFTP-PREPARATION.md.
Next: mount worker/provenance/state transitions and a private real SSH/SFTP/FUSE
fixture after sshfs dependency availability is resolved. N01 and the
full ROADMAP remain open; new previews stay paused.

## Real native laboratory follow-up

See SFTP-NATIVE-MATRIX.md: private OpenSSH/SFTP/FUSE mount/read/write/busy/unmount
PASS, but current Files core copy FAIL errno22; direct renameNOREPLACE unsupported
and exclusive hardlink aliases have distinct virtual inodes. Native mount proof
does not authorize GUI activation or weakening publication checks. No new product
worker/installation. Explicit remote VisibleExclusive adapter now has separate
native PASS for tree/raw/link/collision, with symlink timestamps omitted explicitly;
default copy/overall compatibility FAIL remains. See the matrix for limits.
Next is product mount lifecycle and explicit policy selection, not N01 closure.

## Transaction controller foundation (2026-10-31, not a process worker)

`sftp_lifecycle::Controller` now owns one immutable Target/point/source-marker
intent and opaque monotonic ticket. Spawn must be admitted before the worker
creates a process. Cancellation prevents future admission; late matching mount
observations are retained, not dropped. Foreground termination requires reconcile
and retained recovery if a mount remains. Exit status alone is never Ready or
Absent. Read/parse errors stay Uncertain; a fresh complete absence observation and
no active spawn/unmount are required before admitting the next transaction.

Explicit disconnect requires a fresh exact previously recorded Mount. Busy/error
retains it. Successful unmount still requires absence proof; a changed mount
identity blocks disconnect. A matching marker without an admitted spawn is never
adopted. This is pure state logic, not mountpoint provenance or syscall authority;
caller must pin/create/revalidate owned storage and serialize admission/spawn.
No worker, persistence across window close, daemon accounting or Connect UI yet.

Preparation Review now retains the full parsed Target (including numeric port)
and capability probe epoch. Recheck revokes the review. The drawn summary includes
the chosen port or SSH-config inheritance. It remains preparation, not a connection.

Validation logs: sftp-lifecycle-workspace-tests.log 885 Rust PASS
(289core/85helper/511UI), three existing ignored. After the final foreground-exit
retention change, sftp-lifecycle-final-tests.log five controller tests PASS.
Preparation's four directed tests include port/probe retention and review
revocation. Clippy/fmt logs are under target/sftp-lifecycle-*. No new release,
GUI scene, installation, FUSE attempt or resources/startup measurement for this
foundation. Previous installed preparation and native adapter report unchanged.

## Mountpoint descriptor foundation (2026-10-31)

`sftp_mount_point` opens an existing private 0700/euid parent by walking absolute
components with openat/O_DIRECTORY/O_NOFOLLOW. It creates up to eight exclusive
128-bit random names, opens/pins the created directory and records acquired
parent/point dev+inode. No chmod of existing paths, adoption of a chosen occupied
mountpoint, recursive deletion or Drop cleanup. Failed post-mkdir verification
reports the retained path instead of deleting an identity it did not acquire.

Pre-spawn/after-unmount verification requires a caller-supplied fresh complete
mount table. Any mount at/below the point rejects before opening its pathname.
Otherwise revalidate parent mapping/private mode, named point versus held FD,
and bounded 4KiB getdents emptiness (at most four reads, no recursive stat).
While mounted, verify only parent mapping; exact fresh mount identity remains a
separate controller/worker requirement. The underlying held FD survives FUSE.

This does not eliminate hostile same-UID races between mkdir and first open,
path revalidation and a syscall, or an ancestor mount change. It is not an atomic
mount/unmount/cleanup authorization. Offline or replaced ancestor paths can still
block a worker syscall; no UI join/deadline claim. Restart provenance/journal,
actual mount worker and explicit retained-directory recovery are still pending.

Native probe report `evidence/sftp-native/sftp-loopback-1793444992430070322.json`:
core creates point/holds descriptors; empty before mount PASS; parent while
mounted PASS; mounted emptiness check rejects at mount-table gate; underlying
identity/empty after ordinary unmount PASS. Seven logged PID/start identities
cleaned up, zero read errors/survivors, mount absent. Separate visible-copy
adapter PASS; default staged copy FAIL errno22 and overallFAIL/exit1 retained.
Not a GUI lifecycle, hostile-writer, kernel-D or resource experiment.

## Actual process engine (2026-10-31, product UI still pending)

`sftp_connection::Connection` now combines Point/Controller with foreground SSHFS
and asynchronous ordinary fusermount3 -u children. It creates no thread or Files
daemon; caller runs it off-UI. Fixed Target argv, absolute tool paths, fsname from
the exclusively generated point, setsid, null stdin/stdout. No shell, password
prompt, host-key bypass, force/lazy unmount or process-group kill. Cancel before
verification can request TERM of the unreaped owned child; a verified mount is
retained instead. Drop neither kills SSHFS nor unmounts/deletes its filesystem.

Diagnostics use an anonymous 8KiB memfd sealed against grow/shrink, inherited as
stderr; no disk log or parent-dependent stderr pipe. Rendering is sanitized and
bounded4KiB with explicit truncation markers. At capacity, further writes can fail;
there is no guarantee about arbitrary external program policy after write errors.
The writer survives the reader's close; this is not measured process PSS/CPU.

A native negative test discovered that SSHFS may appear in mountinfo before auth
completes. Namespace presence alone previously published Ready and then a mounted
read failed ENOTCONN. FAIL1793445880981585872 is preserved. Ready now requires the
first remote directory FD: statx mount-id/device match, bounded4KiB getdents, then
fresh exact mount identity and a foreground status recheck. A failed initial
proof is memoized, not retried each poll. This is initial-serving proof, not
continuous remote-health or offline deadline proof. Blocking filesystem calls
remain off-UI and may be uninterruptible. A matching rejected/non-serving FUSE
mount can be explicitly disconnected after fresh identity/parent revalidation.

Product caller MUST durably register intent/point/process association and own the
cross-window lock before exposing this engine. The registry below now supplies storage/recovery observations; shared AuxIo
integration, recovered control actions, admission/cancel epochs, close ACK barrier,
UI controls and full resource association remain pending. No Connect button or install.
Debug-only start_lab adds an explicit private SSH config for the lab; it is absent
from the release API. The installed preparation remains unchanged.

## Durable registry foundation (2026-10-31; product executor/UI pending)

`sftp_registry::Store` anchors a single active transaction to the existing private
0700/euid root. Its regular0600/nlink1 lock uses nonblocking lifetime flock; a
second opener is read-only and never upgrades automatically. No takeover or
background daemon. All reads/writes are off-UI and descriptor-relative; every
publication revalidates parent mapping, lock inode and the exact observed file
revision+bytes. Symlinks, FIFOs, hardlinked locks, foreign/malformed/future or
oversized records are rejected/preserved, never silently replaced.

Version1 binary codec is at most16KiB, with complete framing/length/UTF8 validation
before payload materialization. It retains Target host/user/optional numeric port/
remote directory without URL decoding, raw local point path, acquired parent and
point dev+inode, phase/generation, server/unmount PID+start_ticks, and full recorded
mount signature. Each variable field is bounded4096B (host253/user64). One active
entry is an initial limit; a multi-connection catalog is not delivered here.

An exclusive0600 stage is synced before rename. First publication is no-clobber;
updates require the last exact preimage and same intent/generation. Active server
and mount identities cannot be erased/replaced until explicit reconciled Absent.
Directory-sync failure after publication is Unsynced, not an unpublished error;
retry only syncs, never rewrites the file. Unexpected post-rename identity/read
failure is Uncertain and blocks future writes. Failed stages are never deleted by filename inference; the requested name is
reported without claiming its current location/existence after an external race.
Advisory locks/preflight checks do not provide hostile same-UID race isolation.

Explicit Absent publication additionally reads a fresh complete own-namespace
mount table, verifies the original unmounted private point, and rejects matching
live or zombie server/unmount identities. PID reuse is distinguished from matching
live identity; inaccessible proc metadata is an error, not zero/missing. These
observations are scoped to the current Linux process/mount namespace, not cross-
namespace adoption or global cleanup authority. Caller must still serialize
admission and ensure no future spawn in a Prepared/PID-publication gap.

Recovery returns observations only: exact registered mount plus matching live
process, no mount with process status, or Uncertain. It never mounts/unmounts,
adopts, signals, deletes or auto-clears a historical phase. Initial-serving proof
is separate from a recovery metadata match. Reopening after an owner exit obtains
the available lock; the former read-only object still never upgrades itself.

Debug lab driver --registry requires durable Prepared before starting SSHFS,
records its PID/start and verified mount, and keeps the registry through owner
exit. Separate sftp-registry-probe only inspects. This is not yet the shared
AuxIo executor, actual-window close ACK barrier, recovered disconnect controls,
publication-gap crash/power-loss acceptance or product Connect activation.

Native current registry case1793450438576487018 PASS scoped: durable Prepared
before process start, PID/start+verified mount persisted; separate process while
owner alive read-only, after owner graceful exit writable with exact mount/process
match; a fresh server-side file still read through FUSE. Inspectors preserve file
hash/generation. After LAB ordinary unmount, historical Verified stays unchanged
while fresh observation reports NoMount+Missing. No automatic clear/adoption.
Seven logged identities gone, zero read errors/survivors, no lifetime coverage.
Earlier1793449119003791297 retains the initial successful prototype case.

908Rust312/85/511+3existingignored PASS, including13 new registry tests; Clippy
all-targets, release and finalfmt PASS. Directory-sync error is a deterministic
post-rename injected EIO hook, NOT a failed real fsync/power-loss experiment.
Original Clippy deprecation/style failures preserved; atomic counter uses checked
CAS instead of deprecated fetch_update. Final changes after full tests only
correct diagnostic/lab-help phrasing; final Clippy/release cover them. No GUI
scene/installation/resources/startup/inputSO acceptance; freeze/preview pause hold.

## Explicit recovered disconnect controller (2026-10-31)

`sftp_recovered::Controller` loads the existing registry and its lifetime lease.
Opening/polling an old transaction does not adopt a Child, signal/reap the original
SSHFS, read remote contents, repeat an unmount, delete or auto-clear its history.
Registered means namespace/process metadata match, not current serving health.
Read-only openers remain read-only even after the former writer disappears.

Explicit disconnect requires current exact record/preimage, private parent and
full fresh registered mount signature. It rejects a previous live/zombie unmount
or a prior Disconnecting entry with no PID/ACK (publication gap). A durable
Disconnecting intent precedes child admission; namespace and record are checked
again after sync. Only ordinary fusermount3 -u is spawned, with existing setsid/
sealed8KiB diagnostic helper. No force/lazy fallback or original-process signal.
StartedUncertain distinguishes post-spawn PID/registry errors from NotStarted;
the owned Child is kept/reaped either way and further admission is blocked.

Busy/nonzero exit retains mount/server identities and bounded diagnostic. Explicit
retry uses the same recorded transaction after fresh checks. Absent is published
only after owned unmount reap, fresh namespace absence and original server gone/
reused; zombie/read-error is not absence. Recovered controller never waits the
original server, which is not its Child: OS reaper must account for it. Sync-only
retry never automatically repeats a side effect. Same-owner known-unstarted
intent is distinct from a new process's unknown PID gap. Reconcile is explicit,
and a repeated confirmed-closed reconcile does not rewrite/grow generation.

Native final1793453782020109493 (first3058010388848 preserved) passed: second
controller while original owner alive denied disconnect/no child/no record or
namespace change; original owner exits, new controller Registered; held FD makes
ordinary unmount Busy, result durably Uncertain/retained; release→explicit retry
→unmount child reaped→absence/original server reap→durable Absent. The private
Python subreaper, NOT recovered controller, reaped adopted original SSHFS.14logged
identities gone/0errors/survivors, not whole lifetimes/PID1 latency proof. No GUI
window close/native input or PID-publication-gap crash recovery is proved.

913Rust317/85/511+3existingignored PASS,5newcontroller tests; Clippyworkspace
all-targets-Dwarnings/release/fmtPASS. Full shared executor/admission/close ACK,
actual Connect/manage UI, resource association, multi-catalog and product install
remain pending. Freeze/new-preview pause/complete ROADMAP remain active.


### 2026-10-31 — SFTP product executor and controls

Core Connection/Store/recovered controller now wired to a lazy fixed transport
worker with bounded1request/1reply and single flight, separate from AuxIo saves.
Explicit reviewed connect; fresh/recovered serving-proof Open; ordinary disconnect,
reconcile and sync buttons/keyboard; polling/reaping while modal hidden. Close
revokes future work and awaits mutation ACK, without claiming blocked readonly I/O
was drained. Registered OS mount survives window exit. Failed publication retains
child/registry and blocks automatic replay; known Absent allows another connection.
Inspection does not hold a writer lease. Missing host sshfs remains visible and no
automatic global install is performed. One registry slot, no connection catalog.

See SFTP-PREPARATION.md and evidence/sftp-native/ui-validation for current logs,
scoped native UI evidence and independent install verification. Native proof uses
actual loopback SSHFS/FUSE with private keys/known_hosts/config and product handlers;
no OS input or resources/startup experiment. Existing default staged-copy FAIL is
preserved; visible-exclusive copy policy needs an explicit product review before
wiring. N01/full ROADMAP and hard/ideal resource goals remain open; previews paused.

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

### 2026-10-31 — Local connection drafts

See SFTP-PROFILES.md: a separate bounded profile catalogue supplements the single
mount registry. Loading invalidates Review and only fills the form; connection
remains explicit. Earlier no-catalogue status applies to its dated checkpoint.
No change to SSH host trust, actual transport, mount ownership or copy publication.

### 2026-10-31 — Diagnostic capture UI

SFTP-REPORT.md: F4 full retained diagnostic display with fixed snapshot, wrapped
visible rows and explicit refresh/export request. Draft is labelled separately
from observed transport. Report/profile modes reject hidden form transport actions.
No protocol, registry, SSH trust, actor, mount ownership or publication policy change.
OS clipboard delivery, extended diagnostics/otherOS/global gates remain pending.
