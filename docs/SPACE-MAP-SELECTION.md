# Selection in SpaceMap

The full explorer goal remains active. This checkpoint adds selection tools to
SpaceMap, including its full-width rows, without adding scans or file operations.

`Marcas` opens the shared selection menu. Mark all, invert, clear, palette actions
and Insert use the unique mapped entry indices in the current projection. Root
listing entries absent from a bounded or partial report are excluded. Hidden
projection applies; a retained text filter from another view does not apply to
SpaceMap and is preserved. The filtered-selection command is disabled with that
explanation. Ordinary listing selection keeps its existing filter semantics.

All/invert/Insert require a Ready report for the current path and a geometry key
matching report revision, listing generation, entry count and hidden projection.
Menus and palette additionally capture the map revision and hidden projection;
a changed context rejects execution. Clear remains available after stopping the
map. Empty bulk selection clears focus and anchor, preventing implicit fallback
selection by subsequent operation capture. Insert retains MC toggle-and-advance
semantics, stopping at the last mapped entry.

Inversion prunes old marks using a sorted membership cache bounded by the report's
2048 nodes, then toggles the cached navigation slice. All and clear do not scan
old marks or compile a name filter. Names and operation paths remain raw. No new
helper, filesystem walk, watcher or automatic recalculation is introduced.

Acceptance: 1294 workspace tests (447 core/105 helper/742 UI), three historical
ignored, passed before the final label-only shortening. Final Clippy all targets
with warnings denied, format check and release passed. Wayland1793555586348347425
and XvfbX111793555590497549394 each passed16captures/3normal exits/6logged helpers;
installed Wayland1793555616429200227 repeated this scope. Cases include raw names,
hidden projection, palette, visible menu and partial unreadable-directory rows.
Installation1793555602802600273 has five verified installed and backup hashes;
app c5254a6503397ff371f11fffe526a25ceb27ca93045ded452d23b9f044e6e9ab.
Evidence: `docs/evidence/listing-model/space-map/selection/assessment.json`. Internal handlers and framebuffer captures do not establish native
OS key delivery, clipboard, accessibility, 2x rendering or global resource gates.
