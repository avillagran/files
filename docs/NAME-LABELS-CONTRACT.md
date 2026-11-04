# File name labels

Status: 1,206 workspace tests passed (425 core/95 helper/686 UI, three existing
ignored). Initial Clippy passed. A later extension-badge presentation edit keeps escaped
labels intact and uppercases only ordinary extensions. Final Clippy passed after that edit. Release and scoped X11/Wayland functional acceptance passed. Installed on NODE with five verified backups and installed hashes. Competing Cargo jobs were never controlled. Normal Unicode borrow, byte/control distinctions and painted
Groups/List/Grid targets are tested.
The per-user installed checkpoint now includes the labels.

`name_label::label` is presentation only. Filesystem operations, focus, selection,
filtering and sorting keep their original OsString/path values. No parser or
rename uses these labels. Ordinary Unicode names borrow their UTF-8 string.

Backslashes double; LF/CR/TAB become visible `\n`, `\r`, `\t`. Other ASCII controls
use `\xNN`; Unicode controls, directional overrides/isolates and Unicode line
separators use `\u{HEX}`. Invalid native encoded bytes use uppercase `\xNN`.
Thus a raw FF byte, the literal text `\xFF` and a Unicode replacement character
remain distinguishable. Encoded display length is at most four times native
encoded length. These labels are not shell quoting or a wire path format.

Current integration covers normal folder/file rows in Groups/List, Grid,
Columns, dual panes, Spatial and the size map; the command palette focus label
uses the same representation. Original names remain the operation targets.
Other path surfaces and adapters need a separate inventory; this is not a claim
that all user-facing paths have been migrated. Native input, accessibility,
clipboard, 2x sharpness and resource/startup gates remain open. New previews
remain paused and resource experiments frozen.

The functional duplicate checker has an optional `--name-labels` capture path
prepared for Groups/Grid/Columns/dual panes after locating the raw filename.
It passed on X11 and Wayland with the new release. This is not native input proof.

Final source receipts: `evidence/duplicates/node-functional-1793523651092796411/receipt.json`
(X11) and `evidence/duplicates/node-functional-1793523667678886824/receipt.json`
(Wayland), each 16 captures, two drained roots and six logged duplicate-helper
identities gone/reused, no zombies. Newly captured raw-name labels cover
Groups/List/Grid/Columns/dual panes; spatial and map integrations compile but
are not newly established by these screenshots. Minimum-size clipping still
applies: escaped strings can be visually truncated in narrow columns. This is
not proof of full visible-byte fidelity at every width or font scale.
The same fixtures preserve exact raw-name selection and unchanged file bytes,
large groups, bounded wire results, group navigation, cancel and close.

Cargo admission was repeatedly deferred for live o3-server builds. Only the
existing idle guard was used; competitors were never controlled. No dependency
was added, no preview feature developed and no resource experiment run.

Feature `fed0c70`, source acceptance `fa24e8e` installed with app
`bc987153594533560adcb1b1bcdefa38a2637812e3d3e39de4923ebf5bf11f8e`;
helper remains `006fdb1a8f3acbb90bf0e2b0185ed0a16b932200093d707fba78d7ccc17a7e53`,
HTML helper remains `04aa7953202d26c3602f2dd5ef6df1107a9e652f26357068541e59e0776694cd`.
Install record: `evidence/0.0.4/installed-checkpoint-1793523727714584004.json`;
five backup and installed checks: `evidence/name-labels/install-verification.json`.
Installed Wayland receipt `evidence/duplicates/node-functional-1793523744549961585/receipt.json`
passed 16 captures/two drained roots/six logged duplicate-helper identities gone/reused.

Installed selected SHA regression `evidence/checksums/node-functional-1793523755256312554/receipt.json`
passed seven captures/two drained roots/four logged helper identities gone/reused.
Existing installed HTML regression `evidence/0.0.4/html-ui-1793523765658482961.json`
passed five captures and a drained root exit. GUI fixtures ran sequentially.

## Navigation header follow-up (installed on NODE)

Breadcrumb components, tab captions, column titles, dual-pane basenames and
parent-path titles, and mount basenames now use the same display-only labels.
Breadcrumb targets remain independent PathBuf values; tabs retain their stable
IDs and exact original paths. Path editors, copy-path payloads, filters and
filesystem operations are unchanged. Other status/context/report/search path
surfaces still require inventory; this is not a global path-format migration.

A new painted test checks escaped breadcrumb/tab captions and exact navigation
destinations for a raw byte plus newline folder. The functional checker now
uses such a root and adds root header captures when `--name-labels` is set.
The follow-up passed 1,207 workspace tests (425 core/95 helper/687 UI,
three existing ignored), Clippy with warnings denied and formatting. Release and scoped functional acceptance passed. This follow-up is installed; a first X11 capture anomaly remains open below.

Header source receipts: `evidence/duplicates/node-functional-1793524327844009335/receipt.json`
(X11 retry) and `evidence/duplicates/node-functional-1793524163344046032/receipt.json`
(Wayland), each 19 captures/two drained roots/six logged duplicate-helper
identities gone/reused. Extra root captures show escaped breadcrumb/tab and
column/dual titles for a directory containing FF plus newline. Navigation and
content checks retain exact paths.

