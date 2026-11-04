# SpaceMap no-area navigation and tiny targets

Source acceptance:1,284 workspace tests (443core/105helper/736UI) PASS,
three historical ignored; Clippy all targets warnings denied, format and release
PASS. Wayland1793550565762001516 and XvfbX111793550568773633837 each passed
12captures/2roots0+drain/5manual helpers absent or reused. Watch, legacy map and
Columns regressions passed. Initial failures and limitations are retained in
`docs/evidence/listing-model/space-map/navigation/assessment.json`. Installation
is recorded separately after this acceptance.

The treemap preserves exact apparent-byte proportions. A complete positive node
whose logical width is below65 or height below45 (no room for its label) is also offered as a full-width row;
its area is not enlarged or assigned fabricated weight. Zero, unknown and partial
nodes remain rows without an area. The observation and watch-gap behavior in
SPACE-MAP-CHANGES.md is unchanged: input and projection changes never rescan.

Geometry now prepares the map and list together. On a changed container key, it
reserves up to three rows, including positive tiny nodes, by monotonically growing
the row reserve and computing the layout at most four times. This closes the old gap where
an all-positive report could discover tiny nodes only after consuming the entire
list space. The final layout is cached for report revision, listing generation,
entry count, dimensions and hidden projection; unchanged paints skip preparation.
Absolute list position is updated on paint. The list has one clamp/reveal/page
geometry shared by the painter, canvas scroll clamp, keyboard, wheel and buttons.

Navigation order is cached as unique mapped entry indices: measured area order,
then other no-area rows. A bounded node-visited buffer eliminates duplicate tiny
nodes. Key movement borrows this cached slice instead of allocating a Vec and
checking an ever-growing output for duplicates per key. Bounds remain the report's
2048 nodes and256KiB names; no new watcher, filesystem walk or helper is added.

Home/End, arrows and PageUp/PageDown reveal the focused no-area row completely.
Page steps use the actual visible list height. Resize/hidden projection rebuilds
also reveal the current focus. Wheel and visible ↑/↓ page controls scroll the
list without moving focus or changing marks; unchanged paints do not snap a
manually scrolled list back to the focused row. Tiny-area keyboard focus is also
revealed as a usable row. Focus remains visually distinct when Ctrl moves it
without replacing the marked set. Shift keyboard and mouse ranges use the same
cached map order, independent of the directory listing's name/sort order.

This does not enable every selection command/MC operation in SpaceMap. Existing
All/Invert/Filtered and file-operation restrictions in the MC strip remain; those broader behavior
choices still need acceptance. No native-input, AT-SPI, clipboard, 2x or global
resource/lifetime claim follows from internal key/wheel handlers and framebuffers.

Unit coverage uses a1TiB apparent node with13one-byte nodes and shuffled entry
indices to verify ratios, positive-only row reservation, cached navigation,
reveal/clamp boundaries, missing geometry, Ctrl focus and Shift range order.
The owned UI fixture `scripts/space-map-navigation-ui-check.py` exercises positive
and all-zero maps, raw names, every navigation target, page buttons/wheel, resize,
hidden projection and entering an empty zero-byte directory. Its sparse size is
fixture input, not a performance/resource experiment. PSS/Swap/CPU/startup remain
frozen and new content previews remain paused.

Clearing/cancelling geometry also revokes entry activation. The same-generation
hits from an earlier paint and an old focused entry cannot bypass that revocation
through double-click or Enter. The unit navigation scenario exercises both paths
with a stale directory target and verifies unchanged path/generation.

F5 now uses a context-sensitive handler shared with the real event dispatcher:
SpaceMap recalculates; other views keep the existing Copy review. The earlier
SpaceMap fixture invoked Action::Refresh directly and therefore did not prove the
advertised F5 route. The MC strip now presents/enables F5 Recalcular in SpaceMap,
with matching mouse dispatch and tooltip, while other restricted operations stay
disabled. The owned fixture toggles the actual strip and exercises both the shared
F5 handler and its painted mouse button. Geometry/reveal also handles the smaller
viewport above the strip. No claim of native key delivery is introduced.

The440-wide MC case reproduced a missing-name access gap: the strip can leave a
24px-high chart, including an otherwise large block with no drawable label. The
row fallback therefore follows the actual65×45 label geometry, not just a24×24
click target. Positive rows say “etiqueta en lista” without mislabeling their byte
size as zero or reducing the measured weight. The failing receipt is retained.

Installed on NODE from source `d05e51c`, receipt
`installed-checkpoint-1793550835411868233.json`; app SHA256
`dc2babf673b9fae6d4f14c2ee904b37d7218c6bc056957857c0ed53fd236c2b3`.
Five installed and five backup hashes verified. Installed navigation
1793550890414818248 passed12captures/2roots0drain/5manual helpers; watch
1793550890942500902 passed7captures/root0drain/5manual helpers; allocation
1793550891808371341 passed22captures/2roots0drain/25measure helpers. Known
helper identities were absent or reused, with no zombie identity retained.
No global lifetime/resource/native-input acceptance follows from this scope.
