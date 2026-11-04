# Bounded text preview helper

`files-helper preview_text` accepts a single JSON request:

```json
{"command":"preview_text","id":1,"path":"/absolute/file.txt"}
```

The `text` response contains `id`, `text`, `encoding`, and `truncated`. Errors use
the existing `error` response. The subcommand must match the request variant.
The process exits after its response, like `du`.

The reader consumes at most 256 KiB plus one sentinel byte, even for sparse files
many gigabytes long. It displays at most 2000 LF-delimited lines, preserving CRLF.
It supports UTF-8 (optional BOM) and UTF-16LE/BE with BOM. A code point split only
by the byte limit is dropped and truncation remains explicit; invalid encodings
are rejected instead of silently replaced. Binary control characters other than
TAB/CR/LF are rejected. Other encodings and symlink targets are pending.

Unix opens use O_NONBLOCK/O_NOFOLLOW, then validate the opened descriptor is a
regular file before reading, so FIFO replacement cannot block open. Blocking
filesystem operations on remote mounts still need the future preview supervisor's
hard deadline and cancellation. The existing Linux helper address-space limit
remains in effect. Text JSON escaping can expand the bounded text: consumers must
allow up to 2 MiB for this response, while retaining the 64 KiB request limit.
The du supervisor's 8 KiB response limit must not be reused unchanged for text.

This is text/source preview plumbing. Space overlay, syntax highlighting,
selection/copy, keyboard navigation, rendered Markdown/HTML, image/video/PDF
backends, integrated PSS and rendered UI acceptance are still pending. It must
not be advertised as Markdown or HTML rendering.

Validation commands: `cargo test -p files-helper`, helper Clippy with warnings
denied, `cargo build -p files-helper`, and `python3 scripts/text-preview-check.py`.

Verified locally: 8 helper Rust tests (5 text + 3 du), helper Clippy with warnings
denied, 8 text protocol cases and all 6 existing du protocol checks passed.
`text-preview-memory.py` measured the debug helper alone with a sparse 4 GiB file
and a 256 KiB response, deliberately holding stdout for 500 ms: sampled peak
PSS+Swap **1.3824 MB**, complete readable samples and cleanup. This tests output
backpressure and bounded allocation, not UI totals or preview latency. Evidence
is `docs/evidence/text-preview/helper-memory.json`.

## Space UI (0.0.4 en validación)

Space o Vista previa del inspector abre el texto en UI propia. Un solo controlador
retiene el último destino solicitado; cancela y recoge el helper anterior antes de
reemplazarlo, descarta respuestas obsoletas incluso al reabrir el mismo path y libera
el documento al cerrar. Navegar, cambiar selección y cerrar ventana cancelan; la
salida espera `is_drained`, también al descartar preferencias espaciales.

Sólo líneas visibles; selección por puntero/Shift+flechas/Ctrl+A y CopyPreview.
`Painter::literal_text` y `text_width` usan la misma fuente; cada línea visible
expande TAB a8columnas escalares y controles a^X, con mapa fuente→display. Se copian
bytes originales UTF8/CRLF/TAB, no la transformación. Gutter tiene clip independiente;
selección vacía sólo pinta caret. Columnas no son graphemes. Para acotar shaping,
se dibujan primeros4096bytes de cada línea y se etiqueta el recorte: resto aún no
accesible visualmente, aunque Ctrl+A/copia conserva fuente completa dentro del límite.
Alt+↑/↓ cambia archivo, Shift+rueda desplaza horizontalmente. Texto plano/código;
Markdown y HTML son **fuente**, sus renderers reales siguen pendientes.

126tests workspace (73core/8helper/45UI), Clippy y release aprobados. Nuevas pruebas
de width/tab/control/gutter/selección vacía, controlador y supervisor. El verificador
`scripts/text-ui-check.py --capture` comprueba fuente Unicode+TAB+CRLF por puntero,
A→B→C, errores binarios, truncado explícito y cierre con helper pendiente. Capturas
reales en evidence/0.0.4, handlers internos; no eventos nativos de teclas/puntero.

Clipboard externo sigue **FAIL, no se cierra el hito**: wl-paste no recibe oferta
cuando la fixture no es ventana activa y FLTK publica con serial0; traza registra
set_selection y ningún keyboard_enter. Falta foco Wayland de esa fixture y repetición
exacta tras CopyPreview. No se sustituye por wl-copy ni se leen contenidos previos.
El fallo inicial de bucle se corrigió por separado: wrapper1.5.23 rechazaba conteos
poll>=2. Dos pipes prueban raw2/callbacks2; escena nueva registra raw4/3 con éxito.
No se conoce raw del fallo original, por lo que no se afirma causalidad exacta de
ese evento. Wrapper local conserva C Fl_wait_for, errno inmediato y retry sóloEINTR;
error real devuelve salida1 tras cancelar/drenar mutaciones/preview/du.

Métricas integradas Space todavía pendientes, separadas de captura/clipboard.
Las cifras aisladas anteriores del helper no son presupuesto del árbol de la app.
