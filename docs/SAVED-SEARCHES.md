# Saved searches

F2/Nerd opens the catalogue inside Search. Ctrl+S captures the current criteria
for a named save. A read-only review shows name/text, size/date UTC fields,
hidden flag, editor and order before committing. Labels use the bounded line
editor, selection, undo and asynchronous paste guards. Names are UTF8; raw Unix
roots are stored losslessly without URI decoding.

Enter/Cargar loads the stored root. Alt+Enter/Aquí uses the current **search root**,
which the catalogue displays separately from the stored root. Both fill a fresh
search draft and require another Enter to run. Loading preserves the browser tab
and global hidden preference, cancels the previous intent through the existing
reap/generation rules and drops stale results/marks. Calendar choices retain
their resolved UTC fields rather than recalculating Hoy later.

Storage ACKs update only catalogue/status. Desired selection of an added/renamed
row is bound to the exact request and panel, and is superseded by explicit row
selection, keyboard navigation or closing/reopening. An old ACK cannot apply
criteria to a search or steal another panel's selection. Stamped hits and
owner/input-epoch guards reject actions after a changed tab/search intent.

## Bounds and storage

- 32 entries and128KiB wire; labels96B, absolute roots4096B, query4096B,
 literal1024B, four UTC/size fields32B each.
- Borrowed validation precedes decode materialization. Encode sizing and prospective
 admission precede output/new-payload allocation. Duplicate criteria keep their
 stable entry ID; rename/remove compare the exact expected entry after reload.
- One pending/active storage operation on existing AuxIo, not a thread per gesture.
 Regular-only O_NOFOLLOW/NONBLOCK bounded reads, revision revalidation, a
 nonblocking per-file flock and checked atomic replacement. Only its creator PID
 explicitly unlocks advisory ownership; an inherited child must not unlock it.
- Invalid/future/platform-incompatible files are preserved. Failed writes retain
 their original intent for F5 retry or explicit F8 discard; no tick retry loop.
 Published-but-unsynced updates retain the document and retry only directory sync.
 Discard does not undo publication or cancel a sent operation. Close waits for
 sent operations and exposes unresolved writes; no UI join/stuck-I/O deadline.

The catalogue is loaded lazily and F5 refreshes external changes. Stored roots
are observations, not pinned filesystem identities or mutation permission. No
result cache, index, preload, dependency, decoder or new preview is introduced.

## Compact content results

Extended content statistics reserve44px only when a whole result/snippet row
also fits. Compact status retains L(limited), B(binary), Δ(changed), E(errors),
M(mounts) and partial status, with the F6 tooltip legend/limits. Painting and
keyboard paging share the same height rule. At compact two-column widths,
criteria cells fit all four fields above the footer. The existing scene checks
current input geometries and targets in criteria mode and real rows in results
mode; an intermediate unpainted state cannot satisfy the gate.

Resource/startup experiments remain frozen and new previews paused. Native
input, OS clipboard, AT-SPI, process-lifetime coverage, full ROADMAP and global
acceptance remain open. Local byte/FD limits are not a process-memory claim.

## Validation

993Rust334core/85helper/574UI+3existingignored PASS, Clippyalltargets-Dwarnings,
release/fmtPASS. Final saved-search-ui-1793475882305119370 scopedPASS:4processes
exit0ACK,14realFLTK frames,6loggedPID/start helpers gone/reused strictZno. Save
review, load-without-execution then Enter, order/fields/hidden/primary editor,
raw current and persisted roots, cancel, exact new-row selection, rename/remove
metadata-only, future-file preservation and compact criteria/result rows verified.
Native pointer/keyboard, OS clipboard and full process-lifetime coverage remain
unvalidated. Storage failure/unsynced tests and inherited-lock proof have their
explicit unit scopes, no power-loss/stuck-I/O claim. Original FAILs and first
scopedPASS before selection fix are retained with sidecars, not overwritten.
See evidence/search-order/saved-search-validation/checks.json and source logs.
