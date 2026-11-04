# Propiedades y permisos locales

Bloque parcial U04: Acciones → Propiedades o Alt+Enter abre un objeto seleccionado.
No interpreta la ruta como comando ni lee el contenido. Muestra tipo, tamaño
aparente, bloques asignados (`st_blocks * 512`, no uso físico exclusivo), número
de hardlinks, UID/GID, modo, identidad dev/inode y timestamps de acceso,
modificación y cambio de inode. **ctime no se llama fecha de creación**. El tamaño
de un directorio no es su tamaño recursivo. Fechas se presentan como segundos y
nanosegundos Unix en este primer diálogo. La ruta solicitada es histórica; la
ruta verificada se resuelve en el worker y puede diferir tras renombrar el padre.
Los enlaces muestran su destino raw escapado sin seguirlo ni abrirlo.

El formulario permite preparar bits r/w/x del propietario, grupo y otros;
Aplicar permisos confirma explícitamente el cambio no recursivo. Sólo Linux con
`fchmodat2(AT_EMPTY_PATH)` disponible, archivos/directorios propios, sin bits
especiales existentes. Otros objetos, symlinks, propietarios distintos y
setuid/setgid/sticky quedan de sólo lectura. No chown, escalada, ACL, xattrs ni
recursión. Cambiar un inode afecta a todos sus hardlinks; el diálogo lo advierte.
Esta unidad no ofrece Deshacer de permisos: un cambio confirmado sustituye el
historial previo para no presentar un Undo de otro tipo como si revirtiera chmod.
Leer propiedades o fallar antes del cambio conserva el historial anterior.

## Identidad, cancelación y límites

Captura y cambio se ejecutan en el worker de mutaciones existente: sin otro hilo,
helper o daemon. Un snapshot conserva dos FD (padre y objeto `O_PATH|O_NOFOLLOW`).
El symlink final se fija como objeto y se lee con `readlinkat` sobre ese FD;
lectura raw y rutas están limitadas a 4096 B. El cambio usa el FD fijado con
`fchmodat2`, sin reabrir por ruta ni fallback que siga symlinks. Si el kernel/FS no
lo soporta, se informa capacidad no disponible. La ruta `/` y propiedades de
selección múltiple no están implementadas en este bloque.

Se comprueban identidad del objeto/entrada, UID, GID y modo observados justo antes
de confirmar. Una sustitución o cambio externo exige Recargar, conserva al actor
y no aplica el modo preparado. Un cambio propio con observación posterior válida
actualiza el snapshot para una siguiente edición. No existe syscall condicional
que vincule identidad de entrada y chmod: un actor del mismo UID puede mover el
inode después de la comprobación. El FD nunca cambia a su reemplazo. No se declara
snapshot del contenido, aislamiento hostil ni atomicidad frente a crash.

La cancelación se comprueba antes del syscall; no lo interrumpe. Un commit
confirmado sigue siendo éxito aunque falle el diagnóstico posterior: se informa
advertencia, se deshabilita una nueva edición sin lectura válida y no se ofrece
un reintento como si no hubiera ocurrido. Los mensajes tienen máximo 1024 B y
ocho advertencias, y no se acumulan snapshots/historial de propiedades. Cerrar
durante un cambio cancela y espera ACK antes de salir; Drop no sustituye el drain.
Lecturas o syscalls atascadas en montajes siguen sin deadline efectivo y no se
presentan como recuperadas offline.

Los controles son Painter propio, con foco de teclado y scroll del panel; no se
abren diálogos nativos de toolkit. Las rutas solicitada/verificada y el destino raw de enlaces se distribuyen en
líneas medidas y se recorren con rueda, flechas, Inicio/Fin y PageUp/PageDown.
Se conserva el texto escapado completo; otros metadatos aún pueden recortarse.
Selección/copia/export completo del informe de propiedades queda pendiente. Accesibilidad AT-SPI y entrada física siguen sin evidencia compatible.

## Verificación

