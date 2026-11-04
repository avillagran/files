# Recursos incrustados ligados al descriptor — 0.0.4 (2026-10-27)

Esta unidad cierra la carrera entre la sonda de un recurso Markdown y el FD del
helper que realmente lo decodifica. No cierra todavía su invalidación después de
publicar el documento: watcher de todos los padres, reconciliación y retirada
inmediata del bitmap incrustado siguen pendientes. No se afirma P09 completo.

`relative_source::Opened` mantiene rootFD y fileFD. La raíz final requiere
O_DIRECTORY/O_NOFOLLOW/CLOEXEC; cada componente relativo se abre con openat y
O_NOFOLLOW, intermedios O_PATH/DIRECTORY, archivo NONBLOCK/regular-only.
Padres anteriores a la raíz pueden seguir symlinks; no limita mountpoints ni
hardlinks, no es sandbox. Root dev/ino y archivo dev/ino/size/mtime/ctime/ns forman
`Proof`. No es hash de contenido ni snapshot atómico del filesystem.

El helper de texto ahora devuelve la identidad efectiva de la raíz y revisión del
FD leído, y verifica FD/nombre después de leer. La lectura directa conserva su
semántica de padres symlink; recursos relativos conservan la política más estricta
ya existente. El lector de prefijo no carga una imagen truncada silenciosamente:
se pide cap+1 y el codec compartido rechaza comprimido mayor de8MiB antes de leer
cabecera. Texto mantiene256KiB/2000LF y truncado explícito.

Markdown pasa la raíz efectiva del texto a ProbeRelativeFile. Esta sonda abre el
recurso sin leer su contenido, exige la misma raíz y devuelve Proof. Después de
reap se inicia PreviewRelativeImageChecked con esa firma esperada. El helper
compara contra sus propios FD ANTES de leer, reutiliza decoder/IDCT existente y
verifica después de decode: rootFD, raíz que ocupa el nombre actual, fileFD y
archivo alcanzable mediante todos los componentes relativos. Un reemplazo de
carpeta intermedia también invalida el resultado. Response::Image devuelve la
firma efectiva y Revision del archivo; UI exige ambas iguales a la esperada antes
de aceptar RGB. La generación del controller sigue descartando respuestas de un
documento anterior al navegar/cerrar. No se añade un decoder ni helper paralelo.

Por documento, máximo8 recursos normalizados: manifiesto de paths<=4096B y Proof,
además de metadata Vec/tuplas, separado de RGB4MiB+reserva transiente4MiB. Arc
comparte ocurrencias sin duplicar RGB; capacity del wire drenado sigue cobrada.
La sonda y decoder corren secuencialmente con presupuesto temporal restante10s del
documento; cierre/cancelación usa supervisor existente y reap antes de reemplazo.
No se espera I/O en draw/UI. No hay caché persistente nueva ni archivos de producto
junto al original. Linux verificado; otros SO devuelven Unsupported, no dev/ino0.

Hay una ventana después de la última comprobación de nombres y la publicación,
y cambios posteriores todavía no invalidan las incrustadas. El manifiesto de
firmas prepara la siguiente unidad de watcher/reconcile; no la sustituye. Tampoco
se afirma cierre acotado de una syscall de montaje en estado ininterrumpible.

434Rust141core/47helper/246UI, Clippyworkspacealltargets-Dwarnings y release
limpios. `scripts/relative-source-check.py` real release PASS en
`evidence/relative-source/relative-source-wire-1793074260681793818.json`:
texto/raíz efectiva, A→B mismo path/size/mtime antesopen, nueva B aceptada,
root rename/reemplazo, padre intermedio reemplazado, symlinks/traversal y nombre
raw0xFF. El FAIL1793074121342824158 se conserva: los comandos nuevos faltaban de
la allowlist CLI inicial, corregida antes de la evidencia PASS.

`evidence/0.0.4/markdown-embedded-ui-1793074268202431350.json` funcionalPASS:
5 ocurrencias/2 recursos únicos comprobados, RGB retenido3165524B (capacity y
metadata del ledger; NO PSS), navegación root→subdir→nested→subdir→root,
escapes bloqueados,5capturas1000×650 y callback→previewdrain→exit0. Handlers
internos+framebuffer, NO inputSO. Cleanup en esta captura sólo prueba rootexit0,
no cobertura de lifetimes de hijos: no hubo sampler. Tiempos diagnósticos ya
emitidos por el harness no se usan como gate de rendimiento ni arranque.
 Recursos y arranque
NOT_MEASURED por freeze explícito. Los FAIL globales históricos continúan activos;
sin etiqueta0.0.4 ni PASS global.

d1dbd02 instalado con backup y hashes coherentes, informe
`evidence/0.0.4/installed-checkpoint-1793074346922882547.json`:
files e40bd5c13646007e44874b4eb27dd84ab1e94ee07b975486d427d32122dae576;
helper fcf3a913c39b5173137a904d435a166bb7ae3c39052c17e382905786e3ca9fa7;
HTML a5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.

Seguimiento M4: DOCUMENT-MEDIA añade candidato oculto→watch/probe final→Ready,
invalidation/reconcile de intenciones y recursos, root recovery y pruebas UI.
El pendiente postpublicación descrito arriba corresponde al checkpoint d1dbd02;
ver DOCUMENT-MEDIA para cobertura actual, límites y fallos todavía abiertos.
