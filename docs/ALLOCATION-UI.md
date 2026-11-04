# Regular-file allocation in the size manager

Installed on NODE: source8606261, checkpoint1793539706031130409. Installed
acceptance1793539716211647444 passed22framebuffers/2root0+drain/25logged helpers
gone/reused; all five installed and five backup hashes verified. App
`b9941ab1acfb49f29ae5ad220f399ae425ace5d2739c96c8ac410bce653c28c9`; helper
`a8fc8abaf26c55f799fc77b5c86e7c1e9150c94105fc61f987acf81c0ee0cfbd`.

The action catalog offers **Medir bloques de la carpeta enfocada** and **Medir
bloques de carpetas marcadas visibles**. These are explicit opt-in calculations;
existing apparent-size actions keep their behavior. Only visible real directory
entries are admitted. Marked mode excludes files, symlinks, filtered marks and
the unmarked focus. Empty selection and a busy/draining queue reject atomically.

The size manager exposes **Bloques · B** for its selected row and **Marcadas ⇧B**
for marked browser directories. The row action uses that row's raw name and
attempt ID, independently of browser focus. The current owner, generation, queue
epoch, visible directory entry and idle checks still apply. Old row tickets
cannot silently target a replacement. Report mode never starts a calculation.

Nine footer controls share the existing three rows; the queue/details viewport
height is unchanged. Both new controls are mouse accessible. B and Shift+B work
from queue/details; the action catalog provides keyboard access from the browser.
The **· B** marker identifies allocated-mode rows without replacing the existing
apparent value used for size sorting. Enter opens the full scrollable detail.

## Values shown

Allocated mode captures apparent size plus two observations of regular-file
allocation in512-byte units reported by the filesystem:

- **Bloques por nombre** counts each file name, including hardlink aliases.
- **Por identidad en esta carpeta** counts each observed device/inode once within
  that directory traversal. Two separately measured roots can repeat an inode.

Directory/link inode blocks, filesystem metadata and special files are excluded.
Neither value proves physical ownership, exclusive extents or space freed by
deletion. Compression, reflinks and shared extents are not resolved. This remains
a live traversal, not a snapshot. Full semantics and limits are documented in
ALLOCATION-CONTRACT.md, ALLOCATION-WIRE-CONTRACT.md and ALLOCATION-QUEUE-CONTRACT.md.

Measured zero is displayed as zero. Missing observations remain **SIN OBSERVACIÓN**;
overflow, the65,536-identity limit and conflicting identity observations remain
distinct **NO DISPONIBLE** reasons. A valid apparent total survives unavailable
optional metrics. Pending/error/cancelled attempts never acquire a false final
allocation. Retry/recalculate retain the requested mode and clear old allocation.

## Captured reports

Reports retain the apparent subtotal and include requested mode and completed
per-root block observations. They explicitly reject the interpretation of these
individual observations as a union across roots. No block subtotal implies global
hardlink deduplication or freeable space. Reports containing only apparent-mode
rows retain their existing format.

Capture is immutable and performs no filesystem scan. F5 in a report explicitly
recaptures cached observations; B does not start a helper from a report. Existing
marked-only/all-queue scope,64KiB output bound, scrolling, copy request and captured
measured-ID binding for forgetting remain intact. Forgetting observations does
not modify files or confer deletion authority.

## Acceptance scope

The extended `scripts/sizes-open-ui-check.py --allocated` drives the real focused
catalog action, selected-row mouse handler and marked Shift+B, then checks actual
helper results against metadata of owned sparse/hardlink/raw-name fixtures. Shared
hardlinks in two measured roots are checked independently. It also runs the prior
apparent/marked-report regression,440px enabled-control bounds/nonoverlap, captured
report values/no-extra-helper behavior, scoped forgetting and source-file hashes.

Native OS input, clipboard delivery, AT-SPI,2x/full-lifetime and resource/startup
acceptance remain separate. Footer resource numbers are not evidence. No new
content previews or PSS/Swap/CPU/startup experiments are part of this work.

Source acceptance:1258Rust443/105/710+3historical ignored, Clippy/rustfmt/release
PASS;7helper CLI cases PASS. Wayland1793539566646617514 and
X111793539569606454785 each22captures/2root0+drain/25logged helpers gone/reused.
Sparse/hardlink metadata matched; selected-row B preserved unselected IDs and
browser focus; captured report did not calculate and Forget preserved files.
Raw historical WouldBlock and two corrected fixture dispatch/menu failures are
retained in `docs/evidence/allocation-ui/assessment.json` and linked receipts.
