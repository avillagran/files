# Names before metadata in narrow panes

Earlier QueueReview evidence showed names with no width in dual pane at 440 px
with the sidebar expanded: `max(r.w-116,0)`. In List, the fixed 255 reservation left barely
30 px and could place metadata over the usable space. The control at 1000 showed the
names. This geometric problem is different from the historical wide X11 anomaly.

`list_columns` computes the cells without allocating memory or querying the FS:

- The icon and margins keep their space; the name gets the rest.
- List shows size only if it leaves 160 px for the name; date also requires that
  minimum, the size and the gaps. If it does not fit, the cell is omitted, without showing 0 or
  turning the unknown into known. Omitted metadata is not formatted.
- Dual pane shows its optional size only if it leaves 128 px for the name. At 440 with the
  sidebar expanded it uses the available width for names in both panes.
- Cells do not overlap or leave their area. Heights, scroll,
  virtualization, physical/visible order, selection, owners, revisions and raw paths are not changed.

Sorting by name, size or date is still available even when the data cell is
hidden. The compact headers keep all three targets and show icons or the
initials N/T/F, with the arrow of the active criterion. If there is room, Name shows
its title again. Names still use the common escaped presentation;
labels are not decoded and operation targets are not lossily converted.
An arbitrarily long name can still be clipped: prioritizing its cell does not
promise full readability of 4096 bytes in a small pane, nor AT-SPI acceptance.

Source f6a735f/fixture 031aa00: 1309 tests (447 core/105 helper/757 UI), three
historical ones ignored, with normal concurrency; Clippy all targets with warnings
as errors, format and release PASS. Wayland 1793562545435935498,
Xvfb X11 1793562548431018683 and installed Wayland 1793562954848124239 each passed
27 captures/2 normal closes/drain: List and dual pane at 1000×560 and 440×440,
sidebar collapsed/expanded, raw labels, focus/marks/owner preserved when
sorting, independent order per pane and copy review without queueing.
Visual inspection of both compact panes and List confirms the short escaped
labels. The Wayland regression queue 1793562485302474500 passed 35 captures/
5 normal closes in its five profiles.

The first two attempts failed because they expected 1000×650 in fixture-wide; the helper
and the PNG IHDR say 1000×560. Fixing the expectation was enough with the same binary
on both backends. The FAILs are kept. The fixture hex decoder's initial lint
was fixed with a bounded as_chunks, without suppressing warnings or serializing.
Installation 1793562789004828933, five installed hashes and five backups
verified; app 8f88cc804da339095b0a626579b05b24bc94caa49ff39113119faead6addc1b3.
Evidence in `docs/evidence/narrow-names/assessment.json`. The complete
goal, other ROADMAP tasks and the historical anomalies remain active.
Absolute PSS/Swap/CPU/startup freeze; no preview formats are added. Handlers and
framebuffer do not prove OS input/clipboard, 2x, other OSes or the global lifetime of
workers/helpers.
