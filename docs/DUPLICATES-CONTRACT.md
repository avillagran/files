# Recursive duplicate observations (C01, partial)

Status: workspace/all-target compilation, 1,198 tests (three existing ignored),
Clippy with warnings denied, formatting and release passed on NODE.
Scoped X11 and Wayland functional acceptance passed; installed on NODE with
five verified backups and five installed hashes.
Selected-file SHA-256 and folder scanning are both available in this checkpoint.

Open Acciones → Duplicados or palette `duplicates` to capture the current folder.
Opening the panel does no traversal. Buscar/F5 explicitly starts a read-only
recursive scan including hidden files and subfolders. F6 captures the current
folder again while idle. Browser selection is not required or modified.

The walker pins directory descriptors, skips symlinks and special files, and
counts one candidate per device/inode, excluding hardlink aliases. It groups
metadata by length and hashes only lengths shared by two or more candidates.
Each file's captured revision is checked before content reads and again after
hashing through the existing SHA-256 implementation. Directory changes and errors
make the result partial. Traversal is not an atomic snapshot; hashes are
observations and never authorize a later removal or promise reclaimable bytes.

## Pagination follow-up (installed on NODE)

The new scanner retains all matching observations within the traversal caps.
It sends ordered pages of at most 128 rows and 8 KiB of raw path bytes, followed
by a terminal summary. The decoder requires exact offsets, total row/path caps,
unique paths and inodes, root agreement, complete peer groups and exact final
counts; nothing becomes a report until validation and process reap. A group can
span pages, including a one-member tail page whose peers are retained elsewhere.

The UI retains one bounded observation collection (16,384 rows/2 MiB of paths),
plus one presentation page. Previous/Next buttons and Left/Right change pages
without opening a reader. Copy explicitly includes only the current page, its
number and global row interval; all traversal counts and partial reasons remain
in its header. F5 replaces the retained collection with a fresh scan.

Only `find_duplicates` gets a 32 MiB total wire ceiling, needed for escaped raw
paths and repeated observation metadata; other helper streams retain 4 MiB.
No full wire response is buffered in the UI: the line ceiling remains 256 KiB
and decoded observations retain the traversal limits. This is a data bound,
not a measured memory guarantee. The one-shot scan/reap/cancel lifecycle remains.

1,200 tests (423 core/95 helper/682 UI, three existing ignored), workspace
all-target check, Clippy with warnings denied, fmt and release passed. Scoped
X11 and Wayland functional acceptance also passed. The older bounded-whole-
group `scan` core API remains available; the helper now uses `scan_all`.

## Previous installed checkpoint: explicit bounds

- At most 100,000 entries, 16,384 independent regular files, 2 MiB of candidate
  path bytes, 4,096 directories and 32 directory levels.
- Final display/report: at most 128 matching files and 8 KiB of relative path
  bytes. Groups are retained whole; an oversized group is omitted entirely.
  Total observed group/file counts remain visible with a partial-result reason.
- One supervised helper per scan; sequential 64 KiB content reads, a 120-second
  deadline, at most 512 progress messages and 256 KiB per protocol frame.
  These bounds are not a measured process-tree memory guarantee.
- Existing readers drain before admission. Pending folder-size work is paused
  temporarily and its previous pause state is restored afterwards.

No matching files is reported only after a completed traversal without errors
or limits. Errors, changes, timeouts and limits cannot become proof of absence.
Cancel discards the unfinished report and waits for helper reap acknowledgement.
Panel/root close uses the same acknowledgement gate; retry waits for that gate.

Copy requests an escaped, bounded human report including scope, counts and
partial reasons. It does not prove native clipboard delivery. No filesystem
mutation is available in this tool.

## Acceptance still required

The functional harness checks independent
hashlib results, opaque path bytes, hidden traversal, hardlink exclusion, skipping
a unique-size sparse file, actual-reader cancellation and close, explicit result
limits and unchanged fixture metadata. Native OS input, accessibility, 2x
sharpness, full lifetime and resource gates remain separate open requirements.
C01 and the complete explorer goal remain partial.

Final source acceptance receipts:

