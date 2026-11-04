# Retained listing targets: source checkpoint

The actor now distinguishes the complete configuration version from each
target's lease. An unchanged owner/path retains its native watch, identity
observation, retry deadline, attachment sequence and pending notice when another
target is added, removed or replaced. Model synchronization preserves that
browser's key and debounce state. Removed/replaced targets reject old callbacks
and acknowledgements.

Validation on NODE: normal-concurrency `cargo test --workspace`: 443 core,
105 helper and 726 UI tests passed; three historical core tests ignored.
Workspace Clippy all targets with warnings denied, formatting and diff checks
passed. Logs are adjacent. The added pure test checks pending-notice and ACK
retention and replacement rejection; the native Linux test checks attachment
retention and a subsequent real external write after adding another root.

This is source acceptance only, with no new installation or OS-input claim.
The installed checkpoint remains 81837e5 / acceptance 4108230.

Columns audit: eight columns maximum, a shared 16 MiB data budget, 100,000
entries and one reusable lookahead worker in addition to the active Model worker.
The current actor limit stays two; Columns and SpaceMap still defer auto refresh.
Column integration must distinguish stable watch ownership from stale UI-entry
tickets, route cached reloads to the actual worker target, restore raw-name
selection/viewport and reconcile invalid descendant branches before extending
the bounded target count. No resource experiments were run.
