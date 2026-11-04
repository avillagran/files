# Whole-batch namespace observers

Source-only Linux prerequisite for destination coordination. The opt-in
`plan_batch_namespace_move` and controlled counterpart reuse the existing
expected namespace and journal-gated owned rename paths. They include the two
original-root execution inventories and every private workspace inventory in
one plan. Endpoint-only APIs retain their original one/two-scope restriction.

Renaming a staged hardlink changes ctime on its aliases in other workspaces.
Omitting those owners leaves their snapshots stale for the next publication.
Batch observers undergo both complete preparation walks, the complete post-walk,
coherent moved-inode revision checks, final root checks and the same single
infallible ALL-owner commit. No other inode gets a ctime exception. Unchanged
observer namespaces must match exactly. Directory overlap, root moves, existing
targets, cycles, mount boundaries and unsupported parent nlink models still fail
closed. A late observer failure leaves every recorded owner unchanged, even
when the filesystem rename or durable journal ACK has already happened.

The batch profile permits at most 16,386 scopes (16,384 workspaces plus two input
roots), 49,154 total entries (16,384 original nonroots, 16,384 new nodes, 16,384
workspace roots, two input roots), and 8 MiB + 65,536 relative-name bytes. Name
allowance conservatively covers both final input unions plus all `new` and `old`
subtrees and their prefixes. The aggregate wire bound is entries*(148+255) plus
that name allowance. These are structural quotas, not a claim about resident
memory, allocator overhead or startup cost.

All borrowed input shapes are charged before directory identity sets or
candidate entries are allocated. Every resulting candidate is charged against
its per-scope quota and the aggregate quota BEFORE its path is cloned. The
existing per-scope 16,385-entry, 2 MiB relative-name, wire, path/depth and read
limits remain in force. The aggregate entry allowance does not widen any one
scope or the original capture profile. Preparation's shared read budget still
covers two complete walks across ALL scopes; execution/reconciliation budgets
remain separate. Repeated full walks and journal readbacks remain potentially
quadratic and have no bulk-scaling acceptance.

The public plan remains borrowed expected-state evidence, not proof that a
particular actor performed a rename. Trusted production effects must use the
existing Journal executor: exact sealed intent binding, durable intent before
one FD-rooted renameat2 NOREPLACE, synchronization and durable result ACK before
owner commit. Test calls using fs::rename exercise reconciliation only.

Four new tests cover regular and symlink alias observers with a subsequent
composed move, late observer content/root changes, cancellation and read-budget
failure with unchanged recorded owners, synthetic structural limit/one-over
checks, and real Journal publication of four workspaces followed by all-owner
verification. The Journal fixture also injects failure after the durable result
ACK: the syscall succeeded while recorded owners remain unchanged. This is not
a power-loss or recovery test. Fixture compile diagnostics are retained in the
evidence alongside corrected runs.

Destination batch orchestration, cumulative effect-phase budget admission,
absolute source/workspace guards around every effect, shared-directory metadata
after own child changes, native human approval/helper/source ACK, recovery/undo
and production UI Apply remain pending. No format decoder, release, installation,
GUI/native/global acceptance or resource/startup experiment is part of this
checkpoint. The complete project goal remains active.

The first full workspace run found a pre-existing functional FD test racing
other tests' process-wide opens/closes: its exact `/proc/self/fd` delta underflowed.
That test now runs itself in an exact-filter child process so its FD accounting
has its own descriptor table. It retains all original FD delta and close checks;
the surrounding workspace suite keeps its normal parallel harness. This is
functional descriptor ownership validation, not a resource benchmark. The failed
run is preserved; no global test serialization or competitor control was used.

Final NODE workspace result: 1,611 passed (704 core,112 helper,795 UI), three
historical ignored. Clippy workspace/all-targets with warnings denied and
workspace formatting check passed. Five installed artifact hashes match the
accepted dry-run receipt. See [evidence](evidence/directory-sync-namespace-batch/assessment.json)
for source hashes, preserved failure chronology and raw archived logs.