341 tests Rust (115 core/37 helper/189 UI), Clippy workspace/all-targets
`-D warnings` y release PASS. `properties-ui-1793049068223979012.json` funcional
PASS en dos procesos: lectura real, chmod0600→0640 con bytes intactos, cambio
externo a0600 rechazado conservando la petición0644, Recargar, symlink readonly
sin modificar su destino, directorio y cierre normal0 tras ACK. El segundo cierre
se solicitó inmediatamente después de encolar Aplicar y canceló antes del commit;
no prueba cancelación dentro del syscall. Método: handlers de targets visibles,
framebuffer FLTK real y modos/bytes de fixtures propios; no eventos SO.

SHA256 app `3b40ad35bc1aaad3bbc86764d7b00aa456d8c8edf0e686e760729320ebba6554`.
Las escenas fallidas1793048241761101633 y1793048903461152694 se preservan:
ventana tiled corta dejó un control recortado sin hit (correcto); después se
identificó un marcador READY anterior al repintado. La fixture ahora usa el mismo
Tab/reveal que el formulario y espera una revisión de dibujo posterior al comando.
También se oculta el toast de éxito anterior mientras hay formulario y se limpia
antes de un nuevo cambio, para no contradecir el rechazo actual. Logs de fallos de
compilación/Clippy iniciales y finales se conservan en evidence/properties.

Sin chown ni cambios sobre datos personales. Recursos/startup NOT_MEASURED por
freeze explícito; cotas de FD/bytes no prueban presupuesto del proceso. U04 sigue
parcial (selección múltiple, copiar informe, ACL/xattrs/enlaces, otros SO).


## NODE: rutas completas y ventana mínima (2026-11-01)

El panel reserva el pie global. Las nueve casillas y las tres acciones se ajustan
al ancho disponible en 440×440. Tab revela el permiso enfocado; Inicio/Fin y las
teclas de página recorren el contenido. Las rutas usan etiquetas escapadas comunes
sin reconstruir destinos desde el texto. La partición usa anchos reales y límites
de escalares UTF-8, con caché por ancho/ruta/destino. No promete segmentación de
grafemas, entrada nativa, AT-SPI ni nitidez 2×.

Los snapshots y la autorización por descriptor no cambian. Renombrar el objeto
invalida correctamente la captura; mover su directorio padre permite resolver la
ruta verificada nueva. La primera prueba confundía ambos casos y falló; el fallo
se conserva y se corrigió la fixture, sin relajar la validación del backend.


Validación NODE: 1214 pruebas Rust (425 core/95 helper/694 UI), tres ignoradas
históricas; Clippy workspace/all-targets -D warnings, formato y release PASS.
Flujos X11 `1793529310535401451` y Wayland `1793529319603368419`: 31 capturas
cada uno, tres salidas normales y siete helpers registrados terminados/reutilizados.
La fixture larga contiene bytes inválidos y controles: texto completo en caché,
PageDown/Home, nueve targets, tres botones, chmod0600→0640 sobre el mismo inode,
symlink readonly y contenido conservado. Regresión de propiedades
`1793529331818288088` PASS: lectura, rechazo tras cambio externo, Recargar,
directorio y cierre durante petición. El recibo intermedio IN_PROGRESS se conserva.
App SHA256 `ced5d60bdb1386d3e6927d7a1f00588f036a648e6bffaa858e2f4f7d4e084dc7`.
Son fixtures/handlers y framebuffer, sin prueba de input SO, clipboard ni AT-SPI;
PSS/Swap/CPU/startup no medidos. La anomalía X11 histórica sigue sin causa confirmada.


Checkpoint instalado fuente `d2dfee1`, recibo `1793529424770438069`: cinco hashes
instalados y cinco respaldos verificados. Repetición Wayland instalada
`1793529436364969476` PASS:31 capturas/3 salidas0/6 helpers de duplicados y1 de
búsqueda terminados o identidad reutilizada, sin zombie. Permisos de fixture y
bytes originales verificados. Helpers SHA/HTML conservan sus hashes anteriores.
