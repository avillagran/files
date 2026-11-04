# Doble panel — M27 (2026-10-27)

Unidad siguiente a Columnas, de acuerdo con DESIGN187: F9 sustituye el inspector
por segundo panel en modo MC, no dos inspectores ni dos Model. Previews nuevas
pausadas; freeze de recursos/startup intacto. Aceptación funcional acotada M27; instalación por checkpoint. Sin PASS global.

## Uso, foco y operaciones

F9 alterna una/dos listas. En este checkpoint MC ambos paneles usan Lista; elegir
otra vista sale del modo dual. El panel activo tiene borde/color y título claros;
Tab alterna foco únicamente fuera de omnibar/inputs/modales. Clic en una fila del
otro panel lo enfoca y aplica selección por identidad/token, nunca al índice de
una lista distinta. Enter/doble clic navegan su carpeta; historial, filtro,
selección/rango/anchor y scroll son independientes. Scroll de panel inactivo no
cambia foco. Lugares/omnibar/atajos/inspector normal actúan sobre el activo.

F5/F6 prellenan el directorio del panel opuesto y mantienen revisión explícita.
El diálogo conserva PathBuf raw del destino capturado separado del label de
visualización; hasta editar se usa EXACTAMENTE ese PathBuf. La primera edición
válida sustituye el destino por una ruta UTF8 escrita explícitamente; paste
rechazado no descarta el raw anterior. Ningún unwrap_or_default/to_string_lossy
se convierte en ruta operativa. Nombre0xFF→destino0xFE tiene fixture.

La operación captura fuente/destino y owner(panelID,tabID) al admitir, no consulta
foco nuevo al ejecutar/ACK. Cola, IDs de job, cancel/pause, journal y supervisores
de medios/operaciones son globales únicos. Completion refresca ambos directorios
que correspondan (rutas capturadas más ubicaciones FD verificadas) y preserva
selección/query/scroll. Cambiar o cerrar pestaña/panel no redirige ni cancela cola.
Apply rename/permisos y revisión modal mantienen bloqueo de cambio de foco.

## Pestañas, cierre y persistencia

Cada panel tiene Tabs propio, hasta8 activas/8cerradas. TabIDs locales sólo son
válidos junto a panelID monotónico estable; no se reutiliza ID de panel cerrado.
Owner de job y diagnóstico usan el par, nunca u64 tabID solo. Jobs conservan
rutas/proofs aunque su owner ya no exista. Los ACK no cambian foco de otra pestaña.
El límite3MiB de snapshots inactivas+cerradas es CONJUNTO de ambos grupos, no
3MiB por panel; admisión resta sólo lo liberado por el grupo modificado.

F9 al salir mantiene la ruta enfocada como lista única y suspende listado del
otro panel, liberando entries/order/stat-demand después de capturar selección
raw acotada; conserva Tabs/history/query/scroll y esa selección dentro del
presupuesto conjunto. Al volver se revalida su ruta. Cerrar última pestaña de un panel dual
mantiene el otro como panel único, sin cerrar ventana ni cancelar cola. Se elimina
el grupo cerrado (no reapertura implícita por ACK); CtrlShiftT opera sobre cerradas
del grupo restante. Cierre de ventana sí cancela/drena cola y guardados como M25.
Sesión durable M23 sigue guardando sólo el grupo ENFOCADO; aviso explícito en modo
dual. No prometer restore de ambas mitades ni reanudar operaciones. Ampliar formato
para ambas es pendiente explícito, no reinterpretar sesiones existentes.

## Ownership y presupuestos locales

Un Model activo; Browser secundario sólo datos/listado/worker. Al cambiar foco se
mueven Vec/HashSet/worker/history/Tabs y snapshots pendientes, no clones de listas.
Cada worker reutilizable conserva latest-pending/generation y límiteFSglobal4;
no reemplazar hilos ante syscall bloqueada. Columns lookahead anterior queda
suspendido al entrar modoMC: se reutiliza el mismo worker secundario de Columns al cambiar modo;2 listados
solicitados, sin crear otro hilo al alternar. Una syscall antigua bloqueada no
se considera interrumpida ni drenada; el límite FSglobal4 sigue aplicando, sin bitmaps/preview
secundarios. Desactivar/cerrar cancela pendiente, sin fingir kernelD interrumpido.

