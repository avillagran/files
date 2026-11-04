# Sesión durable — contrato M23

Estado: M23 aceptado funcionalmente e integrado; sesión de pestañas versionada.
527 Rust (141 core/67 helper/319 UI) y Clippy workspace/all-targets -D warnings
PASS, release/fmt PASS. Escena `tabs-session-ui-1793111266679001110.json` PASS
con ocho procesos de app y seis capturas reales; instalación registrada aparte.
M22 instalado se conserva hasta aceptar este bloque. Freeze de recursos/startup
vigente: NOT_MEASURED. No reanuda operaciones ni medios automáticamente.

## Arranque y propiedad

- Sin ruta en argv: restaurar sesión compatible si existe; si no, Home. Una ruta
  explícita, incluso después de `--`, abre sólo esa ruta y no aplica ni autoguarda
  la sesión. Sólo la acción visible Guardar sesión habilita un intento de ownership
  en esa instancia, sin restaurar el archivo existente.
  `--no-session` desactiva lectura y escritura de sesión para esa instancia;
  escenas smoke quedan aisladas salvo activación explícita de su fixture.
- La primera instancia que adquiere lock interproceso no bloqueante del archivo
  `session.lock` es dueña de escritura **hasta salir**. El descriptor permanece
  abierto en AuxIo y el lock se mantiene durante validación, temp/fsync/rename y
  sync del directorio. No se usa CONFIG_UPDATE como exclusión interproceso.
- Otra ventana puede leer la última sesión completa pero no guardarla; muestra
  aviso persistente de sólo lectura de sesión. No reclama ownership automáticamente
  cuando desaparece la primera: evita que su snapshot antiguo reemplace uno nuevo.
  Sus preferencias globales conservan el alcance actual, sin nueva promesa CAS.
- El lock no se elimina al cerrar; se libera por FD. Verificar identidad del
  pathname del lock antes de guardar. Reemplazo externo o conflicto => bloquear
  guardado y error recuperable, nunca sobreescribir silenciosamente.

La respuesta inicial de AuxIo lleva token/generación de arranque. Si el usuario
navegó, cambió pestaña/editó destino o inició cierre, se puede conservar el
resultado de ownership/diagnóstico pero **no aplicar** el documento tardío ni
arrancar listados/helpers por él. No hay I/O síncrono en UI ni bloqueo de entrada.
Regresión determinista: respuesta tardía tras navegación y tras Closing no cambia
ruta/generación ni revive previews.

## Formato y límites

Archivo privado `session.bin`, esquema versionado y tag de plataforma. Rutas y
nombres raw, sin conversión con pérdida ni I/O en draw. Unix raw bytes; soporte
Windows/macOS se distingue de evidencia Linux. Máximo ocho activas y ocho cerradas,
misma selección/historial/vista/sort/descending/query/scroll/grid_zoom/pan y
anchor/foco que M22. Mostrar ocultos sigue GLOBAL, no se serializa por pestaña.
IDs nuevos al reconstruir; no restaurar hits, índices, workers, bitmaps ni helpers.

Máximo4MiB de archivo antes de parsear; longitudes/counts se validan contra bytes
restantes y límites de M22 antes de reservar/copiar. Preflight de tamaño de salida
antes de construir buffer. Sin truncamiento silencioso. Restauración primero
valida todo y presupuestos agregados, después publica el conjunto coherente;
selección raw se intersecta con hidden/query sólo tras Done de generación final.

Archivo inválido, truncado, plataforma incompatible o versión futura se preserva
byte a byte: error visible, sesión deshabilitada para escritura en esa instancia.
No se transforma automáticamente al guardar. Para recuperar se puede cerrar sin
guardar y renombrar/restaurar ese archivo explícitamente fuera de esta unidad.

## Guardado y cierre

Reusar AuxIo/canal acotado existente; una petición outstanding de sesión, ACK con
revisión exacta. Snapshot/buffer retenido hasta ACK, Full conserva pendiente,
desconexión/error nunca equivale a éxito. Guardado al cierre normal y acción
explícita de guardar sesión; no capturar/clonar estado en cada draw/gesto.
Cambiar de pestaña no hace I/O síncrono. Guardar no interrumpe cola global confirmada.

El cierre drena operaciones/previews/du/layout/config y sesión. Error permanece
visible con Reintentar, Seguir trabajando o Cerrar sin guardar pendientes. Si
rename ya publicó pero sync del directorio falla, el resultado distingue
**publicado, durabilidad no confirmada**; reintento bajo mismo lock valida el
estado efectivo y no supone baseline anterior. No se afirma atomicidad entre
config/layout/sesión ni recuperación de tareas tras kill/power loss.

## Aceptación de este bloque

Suite/Clippy/release/fmt serializados, luego dos procesos consecutivos con el
mismo perfil privado save→restore: historial/vista/filtro/selección raw y scroll;
argv explícito ignora restore; inválido/futuro queda intacto y error visible;
segunda ventana no pisa estado nuevo del dueño. Capturas de instancia restaurada
y error recuperable. Handler/framebuffer no equivale a eventos SO; sin benchmarks.
Sólo tras evidencia funcional instalar app/helper/HTML coherentes con backup/hashes.


