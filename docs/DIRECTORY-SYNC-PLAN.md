# Canonical logical synchronization plan

`directory_sync_plan` derives review decisions and logical effects from a complete
canonical evidence `Record`. This is source-only foundation. The installed
read-only dry-run is unchanged. No operator approval, executor, journal, backup,
retirement, queue, recovery or filesystem write is implemented here. C02 and the
full explorer goal remain active.

## Modes, coverage and metadata

All five modes are derived from the evidence's mode, never a decoded override:
Update A→B/B→A retains destination-only names; Mirror A→B/B→A requires explicit
Skip/Retire for destination-only names; Merge copies missing names both ways.
Missing source names start Include and may be skipped. Existing content, link,
kind and metadata differences start Pending. Update/Mirror Take permits only the
chosen source direction; Merge permits either side. There is no newer-wins rule.

Shared directories merge containers and their children remain independently
reviewable. A metadata choice for a shared directory does not cover its children;
Skip retains that directory's metadata while child effects remain. Missing,
retained, kind-conflicting or destination-only directories cover their entire
subtree. Covered descendants have no independent choice, including after the
parent is skipped. Coverage follows raw slash ancestors, so an intervening raw
name such as `a-` cannot break the coverage of `a/child` by `a`.

Root names are empty. Root metadata is excluded by default and can be explicitly
included in policy. Metadata policies are Ignore, Basic (mode plus non-directory
mtime) and ExactObserved (complete captured metadata digest). Basic ignores
natural directory mtime differences. Hardlink policy is explicitly sealed as
IndependentNames or PreserveInternal (default). These are requirements on a
future executor, not a claim that metadata application or topology effects exist.

Content-changing effects still need a precise executor metadata contract: Ignore
means metadata alone does not trigger an effect; it does not authorize silently
losing retained destination attributes. ExactObserved requires reacquiring and
preserving actual attribute values, not applying a hash. PreserveInternal needs
group-level staging and coherent choices; a scalar copy loop cannot fulfill it.
Its scope is the selected materializations of a source inode; skipped, retained
and unchanged names need a separate topology review before any rewrite.
External hardlinks require isolation from names outside the captured roots.
The subsequent DIRECTORY-SYNC-PHYSICAL.md derives staging cohorts and explicit
metadata-value constraints; these must be resolved before approval. A resolved logical plan is not executable authorization.

Logical effects distinguish Copy, preserve-and-replace, preserve-and-set-metadata,
and preserve-and-retire; “preserve” describes the required recoverable behavior,
not an existing backup. `effect_order()` rejects any Pending row, puts content
before shared-directory metadata, then puts directory metadata deepest first,
including the root last. It is not a complete physical publication schedule.

## Canonical bytes and stale decisions

Version1 is `FLSPLN\0\x01`, the32-byte evidence seal, three policy bytes, a little-
endian u32 row count, one decision byte per derived row, and a32-byte SHA256
trailer over `Files:sync-action-plan:v1\0` plus all preceding bytes. Raw sorted
names, locations, proposals and coverage are derived from the evidence; they
cannot be supplied independently by a decoded plan. Limits are16,385 rows and
16,464 bytes, with no truncation. The complete two8192-child inventories plus
shared root fit. Encoding reserves exact length. Decode checks length, digest,
evidence seal, policy tags, derived row count and legality of every decision.
Pending history can decode; binding rejects it. A resealed legal edit is valid
history, not trusted provenance or approval.

Each in-memory plan has a unique checked nonwrapping instance and monotonic
revision. A row receipt binds instance, revision and row. Changed decisions
invalidate all previous receipts; reverting a choice restores the canonical
bytes but never revives an old receipt. Decode creates a fresh instance.
Selection/window/job ownership stamps remain the future UI owner's duty.

On Linux `Plan::bind` rejects unresolved choices, matches the exact evidence
against a `Prepared` capture, and revalidates the live roots, paths, revisions,
metadata, payload and topology through `bind_record`. `BoundPlan` borrows both
owners, preventing choices from changing while held. This is a point-in-time
check, not a filesystem snapshot or a substitute for checks before effects.

Its88-byte `FLSREV\0\x01` comparison key binds nonzero16-byte job, evidence seal
and complete plan seal. Stored keys decode only comparison data. They cannot
construct `BoundPlan`, prove fsync, grant approval or refresh evidence. A stored
key may still match its historical key after filesystem mutation while a new
live binding fails. Evidence-only56-byte keys and this format are distinct.

## NODE verification

Eleven new tests cover all five modes/directions; raw/hidden names; kind/link
conflicts; subtree coverage with raw-sort interleaving; skipped parents; shared
directory and root metadata order; Basic versus ExactObserved; policy seals;
stale/foreign/reverted receipts; canonical roundtrip and every truncation;
recomputed-digest invalid shapes/choices; live cancel/pending/content changes;
foreign evidence; key jobs/choices; and full16,385-row/16,464-byte plans from
actual manifests exceeding4MiB. The initial focused run passed with an atomic
API deprecation warning; an equivalent checked compare-exchange loop removes
that warning without raising the minimum compiler API requirement.

Final workspace, Clippy and formatting results are recorded in
`evidence/directory-sync-plan/assessment.json`. No release build, helper/UI
integration, installation or new resource measurements are part of this change.
