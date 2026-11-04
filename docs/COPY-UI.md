# Copia local con UI (0.0.4 en validación)

F5 en listados o «Copiar a…» del inspector abre un formulario propio con destino absoluto,
selección lossless y opción de iniciar en pausa. Encolar admite ocho trabajos
pendientes y uno activo; toast con progreso de bytes/entradas, pausa/reanudar,
cancelar activo y vaciar pendientes. Ningún conflicto sobrescribe un destino.
Los errores mantienen los contadores de copias parciales, rutas de rescate y
limpieza visibles antes del detalle habitual. Un éxito no obliga a cerrar modal.

Preflight prestado antes de clonar nombres o construir paths. El controlador
cobra capacidades retenidas de solicitudes, hasta4MiB incluyendo una reserva de
2MiB derivada del productor actual del reporte (paths4097B, errores1024B, sets128).
No es una afirmación de PSS total: el registro auxiliar del motor está limitado
separadamente a16MiB por defecto/hard32MiB, además del buffer256KiB y memoria UI/SO.
La interfaz conserva un único resultado completo y lo libera antes de despachar
la siguiente copia; no hay historial ilimitado de reportes.

Informe visual máximo64KiB y truncado explícito; la operación completa se conserva.
«Copiar informe completo» lo formatea bajo demanda, máximo8MiB, y rechaza una
versión truncada. Esa acción puede coexistir con el String temporal, CString de
FLTK y clipboard; no está incluida en los4MiB del controlador ni en una escena que
no la ejerza. La publicación real del clipboard requiere foco Wayland y sigue
pendiente junto con el caso Space; invocar un handler no prueba clipboard del SO.

Todos los resultados se aceptan con ID y ACK. La cola no despacha en el mismo
pump que entrega un resultado, para permitir detenerla ante rescate. Cerrar
cancela activo y pendientes y sigue bombeando hasta recoger el trabajador;
Drop no es evidencia de cierre seguro. Un syscall de FS bloqueado no se rodea
creando trabajadores nuevos. Cancelar no despublica raíces ya terminadas: se
muestran sus contadores/rutas. El origen nunca se elimina.

Al terminar raíces en la carpeta visible se recarga asíncronamente, conservando
filtro, scroll, layout y un snapshot lossless de selección/foco. Dos completions
antes del reload reutilizan el snapshot. Mientras está pendiente, selección por
clic/rango/teclado queda bloqueada y la barra lo declara. El snapshot admite
100k nombres y presupuesto8MiB antes de clonar: si excede, mantiene listado y
selección, muestra error y permite reducir selección y reintentar Ctrl+R.

Esto implementa copia local, no movimiento ni cortar/pegar archivos, DnD,
reanudación durable tras fallo, journal de crash, resolución reemplazar/fusionar,
ACL/xattrs ni todo el alcance O01/O02/O03. Permisos rwx, timestamps y symlinks sí
forman parte del motor; ver sus pruebas y la comparación aislada en COPY-BENCH.

## Comprobaciones reproducibles

- `cargo test --workspace`, Clippy all-targets con warnings como errores y build
  release: deben corresponder a las fuentes actuales, no a la suite previa.
- `python3 scripts/copy-ui-check.py --capture`: framebuffer real y handlers
  internos sobre fixture propia; datos, SHA256/modo/mtime/symlink, no-clobber,
  cola/pending cancel, destino recargado y salida0 después de ACK exacto.
- `python3 scripts/copy-ui-check.py`: misma escena sin capturas ni clipboard;
  árbol observado PSS+SwapPss, estable postcopia con límites temporales, CPU,
  FD/hilos/procesos y cleanup PID/starttime. Objetivo50MB decimal, techo150MB.
  No reemplaza baseline10k/1080p ni representa copias de árboles de100k entradas.
- Throughput contra cp en corrida separada: COPY-BENCH.md.

171tests Rust (73core/8helper/90UI), Clippy all-targets y build release PASS.
Captura funcional PASS en copy-ui-1793025091272455448.json: conflicto real entre
carpetas distintas, controles visibles, cancelación tras3120archivos en staging y
salida0 tras ACK exacto. Primera captura conservada: toast tapaba controles y el
conflicto era copia sobre sí misma; no equivale a la evidencia corregida.

copy-resources.json PASS **muestreado con hueco de salida**: pico34671KiB =
35.503104MB =33.8584MiB; estable32542KiB =33.323008MB; Swap observado0. Máximo19FD,
6hilos y1proceso legible simultáneo; una identidad hija de arranque salió durante
smaps sin memoria disponible. No se cuenta como cero ni se promete cobertura de
lifetimes.125muestras, intervalo máximo104.188ms (solicitado10ms). CPU postcopia
mediana0%, máximo118.82% de un núcleo: no es idle asentado. PID/starttime observado
sin supervivientes, cierre normal después de cancelar trabajo pausado y ACK.
En esta corrida, cancelación activa tras3280archivos/57930bytes lógicos limpió
la raíz no publicada; puede usar reflink, no se afirma transferencia userspace.

Fixture:16MiB +directorio10000archivos pequeños +symlink; tres raíces y listado
visible de3/4entradas, no10000filas UI. Geometría1000×650 lógica/2000×1300 física.
Hash files d8f2934638a56d069428e55a2209948db1df1a562c1eef8a012dd010e4059c40;
helper9a4719bba08f63e5b7e2918cd896a8ee4a74684de75d638ce9c2a4e8a152ec2d.
Baseline10k/1080p nuevo FAIL56.743936MB y startup FAIL151.978267ms máximo;
no PASS global ni etiqueta0.0.4 mientras Space/otros gates estén pendientes.
Fallos iniciales de suite/Clippy se conservan en evidence/0.0.4/copy-initial.

Post-checkpoint UI correction: measured Nerd text width now determines the Cancel
button width (short label only if narrow); reports say «Bytes lógicos copiados»
and translate clean cancellation/destination-exists causes. Updated framebuffer
copy-ui-1793026558872068261.json PASS; no repeat of the already approved resource
scenario for these label-only changes. Its hash differs from the earlier measured
candidate; the35.503104MB figure remains attributed to the exact earlier hash.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Opciones in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925Rust319/85/521+3existingignored, Clippyworkspace/alltargets-Dwarnings/release/fmt
PASS. Native1793457917645506527 UI/copy/mount PASS16framebuffers incl440² and
1000×560;4copiedroots(raw0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expectedFAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves57partial
files and original3000files intact.24loggedPID/start identities gone/0errors/survivors,
not lifetimes;3roots exit0 after normal callback. Native defaultcopy/overallFAIL
exit1 retained. Final copy-names UI1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visualFAIL preserved+sidecar;
explicit bounded lines fixed and finalframebuffer inspected. No source delete,
automatic removal of visible partials, powerloss/nativeinput/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.

La disponibilidad de otra revisión durante la cola activa y la geometría del progreso con MC se detallan en QUEUE-REVIEW.md. F5 en Espacio recalcula; la paleta y el inspector conservan Copiar como acción explícita.
