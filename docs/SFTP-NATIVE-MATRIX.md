# Native SFTP/FUSE laboratory — transport PASS, Files copy FAIL

This is real OpenSSH internal-sftp + SSHFS + kernel FUSE, not simulated IPC/mountinfo.
It is NOT the Files mount worker or GUI integration. Installed preparation
86bf3ff/f0df843 remains unchanged; Connect is not enabled. N01 remains open.

## Isolation and provenance

SSHFS3.7.6 was fetched from the official release and built privately with existing
Meson/Ninja(-j1), fuse3/glib. No install phase or global package/config changes.
Source/archive/binary/build-log hashes and release URL are in
`evidence/sftp-native/tool-provenance.json`. GPL upstream program is a standalone
private lab tool; no source is incorporated, linked or bundled into Files.
Hashes provide provenance, not a verified detached signature. Build lives under
`.cargo/tmp/sftp-lab`, not user PATH or the installed Files directory.

The script is inert/help without --attempt. Attempts use an owned0700 fixture,
new test-only SSH keys and matching known-host record, an unprivileged loopback-
only sshd with a private config/internal-sftp, and foreground SSHFS. Core Target
constructs bounded argv; explicit -F is LAB-ONLY authentication config override,
not a product feature. No real user's SSH config/keys/agent are used. Fixture keys
are retained privately, never printed or installed in ~/.ssh. No recursive
fixture deletion around uncertain mountpoints. A private process-only subreaper
adopts/reaps lab descendants, never a product daemon. Cleanup signals use exact
known process identities/pidfds; no global pkill or unrelated listeners/windows.

## Actual observations

Final report `evidence/sftp-native/sftp-loopback-1793442486810260624.json`:

| Check | Result |
| --- | --- |
| Strict core mountinfo identifies unique SSHFS mount | PASS |
| Literal Unicode/percent/$() remote directory via core argv | PASS |
| Raw0xFF file bytes, literal relative symlink | PASS |
| Independent cat process reads mounted local path | PASS |
| Write through native mount reaches private server directory | PASS |
| Ordinary unmount while file FD held | Rejected busy; identity unchanged |
| Close FD → ordinary unmount → strict mountinfo absent | PASS |
| Foreground SSHFS exits0 after unmount | PASS |
| Six logged PID/start identities after cleanup | Gone/reused; Z not accepted |
| Identity read errors / observed survivors | None |
| Direct renameat2(RENAME_NOREPLACE), absent target | Unsupported: errno22; source intact |
| Exclusive file/symlink links, occupied target | Collision rejected, occupant preserved |
| Linked source/destination inode numbers | Different virtual IDs for both kinds |
| Production core copy into mount | FAIL errno22,0 roots published |
| Core copy cleanup | No staging, cleanup errors or rescue paths; source intact |
| Overall native compatibility gate / script exit | FAIL /1 |

The copy error is generic EINVAL: no syscall trace of its exact failing operation
was captured. An independent direct rename2 syscall and upstream source show the
backend flag limitation, but do not turn that generic core errno into a traced
failure phase. The observed failure must remain FAIL, not guessed success.

Native links succeed but have distinct client-visible inode numbers. Therefore
an unconditional rename→link fallback cannot satisfy the engine's existing
same-object checks here. Ordinary overwrite-capable rename is not an acceptable
fallback. No existing core no-clobber/identity/source protections were relaxed.

## Preserved attempts and limits

- 1793441407601397594: native transport/read/write/busy/unmount/cleanup only PASS.
- 1793441566821275255: first core copy failed; initial UNSUPPORTED classification
 of generic errno22 retained with sidecar; later classification is copy FAIL.
- 1793441683924377602: added direct rename2 rejection, copy FAIL.
- 1793441829445121634: added hardlink alias/occupant probes, copy FAIL. Earlier
 script returned0 based only on mount_gate; no overall field existed. Final
 script now reports overall FAIL and exits1 when compatibility fails.
