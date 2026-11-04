# Mover — contrato M24 / dependencia EXDEV

Estado: M24 rename nativo implementado y aceptado funcionalmente; EXDEV sigue
pendiente como dependencia explícita, sin marcar Move completo.
Freeze recursos/startup vigente. Reusar cola global, controles pausa/cancel/ACK,
preflight de rutas y UI de destino; F6, contextual e inspector tienen acceso real.
No confundir copiar con mover ni marcar O01/O03/O06 completos con esta unidad.

## Revisión y captura

El diálogo Mover recibe nombres raw/ruta origen capturados. Revisar destino corre
fuera de UI en AuxIo existente y devuelve token: padres origen/destino abiertos,
FD O_PATH|O_NOFOLLOW por raíz, identidad dev/ino/tipo y rutas lossless. Confirmar
sólo encola ese token revisado, no vuelve a interpretar la selección actual.
Editar destino descarta token y exige revisión nueva. Cancelar/cerrar durante
revisión solicita cancelación y drena ACK; respuesta obsoleta sólo libera FDs.

Primera unidad Linux: máximo128 raíces por movimiento,128FDraíz+2padres. La cola
presupuesta adicionalmente metadata/FDs y limita256FD de tokens admitidos; un
único token en revisión/respuesta sin consumir puede coexistir con esa cola
(hasta130FD adicionales: máximo386FD de tokens simultáneos, más temporales
acotados del recorrido de ancestros/diagnóstico). Activo y resultado de worker
aún sin ACK conservan el cargo de FDs hasta consumir la respuesta. Límites se verifican antes
de clonar/abrir; exceso muestra reducir lote. No son límites PSS ni garantía de
ulimit; fallo al abrir conserva originales. Otros SO explícitamente Unsupported.

## Mismo filesystem

`renameat2(RENAME_NOREPLACE)` relativo a padres capturados. Symlink se mueve
literalmente, nunca se sigue su destino; archivos/carpetas conservan inode y
metadatos/ACL/xattrs por rename, sin recorrer/copiar descendientes. No reemplazar
ningún destino, incluidos symlinks rotos. Detectar duplicados del lote y self-move
por identidad de padres+nombre; rechazar destino dentro del origen incluso por
alias mediante recorrido acotado de ancestros del FD destino (o error seguro si
no puede verificarse). No usar sólo comparación de strings para confinamiento.

Revalidar entrada y FD raíz antes y después de syscall. Linux no ofrece rename
condicionado a inode: existe intervalo check→rename frente a escritor hostil.
Mismatch posterior => estado no confirmado, conservar todo, informar carpetas de
revisión verificadas por FD y contadores; no borrar/limpiar/rollback a ciegas una
entrada reemplazada. No prometer protección absoluta frente a cambios hostiles.

Pause/cancel se evalúa antes de cada publicación. Cancel después del último
checkpoint puede llegar durante el rename ya en curso: un root puede publicarse.
Raíces anteriores permanecen movidas, restantes en origen. Resultado distingue
completo/parcial/cancelado/no confirmado; no hay rollback global ni undo durable
para este lote en M24. Cierre mantiene worker hasta ACK y muestra rescate/errores.

Capturar origen y destino por ID al admitir. Al terminar, invalidar/refresh AMBAS
carpetas si la activa corresponde a cualquiera; volver a inactiva relista. No
refrescar otra pestaña por coincidencia de nombre ni destruir su scroll/selección.
Rutas mostradas capturadas pueden quedar obsoletas si un tercero renombra padres;
resultados verifican rutas actuales por FD cuando posible, nunca reconstruyen
rutas falsas de rescate como autorización para borrar.

## EXDEV explícito, sin degradación silenciosa

Primer checkpoint M24 sólo rename nativo. EXDEV (preflight por dispositivo o
syscall) => error visible: entre filesystems pendiente, origen conservado; NO
fallback copy+delete. Tests de error EXDEV con backend inyectado no equivalen a
prueba de un montaje real. Esta limitación no cierra requisito Move completo.

