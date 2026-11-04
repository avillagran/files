# Capture publication and observational audit

Status: 1,210 workspace tests (425 core/95 helper/690 UI, three existing
ignored), final Clippy with warnings denied, formatting and release passed.
Source functional acceptance passed; installed on NODE. The first anomalous X11 frame remains
in `evidence/path-labels/validation/render-assessment.json`; no cause or fix for
its missing left-pane labels has been established.

The capture path is fixture-only (`--ui-smoke` plus FILES_TEST_CAPTURE). Empty
legacy requests remain observational. Optional JSON requests (at most128 bytes)
carry a positive id and optional redraw boolean; other fields/types are refused.
The response logs id, explicit-redraw flag, last full-paint revisions before and
after flushing, and captured dimensions. These counters prove callback execution,
not that every pixel belongs to a particular model revision or native input.

PPM publication now validates RGB dimensions, writes a create-new private file
in the destination directory, closes it and renames it into place. Readers see
a completed publication rather than an in-place file being extended. No fsync
or crash durability is claimed. Owned temporary files are cleaned on errors;
create-new collisions never authorize deletion of an existing file. This fixes
a publication weakness, not a confirmed cause of the original anomalous image.

`duplicates-ui-check.py --capture-audit --name-labels` requires the matching id
ACK before reading. PNG conversion uses the same immutable bytes already read.
For the root dual frame it retains observed, observed-again and explicitly
redrawn captures. It compares only the fixed fixture's left names/icons region
(x70..350,y160..420 at1000x650,1x), excluding headers, metadata and footer.
Explicit redraw must advance the full-paint counter; observational mode never
silently redraws. A pixel difference fails the audit and preserves all images.
Equality in some runs does not prove absence or root cause of an intermittent
app/render/capture fault. No native-input, accessibility,2x or lifetime claim.

Existing automatic footer values are not resource evidence. No PSS/Swap/CPU/
startup experiment is part of this audit; the freeze and preview pause remain.

Source audit receipts: `evidence/duplicates/node-functional-1793525549303666587/receipt.json`
(X11), `1793525589163827526/receipt.json` (X11 repeat, same parent directory),
and `1793525580202075468/receipt.json` (Wayland). Each run passed21 captures,
two drained root exits and six logged duplicate-helper identities gone/reused.
For each paired root frame the observed/observed-again/explicit-redraw ROI hashes
were equal, and explicit redraw advanced the full-paint counter. The first X11
observed frame was visually inspected and showed both panes' names/icons.
The original defect did not reproduce in these samples and remains open.

Unit tests cover invalid-dimension preservation of an old publication, complete
replacement, bounded/closed request shape and an acknowledged concurrent reader
observing only complete old/new frames during sixteen publications. Initial
compile failure from an unavailable direct serde derive was replaced with the
existing serde_json parser; an unnecessary vec in a test was also corrected.
Both failing logs remain in `evidence/capture-audit/validation`. No dependency
was added. Competitor Cargo builds were only waited for via the idle guard.

Source `e9281b5` installed with app
`9e01f8493b57818f002c560e351d4ed8e420f9f14bc66d0a28a01570efcaf500`;
helper006fdb1a8f3a/HTML04aa7953202d unchanged. Install record
`evidence/0.0.4/installed-checkpoint-1793525756279100968.json` and
`evidence/capture-audit/install-verification.json` verify five backups and
installed hashes. Installed Wayland receipt
`evidence/duplicates/node-functional-1793525803034684286/receipt.json` passed
21 captures/two drained roots/six logged duplicate-helper identities gone/reused,
matching observed/repeated/fresh ROI hashes and advancing explicit paint count.
Its observed dual frame was visually inspected with both panels present.

Legacy empty-request compatibility passed installed SHA regression
`evidence/checksums/node-functional-1793525879444100701/receipt.json`
(seven captures/two drained roots/four logged helpers gone/reused) and existing
HTML regression `evidence/0.0.4/html-ui-1793525892091755963.json`
(five captures/drained root exit). GUI fixtures ran sequentially.
The original anomaly remains unresolved; this audit is not its causal fix.
Raw validation logs retain their original terminal blank lines.