- 1793442486810260624: final strict overall result above. All original reports
 retained; no rerun discarded a failure or sought a green copy gate.

The initial compile-only Python else1 SyntaxError is recorded separately; no
attempt was executed until syntax was corrected. Examples/default script help
are inert (mount-table default mode reads own namespace only). Core all-targets
Clippy-Dwarnings and fmt/examples build PASS in target/sftp-native-*.log. No
production source change, full workspace rerun, release or installation in this
unit. No new GUI image; previous preparation screenshots are not native evidence.

No physical/native GUI input, credential UI, negative host-key case, offline/
network-drop/reconnect, daemon persistence, kernelD, powerloss, remote hostile
writers, snapshots or whole-lifetime acceptance. Logged process identities are
not complete lifetime coverage. Mount IDs are observations, not global permanent
identities or atomic unmount authority. No PSS/Swap/CPU/startup measurement;
freeze and ideal50MB/hard150MB remain. No speed/superiority claim.

## Next implementation decision

A native mount is not enough when Files cannot publish copies on it. Implement
an explicit remote publication adapter rather than changing default local
staged rename semantics. Candidate: exclusive visible destination creation for
regular files/directories/literal links, per-entry no-clobber writes, accurate
partial/rescue reporting, no automatic deletion of uncertain remote contents,
source always retained on errors/cancel. Visibility during transfer must be
honest in review/progress. This still needs design/code/race/cancel/native tests;
it is not implemented or approved as verified merely by writing this plan.

Do not use CopyReport.completed as permission to delete a source in EXDEV; the
existing durable verified-manifest contract stays separate. After publication
semantics are verified, integrate the mount worker, immutable reviewed Target,
private mountpoint provenance/identity, late-result recovery, disconnect/busy
controls and OS-mount/window-close lifetime. Resolve system sshfs dependency only
with proper scope, not an inferred global install. No Files background daemon.

