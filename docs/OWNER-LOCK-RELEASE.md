# Advisory owner release across fork/exec

Source acceptance PASS:1,290 workspace tests (447core/105helper/738UI),
3historical ignored, core fork matrix8PASS, Clippy all targets warnings denied,
format and release PASS. UI Wayland: folder views1793553295569501392 passed
9captures/3normal exits; bookmarks1793553007936334141 passed11captures/4exits;
sessions1793553007697099900 passed6captures/8exits. A concurrent toolkit popup
attempt failed without a catalogue request; it is retained separately. Installation
is recorded after source acceptance.

`folder_views` and `bookmarks` acquired nonblocking exclusive flock ownership and
returned a bare File. `session_store` similarly retained a bare File for the
writer's lifetime. O_CLOEXEC was set, but dropping the creator's File does not
release flock if another thread forked and its child still retains the same open
file description before exec. A successor can receive WouldBlock after the
writer has already finished. Kernel lock metadata can still name the creator
while the only descriptor holder is the child.

The owned Linux syscall audit in
`docs/evidence/catalogue-lock-release/fork-fd-audit.json` records that state:
CLOEXEC true, parent descriptor closed, child-only holder, successor busy. The
control explicitly unlocks before close and the successor succeeds while that
child remains alive. No resource or latency experiment is involved.

Deterministic Rust tests reproduced the defect in all three consumers before the
fix: core fork tests6PASS/2FAIL (folder views/bookmarks creator release), session
fork tests1PASS/1FAIL. Paired cases also require that dropping a forked copy never
unlock the still-live creator. Children execute only inherited guard/File drop
and libc pipe read/write/close/_exit; no allocation, logging, test assertions or
other runtime locks run after fork. Each child is released and reaped before
parent assertions, including failing assertions.

The missing consumers now follow the existing saved-search, SFTP-profile,
SFTP-registry and move-journal policy: record creator PID, explicitly LOCK_UN on
drop in that PID, retry Interrupted, then let File close. A child copy only closes
its descriptor. The existing nonblocking acquisition, actual-writer contention,
regular-file/NOFOLLOW checks, data revisions, lock inode checks, CAS, atomic
publication/fsync/unsynced outcomes and read-only session takeover policy remain.
Read-only folder catalogue loads do not acquire this write lock.

No file format or public operation/result schema changes. Folder views and
bookmarks still execute on AuxIO; session ownership remains on that worker for
its existing lifetime. There is no new worker, global mutex, polling retry,
forced takeover, sleep in production or test-harness serialization. Explicit
unlock does not make blocked filesystem/native syscalls bounded or establish a
global lifetime/resource guarantee; functional acceptance here is Linux NODE.

The historical UI test
`folder_view_explicit_save_keeps_captured_path_through_navigation_and_forget_is_cas`
used a private fixture directory and mocked AuxIO dispatch. Inspection found no
queued writer of that fixture's catalogue, and catalogue Read takes no flock.
The reproduced inherited-description defect is consistent with its intermittent
WouldBlock after a prior apply returned. No PID/FD snapshot was captured at those
historical failures, so this work does not attribute every earlier failure to a
specific process or claim that all possible WouldBlock causes are eliminated.
A legitimate still-live writer must continue to produce WouldBlock.

After the fix, core fork tests8PASS and the normal workspace suite1,290PASS
(447core/105helper/738UI), with3historical core tests ignored. The original UI CAS
case passed in that run. Earlier FAIL logs and syscall evidence are retained;
passing a retry alone is not evidence of the repair.

The acceptance fixtures cover production Save/Forget/CAS/error/lock/retry,
favourites stale merge/remove and genuine lock contention, and multi-window
session writer/read-only/restore/future-version behavior. The tabs session mode
now checks its own painted1000×650 canvas rather than using compositor float/
resize IPC. Bookmarks/tabs accept --installed and complete log lines. Private
handlers and real framebuffers do not prove OS input, clipboard, AT-SPI,2x or
power-loss/global lifecycle behavior. PSS/Swap/CPU/startup remain frozen and new
content previews remain paused.

Installed on NODE from source `ebfbd46`, receipt
`installed-checkpoint-1793553641613427210.json`; app SHA256
`2c4c4ae55761680a97f9ef9219bd850176404d19f9e182c0ff0ad1f5f20f16ba`.
Five installed and five backup hashes verified. Installed folder views
1793553696558344229 passed9captures/3normal exits, bookmarks
1793553763277963787 passed11captures/4exits and sessions
1793553762994981888 passed6captures/8exits, all with existing close-drain checks.
The toolkit-popup fixture ran alone; no popup/input routing code changed.
