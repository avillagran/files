# Favourites and quick folder navigation

CtrlD adds the current loaded directory; CtrlAltD/the Nerd star opens the quick
navigation hub. Favourites and existing XDG places share keyboard/mouse navigation.
Custom favourite labels can be renamed; removing a favourite never removes the
folder. Paths retain original bytes, with no URL decode/canonicalization or FS
probe on the UI thread. Navigation reuses the asynchronous directory browser;
missing/offline/replaced-by-file destinations must report normal load errors.

Bounds:64 favourites,4096B/path,96B UTF8 label,64KiB total wire; one active
mutation/retry intent. Binary versioned codec validates borrowed wire before
allocating rows. Platform mismatch, invalid/future/symlink/nonregular/oversized
stores are preserved, never silently reset/overwritten. ID allocation persists
monotonically; row actions carry expected ID/path/label, so stale edits fail.

One existing AuxIo worker performs all reads/writes, no extra worker or watcher.
Mutations acquire short nonblocking advisory lock, reload latest, apply exact
intent, publish exclusive same-directory temp0600 with file+directory sync. It
merges additions between cooperating windows rather than overwriting stale UI
snapshots. Non-cooperating hostile writers are not an atomic CAS guarantee;
file/lock revisions are rechecked immediately before rename. Post-rename sync
failure is explicitly published-but-unsynced and retry only syncs/reloads; it
never replays an add/remove as if publication did not happen.

Closing the panel preserves active work. App exit waits for mutation ACK; errors
retain the exact retry intent. Explicit discard can drop an unadmitted/failed
intent after ACK; it cannot undo an already published write. No UI join or promise
of instantaneous exit from kernel-blocked/offline filesystem calls. Native input,
other platforms, complete resource/startup acceptance remain pending; freeze and
new-preview pause continue. Scoped functional evidence is recorded below; native/resource acceptance remains open.

## Interaction and synchronization

The hub borrows at most8 existing XDG places and the tab's bounded64 back/forward
history rather than storing another history. Filtering uses labels, not paths or
filesystem content. The rail always exposes a Nerd star; Actions and the53-entry
command catalog expose the same actions. System places still load while a
favourites read is pending or failed. F2 edits only the label and offers a visible
Guardar button; Enter submits it. Ctrl+F returns to the query; Escape cancels label
editing/details before closing. F1 opens wrapped diagnostics/path details.

`bookmarks.bin` and `bookmarks.lock` live under the app config directory. Opening
the hub/F5 explicitly reloads; there is no background cross-window watcher. A
failed mutation must be retried as captured or explicitly omitted, then reloaded
before a new mutation. Read errors alone do not prevent app exit. Active reads
and mutations await their actual AuxIo reply; post-publication sync failure holds
an explicit sync-only retry, not a replay of the original mutation. Permanent
kernel-blocked calls are not given a fictitious deadline or ACK.

Labels accept96 UTF8 bytes (not96 graphemes). Raw Unix paths remain bytes and may
be escaped in display. Symlink aliases are not deduplicated by canonicalization.
There is no favourite reordering, per-panel catalogue, cloud sync or native
accessibility/input acceptance yet. Other platforms' coordinated writer support
remains explicit Unsupported; a Linux fixture is not a portability gate.

Explicit Enter/Entrar always reloads through the asynchronous browser, including
the current pathname; a previous loaded flag is not proof the directory still
exists. No UI-thread stat is introduced. CtrlAltD closes the open hub and cannot
fall through to CtrlD Add; label editing changes the painted control stamp.

## Scoped validation — 2026-10-31

793 workspace Rust tests PASS (244core/79helper/470UI),3 existing core ignored;
workspace/all-targets Clippy-Dwarnings, release and fmt PASS. Logs:
`target/bookmarks-final-regressions.log`, `bookmarks-accepted-clippy.log`,
`bookmarks-accepted-release.log`, `bookmarks-accepted-fmt.log`.

`evidence/bookmarks/bookmarks-ui-1793422800552758666.json` PASS:4 owned processes,
11 real framebuffer PNGs1000×650/800×440/440², stale-snapshot additions merge,
raw0xFF path restored, label edit without folder rename, stale label/remove
rejected, failed-write close held until cancel/omit/reload, new ID after removal,
stale control stamp rejected, nonblocking lock busy and exact original-root retry
after navigation, directory replaced by file load error/recovery, new-process
restore and future-version store unchanged. Four normal exit0 after drain ACK.
Original note.txt hashes and the actor's file were unchanged. All11 PNG inspected;
long single-line paths/status can clip, with F1 wrapped details. No native input,
clipboard/AT-SPI/HiDPI/mount/kernelD/lifetimes/powerloss/resource proof is implied.
History is visible in the final retry screenshot; history-jump full semantics
remain the preexisting navigator's contract, not an extra native-input gate.

Same-binary command catalogue regression
`evidence/commands/command-palette-ui-1793422810269958601.json` PASS11FB/53commands.
No new dependency, background watcher/index/service or per-favourite thread.
Freeze PSS/Swap/CPU/startup and new-preview pause remain; footer incidental PSS
is not a process-tree budget. Historical baseline/startup/clipboard FAILs stay open.

Initial scene1793422449596475652 FAIL expected a state marker that is suppressed
while closing; checker now uses the existing close WAIT trace. Second scene
1793422508905329953 FAIL found same-current-path Jump skipping a required reload;
fixed with a focused regression. Original reports/screens/logs and sidecars remain.
Static review also fixed CtrlAltD inside the hub matching Add and invalidated
painted commands when label edit mode changes. No PASS was obtained by reducing
store/revision/close checks. Directory-sync failure is a synthetic hook unit test,
not actual failed fsync/powerloss evidence. Cooperating-window locks are not a
hostile same-user CAS guarantee.

Source `1b448f0` installed through
`evidence/0.0.4/installed-checkpoint-1793422888499506241.json`. Five installed
artifacts and five backups independently rehashed exact. App
`12d5d44825a0ea647f96833595ec309bfc6cc3a1d098b4a7106557eca23a61ad`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user install authorized, no source publication/global config/window control.

## Descriptor ownership on normal release

OWNER-LOCK-RELEASE.md records the creator-PID explicit flock release added in
source ebfbd46. A child retaining an inherited descriptor no longer extends
normal owner lifetime; dropping a child copy cannot unlock the active creator.
Nonblocking acquisition, genuine busy/read-only/CAS behavior and formats remain
unchanged. Linux fork repros failed before and passed after;1290workspace tests
and corresponding source/installed UI checks passed, with scoped limitations.
