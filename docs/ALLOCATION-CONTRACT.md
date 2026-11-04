> Current product integration: see [ALLOCATION-UI.md](ALLOCATION-UI.md). Earlier source-only milestones below are historical.

# Optional regular-file allocation observation

Status: source helper/core wire integration; see ALLOCATION-WIRE-CONTRACT.md.
The source worker/queue now retain it; no installed binary or user control uses
it yet. Report presentation is pending.

The current calculator totals `st_size` for each regular-file name. Hardlink
aliases count repeatedly. Directories, symlinks and special files add no bytes.
`file_properties::Properties` already exposes a single entry's allocation.

Linux defines `st_blocks` in units of 512 bytes, independently of `st_blksize`.
Holes can make this allocation smaller than apparent size. Device and inode
together identify a file within a filesystem namespace; `st_nlink` is a link
count, not an authority for how many aliases this traversal will encounter.
Sources: [stat(3type)](https://www.man7.org/linux/man-pages/man3/stat.3type.html)
and [inode(7)](https://man7.org/linux/man-pages/man7/inode.7.html).

## Foundation implemented

`du::measure_allocated_progress` opts into the existing held-directory-FD
walker. It returns the unchanged apparent `Usage` and an `Observation` only
after traversal succeeds. The default and streaming callers pass `None` and
allocate no identity collection. The extra observation consumes the same
regular-file metadata already read by the walker; no second traversal is added.

- `by_name`: checked sum of regular-file `st_blocks * 512`, counting aliases.
- `by_identity`: checked sum once per observed `(device, inode)` within this root.
- Every identity is tracked, including zero-block and single-link files. No
  `nlink > 1` shortcut can silently miss an alias after a live link-count change.
- The collection admits at most 65,536 identities. A repeated identity at the
  limit remains valid. A new identity beyond the limit makes `by_identity`
  unavailable, clears retained identities, and continues apparent/by-name work.
- Conflicting block counts for a repeated identity make that metric unavailable.
  Overflow, identity limit and conflicting observations are distinct reasons;
  unavailability is sticky and never represented as zero or a partial total.
- Name and identity sums fail independently. An overflowing name sum of repeated
  aliases can coexist with a valid identity sum.
- Empty successful scope has measured zero. A failed traversal returns an error,
  with no partial successful observation. Relative, oversized and NUL paths are
  rejected; final symlinks are rejected by the existing directory entry point.

Only regular files contribute to these two metrics. Directory inode blocks,
symlink inode blocks, filesystem metadata and special files are excluded. This
is deliberately narrower than whole-directory disk usage from external `du`.

## Meaning and limitations

Neither metric proves physical ownership, exclusive extents or space freed by
deletion. Shared/reflink/compressed storage is not resolved. Summing separately
captured roots may count a cross-root hardlink repeatedly. Root observations do
not carry identity sets into UI caches or imply global deduplication.

The existing traversal is live, not a snapshot. Reuse of an inode or changes with
the same observed block count cannot be detected by this accumulator. Descriptor
pinning, final root device/inode recheck, mount traversal and parent-symlink policy
remain those of the existing calculator. The cooperative 15-second timeout
cannot interrupt a filesystem syscall. Existing depth 128 and million-entry
limits remain; identity cardinality is a bound, not measured memory evidence.

## Integration still required

The explicitly versioned/opt-in typed wire contract and bounded reader now
exist in source and are connected to the typed worker and per-attempt queue.
Keep literal legacy `Usage`/`UsageProgress` JSON and default list sorting unchanged.
Carry optional metric availability into
captured report, scoped forgetting and UI labels. Progress must not present an
incomplete allocation as final. Existing cancel/drain/ACK and attempt identities
remain mandatory. Define cross-root subtotals without implying shared-identity
deduplication. Only then run helper CLI and X11/Wayland/installed functional
acceptance and install the reviewed feature.

No PSS, Swap, CPU, startup or other resource measurements are part of this work.
