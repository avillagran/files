# Renombrar y crear (0.0.3)

F2 o Renombrar del inspector abre un formulario propio con vista antes/después.
Selecciona varios elementos con Ctrl/Shift; el lote incluye también seleccionados
ocultos por el filtro y se ordena naturalmente. Antes de clonar nombres, la UI
rechaza más de100000 seleccionados, nombres>4096B o suma>8MiB. No trunca el lote.
Crear archivo/carpeta no copia la selección. Ajustes expone crear, deshacer y
rehacer; atajos Ctrl+N, F7/Ctrl+Shift+N, Ctrl+Z/Ctrl+Shift+Z.

Patrones: `{name}`, `{ext}`, `{dotext}`, `{n}`, `{n:03}`, llaves escapadas; inicio y
paso, reemplazo literal y caso conservar/minúsculas/mayúsculas. Prefijos/sufijos y
extensión se expresan en el patrón. **Regex, fechas y edición avanzada siguen
pendientes**. No hay normalización/casefold exacto del filesystem. Nombres noUTF8
seleccionados se rechazan sin pérdida; todos los DirEntry cuentan hacia el límite,
incluidos los noUTF8 no seleccionados. El plan se calcula en un trabajador único,
con cancelación entre entradas y fases; nunca se aplica automáticamente.

Aplicar revalida identidades y usa staging exclusivo + renameat2 NOREPLACE en
Linux. Ciclos, case-only y directorios no vacíos tienen pruebas del motor. Rollback
se ejecuta sin cancelarse entre sus pasos: Cancelar solicita detener la operación,
no matar el hilo. El cierre/exit-after solicita cancelación y espera ACK final antes
de salir, incluso si se descartan preferencias espaciales. Errores, rutas verificadas
de rescate, antiguas no verificadas y contadores omitidos se muestran en informe
copiable. El informe no convierte rutas anteriores en ubicaciones actuales seguras.

Se conserva un token de historial; deshacer/rehacer revalida y puede cancelarse.
Un Crear fallido conserva sus indicadores. Un Crear exitoso invalida ese historial;
**deshacer creación sigue pendiente**. Crear usa validación portable compartida y
límite255B, mkdirat/openat O_EXCL anclados al directorio; no sobrescribe.

No hay journal durable ni recuperación frente a kill/power loss: cierre normal
esperando rollback no equivale a atomicidad ante crash. Tampoco se migra aún la
identidad espacial path+dev+ino al renombrar/mover una carpeta; su disposición puede
no recuperarse con el nuevo nombre. Windows/macOS y syscall remoto bloqueado no
están validados. El árbol de UI+planner+plan no tiene todavía presupuesto integrado
para100k/8MiB; benchmark del planner aislado no lo demuestra.

`python3 scripts/rename-check.py --capture` recorre lote2 desde handlers internos,
comprueba contenido/nombres antes de aplicar, después de aplicar/deshacer/rehacer,
crea archivo/carpeta y captura framebuffer. `python3 scripts/rename-check.py` hace
la misma secuencia con sampler de árbol y sin capturas. Incluye cierre con Apply
pendiente mediante estado normal de cierre, salida0 y ausencia de staging. Puede
cancelar antes de mover o competir con finalización; no prueba por sí solo cierre
en cada instrucción del motor. Los tests del núcleo inyectan fallos en cada paso
de staging/rollback; las pruebas UI comprueban bloqueo hasta ACK, no power loss.

Ejecutar sólo tras build, con Cargo/capturas/benchmarks mutuamente excluyentes.
Evidencia funcional/recursos de lote2 en VALIDATION; no acredita lote100k ni PASS global.
