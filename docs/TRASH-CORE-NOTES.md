# Papelera Linux: núcleo parcial

2026-10-26. Integración parcial Linux visible en Acciones/Supr y Deshacer. Los resultados funcionales y unitarios se registran abajo; recursos y arranque NOT_MEASURED por suspensión explícita del usuario. No añade dependencias ni declara O04 completo.

La [Trash Specification 1.0 de freedesktop](https://specifications.freedesktop.org/trash/1.0/) define la papelera principal en `$XDG_DATA_HOME/Trash`, con directorios `files` e `info`. Cada objeto tiene un nombre único y un `.trashinfo` correspondiente. Se reserva primero el metadato con creación exclusiva; `Path` guarda los bytes de la ruta escapados como URL y `DeletionDate` usa fecha/hora local. Este motor aplica ese formato a una raíz de papelera principal explícita. No elige ni implementa papeleras por volumen, ni interpreta un nombre almacenado como ruta de restauración.

## API

- `TrashContext::open(root) -> Result<TrashContext, Failure>`: abre o crea únicamente `root`, `root/files` y `root/info`. El padre de `root` debe existir.
- `context.trash(source, cancel) -> Result<Receipt, Failure>` y conveniencia `trash(source, root, cancel)` para un objeto.
- `restore(&receipt, cancel) -> Result<RestoreReport, Failure>`: devuelve el objeto al **descriptor del padre original**, aunque ese directorio se haya renombrado. El nombre final original se mantiene, sin sustituir ningún ocupante nuevo.
- `Receipt::original_path()` es histórica, corresponde al metadato. `current_original_path()`, `trashed_path()` e `info_path()` resuelven ubicaciones actuales mediante `/proc/self/fd` y verifican identidades. Un error al resolver no se convierte en una ruta antigua presentada como rescate.
- `Failure` conserva `cause`, `preserved_paths`, `cleanup_paths`, `rollback_errors`, `unresolved_paths` y `restore_retryable`. Este último comienza en `false` y, al fallar una restauración, sólo pasa a `true` si una comprobación nueva encuentra tanto el objeto esperado como su metadato esperado todavía en sus slots de Papelera. El tipo de error no decide el reintento; cualquier error al comprobar identidades mantiene `false`. Las rutas de objetos conservados y las de metadatos residuales tienen funciones distintas; el caller debe mostrarlas.
- `RestoreReport` confirma que el movimiento de restauración y la identidad del objeto ya se verificaron. `restored_path: Option<PathBuf>` contiene `Some(ruta_actual_verificada)` o `None` si falla sólo la resolución diagnóstica posterior al commit. En ese caso `unresolved_paths` aumenta y `cleanup_errors` explica el fallo, sin inventar una ruta histórica ni devolver un error de movimiento. El caller debe retirar ese recibo del historial con cualquier `Ok(report)`, aunque haya advertencias, y seguir mostrando los diagnósticos. La limpieza de su propio `.trashinfo` se intenta igualmente. `cleanup_paths/cleanup_errors` también indican metadatos residuales; cada ruta residual que no pueda resolverse aumenta `unresolved_paths`.

El contexto es `Clone` mediante `Arc` y comparte tres FD: raíz, `files`, `info`. Cada `Receipt`, sin `Clone`, conserva tres FD: padre original, objeto `O_PATH` y metadato. Los dos últimos evitan confundir una reutilización de inode con el objeto confirmado. Un lote UI de 64 recibos con contexto compartido retiene aproximadamente 195 FD, además de los que ya tenga el proceso. El caller debe limitar el lote y el historial; esta API no promete 100000 recibos ni un historial ilimitado. Soltar el recibo no elimina nada de la papelera.

## Seguridad y límites

La raíz, `files` e `info` deben pertenecer al UID efectivo y tener modo exactamente `0700`; los `.trashinfo` creados son regulares, de un enlace y `0600`. No se corrigen silenciosamente permisos existentes. Se recorre cada componente administrado sin seguir symlinks. Los ancestros deben pertenecer a root o al UID efectivo y no permitir escritura a grupo/otros; el padre inmediato debe pertenecer al usuario. Un padre `~/.local/share` con modo `0755` es válido. No se usa `create_dir_all` para crear ancestros no comprobados. El caller debe preparar de forma segura `$XDG_DATA_HOME` si falta y pasar esa raíz; la biblioteca no infiere la configuración global ni usa datos personales en tests.

Las rutas de entrada tienen máximo 4096 bytes, son absolutas, sin NUL, `.` o `..`; no se acepta `/`. Se resuelve una vez el padre del origen, conservando el último componente como objeto sin seguir su symlink. El origen puede ser archivo, directorio no vacío o symlink; `renameat2(RENAME_NOREPLACE)` conserva el objeto y su contenido/metadatos al moverlo. Se rechaza que el origen esté dentro de esta papelera o sea la raíz/algún ancestro de ella, comprobando también identidades de directorios por FD.

Antes de mover, se escribe/sincroniza el metadato exclusivo y su directorio. Se revalidan contexto, objeto y metadato, y se comprueba cancelación inmediatamente antes del movimiento. La cancelación es cooperativa: no interrumpe una syscall bloqueada; después de un commit confirmado se devuelve éxito. La comprobación posterior detecta sustituciones observadas. La recuperación usa movimientos exclusivos y nunca sobreescribe para recuperar. Si no puede demostrar una ruta actual, conserva una cuenta sin resolver en lugar de inventarla.

No existe en Linux una syscall general «rename/unlink sólo si este inode sigue aquí». Un actor concurrente puede modificar el intervalo entre comprobación y syscall; las identidades y los FD no convierten toda la secuencia en una transacción condicional. Procesos del mismo UID pueden modificar una carpeta `0700`. Esto no es aislamiento contra un actor hostil con las mismas credenciales ni garantiza una instantánea del contenido modificado in-place. Los fallos de rollback se exponen; no se declara ausencia absoluta de carreras.

No hay fallback de copia. Un dispositivo distinto o `EXDEV` produce error claro y conserva el origen. Las papeleras `.Trash/$uid`/`.Trash-$uid`, montajes, restauración de recibos persistidos, limpieza/vaciado, redo y recuperación tras crash están pendientes. No se elimina permanentemente contenido del usuario. Una interrupción del proceso puede dejar un metadato huérfano o un objeto pendiente de recuperación; los `fsync` previos no equivalen a journal ni atomicidad ante corte eléctrico. El cache opcional `directorysizes` no se implementa.

Los nombres almacenados usan prefijo raw limitado del nombre original más PID/secuencia; se intentan hasta 128 candidatos exclusivos. `.trashinfo` queda por debajo de `NAME_MAX` típico de Linux. Los nombres originales permanecen lossless en `Receipt` y en `Path` percent-encoded, incluidos Unicode, `%`, saltos de línea y bytes no UTF-8. No se trata el texto de diagnóstico como una ruta operable.

La operación de un objeto conserva estado constante: metadato de menos de 13KiB con ruta de 4096 bytes, nombres acotados y, como máximo, cuatro rutas de diagnóstico por colección de 8192 bytes y mensajes de 1024 bytes. Son cotas de los datos retenidos, no una medición PSS de proceso ni un presupuesto del lote UI. No se ha abierto la fase de mediciones de recursos/arranque.

## Cobertura de tests

Los tests Linux usan sólo carpetas propias bajo `~/.cache/files-test`: contenido/round-trip, nombres no UTF-8 y escapado, no-clobber al restaurar, symlinks sin seguir, directorio no vacío, cancelación antes del commit, sustitución del origen, padre renombrado/reemplazado, raíz de papelera reubicada, objeto/metadato reemplazados, nombres repetidos, destino ocupado, permisos/ancestros y limpieza que rechaza metadato ajeno. Los tests puros cubren formato/escapado y preflight. La suite integrada y la escena UI se verifican serialmente en la raíz. EXDEV necesita fixture aislada en un segundo filesystem, nunca un montaje/dato personal por inferencia.

## Corrección del diagnóstico tras restaurar

La revisión de integración detectó un caso estático: después de mover el objeto y confirmar su identidad, una resolución de `/proc/self/fd` fallida podía devolver `Err`. El historial conservaría entonces un recibo cuya ubicación en `files` ya estaba vacía. La corrección devuelve `Ok(RestoreReport { restored_path: None, unresolved_paths: 1, ... })` para ese fallo de diagnóstico, continúa la limpieza revalidada del metadato y conserva una advertencia explícita. No se revierte un commit confirmado sólo porque falte una ruta de presentación. Los errores anteriores al commit y los fallos de identidad posteriores siguen como `Failure` con recuperación/diagnósticos.

Regresión preparada con un resolver inyectado: comprueba dentro del callback que el objeto ya está en el padre original con la identidad esperada, el slot en Papelera está vacío y el metadato aún existe; fuerza `PermissionDenied`; exige `Ok`, ruta `None`, una ubicación sin resolver y la eliminación revalidada posterior de su propio metadato. La regresión forma parte de la suite integrada del núcleo.

## Sustitución externa durante restauración

Si después del movimiento el nombre del padre original contiene una identidad distinta, no se mueve ese ocupante ajeno a Papelera como supuesto rollback: no se puede distinguir una sustitución previa al syscall de otra posterior. El ocupante observado permanece en su sitio, el objeto original se localiza mediante su FD `O_PATH`, y los metadatos se conservan para recuperación manual. El informe muestra las ubicaciones actuales verificadas, sin ejecutar nuevas relocaciones sobre identidades ajenas.

`Failure.restore_retryable` se calcula al salir del wrapper de restauración mediante identidades actuales de ambos slots, no mediante `AlreadyExists`/cancelación. Conflicto y cancelación normales conservan `true`; objeto movido/reemplazado, metadato reemplazado o comprobación no disponible dejan `false`. La UI debe separar recibos reintentables de recibos retenidos únicamente para recuperación, manteniendo el total acotado y sin ofrecer Undo para un token obsoleto. Se añadieron fixtures de sustitución después del movimiento y cancelación con metadato luego reemplazado, además de ampliar los asserts del caso de reemplazo anterior a la operación. No se ejecutaron Cargo, procesos ni medidas durante este ajuste.

## Integración visible y recuperación acotada

Acciones/Supr envía la selección al worker único de mutaciones; no abre ni sondea
el filesystem en el hilo UI. El preflight prestado rechaza lotes de más de 64
objetos, nombres repetidos o más de 256 KiB antes de clonar. No trunca una selección
silenciosamente. El primer commit confirmado sustituye el historial previo; fallos
anteriores conservan Undo. La creación fallida tampoco borra el historial. No hay
redo de Papelera ni rollback atómico de un lote: se conservan recibos de los objetos
confirmados y se muestran errores y rutas de rescate.

Undo restaura en orden inverso. Un destino ocupado conserva el nuevo ocupante y
permite reintentar mientras objeto y metadato sigan siendo los esperados. Un recibo
no reintentable se aparta en recuperación manual, sin ofrecer un Undo obsoleto;
retiene sus FD hasta la siguiente mutación confirmada o el cierre. El total de
recibos utilizables y apartados sigue limitado a 64. El cierre cancela y drena el
resultado del worker antes de salir. La escena funcional verifica cierre normal
tras ACK; no prueba cierre durante una syscall bloqueada ni crash recovery.

Los errores usan un panel con scroll vertical y desplazamiento horizontal mediante
Shift+rueda o flechas. Las rutas Debug son diagnósticas, incluidas las no UTF-8;
no se vuelven a interpretar como instrucciones. El éxito usa estado compacto.

La revisión encontró una simetría importante: tanto al enviar como al restaurar,
una sustitución externa posterior al movimiento nunca autoriza mover al ocupante
ajeno como supuesto rollback. Sólo se intenta devolver el inode esperado, con
comprobación nueva y destino exclusivo; si ya no está en su slot, se preserva todo
en su ubicación observada y se informa recuperación manual. Una regresión deja
vacío el nombre original para impedir que no-clobber oculte el caso incorrecto.
El intervalo check→syscall frente a otro proceso del mismo UID continúa explícito.

## Verificación integrada

314 tests Rust (98 core, 37 helper, 179 UI), Clippy workspace/all-targets
`-D warnings` y `cargo build --release` PASS. Escena final
`trash-ui-1793045192155795906.json` funcional PASS para lote de cuatro, conflicto y
reintento, raw no UTF-8, directorio/symlink y rechazo de 65 elementos; cierre normal
tras ACK. Capturas y límites exactos en VALIDATION. Recursos/startup NOT_MEASURED.
