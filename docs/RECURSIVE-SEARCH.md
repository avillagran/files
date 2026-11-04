# Recursive filename search — 2026-10-30

Status: first bounded implementation, scoped functional acceptance passed. This
extends local filtering; it does not complete the full search/index ROADMAP.

## User workflow

Ctrl+Shift+F opens the search panel for the current directory; the Actions menu
and background context menu expose the same action for mouse use. Type a name
or use the local filter syntax: AND words, !exclusions, full-name globs, literal
phrases, ext:rs,md and type:dir/file/link/other. Enter or Buscar starts a scan.
A fresh query invalidates previous results and cancels an older request.

Results display the basename and relative parent, so two main.rs files remain
distinguishable. Up/Down, PageUp/PageDown, Home/End navigate the bounded result
list. Enter opens a file through the existing system open worker or enters a
folder; Alt+Enter / Localizar loads its parent and selects the original raw name.
Locating waits for the current listing generation and wins over refresh selection
restoration; paint reveals the selection. Esc closes and cancels the search.

The root is captured per panel opening. Hidden inclusion is local to the search,
initially copied from global preferences; toggling it does not change that global
preference. To change root, navigate to a directory and reopen search.

## Traversal and result semantics

One read-only helper, no index, content scan or stat request per regular file.
Linux directory traversal uses held FDs and O_DIRECTORY|O_NOFOLLOW, with statx
mount IDs for the root and each opened directory. It does not follow symlinks or
cross to another mount (including a different bind mount). The root FD stays held
until a final root-name/mount identity check. Missing/unreadable entries, excessive
paths or depth produce explicitly partial results; an invalid root is an error.
A result is an observation, never a proof authorizing a mutation. The tree is not
a snapshot, and names can change after a scan or before a later open.

Bounds: 500 hits, 256 KiB retained result paths, 4096 B per final absolute path,
250,000 visited entries, depth64 and 10s cooperative scan limit. Query limit is
4096 B/32 terms. Empty queries require an explicit pattern such as '*'. Reaching
a result/path/visit/time bound never claims completion or global no-matches.
Partial status/reason comes first in the UI, so narrow windows cannot hide it
behind counts. Errors and mount exclusions have separate counts.

Serde limits hit allocation and path sizes on receive; a shared validator checks
relative-only paths, budgets and report consistency. Unicode strings and Unix
raw byte arrays use the existing IPC path codec. The operation never decodes raw
paths from display labels. No new dependency or package was added.

## Lifecycle and interaction

The existing bounded space-map pipe supervisor now exposes an internal command
parameter reused by search; its space-map wrapper/tests remain. Both directions
are nonblocking, request<=64KiB/output<=4MiB, deadline15s for search, kill/reap on
worker exit paths, owned-child parent-death signal unchanged. A stuck kernel FS
syscall can still delay reap; no instant offline/kernel-D termination is claimed.

The UI retains one active task and at most one pending latest intent. A revision
separates query edits, cancellation, scope changes and close; old results cannot
publish over a newer intent. New requests wait for reap/result ACK and another
pump. Preview/thumbnail/DU/treemap and search share admission gates: opening search
cancels/drains the previous helper before starting its own. Root close and runtime
failure drain search too, without joining a worker from the UI thread.

Search is modal for event routing and rejects underlying DnD/file clipboard and
stale popup actions. Text paste uses the existing context/receipt gate plus the
search revision; editing invalidates a delayed old callback. No desktop focus,
selection ownership or native clipboard interoperability is inferred.

## Explicit remaining work

Publication is one bounded final report, not streaming live results/progress.
Paging/continuation beyond500, changing root within the panel, metadata/date/size
facets, content regex, index, full accessibility and other platforms remain open.
Mount confinement uses actual Linux code; a runtime bind-mount fixture has not
been run and no global mount was created. Real OS keyboard/mouse, file-viewer
launch and active-helper-at-close acceptance are separate from handler fixtures.
Resource/startup freeze remains: caps and source shape do not prove total PSS/CPU,
latency or compliance with the ideal50MB/hard150MB process-tree budgets.

## Validation

688 Rust tests pass (230 core / 70 helper / 388 UI), with three existing core
ignored tests. Workspace/all-target Clippy -D warnings, release workspace and fmt
pass. Final UI labels/root basename change follows the suite; final Clippy/release
and the final scene cover those labels without repeating unchanged tests.

`evidence/search/recursive-search-ui-1793393137003734053.json` passes with eight
1000×650/800×440 framebuffers: duplicate basenames in separate parents, raw0xFE
parent/raw0xFF file location and selected visibility, hidden scope independent of
global preferences, symlink external/cycle exclusion, 500-hit partial state,
complete no-matches, invalid query, entering a result directory and root callback
exit0 after drain. Eight logged helper PID/start identities are absent/reused;
Z is not accepted, no lifetime coverage. Cancellation before ACK was observed;
active OS helper exactly at cancel/close is not proven. An earlier same-scope
scene passed too; its search finished before the cancel command. Both are kept.

A navigation regression initially accepted a stale text callback because input
had not advanced the search revision; fixed and the same test now rejects the
callback without changing the new query. Initial Clippy style failures are kept
in target logs. No personal clipboard data was read.

Resource/startup NOT_MEASURED. Content search, metadata facets, ignore-directory
rules, progressive results/pagination and complete keyboard/native acceptance
remain pending; S01 is partial, and the whole explorer goal remains active.

## 2026-10-31 — Opt-in literal content

Content search extends the name-only contract; see CONTENT-SEARCH.md for the new
criterion, separate Texto/F6 draft, streaming limits, FD validation and truthful
partial/exclusion semantics. Name-only requests keep their previous no-content-
read branch and protocol compatibility. Progress still carries counters only;
lines and raw result paths remain final-report observations. This does not close
regex/encoding/index/semantic/native/global-resource work.