## Implementación y reservas locales

`session_codec` hace una pasada prestada de validación de todo el documento antes
de copiar rutas/historial/nombres; `validate` se usa en Store sin materializar
sesiones descartables. Encode usa el mismo cargo conservador y cuenta todo el
wire antes del Vec de salida. Activa <=1MiB; inactivas+cerradas <=3MiB, selección
<=256KiB de cargo/100000 nombres y cada campo <=4096B; hasta64 rutas de History
por record. Se rechazan NaN/infinito/zoom fuera0.65..1.8/scroll o pan fuera rango.
Guardar/reintentar no confía en `Saved.charge` importado: recalcula por contenido.

Durante captura puede coexistir snapshot activo <=256KiB con wire <=4MiB. AuxIo
puede retener además lectura anterior <=4MiB (capacidad acotada, regular-only por
FD O_NOFOLLOW|O_NONBLOCK|CLOEXEC); validate no construye History/selecciones. Load
puede coexistir wire <=4MiB y estado decodificado <=4MiB de cargo más contenedores
pequeños acotados, hasta aplicar y soltar archivo. Son reservas adicionales al
listado y cachés existentes; no son un límite PSS del árbol ni sustituyen gates.
No buffer nuevo por tick ni múltiples buffers sesión en cola.

Fallo de captura/encode bloquea reintentos automáticos del cierre hasta Retry;
Full conserva buffer exacto y ACK obsoleto no lo libera. Se prueba sync de
directorio fallido después de rename: archivo nuevo ya visible, resultado
Unsynced y reintento verifica archivo efectivo bajo mismo lock.

Fixtures de arranque con FILES_TEST_SESSION usan smoke sólo observacional tras
startup resuelto y paint; no llaman smoke_test mutador. M22 conserva su smoke
estándar. La escena existente tabs-ui-check.py --sessions comprueba save→restore,
argv, --no-session, otro writer e inválido/futuro sin instrumentar recursos.
Windows raw/ownership siguen Unsupported; Unix codec no implica evidencia macOS.
El lock es advisory entre instancias cooperantes; no confina padres de config
ni detiene un editor externo que deliberadamente ignore/reemplace el lock.


## Evidencia M23, 2026-10-27

`session-final-test.log`: 527 Rust =141 core/67 helper/319 UI.
`session-observational-clippy.log`, `session-release.log` y
`session-final-fmt-check.log` PASS; copias en session-checkpoint-validation.json.
El primer FAIL de tests fue un receptor descartado por el fixture (Disconnected),
corregido reteniendo binding; ambos FAIL Clippy fueron estilo. Logs intactos.

`tabs-session-ui-1793111266679001110.json`: archivo propio566B, save→restore en
procesos consecutivos, A Grid125%/scroll120/pan48/orden descendente/querykeep/2
seleccionados/anchor/foco y B List/querysame/historialback independiente; pestaña
cerrada raw0xFF restaurada al reabrir. Ruta explícita abrió sólo B y no guardó al
cerrar; --no-session rechazó guardar. Otra ventana quedó readonly y no tomó
ownership al salir la dueña ni pisó sus bytes nuevos. Inválido y versión99
permanecieron byte a byte. Ocho salidas normales0 posteriores al drenaje/ACK.

Seis framebuffer lógicos1000×650, acciones/handlers internos sin input SO ni
clipboard. No operación/medio reanudado, ni promesa de recuperación de tareas
tras crash. Recursos/startup NOT_MEASURED; PSS del pie no acredita presupuesto.
V10 continúa parcial por evidencia multiplataforma/input SO y gates congelados;
las pestañas/sesión de instancia ya no se enumeran como sin implementar.


M26 compatibilidad de vista: tag3 Spatial legado se decodifica Columnas, con
scroll/zoom/pan reiniciados (geometría de iconos incompatible); ruta, historial,
filtro, selección/foco/anchor raw conservados. Nuevo tag5 Columnas conserva
scroll vertical. No cambia VERSION ni borra layouts antiguos; lectores antiguos
que desconocen tag5 rechazan/preservan archivo. No persiste toda la cadena.
Regresión encodelegacy→decode y tag5roundtrip PASS; sin nuevo gate de recursos.


M27 doble panel no cambia formato ni promete persistir ambas mitades: Guardar
sesión y cierre capturan sólo Tabs del grupo enfocado. Preferencias muestra
«Guardar sólo este grupo» en modo dual. Metadata del grupo secundario/oculto sigue
acotada conjuntamente con snapshots3MiB, pero no se serializa en M23. Operaciones
y medios nunca se reanudan por restore. Persistencia de ambos paneles pendiente.

## Descriptor ownership on normal release

OWNER-LOCK-RELEASE.md records the creator-PID explicit flock release added in
source ebfbd46. A child retaining an inherited descriptor no longer extends
normal owner lifetime; dropping a child copy cannot unlock the active creator.
Nonblocking acquisition, genuine busy/read-only/CAS behavior and formats remain
unchanged. Linux fork repros failed before and passed after;1290workspace tests
and corresponding source/installed UI checks passed, with scoped limitations.
