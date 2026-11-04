# Visible synchronization dry-run

The panel prepares and reviews synchronization proposals. It does not apply
changes, submit transfers, create backups, retire objects or recover a previous
operation. Full C02 execution still needs a fresh pinned object manifest and
recoverable replacement/removal journal, queue integration and recovery/undo.
A reviewed report is not mutation authority. The full explorer goal remains active.

Open Actions / Ctrl+Shift+P → **Preparar sincronización de carpetas** or the
background menu. The same panel also retains ordinary comparison.

- A initially captures the active folder; B captures the opposite folder in Dual
  or is entered manually. A/B are comparison roles, not physical left/right panes.
  Raw/control paths use the existing reversible local file URI editor. No shell
  expansion or visible byte-escape decoding is added.
- Click the mode header or press F6; Shift+F6 moves backward. Modes are Compare,
  Update A→B, Update B→A, Mirror A→B, Mirror B→A and Merge A↔B. F5 starts a new
  preparation. Editing a root, mode or hash budget invalidates old results and
  decisions. No root resolution walk/hash runs on the UI thread.
- Update copies source-only proposals and retains destination-only names. Mirror
  proposes recoverable retirement of destination-only names, requiring an explicit
  decision. Merge proposes unique names both ways; without a baseline, it never
  infers a deletion or automatically chooses the newer side.
- Content, literal link target, type and basic metadata conflicts start Pending.
  Select a row and click Decisión/F9 to cycle the legal choices; Shift+F9 reverses.
  F10/Pend. restores Pending. Choices are Pending, Omit, preserve-and-replace from
  the legal source side(s), preserve-and-set-basic-metadata, or preserve-and-remove
  for a mirror-only destination. The selected choice is visible above the rows.
  Recovery wording specifies required future behavior; no backup is created here.
- A copied/retired/type-conflict ancestor covers its descendants once. Covered
  rows retain their observation and report the ancestor row index; they do not
  offer independent decisions. Shared directory metadata and child proposals
  remain independent. Empty directories, hidden names and raw paths participate.
- F7 cycles differences, unknowns, all, pending decisions and proposals. Resolving
  a pending row removes it from that filter. Choice/filter/selection changes
  invalidate old action stamps, so a queued click cannot decide a different row.
  Any partial, changed or unsupported inventory blocks every proposal, including
  otherwise known missing names. Zero pending decisions is not permission to Apply.
- Rows and hits are virtualized. Up/Down, Home/End, Page and wheel reveal selection;
  Left/Right pan only the selected long path. Controls fit440×440, retaining at
  least one visible result row and mouse-accessible per-row decisions.
- Enter/Shift+Enter or Ubicar A/B locate the exact raw observed name using a fresh
  browser listing. They neither open the object nor mutate it. Tab switches
  roots/results; Ctrl+L edits A. Existing text cursor/paste/IME bounds remain.
- Informe/Ctrl+C builds the complete bounded diagnostic before requesting a copy;
  no truncated copy is emitted. It includes mode, roots/IDs, blocked state, pending
  count, all observations/proposals/choices and the remaining execution/recovery
  limitation. Without an accepted final result it explicitly reports no verdict.
  Copy preserves an existing error. Native clipboard delivery is a separate gate.

The existing comparison owner now holds either comparison or sync-review Job.
It suspends thumbnail work and size admission, stops SpaceMap, waits for previous
readers, and admits one helper. Cancel/Close retain the Job through EOF/exit/reap
ACK; window close cannot bypass it. A success already queued when Cancel is
requested is discarded. Root/owner context mismatch also discards results. The
previous size pause is restored after ACK, and Map resumes after panel close.
Neither comparison nor sync observations are duplicated into a second full row
array; the panel reads whichever retained result it owns. The sync job module is
private now that its visible owner is integrated.

Root symlinks, canonical containment and identical roots produce errors. The
underlying comparison remains bounded and read-only; it is not an atomic snapshot
or proof against all bind/submount aliases. Metadata is basic mode/mtime, not
ACL/xattr/owner or hardlink topology replication. Apply must independently prove
its sources, destinations, absence, topology and recoverable effects. See
DIRECTORY-SYNC-CONTRACT.md for the execution stages and engine reuse audit.

No PSS/Swap/CPU/startup experiments or new preview formats. Internal handlers and
owned framebuffer captures do not prove native OS input, clipboard, AT-SPI, 2x,
other OS, global helper lifetimes or offline syscall deadlines. Functional owned
runtime evidence and installation hashes are recorded separately in
`evidence/directory-sync-ui/assessment.json` once accepted.

## NODE functional acceptance

1396 workspace tests489core/112helper/795UI plus3 historical ignored pass,
including8 new controller/UI tests. Clippy workspace/all-targets-Dwarnings, fmt
and release pass. The first run failed2 new fixture assertions: pausing an empty
size queue legitimately remains unpaused, and changing synthetic progress
legitimately requests repaint. Fixtures now enqueue actual work and check retained
busy ownership instead of asserting no repaint. Full red/green logs are retained.
Two final module-documentation corrections were followed by fmt/release rebuild;
the changed build identity was rechecked on both backends.

Final app289db327a121ffb8eed49f0386443fa7fbc4b0dc3b07b99cf8207d096bfce2f9,
helper170f995dc6d4f86e6deeb5ef47257aef91fda3654e42f0254c3bc9d485a0ef0f,
HTML04aa7953202d unchanged. Final source Wayland1793574428421481675 and
X111793574443373162313 each pass19captures/6normal root exits+drain and12logged
sync-reader PID/start identities gone/reused. Earlier40493d72 module-doc build
receipts1793574219972422123/1793574264512852275 also pass and remain retained.
Comparison regression on the final binary passes Wayland1793574433356876800 and
X111793574451607334660, each14captures/6roots/7logged comparison identities.
Release helper18real CLI cases pass1793574477573675033, including both full
8192-name retained inventories without truncation. GUI fixtures ran sequentially
per backend, with one Xvfb wrapper at a time; Rust tests kept normal concurrency.
These are scoped functional results, not native input or resource acceptance.

Installed checkpoint71eb40f/record1793574522897173033 passes the same Wayland
scene1793574546356530935:19captures/6normal root exits+drain/12logged sync-reader
identities gone/reused. All five installed hashes and five backups match the
installation receipt. This installs the dry-run only; it does not implement Apply.
