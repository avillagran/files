# Copying text from the preview

Report (2026-11-05): "If I preview a text file, select and copy
(Ctrl+C or Super+C) it DOES NOT COPY, the button doesn't either".

## Cause

- `copy_preview` published `selected_text()` even when it was empty: with an
  empty selection, Ctrl+C, Super+C (Omarchy sends Ctrl+C) and the Copy button
  replaced the clipboard with `""`, so even what had been copied before was lost.
- Double and triple click did not select: `preview_pointer_down` ignored
  `event_clicks()` and each click only moved the cursor. Selecting a word with a
  double click and copying ended up in the previous case.
- Dragging, Ctrl+A and the button with a real selection did work: this was
  verified with a real pointer and keyboard in a private nested Hyprland, on NODE
  and on M2 (installed ARM binary), even with `wl-paste --watch` observers
  like Omarchy's and supernotch's.

## New behavior

- Double click selects the word (letters, digits and `_`, or a run of
  spaces or punctuation) without crossing the end of the line. Triple click selects
  the line with its original line break. Dragging afterwards extends from that start.
  This applies to text, Markdown and HTML.
- If nothing is selected, copying does not touch the clipboard and the footer shows
  "Nada seleccionado: arrastra, doble clic (palabra), triple clic (línea) o Ctrl+A".
- When something is copied, the footer shows "Copiado: N caracteres" until the next click or key.
- The "Copiar · Ctrl+C" button is no longer cut off.

## Verification

`scripts/preview-copy-nested-check.py` starts a nested Hyprland in a hidden
special workspace, uses `scripts/tools/vpointer.c` (a virtual pointer that
refuses `wayland-0` and `wayland-1`) and `wtype` only on the nested socket, and reads
the nested clipboard with `wl-paste`. It never touches the focused window or the
user's clipboard. Evidence in `docs/evidence/preview-copy-20261105/`:
the binary installed before the fix fails (`installed-before`), and the source
code passes.
