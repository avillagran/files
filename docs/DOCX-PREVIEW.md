# DOCX text — 0.0.4, M21

**Bounded functional acceptance PASS**: 500 Rust, Clippy/release/fmt, 10 wire cases
and a real DOCX scene. The earlier ENOSPC FAILs are kept as history, without
attributing their cause. A consistent installation is recorded separately. Resource/startup
freeze in effect: NOT_MEASURED, no global PASS nor 0.0.4 tag.

## Prepared implementation

Space on a .docx reuses the ephemeral helper, the cancel/timeout/reap supervisor,
TextDocument/selection/copy handler and the last-destination State. The existing parent watch +
epoch/generation and visible reconciliation also apply to DOCX.
The request carries the expected Revision; the helper opens the final file O_NOFOLLOW/NONBLOCK, regular,
compares the FD before reading and revalidates FD/name after ZIP/XML. OfficeText returns
the effective revision; Job and State compare it before Ready. Symlink parents/mounts
follow OS semantics: it is not folder confinement nor an atomic snapshot.

Only the main body word/document.xml, UTF-8, normal/strict Word namespaces.
It keeps text from runs, paragraphs, tabs, breaks and the visible text of links (inert).
It does not show field instructions nor deleted text. No faithful Office rendering,
headers/footers/footnotes, images, active links, macros or external resources.
XLSX/PPTX/ODF, old binary formats, DOCM and UTF-16 XML are still pending.

The own ZIP32 listing is reused. It now exposes flags/CRC/local offset as
metadata, without changing the read-only catalog's warning. The helper reads ONE
member into RAM: exact local name, matching flags/method/sizes, the payload
area before the central directory, bounded/non-ZIP64 local extras, STORED/DEFLATE and
an optional DataDescriptor, all verified. Exact CRC and output length. Encryption,
duplicates, symlinks/non-regular types or unknown methods/flags are rejected.
It does not validate the other payloads nor extract files; it never uses the names as OS
paths. This does not complete the roadmap's decompress-files operation.

## Limits

Regular file ≤8 MiB; the existing ZIP listing ≤10000 entries/4 MiB central/256 KiB of names.
Member XML ≤1 MiB declared BEFORE allocating, and the real output checked
per 4096 B chunk before append. CRC over the full decoded bytes. The
DEFLATE of the pinned flate2 1.1.10 rejects an incomplete EOF (zio.rs:155–160), in addition to
checking the exact total_in; the declared length alone is not accepted.

Streaming XML with quick-xml, no DOM: ≤65536 events/depth 64/64 attributes per element,
name ≤256 B/attribute value ≤4096 B, no DTD/PI/unknown entities nor loading of
resources. UTF-8 input and XML characters checked; visible text ≤256 KiB/2000 LF,
explicitly truncated at a UTF-8 boundary. The rest of the XML is still validated after
truncating the presentation. Namespace/parser/allocator have overhead: these limits
are NOT a PSS cap. Wire ≤2 MiB before JSON, decoded text bounded again.
The existing ≤5 s supervisor kills/reaps before delivering the response. The helper's RLIMIT_AS 96 MiB
limits virtual space, not PSS nor the total 150 MB budget. A kernel
D-state syscall can prevent reap/close: no offline deadline nor fake ACK is promised.

Only buffers/pipes: no persistent cache, product temporaries nor files next
to the original. The path label may represent invalid bytes with replacement characters;
path/selection/IPC keep raw bytes. Selection works on the extracted text,
not on XML offsets nor the original layout. The current text renderer paints at
most 4096 B per line; the rest stays in the document for selection/copy, but
the full horizontal window for long lines is still pending. Exact
shaping/graphemes are not claimed just because Unicode is kept. The real clipboard is still pending because of
Wayland/CUA focus; invoking the Copy handler does not prove the clipboard's content.

## Evidence and validation blocker

- Isolated helper 66/66 PASS after cleaning only its own dev cache; includes 8 new
  ZIP/XML tests. It does not measure use nor complete the UI.
- The workspace built every test binary with DEV/TEST_DEBUG=0 and incremental 0
  (per-command flags, not a permanent change); core 136/141 FAIL because of 5 ENOSPC in
  rename/persistence/source fixtures. UI/helper never got to run because of Cargo.
- After the coordinator cleaned regenerable ELFs, two of those fixtures passed with the
  existing binary and the third failed again with ENOSPC in atomic_write(state.toml).
  The attempt was stopped, with no other rebuild nor cleaning of others' files. No exact Btrfs cause.
- With those existing binaries, 9 purely in-memory tests PASS: 3 ZIP,
  4 XML, 2 Job/State (CRC, compression/bounds, DTD/entities, Unicode/truncation,
  Verified A→dirty B/late Ready A and a wrong effective signature). They are not the global suite.
- The first build failed with ENOSPC writing rmeta. A later retry hit
  E0599 because the protocol was edited while building: result discarded and the stable
  tree rebuilt. Both logs kept, without attributing that race to the engine.

