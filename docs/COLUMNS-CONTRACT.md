# Columnas tipo Finder — M26 (2026-10-27)

Decisión explícita del usuario: Columnas/Miller sustituye la vista Espacial. Espacio
(treemap) permanece. Doble panel se aplaza al siguiente bloque visible, sin WIP
funcional descartado. Previews nuevas pausadas y freeze de métricas intacto.

## Interacción y origen

Cada columna representa un directorio. Seleccionar una carpeta mantiene el foco y
la selección EN SU COLUMNA y solicita el listado siguiente; seleccionar un archivo
conserva los ancestros y poda descendientes. Cambiar carpeta en un ancestro poda
la rama posterior. Clic/Izquierda/Derecha cambian columna enfocada; Arriba/Abajo
mueven selección en ella. Derecha entra sólo en la carpeta seleccionada cuando su
listado está disponible; no abre un archivo por accidente. Enter delega archivos
al `open` existente. Scroll vertical independiente, horizontal con reveal del
foco y controles Nerd. Filtro/selección/scroll de cada columna se conservan dentro
de la cadena; ocultos siguen globales y se podan antes de operar.

Copiar/mover/renombrar/papelera/propiedades siempre capturan la ruta y nombres de
la columna enfocada. Un listado de la derecha no cambia ese origen. Cola, IDs,
ACK, journal y supervisores de medios/operaciones siguen siendo únicos. No se
cambia de foco durante Apply/revisión modal. No I/O de archivos en draw/event.

## Ownership y límites de esta unidad

Un Model activo conserva su listado y worker reutilizable; un segundo worker
reutilizable sirve sólo al siguiente listado solicitado. Ambos comparten el
límite global de cuatro consultas FS. Cachés de ancestros NO tienen workers ni
helpers; no se instancian ocho Model. Respuestas de la rama anterior se descartan
por generación interna del worker y token estable de columna, nunca por índice
reutilizado. Hasta ocho listados retenidos; una ventana deslizante elimina el ancestro
más antiguo al profundizar, sin limitar profundidad navegable. Subir/breadcrumb
reconstruye ancestros de forma asíncrona y avisa al descartar estado por presupuesto. Filas virtualizadas: hits y texto sólo para las visibles.

Presupuesto de datos de la cadena:16MiB retenidos (capacidades de entries/nombres,
order/selección y structs), máximo100k entradas/8MiB nombres por listado. Se
valida antes de retener lotes/capturas, con reserva conservadora de selección
y crecimiento exacto de entries/order. Controlador/rutas/menú diferido reservan
64KiB adicionales; buffers acotados de ambos canales (8lotes×256 entradas),
selección transiente≤256KiB y claves de sorting son reservas separadas. No es un límite del proceso ni acredita PSS. Fuera de
Columnas se libera la cadena y se suspende el worker adicional, conservando un
único hilo reutilizable ante syscalls atascadas. Cierre lo cancela sin fingir
interrupción de una syscall de kernel. Recursos/startup NOT_MEASURED por freeze.

## Pestañas, sesiones y migración

Una pestaña contiene su ruta de columna enfocada, historial, filtro y selección
con las mismas reglas acotadas de M22/M23. Cambiar pestaña reconstruye la cadena
desde esa ruta; esta primera unidad no persiste todas las columnas abiertas.
No hay una segunda sesión/cola. Tag3 de sesión legado Spatial se interpreta como
Columns; conserva ruta/historial/selección pero descarta zoom/pan y scroll de
iconos como geometría incompatible. El encoder nuevo usa tag5 libre para Columnas y preserva scroll vertical
nuevo; tag3 queda exclusivamente como migración de Spatial. `--spatial`
queda alias compatible de `--columns`, con nombre visible Columnas; los layouts
antiguos de iconos se conservan en disco sin leerlos ni borrarlos por inferencia.

## Aceptación prevista

Escena existente extendida: cadena raíz/A/nested, misma carpeta hermana B,
archivo con nombre igual/contenido distinto, selección y payload operaciones en
el padre aunque hijo esté visible; poda y respuesta stale tras A→B, teclado,
scroll/reveal/resize y retorno ancestro, hidden global/rawUnix. Sesión tag3
legada restaura datos sin reinterpretar geometría. Capturas reales y handlers
internos declarados, sin afirmar input SO ni nuevas métricas. Tests/build/Clippy
más instalación con backups sólo al validar. V06 en implementación; V08
sustituida por decisión, datos antiguos preservados, ninguna marcada completa.


Estado de validación previo a captura:596Rust194core/67helper/335UI PASS en
columns-workspace-final-test.log; Clippyworkspace/alltargets-Dwarnings limpio.
La suite anterior expuso WouldBlock al reabrir recovery: regresión nueva con
FDduplicado reprodujo RED; Journal ahora suelta flock explícito al destruir owner
sólo en PID creador, sin que Drop en un proceso hijo pueda soltarlo al padre.
Regresión GREEN y suite completa PASS; no atribución exacta a un fork específico
del primer fallo. Se conserva FAIL de aserción de fixture de presupuesto y FAIL
intermitente sin contexto suficiente de entrada A ausente; fixture posterior
usa creación exclusiva+ID atómico y diagnóstico completo, no se relajó selección.
Escena y release aún pendientes al escribir esta entrada; no featurePASS global.


## M26 — aceptación funcional acotada

columns-ui-1793124416464738573.json PASS: seis framebuffer reales con dimensiones
comprobadas (cinco1000×650, uno800×440 inicial), cadena de tres columnas, foco
conservado en padre mientras hijo visible, poda A→B, payload de renombrado y
copia real de bytes desde A/nested (B contiene nombre igual), filtro/nombre raw,
fuentes intactas y dos cierres normales0 posteriores a ACK. Capturas inspeccionadas: 
recorte del ancestro por scroll horizontal esperado, sin afirmar liveResize.
Profundidad12, modificadores, foco/cancel antesDone/Stats stale, menú diferido por
rawnombre+generación y migración de sesión tag3/5 tienen regresiones unitarias;
no se presentan como eventos SO o sesiones completas de la cadena persistidas.
596Rust+Clippy/release/fmt limpios; columns-checkpoint-validation.json conserva
logs y fallos anteriores sin atribuir causa exacta al fixture intermitente.

Límite16MiB es admisión de datos de cadena en lotes/capturas, con capacidades y
reserva de selección; metadatos de demanda del Model, colas de jobs/channels,
allocator/stack/FLTK y medios globales existentes se agregan. No usar este cap
local como prueba del proceso; memoria/CPU/arranque NOT_MEASURED por freeze.
Windows/macOS, inputSO y persistencia de toda la cadena no verificados.
Siguiente: doble panel según DESIGN, seguido de cortar/pegar/DnD/operaciones/SFTP.
