# Copiar texto desde la vista previa

Reporte (2026-11-05): "Si previsualizo un archivo de texto, selecciono y copio
(Ctrl+C o Super+C) NO COPIA, el botón tampoco".

## Causa

- `copy_preview` publicaba `selected_text()` aunque estuviera vacío: con una
  selección vacía, Ctrl+C, Super+C (Omarchy envía Ctrl+C) y el botón Copiar
  reemplazaban el portapapeles por `""`, así que se perdía incluso lo copiado antes.
- Doble y triple clic no seleccionaban: `preview_pointer_down` ignoraba
  `event_clicks()` y cada clic solo movía el cursor. Seleccionar una palabra con
  doble clic y copiar terminaba en el caso anterior.
- El arrastre, Ctrl+A y el botón con selección real sí funcionaban: se
  verificó con puntero y teclado reales en un Hyprland anidado privado, en NODE
  y en M2 (binario ARM instalado), incluso con observadores `wl-paste --watch`
  como los de Omarchy y supernotch.

## Comportamiento nuevo

- Doble clic selecciona la palabra (letras, dígitos y `_`, o una racha de
  espacios o de puntuación) sin cruzar el fin de línea. Triple clic selecciona
  la línea con su salto original. Arrastrar después extiende desde ese inicio.
  Aplica a texto, Markdown y HTML.
- Si no hay selección, copiar no toca el portapapeles y el pie muestra
  "Nada seleccionado: arrastra, doble clic (palabra), triple clic (línea) o Ctrl+A".
- Si se copia, el pie muestra "Copiado: N caracteres" hasta el siguiente clic o tecla.
- El botón "Copiar · Ctrl+C" ya no se corta.

## Verificación

`scripts/preview-copy-nested-check.py` inicia un Hyprland anidado en un
workspace especial oculto, usa `scripts/tools/vpointer.c` (puntero virtual que
rechaza `wayland-0` y `wayland-1`) y `wtype` solo sobre el socket anidado, y lee
el portapapeles anidado con `wl-paste`. Nunca toca la ventana enfocada ni el
portapapeles del usuario. Evidencia en `docs/evidence/preview-copy-20261105/`:
el binario instalado antes del arreglo falla (`installed-before`), y el código
fuente pasa.
