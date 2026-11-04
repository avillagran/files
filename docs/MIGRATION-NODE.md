# Files development moved to NODE

2026-11-01. User requested moving the entire project and development process to
NODE to release M2 disk space. Authoritative workspace: `/home/node/Work/files`.
SSH: `node@192.168.1.177`. This same conversation can continue through SSH;
T3's cross-environment link request is still pending. No remote thread created.

The full root, Git including dangling objects, all ignored files, evidence and
ARM build artifacts were copied before removal on M2. 36,736 files / 2,194 dirs /
102 links matched. Final rsync checksum dry run including --delete had no changes;
private Files fixtures/backups were separately checksum verified. Source HEAD at
migration:77ca979878ba9d4bd5d6ef1ba71c0f85620f68b1. Git fsck exit0.
The first comparison found only Git's index/stat-cache changed by remote status;
index was recopied and the complete final comparison was clean. Original logs
are retained rather than claiming the initial comparison was empty.

ARM target preserved at `/home/node/.cache/files-node-migration/target-m2-aarch64`;
fixtures/backups at `/home/node/.cache/files-test-m2-import`; complete transfer
logs and handoff at `/home/node/.cache/files-node-migration`. A fresh native
x86_64 target is generated inside the workspace. Do not run ARM artifacts as NODE
proof. No Cargo registry, credentials or whole personal history was transplanted.

Only a pointer and receipt remain at `/home/avillagran/Work/files` on M2. Private
`~/.cache/files-test` was removed after verification. Installed M2 app/helper/HTML
were preserved. Available space increased by7,667,535,872 bytes, from4,653,465,600
to12,321,001,472 bytes at the recorded check. Copied logical size is larger than
observed released space; no filesystem-internal cause is asserted.

Native core tests:410PASS,3existingignored. Clippy initially rejected two portable
nlink_t→u64 conversions on x86_64; scoped binding annotations retain ARM's needed
conversion and avoid broader lint suppression. Final core all-targets Clippy -Dwarnings and post-change410test/3ignored PASS.
A competing o3-server Cargo was observed; own attempt did not start until that
PID exited and an idle snapshot admitted it. No competing process was touched.
No whole-workspace/release/UI/installation acceptance on NODE yet.

Goal remains active. Continue folder-view preferences from
`FOLDER-VIEWS-CONTRACT.md`: AuxIo controller/ACK/close, Save/Forget, navigation
restore, late user-intent guards and tab/session priority. No preference UI is
claimed. PSS/Swap/CPU/startup experiments frozen; new previews paused. Historic
M2 failures and resource evidence retain their original host/scope.

## NODE continuation — per-folder view acceptance

2026-11-01: the initial migration scope above is historical. V12 now has a native
NODE workspace test/Clippy/release pass and scoped Xvfb/X11 + Hyprland/Wayland
functional evidence.1173Rust PASS (410core/94helper/669UI),3existingignored;
18framebuffers/6normal exits across final backend runs. AppSHA
`02e0d79995d8ec9af304307615b4e15423f930a2fa19af58b306468a1e3a6e12`.
See `FOLDER-VIEWS-CONTRACT.md` for receipts and exact limits. No OS-input,
clipboard, AT-SPI,2x, lifetime, resource/startup or installation claim.

The five-artifact installer set is not ready: native `files-html-helper` and its
stamp/licenses have not been staged for NODE. Imported ARM support remains
archived and must not be used to satisfy that gate. Rebuilding an existing
adapter for NODE must not turn into new preview development or resource tests.

## NODE native runtime and installation — 2026-11-01

The earlier missing-adapter state is now historical. Only the immutable source
archive (SHA82cfdc8347eb…) was copied from imported support, verified, extracted
and built serially on NODE from pinned litehtml6fd7aaa plus the existing adapter.
All three executables are ELF64/x86_64; no ARM binary was staged or executed.
The five-artifact user installation passed exact hash verification; first NODE
install means five previously absent files and an empty recorded backup directory.

- App02e0d79995d8/helperd2e925c49701/HTML04aa7953202d.
- Record `evidence/0.0.4/installed-checkpoint-1793515028382395392.json`.
- Receipt `evidence/node-install-native/receipt.json` includes source/build records.
- Installed-path V12 Wayland scopedPASS:9PNG/3exit0drain; installed-path existing
  HTML scopedPASS:5PNG/exit0drain and7logged helpers gone/reused, strict no-Z.
- `files --version` resolves to `~/.local/bin/files`, reports0.0.4.

Rust code is unchanged since bdb5882/1173testsPASS; this phase built the existing
native adapter and added installed-path switches to functional scripts. No native
OSinput/clipboard/AT-SPI/2x/lifetime/resource/startup/globalgate claim. Full goal
active; source not pushed or tagged; M2 installation untouched.