- `docs/evidence/duplicates/node-functional-1793520989060349335/receipt.json` (X11)
- `docs/evidence/duplicates/node-functional-1793520998264148245/receipt.json` (Wayland)

Each run passed seven painted captures, two root exits with drain acknowledgement
and four logged helper PID/start identities gone or reused (zombies rejected).
The unique-size 64 GiB sparse fixture caused no content reads: exactly nine bytes
were hashed across three independent matching files. A group of 129 files
produced 129 observed matches, zero visible rows and an explicit partial limit.
Small fixture hashes and sparse inode/size/mtime/ctime stayed unchanged.

An initial test failure exposed directory revalidation using a regular-file-only
primitive; a directory-only NOFOLLOW openat walk fixed it. A subsequent missing
OsStr trait compile failure was fixed with encoded bytes. Both failures remain
in `docs/evidence/duplicates/validation`; the full parallel suite then passed
422 core + 95 helper + 681 UI tests, with three existing ignored tests.
The last source edit only clarified the visible limit text; final release,
Clippy and both functional runs include it. Menus contain 23 actions and the
palette 59 commands; search remains first. No tests were serialized to hide
contention and no competing Cargo was controlled.

Installed source checkpoint: `79a906e`; app SHA-256
`5e3f5e851a49fd4f7ccb81c1937b636cfae6ddf2d2a61dab73e41a34713bdae3`,
helper `faee512719ea125e797a9f681621aef3aa5c249c1c43e8d204a164b8acf9eb6f`.
HTML helper remains `04aa7953202d26c3602f2dd5ef6df1107a9e652f26357068541e59e0776694cd`.
Install record: `docs/evidence/0.0.4/installed-checkpoint-1793521035792068634.json`;
independent five-file backup/installed check: `evidence/duplicates/install-verification.json`.
Installed Wayland duplicate receipt: `evidence/duplicates/node-functional-1793521038280864149/receipt.json`
(seven captures, two drained root exits, four logged helper identities gone/reused).
Existing HTML regression: `evidence/0.0.4/html-ui-1793521054678806944.json` PASS.
Large-group paging and explicit candidate review remain unfinished.

Selected-file SHA regression on final source passed at
`evidence/checksums/node-functional-1793521009316225109/receipt.json`.
Installed retry passed at `evidence/checksums/node-functional-1793521115630802923/receipt.json`.
An initial installed run timed out activating the toolkit popup while a separate
HTML fixture window was also running; its FAIL receipt
`evidence/checksums/node-functional-1793521052637335385/receipt.json` is retained.
The identical installed binaries passed when run alone. Focus interference is
a hypothesis, not a confirmed cause; native input remains unproven.

Pagination source receipts: `evidence/duplicates/node-functional-1793521657826812292/receipt.json`
(X11) and `evidence/duplicates/node-functional-1793521673759801336/receipt.json`
(Wayland), each eight captures, two drained root exits and four logged helper
identities gone/reused, no zombies. At 440 px, the 129-member group has pages
1–128 and 129–129; moving both ways starts no helper. Copy scope is unit-tested.
A separate direct-helper fixture yielded all 4,096 raw-name matches in about
5.4 MB of wire data, with exact offsets, page bounds and independent SHA values.
That larger fixture proves helper transport, not a 4,096-row native-input UI run.

The first functional run `node-functional-1793521636758884081` failed because
the harness checked Copy hits from the initial pre-report paint after already
navigating pages. It was changed to inspect the current returned state; existing
painted Copy activation had succeeded. The original FAIL receipt remains.

Paged source checkpoint `a37cbca` installed with app
`d0401cd1c43ea627dd29055f522f1efb0a72ffd8586d089d321863fef3dce340` and helper
`006fdb1a8f3acbb90bf0e2b0185ed0a16b932200093d707fba78d7ccc17a7e53`.
Five backup and installed hashes verified in `evidence/duplicates-pages/install-verification.json`;
installation record `evidence/0.0.4/installed-checkpoint-1793521726143496172.json`.
Installed Wayland receipt `evidence/duplicates/node-functional-1793521733463669320/receipt.json`
passed eight captures/two drained root exits/four logged helper identities gone/reused
and the separate 4,096-file direct-helper fixture. The original selected SHA
source regression passed at `evidence/checksums/node-functional-1793521686071442510/receipt.json`.

