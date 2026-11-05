# Folder summary in Space

0.0.4, partial. Space on a folder shows its recursive apparent size and
descendant entries. It reuses the existing du helper: no walk or
filesystem probes on the UI thread. The folder intent comes from the listing.
The new preview_directory command requires a real folder when it opens it; if the
name was replaced by a file or a symlink, it returns an error. The original du
command keeps its API for computing file sizes.

The Linux walk anchors each directory with an FD and O_DIRECTORY/O_NOFOLLOW.
It does not follow symlinks, counts hardlinks by name and measures apparent bytes, not
physical blocks. It is not a snapshot against concurrent changes. du limits:
1M entries, 128 levels and 15 s cooperative; supervisor 20 s, cancellation/reap
outside the UI. A syscall on an offline mount may not be interruptible by
the signal; that scenario and adapters for other OSes are still pending.

279 Rust tests (73 core, 35 helper, 171 UI), Clippy workspace all-targets
-D warnings and release clean. folder-ui-1793040015107090730.json functional
PASS: own tree of 8 bytes/5 entries with a cyclic symlink and a link to another
1 MiB file excluded from the total, empty folder, folder replaced by a
file after listing, quick changes and normal close after ACK. Method: handlers
and real framebuffer; no OS input and no test killing an active child on close.

Resources and startup NOT_MEASURED because of the user's explicit phase. The
earlier du gate is not extrapolated to the current binary. Treemap, cache, tree
snapshot and offline mounts are not complete.