References: [upstream release](https://github.com/libfuse/sshfs/releases/tag/sshfs-3.7.6),
[upstream source](https://github.com/libfuse/sshfs/blob/sshfs-3.7.6/sshfs.c).
SFTP-MOUNT-CONTRACT.md and SFTP-PREPARATION.md retain the broader requirements.

## Explicit visible adapter (2026-10-31)

Report `evidence/sftp-native/sftp-loopback-1793444281194564656.json` adds a
separate `visible_adapter_gate=PASS`: production core opt-in VisibleExclusive
copied a directory, nested data, raw0xFF file and literal relative symlink over
the real mount. One root acknowledged/completed, zero uncertain creations,
37 logical bytes, one explicitly omitted symlink timestamp. A second transfer
into an occupied file rejected it and preserved the occupant. Sources remained.
Busy-unmount, ordinary unmount/absent and six logged identities cleaned up;
no identity read errors. No GUI, cancellation-on-FUSE or hostile-remote proof.

Default staged copy still FAILs errno22, with zero published roots and clean
staging. `overall_gate=FAIL` and script exit1 deliberately remain: adapter success
does not erase the default compatibility failure or validate integration.

VisibleExclusive must be explicitly selected before mutation. Every file,
directory and link is created exclusively; existing directories are not merged.
Partial roots survive errors/cancellation. Verified held-FD rescue paths are
separate from uncertain creation diagnostics. Completed is never authorization
to delete an EXDEV source. Same-user races before the first acquired identity,
remote snapshot isolation and all server metadata semantics are not guaranteed.

SSHFS3.7.6 source uses SETSTAT for symlink timestamps (FIXME LSETSTAT); no-follow
kernel flags cannot establish server safety. The adapter therefore explicitly
omits symlink timestamps, reports their count, and rejects requested preservation
before creating a visible symlink. Regular/directory metadata still uses the
existing FD operations. No ordinary rename or inode-assuming hardlink fallback.
The GUI does not yet select this policy or mount/connect; installed preparation
is unchanged. Resource/startup freeze and preview pause remain.

Validation: `target/visible-copy-workspace-final.log` 879 Rust PASS
(284 core /85 helper /510 UI), three existing ignored; six new visible-copy
regressions. `target/visible-copy-clippy.log` all-targets -Dwarnings PASS.
Intermediate parse/fmt logs preserved; final formatting log inspected empty.
Native fixture reports real protocol, not GUI acceptance or resource coverage.
Release workspace PASS (`target/visible-copy-release.log`); final fmt --check PASS
(`target/visible-copy-final-format.log`, empty). No installation for core-only adapter.

## Held mountpoint descriptors through real FUSE

Report1793444992430070322 extends the same lab with production core
sftp_mount_point through an inert-by-default developer example. The same helper
holds both provenance FDs across mount/unmount: newly created empty private point,
parent verification while mounted, explicit refusal to inspect mounted root,
then identity/empty check after ordinary unmount PASS. Seven logged identities,
zero observed survivors/identity errors; no auto-delete or recursive fixture
cleanup. Binary SHA is in the report. Default staged-copy/overall FAIL remains,
visible adapter PASS remains. No Files worker/UI/persistent restart acceptance.

## Core SSHFS process engine — current-source cases

This is actual Connection::start_lab/process supervision, not private ACK
simulation and not the AuxIo/product UI yet. Final debug binary SHA is recorded
per report; the only lab argv addition is private -F. No production config/key
rewrite, dependency install or Files background daemon.

| Report suffix | Case | Scoped result |
| --- | --- | --- |
| 1793446796308586156 | Initial remote FD/statx mount-id + getdents proof → Ready; cancel Ready retains mount; busy ordinary unmount retains diagnostic/identity; retry → both children reaped + fresh absence | Engine PASS, 12 logged identities gone/0 read errors/survivors |
| 1793446796591346333 | Graceful owner-process exit while mounted; exact SSHFS PID/start still live; new server-side file created AFTER owner exit read exactly through mount; lab ordinary unmount + adopted server exit0 | Owner-lifetime PASS, 7 logged identities gone/0 errors/survivors; not GUI close/restart recovery |
| 1793446796781664042 | Deliberately mismatched private known-host key; never publish Ready; bounded hostkey diagnostic; rejected/unverified FUSE reconciled to Absent with explicit ordinary disconnect or strictly verified already-absent result | Hostkey rejection PASS, 7 logged identities gone/0 errors/survivors; mount matrix NOT_APPLICABLE |

Both positive cases retain default staged-copy FAIL errno22, visible adapter PASS
and overallFAIL/exit1. The negative scenario also exits1: it is not a serving
mount matrix. No blanket green gate is inferred. Logged IDs are observations,
not coverage of every short child lifetime. No PSS/Swap/CPU/startup experiment.

Historical process reports555917/562940/588938 remain. FAIL588098 found the
namespace-only Ready error; FAIL611859 lost the exact request error in its
harness and its historical cause stays unknown (sidecars). Later616410 first
proved hostkey rejection/cleanup after the serving-proof fix; final679678 uses
current source. All raw reports are preserved, no rerun removes a failure.

895 Rust PASS299/85/511+3existingignored; all-targets Clippy-Dwarnings, release
and final fmt PASS. Core tests exercise memfd seals/capacity/reader close and
explicit truncation, failed exec retention, foreign mount identity and wrong-FD
proof; lifecycle additionally rejects next intent while unmount ACK is pending.
One diagnostic fixture wrongly assumed an oversize sealed write would partially
fill the file; original failed log is retained, final test explicitly fills to
the cap before asserting denial/truncation. First Clippy style failure retained.
No new product installation, UI scene or N01/global acceptance. Durable
cross-window registration, restart recovery and actual visible Connect remain.

Raw validation logs (including original failures), hashes and core source hashes
are retained in `evidence/sftp-native/connection-validation/`; checks.json is a
scoped record, not a product/global acceptance verifier.

## Durable registry, separate-reader case

Final1793450438576487018 (initial prototype1793449119003791297 preserved) uses
--connection-engine --engine-close --engine-registry. Prepared is durable before
SSHFS starts; driver records PID/start and full verified mount signature. While
owner lives another process is read-only. After graceful owner exit it reads
same generation/signature and obtains the available lock; SSHFS remains serving
a file newly created on server. Inspector does not adopt a Connection or mutate
the record. After LAB unmount it reports NoMount/Missing but keeps historical
Verified and identical file hash. No automatic reset or force takeover.

Registry observation gate PASS, mount gate PASS,7loggedIDs gone/0errors/survivors;
visible-copy adapter PASS/default stagedcopyFAIL/overallFAILexit1 retained. No
GUI close, recovered-disconnect action, PID-publication-gap crash/powerloss,
cross-namespace adoption, whole-lifetime or resource acceptance.13registry tests
and908workspaceRustPASS; raw logs/hashes under registry-validation. No new install.
The harness now refuses to declare cleanup absence if an engine point was never
received; it no longer treats a guessed default path as that engine's identity.

## Recovered controller performs explicit ordinary disconnect

Current1793453782020109493 adds --recovered-control to owner-exit/registry case.
While original owner holds lease a second controller rejects disconnect, creates
no unmount child and leaves record hash/mount identity intact. After owner exits,
new controller loads Registered, publishes intent before actual ordinary unmount.
Held file causes Busy, recorded/retained. Release and explicit retry completes,
owned unmount child reaped and original server reaped by private LAB parent;
controller then confirms absence and writes durable Absent.14loggedidentitiesgone,
zero read errors/survivors, not every short lifetime. Initial3058010388848 retained.

Recovered-control and readonly gates PASS; mountPASS, defaultcopyFAIL/overallFAIL
exit1 retained. No GUI/native input, PID1 latency, unregistered PID gap/crash,
force/lazy behavior or resource acceptance. Raw validation under
recovered-validation;5newtests/913RustPASS/Clippy/release/fmt. Product UI not wired.

Validation:919Rust318core/85helper/516UI(+3existingignored), Clippyworkspace/alltargets-Dwarnings, release/fmt PASS. Native `sftp-loopback-1793455941407692094.json` has UI+freshConnect+mount PASS with8PNG1000×650/3root0;25loggedPID/start identities gone,0readerrors/survivors, not lifetimecoverage. Fixture parent reaped the old SSHFS, product only its own children. DefaultcopyFAIL/overallFAILexit1 are unchanged. Initial recovered UI mismatch1793455368964965907 and its sidecar preserve wrong Registered-vs-Retained expectation; no broadening to arbitrary states. Final negative-tool/440² scene1793455971563736333 has6PNG, no registry creation/source mutation/root0. No native input, clipboard or PSS/Swap/CPU/startup measurements. Long diagnostic lines can still clip; a full copyable transport report remains pending.

Final lifecycle follow-up:921Rust319core/85helper/517UI(+3ignored) and Clippy PASS. Closing increments a shared admission epoch; cancelling close resumes only NEW epoch requests, never revives a queued revoked command. Added deterministic test. Registry Lease now explicitly unlocks only in its acquiring PID, including construction-error paths: deterministic forked-FD test proves child cannot unlock parent, and owner Drop allows a new opener while child retains inherited FD. The prior concurrent new_owner.writable() failure remains in ui-validation/sftp-executor-close-epoch-tests.log; which child caused that historical failure is UNKNOWN. No serialization/relaxed assertions/rerun-only resolution. Current evidence references are listed in ui-validation/checks.json.

Final release UI acceptance: native `sftp-loopback-1793456384121839709.json`, product controls/freshConnect/mount PASS8PNG/3root0; negative-capability and440² `sftp-preparation-ui-1793456409825960118.json` PASS6PNG/root0.25loggedPID/start identities gone/0errors/survivors; not whole lifetimes. Native overall/defaultcopy FAIL remain. Source/log/binary hashes and scopes in ui-validation/checks.json; supersedes previous display-only scene1793455941407692094 for installation. All original reports retained.


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
