# Search display order

F7 or the Nerd order button cycles Path → Name → Kind. The selection remains
bound to the same raw result path when cycling; Up/Down/Home/End/Page keys and
scrolling follow the displayed order. Enter opens and Alt+Enter locates the
selected result through existing browser/open validation. F1 exposes the shortcut.
New search windows default to Path; this is transient, not a global sort setting.

Path compares raw relative paths. Name uses the existing NaturalKeyRef contract:
Unicode lowercased presentation keys, numeric digit-run comparison without integer
conversion, original raw name tie-break, then raw relative path for equal basenames.
This is not Unicode normalization/casefold/locale collation. Kind groups directories,
regular files, symlinks, other, then natural basename/path. It is entry kind rather
than MIME/extension sorting. Reverse/size/date/relevance ordering remains pending;
no metadata is fetched to simulate those orders. Hidden/scope/facet limits remain.

The wire report/provisional Arc and their exact terminal reconciliation remain
unchanged. Only an index vector (<=500 slots, allocator capacity may have slack)
is retained; no cloned paths/snippets/RGB, added helper, worker, IPC or disk cache.
Accepted population changes invalidate the index even if an allocation address is
reused. Invalidated helper results are inaccessible before drawing/access. Old
painted hits are cleared when search pump changes visible state or sort changes.
Selection's physical index stays distinct from display position; final reconciliation
keeps existing raw-path selection remapping. Closing releases the order index.

Sorting is cached by source and order; unchanged repaints don't rebuild keys or
sort. Natural basename keys are transient, allocated once per admitted population/
order change and dropped after sorting. Their folded strings can allocate more
than the original byte count; this is additional transient memory bounded by the
existing500-hit/256KiB-path protocol, not a PSS or speed proof. No file content or
filesystem metadata is reread. Incremental arrivals may move displayed rows;
selection is kept and revealed, not mistaken for a new operative result.

## Scoped validation — 2026-10-31

858 Rust PASS269core/85helper/504UI,3 existing core ignored; Clippyworkspace/all-
targets-Dwarnings/fmt PASS, logs target/search-order-tests/clippy/fmt.log. Tests
cover natural2/10, equal basenames/path ties, kind grouping, raw lossy-label ties,
invalid local population cap, selected raw path and visual keyboard movement.
Release PASS (`target/search-order-release.log`).

`evidence/search-order/search-order-ui-1793437888618938167.json` PASS:
five real FLTK framebuffer PNGs1000×650/440², all five inspected. Existing
fixture gate holds first batch≤5s (not natural latency): change to Name while
provisional, final42 hits retain selected raw path; numeric2before10/equal-name
path ties, directory-first/symlink-last Kind, visualHome/Down, raw path order,
raw0xFF selected after reordering→Locate exact row440. Original source hashes
unchanged, root exit0/drainACK, one logged PID/start absent/reused (Z not accepted).
No whole-lifetime/exact-active-close claim.

Regression `evidence/search-batches/search-batches-ui-1793437899122910460.json`
PASS seven framebuffers/six logged helper identities gone or reused strictZfalse,
root0ACK; cancel/root replacement/newquery/raw content/provisionalLocate unchanged.
Two regression440 PNGs inspected: snippet highlight and located raw row intact.

Original1793437832543422732 FAIL preserved+sidecar: report42 already accepted,
but harness read pre-paint state (row_hits0/display_paths0). Corrected harness
requires painted nonempty rows and index count matching population; production
and deadlines unchanged. Do not promote original to PASS.

Source `64e18d3` installed through
`evidence/0.0.4/installed-checkpoint-1793437954301097754.json`.
Five installed artifacts and five backups independently SHA256 rehashed exact.
App `6bf10fe18b942763af3df026c1ae5651409cf2c0688bdc66dab6a4ee6c75f7dc`;
helper `d6bc5579a3072be862d3f62c3ebf88e6ac29ca85c25b520faf071fa2256ebe82`;
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user installation; no source push/tag/global config/window control. No native input/protocol/clipboard/lifetimes/
resource/startup/global acceptance. Freeze and new-preview pause remain.
