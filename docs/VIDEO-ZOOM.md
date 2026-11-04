# Vídeo y GIF: zoom y encuadre — 0.0.4

M18 reutiliza `image_preview_view::Viewport`, sin decoder ni caché nuevos.
Ctrl+rueda o +/- amplía de1× a8×; arrastrar desplaza el encuadre y0 vuelve a
ajustar. Arriba/abajo desplazan verticalmente; izquierda/derecha conservan la
búsqueda de5s. La línea de tiempo tiene prioridad sobre el gesto de arrastre.
No se precargan vídeos ni se reproduce al abrir.

La llegada de un frame invalida sólo los píxeles remuestreados, sin reiniciar
zoom/pan ni liberar el único buffer de viewport. Pausa/resume conserva el mismo
visor. Buscar libera el RGB anterior y conserva únicamente tres números; el
nuevo snapshot restaura ese transform. Reabrir archivo, cambiar origen o recibir
invalidación de su revisión reinicia el encuadre y descarta la generación vieja.
El encuadre se ajusta a los límites de la imagen durante paint, sin I/O.

El raster de muestreo sigue acotado al viewport (máximo1160×800×3bytes), incluso
con zoom8×. La capacidad puede conservarse al reducir ventana; no es prueba del
consumo del proceso/helper/allocator ni del presupuesto global. RAM/pipes, sin
caché de producto en disco ni historial de frames. La transformación es de la
sesión del origen seleccionado; no se persiste por archivo.

488Rust (141core/58helper/289UI), Clippyworkspace/all-targets-Dwarnings,
release yfmt limpios. Prueba de muestreo verifica nuevo RGB sin píxeles stale,
capacidad reutilizada, transform conservado y reset distinto. TestsState de
stream/seek/coalescing comprueban persistencia del transform y reset por archivo.

`video-play-ui-1793101116101351845.json` GIF y
`video-play-ui-1793101152229152975.json` FFV1/MKV: PASS funcional,11/9framebuffers
reales1000×650. Action compartida amplía2×, handlers de pointer desplazan48/36;
asserts tras rojo→azul/pausa/seek2s/resumeEOF mantienen esos valores. Reabrir
reinicia1×/0/0. Cierre con stream observado vivo termina0trasACK y12/10identidades
PID/start registradas ya no vivas (Z no aceptado), sin cobertura de lifetimes.
La escena GIF incluye miniatura inspector160px ya existente; ningún decoder nuevo.
Se inspeccionaron capturas de ambos perfiles; campos/controles sin solapamiento
evidente. Uniformes rojo/azul no prueban detalle fotográfico ni exactitud cromática.

No se generó clip24frames ni se infiere memoria del footer incidental. No inputSO ni prueba de
nitidez2×. Recursos/CPU/arranque NOT_MEASURED por freeze; los FAIL/UNVALIDATED
históricos permanecen y P07/P09 continúan parciales (VFR/PTS, color, A/Vsync,
codecs grandes y otrosSO).

Checkpoint59e348c instalado automáticamente con backup`/home/avillagran/.cache/files-test/installed-backups/1793101224102815320`.
files`8cb2c80c87045b60f9a8ea02010f1831be0d61b1323dab7f2c124d948b5a9b7f`; helper/HTMLcoherentes
verificados en`docs/evidence/0.0.4/installed-checkpoint-1793101224257101226.json`. No se controlaron ventanas existentes.
