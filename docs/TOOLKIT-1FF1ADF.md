# Toolkit lib-fltk-omarchy 1ff1adf en Files

2026-11-04, pedido del hilo del toolkit. Export exacto de los archivos
trackeados de `1ff1adfcf4589a490c8f08423df7b6bc1361e3ee` (GitHub main, sin
tag; incluye glass ff3d315 y pixels_per_unit a967007) en
`vendor/lib-fltk-omarchy`. Files compila sin cambios de código; el lockfile
suma `unicode-bidi` y `unicode-segmentation` (MIT/Apache, ver
DEPENDENCIES.md).

Recibe: tooltips Omarchy dibujados por el toolkit (tooltips.json en vivo,
tema/esquinas, arreglo de `#000000`), animaciones según el sistema
(Hyprland `animations:enabled`; `OMARCHY_FLTK_ANIM=on|off`) con fundido de
menús/popups/diálogos, capa de plataforma, i18n, RichText, ListModel, RTL,
editores y el arreglo de FL_SHORTCUT con first_window() nulo.

Verificación (`scripts/toolkit-visual-check.py`, workspace oculto Wayland y
Xvfb X11 privado, sin input SO): tooltip «Preferencias» del engranaje
pintado por el toolkit; menú de Preferencias con fundido (primer fotograma
≠ final) con animaciones, idéntico sin ellas. Los checks que comparan
píxeles de menús (toolkit-input, toolkit-menu, popup-media) fijan
`OMARCHY_FLTK_ANIM=off`. 1640 pruebas (Xvfb), Clippy.

Fixtures corregidos en la misma entrega (no son API del toolkit):
`toolkit-menu-ui-check` ya no supone la posición de Renombrar (el menú tiene
secciones desde el bloque 1): Files informa índice y pulsaciones; y los
scripts de los bloques comprobaban `'REV' in línea`, que «PREVIEW» siempre
cumple, de modo que nunca reintentaban ante la recarga del listado (un FAIL
intermitente de video conservado como evidencia).