Logs: target/office-helper-first-test.log, office-helper-after-clean-test.log,
office-helper-stable-test.log, office-helper-debug0-test.log,
office-workspace-test.log, office-core-five-fixtures-after-coordinator-clean.log,
office-ui-memory-tests.log, office-parser-memory-tests.log, office-zip-memory-tests.log.
Static dependency audit: evidence/0.0.4/office-dependency-audit.json.

scripts/office-ui-check.py is prepared, NOT run: it waits for an exact Revision and a
fresh READY; it captures text/selection, raw path, replace keeping size/mtime,
delete/recreate with callbacks suppressed, CRC/XML/size/symlink rejection and truncation.
Before accepting it, a normal host, suite/Clippy/release and a real scene,
visual review and a lifecycle without children are missing. This WIP is not installed nor tagged.

## M21 implementation review (2026-10-27)

- `core/archive_list.rs:100–214`: full catalog, limits before copies,
  duplicates flagged on both entries; rejects a truncated central directory, no partial list.
- `helper/zip_member.rs:22–170`: only a fixed member, local/central validation,
  size before reserving output, expansion checked per chunk, CRC, exact DEFLATE
  consumption and descriptor; it writes no members. The catalog does not prove the
  integrity of other payloads.
- `helper/office_preview.rs:31–42`: expected revision against the real FD before
  ZIP/XML and FD/name revalidation after parsing; `office_preview_job.rs:64–97`
  verifies id/effective revision/caps before publishing. State also checks
  generation and revision (`text_preview_state.rs:1173–1210`).
- `office_preview.rs:195–241`: the RED regression kept in
  `target/office-xml-regression-red.log` reproduced CDATA before the root. CDATA
  outside the root is rejected even when empty, as is a repeated/non-initial declaration; a valid BOM
  and outer comments are still accepted. This does not imply validation of all of OOXML.
- Office returns ordinary errors for codec/XML/cap, not `SourceUnavailable`.
  State keeps the expected revision in Error and `needs_start=false`; reconciling
  the same revision does not launch another decode. Only a real change/invalidity of the source
  allows another request. The policy is visible in `verify_source:614–625`,
  the Err branch 1318–1331 and dispatch 1337–1348; no native watcher test is claimed.
- The shared supervisor (`text_preview_job.rs:295–357`) bounds the wire before
  append, cancels/kills and waits for the direct child before the result. It introduces no
  grandchildren. Kernel D-state keeps the documented limitation; Drop only cancels and a
  normal close must keep pumping until ACK.

After the user reported the space recovery, the coordinator ran the
five exact fixtures that had been affected: PASS with the existing binary,
`target/office-core-five-coordinator-restored-space.log`. The new full suite
passed 500 tests (141 core/67 helper/292 UI), Clippy workspace/all-targets
-D warnings and fmt PASS. New logs `office-workspace-restored-test.log`,
`office-clippy.log`, `office-fmt.log`; the earlier ENOSPC FAILs remain intact.
Release/scene/installation still pending when this review was written.

## Functional acceptance after recovering space

- `target/office-workspace-restored-test.log`: 500 Rust (141 core/67 helper/292 UI)
  PASS. `office-clippy.log`: workspace/all-targets -Dwarnings PASS;
  `office-release.log` and `office-fmt.log` PASS. The first release was interrupted
  on purpose because of a competing unrelated build; separate log preserved.
- `office-xml-wire-restored.json`: 10/10 real helper cases PASS, including BOM,
  inner CDATA, invalid prologue/epilogue and a declaration repeated/after a comment.
- `office-ui-1793105654289278359.json`: 6 real framebuffers 1000×650, Unicode,
  a 102 B selection and explicit truncation at 256 KiB; CRC/size/DTD/symlink with fresh
  errors; lossless Unix 0xFF names. A replacement keeps size/mtime and changes
  identity; delete/recreate invalidates and recovers even with notify callbacks suppressed
  in the fixture (visible reconciliation). The last destination wins; exact FD stamp on Ready.
- Normal callback→cancel/drain→exit 0, 9 registered PID/start absent, zombies not
  accepted. It does not prove a helper alive right at close nor every lifetime.
- Internal handlers/framebuffer, no OS input/external clipboard/faithful Word layout.
  Resources/startup NOT_MEASURED; the incidental PSS footer is not used as a gate.

## Authorized consistent installation

Source checkpoint 8032120 installed in ~/.local/bin with an earlier private backup and
SHA256 verification of the five artifacts (app/helper/HTML/licenses/stamp).
`installed-checkpoint-1793105752211923245.json` keeps the full paths/hashes.
Files 9703332b6c7d3767aedb769c07eb3080e6c2c07a7e23a9994b704f3b1cd3a6db,
helper 1b06c98aa9037ee2ab1d461e874de08fab87823fcd0ef33f480b245a5ef230f5,
HTML 85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
M20 replaced with a backup; no other windows controlled and no publication.
