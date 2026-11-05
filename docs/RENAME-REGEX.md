# Regex replacements in the renamer

The F2 form lets you switch explicitly between literal and regex replacement.
Literal mode is still the initial value. The general pattern keeps
`{name}`, `{ext}`, `{dotext}`, `{n}` and `{n:03}` for prefixes, suffixes, extension and
sequence. The replacement applies to the name without its last extension, before the
general pattern and the case change. A dotfile with no other dot
is treated as a whole name. The extension only changes when you modify the pattern.

Example: replacing `photo(?<n>[0-9]+)` with `imagen-${n}` turns
`photo1.txt` into `imagen-1.txt`. `$1` uses a numbered group, `${name}` a named
one and `$$` produces a dollar sign. As in Rust regex, `$1_suffix` takes the longest
complete reference; use `${1}_suffix` to separate the group from the suffix. A capture
that does not exist or does not participate produces empty text. An empty regex matches at
UTF-8 boundaries. `\p`/`\P` properties, Unicode classes (`\w`, `\d`, `\s`/boundaries) and
Unicode casefold are not enabled: their tables need a license that is not in
the allowlist, and exceptions are not widened by inference. Use explicit
ranges such as `[0-9]` or `[áéíóú]`; dot and UTF-8 literals do work. ASCII
classes can be used in a `(?-u:...)` group when the pattern stays valid
for UTF-8 text. The existing selector's case changes still use Rust
outside the expression. Look-around and backreferences are not supported;
they are shown as an error, with no applicable plan.

The plan is computed on the existing mutation worker. It does not modify files;
Apply only accepts a current prepared plan and verifies identities and
destinations again through the transactional engine. Changing the mode or editing invalidates the
earlier review. Collisions, occupied destinations, an invalid pattern, excessive
expansion and cancellation do not deliver a partial plan. Apply is disabled
when there are no prepared changes. Undo/Redo use the same safe history.

## Explicit limits

The regex is compiled once per batch. Pattern/replacement are at most 4096 B in
the API and each editable UI field at most 2048 B. Only `std` is enabled, without the crate's Unicode tables. The builder limits the NFA to 1 MiB and
depth 64; at most 64 groups and 256 substitution segments. The optional regex
DFA/backtracking/perf engines are disabled. A single bounded search or
compilation is not interrupted inside the library; cancellation is checked
before/after and between matches, segments and entries.

Substitution first walks borrowed slices and adds up the expansion up to 4096 B,
before creating the output String. The second pass builds only that bounded
result. The planner limits are kept (100k entries per list, 4096 B per
name, 8 MiB aggregate input/output each) as well as the UI preflight before
cloning the selection. Regex errors are formatted with a writer of at most 1024 B of
UTF-8. These bounds are not a PSS measurement nor a total latency guarantee.

File dates, exact per-filesystem normalization, mass renaming with an
integrated budget and a durable journal are still pending (R02/R03). A selected
non-UTF-8 name is never lossily converted.

## Validation

The integrated suite and `scripts/rename-regex-ui-check.py` record this
unit's results. The scene is functional only: a batch of two, preview, errors,
Apply/Undo/Redo, byte integrity and close after ACK. It uses internal
handlers and the real framebuffer, without OS input or a 100k batch budget.
Resources and startup remain NOT_MEASURED because of the user's explicit suspension.

324 Rust tests (106 core/37 helper/181 UI), Clippy workspace/all-targets
`-D warnings` and release clean. Final scene
`rename-regex-ui-1793046339654903640.json` functional PASS: batch 2, preview without
mutation, invalid regex and rejected expansion without a plan, Apply/Undo/Redo/Undo with
exact bytes, no leftover staging and normal exit 0 after ACK. App SHA256
`5a9b4e38cf6f469efd92951b248376fba15d660b6741a43490725da97aa89d8d`.

The first scene `1793046051262606727` keeps its filesystem PASS; visual
review found a multi-line diagnostic hidden in a one-line footer. The sidecar
marks that visual FAIL. A new bounded drawing summary keeps the original
diagnostic and the new capture shows the reason and Apply disabled. Earlier
captures and historical benchmarks were not overwritten.

`scripts/rename-plan-check.py` now prepares a Cargo fixture that depends on the
real core and its features; the old `rustc #[path]` skipped the new dependency.
The Python compiles syntactically; the adapted benchmark was not run because of the freeze and
its historical measurements are not extrapolated to the current binary.