El siguiente bloque EXDEV necesita manifiesto acotado por entrada, origen FD y
revisión/contenido validado (root inode/mtime no acredita descendientes), destino
copiado y verificado/durable antes de eliminar ninguna fuente; cambios => conservar
origen y resultado parcial. CopyReport.completed son sólo rutas/contadores, no
permiso de remove_dir_all/unlink. Definir preservación permisos/ACL/xattrs/hardlinks
sin borrado de original si el backend no puede conservarlos; journal/recuperación
segura como dependencia explícita para árboles grandes y fallos de proceso.
No adaptar el copy engine para aparentar move con un borrado inseguro.

## Aceptación

Fixtures privados: file/carpeta no vacía/symlink literal/raw, inode/bytes/metadatos,
colisión existente y de lote/self/descendiente/alias, origen sustituido entre review
y Apply, padre renombrado tras captura, cancel antes y después de algunos roots,
pausa/cancel/cierre/ACK sin SIGTERM, refresh A y B con otra tab activa. Suite,
Clippy/release/fmt serializados, capturas de revisión/resultado/error y filesystem
comprobado. Handlers/framebuffer no prueba input SO. Instalar sólo checkpoint
validado con backup/hashes. Métricas NOT_MEASURED; resto ROADMAP sigue abierto.


## Evidencia M24 — 2026-10-27

537Rust148core/67helper/322UI, Clippyworkspace/alltargets-Dwarnings, release yfmt
PASS. Logs finales move-final-test, move-second-clippy, move-release ymove-final-fmt;
copias con fallos previos en move-checkpoint-validation.json. Regresiones motor:
pinnedparents frente a pathreemplazado, source reemplazado, destino descendiente
movido tras review, preCancel/canceltrasunroot, EXDEV inyectado y postrename
uncertainty incluso source recreado hardlink al original. published/verified
explícitos; identidad se repite después de recorrer ancestros, junto al syscall.

Queue test cuenta activo/unreadACK+pendientes en256FD, libera al ACK; input
metadata se cobra además de rutas/capacidades. Cierre señaliza TODOS los
subsistemas antes de readiness: revisión pendiente no retrasa cancelación de
cola/helpers. Kernel IO atascado sigue cooperativo, no se declara interrumpido.
Tests Model refrescan A/B pero no C/query/foco, menú busca Propiedades/Mover
semánticamente, sin índices dependientes de la nueva fila.

`move-ui-1793113502303715506.json` PASS6FB1000×650. Fingerprints exactos antes
de Confirmar y tras Confirmar-Pausado prueban que revisar/pausar no publica.
Tras Resume,4roots (archivo64KiB, carpeta+texto Unicode, symlink literal y raw0xFF)
conservan dev/ino/modo/uid/gid/mtime/ns/bytes y xattrs presentes; symlink conserva
su texto. Target activo conserva selección untouched tras refresh; volver a
Source muestra roots ausentes y selección vacía. Colisión existente no pisa;
/proc en otro dispositivo rechaza EXDEV en preflight sin copy/delete.
Cancel activo pausado y cierre activo pausado conservan sus orígenes; cierre
exit0 posterior a ACK. Cancel después de roots sólo probado por callback motor,
no timing de input UI. No evidencia de transferencia EXDEV ni crashresume/undo.

Método Actions/handlers internos y framebuffer lógico; no inputSO/clipboard ni
nitidez nativa2x. PSS incidental del pie no cuenta. Recursos/startup NOT_MEASURED
por freeze. La migración de layout espacial al cambiar ruta de carpeta sigue
pendiente (su clave actual incluye ruta canónica); no se afirma conservar esa
preferencia por el simple hecho de preservar inode de archivos/carpetas.

Siguiente bloque O01/O03: manifiesto/metadata/durabilidad y recuperación segura
para EXDEV; no habilitar borrar origen usando sólo CopyReport.completed.
