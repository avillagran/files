# Icon grid — 0.0.4

Nerd Grid button or Ctrl+5. Automatic layout of files/folders, different
from Spatial (persisted manual positions) and Space (size areas).
Only the rows that intersect the viewport are drawn; it does not create positions,
widgets or a bitmap per entry. The global sorted/filtered index is reused,
with lazy stat of visible items and real sizes as they arrive. Icons are Nerd
Font glyphs by type, **not thumbnails**: P09/thumbnails are still pending.

- −/+ buttons and Ctrl+wheel/Ctrl+±: zoom 65–180%, independent of the Spatial layout.
  It writes no positions and no Spatial zoom. The Grid zoom lasts for the
  instance; per-folder/session persistence is still pending (V12).
- Arrows move across columns; Shift extends the range, PageUp/Down by rows,
  Home/End first/last, and reveal by row. Click/Ctrl/Shift share the selection,
  Enter/double click navigates/opens through the existing action; shared context menu.
- On resize, columns/visible range are recomputed without duplicating names or keeping
  one object per cell. Long names are visually clipped; the raw identity
  stays in the listing. Hidden files follow the SAME persisted global option.
- Switching views invalidates hits/hover/drag/visible before the next paint.
  The compact header keeps separate controls; tooltips/shortcuts name the Grid.

Pure tests exercise visible range 100k/ends/resize/zoom and a painter recorder
over 1000 entries, selection/range/reveal/hidden, without touching the Spatial layout.
They are not equivalent to a 100k app budget. Resources/startup NOT_MEASURED because of the freeze.
Scene method: internal handlers/visible hits, its own size request
behind a test flag, framebuffer capture and real FLTK callback; NO OS events.

Preserved failures: a freshly written executable du fixture produced ETXTBSY in the parallel
suite. The test uses the installed, immutable /bin/sh interpreter and the real supervisor;
production keeps Command(helper). The first capture showed clipping of Nerd ink
larger than its monospaced advance: the icon region is now clipped to the cell,
not to the advance. Resizing via fullscreen was rejected because of the fixed fixture's max=min;
the fixture's own explicit size is used and an 800×500 framebuffer is verified.
Another FAIL found an initial READY before smoke_test's repaint: it now requires a
paint_revision after the hit invalidation, without accepting stale state.

Final validation: 391 Rust (126 core/44 helper/221 UI), Clippy workspace/all-targets
-Dwarnings and release PASS. `grid-ui-1793060477438092576.json` functional PASS:
162 visible names + two hidden, eight real captures, zoom 100→132%, range,
last item revealed, context menu, folder/back, raw toggle, own resize
800×500→1000×560 and callback/drain/exit 0. No preview/du/treemap helper was requested;
the main lifecycle is tested, not cleanup of a tree with hypothetical children.
SHA files e8c61f3cbf1659387d7c73b5fecd6e80d8ccc0cb8284765f240cc8a0eea3c836;
helper 4f7910cf27b1723b25d17a208b9c2287858decfb0e65abf23c534305aad620c0.
No thumbnails, no new resources/startup measurement, no 0.0.4 tag/global PASS.