Initial X11 receipt `node-functional-1793524131083329478` passed handler checks,
but its root dual capture lacks active-pane names/icons while retaining metadata.
Wayland, prior installed app on the same new fixture (`1793524260354395471`) and
an unchanged X11 retry render both panes. Cause remains unconfirmed; the image
is retained in `evidence/path-labels/validation/render-assessment.json`. This is
not a fix or proof of absence of a general rendering/capture defect.

Header source `64b7a26` installed with app
`0e902ec5f9ef1c86a27ad46c386fcbb9bfe33a6bcdfe965a7f7717ddea614215`;
helper/HTML unchanged. Install record `evidence/0.0.4/installed-checkpoint-1793524392203621640.json`
and `evidence/path-labels/install-verification.json` verify five backups and
five installed hashes. The earlier capture anomaly remains open.

Installed header receipt `evidence/duplicates/node-functional-1793524403479977930/receipt.json`
passed 19 captures/two drained roots/six logged duplicate-helper identities gone/reused;
its inspected dual-root frame shows both panels. Installed SHA regression
`evidence/checksums/node-functional-1793524415642617136/receipt.json` passed
seven captures/two drained roots/four logged helper identities gone/reused.
Existing installed HTML regression `evidence/0.0.4/html-ui-1793524443078694148.json`
passed five captures and a drained root exit. GUI fixtures ran sequentially.

## Search and bookmark path labels (installed on NODE)

Search result basenames and relative parents now use the common display label.
The report's original relative paths, filesystem name matching, metadata/content
queries, marks and Locate destinations remain unchanged. Painted tests cover
440/1000 widths and exact bytes passed to Locate. Search queries do not gain
escape-to-byte decoding: a displayed escape is not a new filesystem filter syntax.

Bookmark path lines and history captions use the same representation. Saved
user aliases and default alias generation are unchanged. The panel already
filters displayed captions; escaped history captions therefore become searchable
with their visible escape text. A test checks both the escaped caption/path and
the original history destination, including a literal-escape distinction.

1,212 tests (425 core/95 helper/692 UI, three existing ignored), Clippy with
warnings denied, release and fmt passed. Source X11 receipt
`evidence/duplicates/node-functional-1793526790428126272/receipt.json` and
Wayland `evidence/duplicates/node-functional-1793526860637069317/receipt.json`
each passed25 captures/three drained roots/six logged duplicate helpers plus
one logged search_batches helper gone/reused (zombies rejected). Search raw
leaf/parent, Locate, saved favourite original path/user alias, filtered history
Jump and unchanged file content were checked. Capture-audit ROI equality passed
but does not close the original X11 anomaly. Initial harness query-escaping
FAIL `1793526685989061509` is retained; see search-labels/validation assessment.

Search/bookmark source `cb74513` installed with app
`ff8b0c38b940322382ac403e8063eec6f5cfc9aa52ada35a2ded56d13a9ccfe7`;
helper006fdb1a8f3a/HTML04aa7953202d unchanged. Install record
`evidence/0.0.4/installed-checkpoint-1793527005652808719.json` and
`evidence/search-labels/install-verification.json` verify five backups and
installed hashes. Installed Wayland receipt
`evidence/duplicates/node-functional-1793527035241672797/receipt.json` passed
25 captures/three drained roots/six duplicate plus one search helper gone/reused,
exact raw Locate/Jump, persisted favourite alias/path and unchanged file bytes.
The minimum search result was visually inspected. Source selected SHA regression
`evidence/checksums/node-functional-1793526897926702675/receipt.json` passed
seven captures/two roots/four logged helpers gone/reused.


## Operation review labels (NODE 2026-11-01)

Copy/move source captions, archive source/short name summaries, copy options
source/destination captions, original-name review, current copy progress path,
and rename directory/from/to review now use the common escaped native labels.
These painted labels use literal text; filename `@` prefixes are not FLTK symbols.
Destination and basename editors keep their existing raw/captured protections and
text syntax. Labels are never parsed back into operation targets. Original-name
review distinguishes an invalid byte from a literal backslash escape and exposes
bidi controls while retaining the exact original plan. Short archive summaries
still truncate with ellipsis; fixed operation captions/rename columns can clip.
This change does not claim complete path visibility in those panels, change
rename support for non-UTF8 basenames, or add OS-input/AT-SPI/2× evidence.


Source operation validation: copy Wayland `1793529876642064456` and X11
`1793529921947404739` PASS13 captures each/one drained root0 each. Own source
parent includes invalid byte/LF, original basename also backslash/bidi. Actual
five-root named copy preserves file hashes/nested content/literal symlink;
occupied target and duplicate plan refused, Unix escape-looking filename stays
literal, original sources intact. Create/extract ZIP review captions captured
and cancelled without committing archive jobs. Rename regex
`1793529887572115455` PASS from raw parent: preview, invalid/oversize rejection,
actual rename and undo/redo. See operation-labels/validation and source receipts.

Final current-source1215 Rust425/95/695 PASS+3 historical ignored; Clippy
workspace/all-targets -D warnings, fmt and release PASS. App SHA256
`1703d123e71333e436685b7f9e0ea3431e96cbfee2c08d8111f4066de88d79ae`.


Installed source53b61e9: record1793530016200693776, five installed plus five
backup hashes verified in operation-labels/install-verification.json. Installed
Wayland copy receipt1793530024072915031 PASS13 captures/one drained root0,
raw/control/bidi review, owned copies and no-clobber checks, ZIP review captions.