Installed selected SHA regression `evidence/checksums/node-functional-1793521747246919966/receipt.json`
passed seven captures, two drained roots and four logged helper identities gone/reused.
Existing installed HTML regression `evidence/0.0.4/html-ui-1793521765253368017.json`
passed five captures and a drained root exit. These GUI fixtures ran sequentially.

## Candidate review follow-up (installed on NODE)

Rows now show the observed group number and independent-file count. Membership
is indexed by digest plus length across the retained collection, including
interleaved transport order and page boundaries. G−/G+ and Ctrl+Up/Down select
the first member of the previous/next group. Group indexes are built once when
a validated scan is accepted; painting does not rescan the collection. Copied
page rows include group numbers/counts inside the existing 128 KiB report cap.

Ubicar/Enter closes an idle folder-result panel and navigates to the captured
relative path's parent, then requests exact byte-name selection after the
matching browser generation is loaded. It uses the normal navigation worker,
not a synchronous filesystem check or external file opener. A stale path is
still only an observation; navigation performs no mutation or hash-authorized
operation. A rejected navigation keeps the report open. Hidden-file policy
stays global and unchanged. If the target is hidden or absent, locate clears
selection/focus instead of retaining an unrelated default row and displays the
existing browser notice. That correction also applies to search locate.

Current follow-up: 1,203 tests (423 core/95 helper/685 UI, three existing
ignored), all-target check and final Clippy passed. Final release and functional
acceptance passed on NODE. The full parallel suite initially hit WouldBlock in
an existing folder-view save test; isolated and unchanged parallel retry passed.
Its cause remains unconfirmed and all logs are retained. A further complete
parallel run passed after adding the global-close Locate guard. The strengthened
hidden-or-visible-removed target test also passed separately without production
changes. Clippy initially caught
an unnecessary PathBuf allocation in the new hidden-target test; corrected.
Cargo from another project caused NOT_STARTED admission responses and was not
controlled. Existing installed paged version remains the accepted checkpoint.

Review source receipts: `evidence/duplicates/node-functional-1793522765637365514/receipt.json`
(X11) and `evidence/duplicates/node-functional-1793522778032344026/receipt.json`
(Wayland). Each passed 12 painted captures, two drained root exits and six logged
helper identities gone/reused, no zombies. Fixtures cover group counts, switching
groups across pages without another reader, locating raw newline/non-UTF-8 names
inside a hidden parent, same-folder exact-name selection after refresh, and
hidden-target empty selection with global hidden policy unchanged. Existing
large-group, wire, sparse cancellation/close and unchanged small-content checks
still pass. The removed-visible-target and global-close cases are unit-tested.

The located raw-name browser capture still renders the newline on two lines and
non-UTF-8 bytes lossily; this existing browser label issue is not resolved here.
Path capture and resulting selection preserve exact bytes. Browser label escaping
is a concrete follow-up; these captures do not prove visual byte fidelity, native
keyboard/mouse input, clipboard delivery, accessibility, 2x sharpness or resources.

Review source `2ec389f` installed with app
`441261c2b883a67b5556a8db5410b65aa6748eb114a3e13d571328c98faf0a0c`;
helper remains `006fdb1a8f3acbb90bf0e2b0185ed0a16b932200093d707fba78d7ccc17a7e53`
and HTML helper remains `04aa7953202d26c3602f2dd5ef6df1107a9e652f26357068541e59e0776694cd`.
Install record `evidence/0.0.4/installed-checkpoint-1793522833615810001.json`
and `evidence/duplicates-review/install-verification.json` verify five backups
and five installed hashes. Installed Wayland receipt
`evidence/duplicates/node-functional-1793522845428741863/receipt.json` passed
12 captures/two drained root exits/six logged helper identities gone/reused.

Final installed SHA regression `evidence/checksums/node-functional-1793522866783872583/receipt.json`
passed seven captures/two drained roots/four logged helper identities gone/reused.
Existing installed HTML regression `evidence/0.0.4/html-ui-1793522879426817804.json`
passed five captures and a drained root exit. GUI fixtures ran sequentially.
