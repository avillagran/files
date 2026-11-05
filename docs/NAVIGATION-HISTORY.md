# Navigation history and bounded omnibar — 0.0.4

Nerd Back/Forward buttons to the left of the omnibar, with a tooltip and an inactive
state; Alt+Left/Alt+Right call the same Actions. Per-instance record of
paths only: at most 64 entries across both stacks and 4096 B per lossless
path, checked before copying. Name payload ≤256 KiB plus fixed containers;
this is not a process budget and does not include the PathBuf already owned by the caller.
It adds no directories, FS probes, listing cache, helpers or a worker per visit.

Requested destinations are recorded, including one that later fails to
list: Back can recover the previous location without a synchronous probe on the UI.
Visiting a new path discards Forward; the same path refreshes without duplicating history.
The Spatial save must accept the snapshot before history advances. A
save/validation refusal keeps the path and the stacks. Load location invalidates by
generation entries/hits/hover/selection, cancels previews/du/treemap and reuses the
listing worker. Fast navigation replaces the last pending path; it does not create
threads to get around a blocked syscall. It does not guarantee fast offline recovery.

History does not restore filter/selection/scroll/view per entry. Tabs/reopening a
tab/sessions and history persistence are still pending (V10/X02); this
block does not count as those features. There is no path canonicalization/sandbox,
and FS reads keep their real errors.

Omnibar/filter run a borrowed preflight before copying a paste/IME: at most 4096 B of
UTF-8 after excluding CR/LF/NUL. A rejection shows an error and keeps the text and the
replace flag, without duplicating the whole paste. A later valid input recovers.
The tests are not equivalent to the Wayland clipboard: the focus/serial FAIL still stands.
Source ranges/shaping/full IME are still pending (U03).

Reproduce: cargo test --workspace; cargo clippy --workspace --all-targets --
-D warnings; cargo build --release; python3 scripts/history-ui-check.py. Own
fixtures in ~/.cache/files-test. Pure tests cover limit/branch/duplicates/raw,
Unicode/oversized paste; Model covers several navigations before draw, a rejected
save and preserving/retrying input. The scene uses shared hits and
real framebuffer/callback, not OS events or an external clipboard.

Resource/startup freeze still NOT_MEASURED. The budget is not changed and the
baseline/startup/clipboard failures are not replaced; no 0.0.4 tag or global PASS.

Final validation: 399 Rust 129 core/44 helper/226 UI, Clippy workspace/all-targets
-Dwarnings and release PASS. history-ui 1793061306527375025 functional PASS: six
framebuffers 1000×560, enter/back/forward/branch, a real missing path and recovery,
1 MiB paste rejected keeping text/replace and Unicode recovery, callback
request/drain/exit 0. No preview/du/treemap helpers requested; root lifecycle,
no cleanup of unobserved children is claimed. Alt+arrows have shared wiring;
the fixture uses internal buttons/handlers, not OS events or an external clipboard.
SHA files 39db06227fa069b43d8bfe9c323b262d82ec46e420abea8dbcc8cdf10637426b,
helper 4f7910cf27b1723b25d17a208b9c2287858decfb0e65abf23c534305aad620c0.
Another OmaBridge Cargo build in an unrelated target overlapped with the build; it was not touched.
Built with one job. No timing comparison and no new resource measurement.