Admisión de datos de AMBAS listas antes de cada lote y expansión de selección:
32MiB conjuntos; máximo100k entradas/8MiB nombres por lista. Parcial/error explícito
si supera, nunca decir carpeta vacía. Reservas separadas:3MiB snapshots conjunto,
≤1MiB snapshot transiente y256KiB metadata/rutas de grupos, canales8×256 y jobs
bounded por worker, claves de sort, stacks/allocator/FLTK/medios/cola global según
sus contratos. El Model activo existente y el secundario se contabilizan juntos;
no atribuir memoria total a chunk/cap ni declarar PSS con límites teóricos. Bajo
freeze todo presupuesto real sigue NOT_MEASURED y los FAIL previos siguen activos.

## Aceptación

Extender escena de pestañas existente: F9/Tab por handlers compartidos, A/B mismo
nombre distinto contenido, navegación y filtro independiente, multiselección,
operaciones F5/F6 al opuesto con revisión; destino/nombre raw0xFE/0xFF; job deB
termina estandoA enfocada, refresco del correcto sin pisar selección/filtroA;
pestaña/panel cerrado durante cola pausada no cancela/redirect; cerrar ventana
normal0 después ACK. Límites conjunto y IDs en unitarios, capturas reales en
1000×650 y ventana inicial estrecha. No inputSO/clipboard/liveResize ni métricas.


## Validación M27

603Rust194core/67helper/342UI PASS (tres core ignored preexistentes), Clippy
workspace/all-targets-Dwarnings, release y fmt limpios.
`dual-ui-1793126826929914048.json` PASS: ocho framebuffer IHDR verificados,
seis1000×650 y dos de instancia inicial800×440; dos procesos normales0 después
ACK. A/B navegan carpetas/volver por separado; contextual sobre lote del otro
panel conserva selección, fondo y targets obsoletos no la redirigen. F5 copia
bytes raw0xFF al destino0xFE, origen y ocupantes intactos, mientras B mantiene
foco/query same y A muestra filtro copy. F6 revisión y pausa no publican; cerrar
B con job pausado conserva cola y al reanudar mueve al destino capturado. Ambos
paneles misma ruta se refrescan tras rename/Undo en tests; grupo oculto sin datos
y joint metadata budget probados. Doce alternancias con backend bloqueado usan
el mismo worker secundario (test, no garantía de interrumpir syscall del SO).

Títulos muestran basename primero y contexto padre, filtros visibles en cada
cabecera además del pie. Así A/B se distinguen estrechos y la notificación de
éxito no tapa el único indicador de filtro. Instancia inicial estrecha, no live
resize; handlers/Actions/framebuffer, no eventos SO. No preview nuevo ni lectura
de clipboard. PSS incidental del footer sólo principal, no gate.

Se conservan primeros FAIL compile/Clippy y fixture de contextual que clicaba
carpeta no seleccionada. Fixture corregido verifica por nombre raw un miembro
del lote y caso separado fuera del lote→selección1, sin cambiar esa semántica.
Primera escena PASS funcional con títulos/indicadores antiguos conservada con
review sidecar. `dual-checkpoint-validation.json` conserva logs/hashes.
Persistir ambos grupos, otros SO/input nativo, vistas distintas por panel y
recursos/startup siguen pendientes. Siguiente: cortar/pegar/DnD y operaciones,
sin reabrir previews ni levantar freeze.


Corrección de reporte M27: `dual-title-fmt.log` era FAIL de una única línea por
wrapping, aunque se había descrito PASS. Original preservado. Formato aplicado
sin cambio funcional y `dual-format-correction.log` registra fmt--check exit0.
Clippy/release/escena/hashes instalados previos siguen válidos; no se reconstruye
ni instala M28 WIP por esta corrección sólo de whitespace.

## Sort controls (2026-11-01)

Both lists expose Name/Size/Modified buttons and the selected direction. Clicking
the current criterion reverses it; selecting another defaults Name to ascending,
Size/Modified to descending. Compact panels show Nerd icons with names/tooltips.
The same header is used by a normal single list.

Inactive pane sorting uses its ID+listing revision; stale or refreshing targets
are rejected. It preserves active owner, generation, selections, and shared DU
observations instead of switching panels. Metadata requests still use the existing
worker. No filesystem calls in the header/sort handler, no new worker or cache.
Keyboard: Ctrl+Shift+P, search ordenar nombre/tamaño/fecha, Enter sorts the active
panel; repeating reverses. Unknown folder sizes remain at the end both ways.
