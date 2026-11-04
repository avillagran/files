# Alternate destination names

Copy source basenames remain immutable raw OsStrings. A transient plan binds
indices to the captured Arc selection and stores at most128 explicit UTF8 target
names, <=255bytes each. Target names must be a single component, not . or ..,
with no path separators/controls (Unix backslash is literal); no URL decode or normalization. Unchanged raw names
remain original bytes. Duplicate effective targets and complete path/count/byte
limits reject before constructing request paths or enqueuing any root.

This explicit name editor is Copy-only, not source rename or replacement.
Automatic numeric suffix allocation is now opt-in separately through
[Keep Both](COPY-KEEP-BOTH.md). The existing no-clobber engine decides destination collisions at
execution, including late races and explicit Skip policy. No claim of atomic
multi-root copy, durable recovery, snapshot, complete O03 or native input.

Effective source proof: selected/clipboard copies verify the actual O_PATH root
FD against Source capture, not just a pre-dispatch name probe. Root node matches
that descriptor; regular/directory open checks remain. Root symlink uses readlinkat
on its pinned FD (empty name), not a reopened pathname. One additional transient
root FD, no retained FD per row and no extra worker. Existing parent/name/postcopy
revalidation before publication remains. Limits are not PSS proof; resource and
startup freeze/new-preview pause persist. Scoped acceptance: 818 Rust tests (252 core/79 helper/487 UI), three existing
core ignored; workspace all-target Clippy -Dwarnings, release and fmt PASS.
Final scene evidence/copy-names/copy-names-ui-1793429532169892092.json PASS:
nine real FLTK framebuffers1000x650/440x440, five roots independently mapped,
Unicode/raw names, directory/literal symlink, duplicate targets refused before
mutation, occupied target preserved, stale paste rejected across rows/fields,
private Source clipboard and same-folder duplication; root0 after drain ACK.
All nine images inspected. F2 edits name, Ctrl+L folder, Alt+Up/Down changes row;
painted previous/next/reset controls share stamped handlers. No custom Tab router.

Original functional PASS scene1793428866536186371 retains visual FAIL sidecar:
header omitted current unsaved fifth proposal; changed_count now includes it,
with singular/plural tests. Final title shows five names before enqueue.
Original incidental main-process footer53.7MiB exceeded ideal at face value;
final footer44.4MiB is also incidental, not comparable tree+Swap evidence or a
resource PASS. No new resource measurement under freeze. No native input,
clipboard MIME, snapshot, complete conflict policy, Move naming or global PASS.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Opciones in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925Rust319/85/521+3existingignored, Clippyworkspace/alltargets-Dwarnings/release/fmt
PASS. Native1793457917645506527 UI/copy/mount PASS16framebuffers incl440² and
1000×560;4copiedroots(raw0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expectedFAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves57partial
files and original3000files intact.24loggedPID/start identities gone/0errors/survivors,
not lifetimes;3roots exit0 after normal callback. Native defaultcopy/overallFAIL
exit1 retained. Final copy-names UI1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visualFAIL preserved+sidecar;
explicit bounded lines fixed and finalframebuffer inspected. No source delete,
automatic removal of visible partials, powerloss/nativeinput/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.
