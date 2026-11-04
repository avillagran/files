# Pestañas — contrato V10, M22

Estado: M22 aceptado funcionalmente. La suite final
`tabs-checkpoint-test.log` registra 513 tests PASS (141 core, 67 helper, 305 UI);
Clippy workspace/all-targets con `-D warnings` PASS en `tabs-empty-clippy.log`.
La escena final `tabs-ui-1793109776417107918.json` pasó con nueve capturas. No modifica
el diseño aprobado ni abre otro motor de previews.

La siguiente unidad visible es abrir/cambiar/cerrar/reabrir pestañas con Nerd
Fonts, Ctrl+T, Ctrl+Tab, Ctrl+Shift+Tab y Ctrl+W. Cerrar la última pestaña solicita
el cierre normal de ventana y conserva el drenaje/ACK existente. No sustituye la
ruta de cierre Super+W del compositor, que sigue pendiente de input SO verificable.

## Estado y presupuesto

Máximo ocho pestañas y ocho cerradas recuperables. Cada pestaña tiene identidad
monotónica, ruta lossless, historial atrás/adelante acotado, vista, sort/descending/grid_zoom/pan, filtro, scroll
y anchor/foco/selección por nombres raw sujetos a preflight agregado. El snapshot corresponde al Model implementado; no se retienen ocho listados,
bitmaps, layouts o workers por conveniencia. Sólo la pestaña activa posee listado,
hits y helpers. Al cambiar se invalidan generaciones y targets antes de draw,
se guarda snapshot espacial por ACK y se cancela/drena preview/du/treemap.

Operaciones ya confirmadas siguen en la cola global con sus rutas capturadas;
no se redirigen al destino activo ni se eliminan al cerrar una pestaña. El cierre
de diálogo activo debe resolver/cancelar explícitamente su estado. Hits con id
obsoleto de pestaña no actúan. Mostrar error al superar límites, sin truncamiento
oculto ni clonado previo al presupuesto. No son límites PSS del proceso.

La restauración durable se integra en M23: ver SESSION-CONTRACT para argv,
ownership interproceso, límites, invalid/futuro y ACK de cierre. No se marca V10
completo mientras falte validación de recursos autorizada y otros SO/input nativo.

## Aceptación observable

1. Abrir A y B, navegar dentro de B, volver a A y verificar ruta/vista/filtro y
   selección raw; dos cambios antes del primer lote no publican listado obsoleto.
2. Ctrl+Tab/reversa y botones comparten Actions; cerrar la activa elige vecina,
   reabrir cerrada recupera ubicación, límite ocho muestra error recuperable.
3. Cambiar con helper activo cancela y recoge antes del siguiente; sólo último
   destino pendiente. Cambiar durante copia conserva job/origen/destino reales.
4. Carpeta vacía/error/ocultos no hereda hits ni selección invisible. Resize
   conserva controles visibles; rutas largas se recortan sin perder identidad.
5. Capturas de UI real y filesystem propio, lifecycle callback+ACK sin SIGTERM
   como prueba; handlers/framebuffer no equivalen a input SO. Instalar sólo tras
   suite/Clippy/release/escena PASS, con backup y hashes coherentes.

Freeze recursos/startup vigente: NOT_MEASURED, sin nueva comparación baseline ni
promesa de cumplir50MB. SFTP como montaje SO, shaders integrados, movimiento/
journal, doble panel/Miller e i18n siguen pendientes de ROADMAP y no se abandonan.


## Wiring M22

`tabs.rs`/`tabs_ui.rs`: sólo snapshots, ocho activas y ocho cerradas como máximos,
IDs monotónicos (reabrir recibe ID nuevo); no pending por índice. Cambio síncrono
en UI, el listado sigue asíncrono con generación del worker existente. No hay
nuevo hilo/listado/bitmap por pestaña. Activa/inactiva globalshow_hidden no se copia.
Los listados de inactivas se descartan: volver siempre relista, por lo que copias
terminadas en otra carpeta se observan al volver sin refrescar A erróneamente.
`copy_locations` retiene como máximo un parent lossless de4096B por job de UI
(hasta9jobs ya acotados), liberado al ACK/cancel. La cola mantiene rutas originales.

Preflight de snapshot antes de clonar nombres: selección<=256KiB de cargo
conservador (payload+contenedores+64KiB fijos), nombre4096B, snapshot<=1MiB
incluyendo History/query/path. `History::retained_bytes` cuenta payloads/deques y
holgura por ruta; History sigue limitado64 ubicaciones. Suma de snapshots
inactivos+cerrados<=3MiB antes de admisión; reserva adicional1MiB para la activa/
transición y128KiB para filas/rutas/containers. Exceso conserva estado y error
visible: puede impedir abrir/cambiar aun antes de ocho pestañas. No limita heap
completo, listado vivo, Snapshot de refresh ya existente (cap8MiB), thumbnails ni
helpers; no acredita PSS. No clona History: mueve ownership.

