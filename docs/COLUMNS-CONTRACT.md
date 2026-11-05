# Finder-style Columns — M26 (2026-10-27)

Explicit user decision: Columns/Miller replaces the Spatial view. Space
(treemap) stays. Dual pane is deferred to the next visible block, with no functional
WIP discarded. New previews paused and the metrics freeze intact.

## Interaction and source

Each column represents a directory. Selecting a folder keeps the focus and
the selection IN ITS COLUMN and requests the next listing; selecting a file
keeps the ancestors and prunes the descendants. Changing folder in an ancestor prunes
the later branch. Click/Left/Right change the focused column; Up/Down
move the selection within it. Right only enters the selected folder when its
listing is available; it never opens a file by accident. Enter hands files
to the existing `open`. Independent vertical scroll, horizontal scroll with reveal of the
focus and Nerd controls. Each column's filter/selection/scroll are kept within
the chain; hidden files stay global and are pruned before operating.

Copy/move/rename/trash/properties always capture the path and names of
the focused column. A listing on the right does not change that source. Queue, IDs,
ACK, journal and the media/operation supervisors are still single. Focus does not
change during Apply/modal review. No file I/O in draw/event.

## Ownership and limits of this unit

An active Model keeps its listing and reusable worker; a second reusable
worker serves only the next requested listing. Both share the
global limit of four FS queries. Ancestor caches have NO workers or
helpers; eight Models are not instantiated. Responses from the earlier branch are discarded
by the worker's internal generation and a stable column token, never by a reused
index. Up to eight retained listings; a sliding window drops the oldest ancestor
when going deeper, without limiting navigable depth. Going up/breadcrumb
rebuilds ancestors asynchronously and warns when discarding state because of the budget. Virtualized rows: hits and text only for visible ones.

Chain data budget: 16 MiB retained (capacities of entries/names,
order/selection and structs), at most 100k entries/8 MiB of names per listing. It is
validated before retaining batches/captures, with a conservative selection reservation
and exact growth of entries/order. Controller/paths/deferred menu reserve
an additional 64 KiB; bounded buffers on both channels (8 batches×256 entries),
transient selection ≤256 KiB and sorting keys are separate reservations. It is not a process limit and does not prove PSS. Outside
Columns the chain is freed and the extra worker is suspended, keeping a
single reusable thread against stuck syscalls. Close cancels it without faking
interruption of a kernel syscall. Resources/startup NOT_MEASURED because of the freeze.

## Tabs, sessions and migration

A tab holds its focused column path, history, filter and selection
with the same bounded rules as M22/M23. Switching tabs rebuilds the chain
from that path; this first unit does not persist every open column.
There is no second session/queue. The legacy Spatial session tag 3 is read as
Columns; it keeps path/history/selection but discards zoom/pan and icon scroll
as incompatible geometry. The new encoder uses the free tag 5 for Columns and keeps the new vertical
scroll; tag 3 remains exclusively as the Spatial migration. `--spatial`
remains a compatible alias of `--columns`, with the visible name Columns; old
icon layouts are kept on disk without reading or deleting them by inference.

## Planned acceptance

Extended existing scene: root/A/nested chain, the same sibling folder B,
a file with the same name/different content, selection and operation payload in
the parent while the child is visible; pruning and a stale response after A→B, keyboard,
scroll/reveal/resize and returning to an ancestor, global hidden/raw Unix. A legacy tag 3
session restores data without reinterpreting geometry. Real captures and internal
handlers declared, without claiming OS input or new metrics. Tests/build/Clippy
plus installation with backups only on validation. V06 in implementation; V08
replaced by decision, old data preserved, none marked complete.


Validation status before capture: 596 Rust 194 core/67 helper/335 UI PASS in
columns-workspace-final-test.log; Clippy workspace/all targets -Dwarnings clean.
The earlier suite exposed WouldBlock when reopening recovery: a new regression with a
duplicated FD reproduced RED; Journal now releases the flock explicitly when the owner is destroyed,
only in the creating PID, so a Drop in a child process cannot release it for the parent.
Regression GREEN and full suite PASS; the first failure is not attributed exactly to a specific fork.
The fixture budget assertion FAIL and an intermittent FAIL
without enough context about a missing input A are kept; the later fixture
uses exclusive creation + an atomic ID and full diagnostics; the selection was not relaxed.
Scene and release still pending when this entry was written; no global feature PASS.


## M26 — bounded functional acceptance

columns-ui-1793124416464738573.json PASS: six real framebuffers with checked
dimensions (five 1000×650, one initial 800×440), a three-column chain, focus
kept in the parent while the child is visible, pruning A→B, rename payload and a
real byte copy from A/nested (B contains the same name), filter/raw name,
sources intact and two normal closes 0 after ACK. Captures inspected:
the ancestor cut off by horizontal scroll as expected, without claiming liveResize.
Depth 12, modifiers, focus/cancel before Done/stale Stats, the deferred menu by
raw name+generation and the tag 3/5 session migration have unit regressions;
they are not presented as OS events or full persisted sessions of the chain.
596 Rust + Clippy/release/fmt clean; columns-checkpoint-validation.json keeps
logs and earlier failures without attributing an exact cause to the intermittent fixture.

The 16 MiB limit is the admission of chain data in batches/captures, with capacities and
the selection reservation; the Model's on-demand metadata, job/channel queues,
allocator/stack/FLTK and the existing global media add to it. Do not use this local
cap as proof for the process; memory/CPU/startup NOT_MEASURED because of the freeze.
Windows/macOS, OS input and persistence of the whole chain not verified.
Next: dual pane according to DESIGN, followed by cut/paste/DnD/operations/SFTP.
