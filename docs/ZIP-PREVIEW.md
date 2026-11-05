# Read-only ZIP catalog

0.0.4, partial. Space on a .zip shows the real central directory: names,
declared compressed/uncompressed sizes, method, Unix mode and labels for
symlink, encryption, unsafe path or duplicate name. It does not extract, decompress
or run entries. It does not check local headers, payload or CRC. A consistent
catalog does not certify that it can be extracted safely.

## Limits before allocating

Own parser in core, with no new dependency; it runs only in files-helper.
It looks for the EOCD in the last 65557 B and requires an exact end/comment, a single disk
and ZIP32. ZIP64, trailers and additional central signatures are explicitly
unsupported. Arithmetic checks of offsets and lengths, up to
10000 entries and a 4 MiB directory before reserving it. Names up to
4096 B and 256 KiB in total before copying them into entries/the duplicate index.
Those caps do not include every container nor the process PSS.

Names keep their exact bytes. The presentation escapes control characters,
backslashes and non-UTF-8 bytes; that display string is never used to open a
file. Duplicates are byte-equal, without promising casefold or filesystem Unicode
equivalence. Absolute paths, parents, drives and backslashes are flagged
as unsafe; no internal name ever reaches the filesystem.

The Linux helper opens the final regular file with O_NOFOLLOW/O_NONBLOCK, up to 8 GiB,
and uses that FD while reading; parents/mounts and in-place changes are not a
snapshot. Text output up to 128 KiB/2000 lines with listed/total counts and a visible
notice. Wire up to 1 MiB under a 5 s supervisor; it validates id/type/encoding/text.
The helper's AS 96 MiB is not PSS. State cancels and reaps before the next job.

## Evidence

288 Rust tests (79 core, 37 helper, 172 UI), Clippy all-targets -D warnings and
release clean. zip-ui-1793040518449068984.json functional PASS: real catalog
with Unicode, symlink, traversal/drive and duplicates; a non-UTF-8 name shown
as raw\xFF.bin; empty ZIP; 2500 entries with the listing truncated to 130846 B;
malformed and 10001-entry archives rejected; last destination/normal close after ACK.
The initial Clippy warning is kept in evidence/zip-preview/initial-clippy.log.

Method: internal handlers and real framebuffer, no OS input/clipboard. No
file was created from the entries. Closing before the recorded new spawn
does not prove that an active helper is killed. Resources/startup NOT_MEASURED because of the phase.

Create/extract, destination protection, expansion limits, TAR/other formats,
ZIP64, content validation, Office and the budget are still pending. This
delivery does not complete Z01 nor P08/P09.

## 2026-10-31 — Shared streaming foundation (source only)

ARCHIVE-OPERATIONS-CONTRACT.md documents archive_member and archive_extract
read-only planning. Catalogue display remains observational (no payload CRC).
Office uses the shared bounded decoder; no new preview feature or installed change.
Creation/extraction destination execution/UI remain pending; do not infer them
from a codec or plan test. Parser additionally retains central needed-version
for local-header agreement in the shared member reader.