Snapshots pendientes se mueven al cambiar antes deDone; sólo el Done de generación
actual restaura nombres. Selección/foco/anchor se intersectan con hidden+query
vigentes sin búsqueda cuadrática. Se conserva scroll durante lotes incompletos;
clamp sólo con geometría final: listado cargado, layout espacial listo o treemap
resuelto. Una restauración podada a vacío permanece vacía frente a Stats/rebuild;
seleccionar o editar el filtro explícitamente permite recuperar foco.
Aplicar/undo/redo ocupado, diálogos o edición de
ruta bloquean cambio de pestaña, manteniendo la invariancia de refresh de mutations.
Operación confirmada de copia sí puede seguir mientras cambia la pestaña.

Barra de32px bajo header, labels recortados con close24px e IDs estables; flechas
Nerd recorren pestañas y se revela activa en strip acotado. Headergrab no cubre
la barra. Inspector compacto/clipped antes de acciones: a500/440px se omite
Calcular tamaño si no cabe íntegro, se conserva a650px. No target invisible.
Scripts/tabs-ui-check.py preparado: UI handlers/framebuffer, A/B mismo nombre
contenido diferente, snapshots/rapid/hidden/raw, límite/reopen, helperB y close,
resize archivo conmetadata ycarpeta a800×500/440. La ejecución final pasó: nueve capturas a 1000×650 y 800×500/440, cierre normal
exit0 posterior a ACK y un helper identificado PID/start desaparecido, sin zombies.
Los cambios fuera del strip usan Actions; los targets visibles usan handlers de
puntero. No equivale a input SO ni prueba de helper vivo exactamente al cierre.


## Fallos conservados y alcance

Se conservan los tres informes visuales originales: `tabs-ui-1793108574031992206.json`
(socket Hyprland incorrecto del harness), `tabs-ui-1793108595977296407.json`
(scroll 120 reducido a 0 por paint antes de completar el listado) y
`tabs-ui-1793108996440627071.json` (selección vacía tras ocultar restaurada
implícitamente a otra carpeta por rebuild). No se reducen los asserts para
aceptarlos; las dos regresiones de estado están cubiertas en la suite.

Undo conserva la ruta capturada de su historial aunque otra pestaña haya fallado
al crear. Un resultado conocido sólo refresca su carpeta; desconexión/id
desconocido conserva refresh conservador de la actual. La ruta capturada no
acredita resolver el directorio efectivo del FD si un tercero renombra el padre.

V10 sigue parcial por gates de recursos y evidencia fuera de Linux/input nativo.
M23 añade persistencia durable versionada de metadata de pestañas, no de tareas. Ninguna validación funcional sustituye los FAIL
previos de memoria/arranque ni resuelve clipboard o input nativo.


Validación final: `tabs-checkpoint-test.log` 513 PASS,
`tabs-empty-clippy.log` workspace/all-targets `-D warnings` PASS,
`tabs-checkpoint-release.log` y `tabs-checkpoint-fmt.log` PASS.
La selección restaurada mantiene scroll120, zoom125%, filtro/orden/anchor y lote;
A/B contienen nombres iguales con bytes distintos. También se comprobaron nombres
raw no UTF8, ocultos globales, ocho pestañas/reapertura con ID nuevo y metadatos
listos al resize. El recorder exige Calcular tamaño presente a650px para carpeta
y omitido a500/440px; las capturas pequeñas confirman la omisión sin solapar acciones.

Recursos/startup siguen NOT_MEASURED por freeze. El PSS incidental del pie excluye
Swap/hijos y no es evidencia de presupuesto. La geometría física declarada por
compositor no demuestra nitidez nativa a2x de estas capturas de framebuffer lógico.

M23: SESSION-CONTRACT, 527Rust y tabs-session-ui-1793111266679001110 PASS6FB/8procesos;
no repite ni reemplaza los tres FAIL visuales históricos de M22.


M26: Columnas sustituye la UI Espacial. El tab conserva la ruta de columna
enfocada y selección/filtro/scroll; no toda la cadena Miller en esta unidad.
Ownership actualizado: un Model/listing activo más un worker reutilizable
lookahead en Columnas; cachés hasta8 listados sin workers. Cola/medios/AuxIo
únicos. La anterior declaración de exactamente un pipeline sólo aplica fuera
de Columnas. Límites y evidencia en COLUMNS-CONTRACT; sin PSSclaim/freeze intacto.


M27 doble panel: un Model activo y un Browser movible con listado propio. Los Tabs
de cada grupo usan IDs locales, siempre calificados por panelID estable; la cola
global conserva (panelID,tabID) y rutas capturadas. Snapshot3MiB conjunto de ambos
grupos, incluido el oculto. F9off libera entries/order/stat-demand, conserva sólo
metadata acotada; reabrir recarga. CtrlW última tab dual cierra ese panel y mantiene
cola, mientras la última tab única conserva el cierre ACK. El worker secundario
es la misma propiedad transferida desde/hacia Columns, no hilos nuevos al alternar.
Ver DUAL-PANE-CONTRACT. Sesión durable sigue sólo grupo enfocado.

## Background result locations

SEARCH-BACKGROUND-TABS adds metadata-only Saved snapshots without changing active
tab/worker/generation/search. It uses the existing joint inactive+closed+other-pane
budget, eight-tab limit, monotonic IDs and restore/selection pruning; empty-history
reserve is charged. Result-directory vs raw parent/leaf is captured before later
activation; no stat/listing/preload is added to admission. Existing session wire
format is unchanged. Native input/resource acceptance remain separate pending gates.
