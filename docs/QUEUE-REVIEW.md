# Revisión mientras la cola está activa

Copiar, mover, crear ZIP y extraer pueden abrir su revisión mientras otra copia
está activa o pausada. MC, paleta, menú, inspector y Action usan `copy_review`
para distinguir esa revisión de la admisión de otro trabajo. F5 mantiene su
comportamiento contextual: copia en listados, recálculo en Espacio.

| Intención | Efecto |
|---|---|
| Abrir otro destino | Captura la selección y abre el formulario; no cambia trabajos activos ni pendientes. |
| Revisar un movimiento | Usa la preparación FD existente; un token pendiente o su ACK cancelado impiden abrir otra revisión. |
| Cancelar el formulario | Libera esa revisión; no cancela el activo ni elimina pendientes admitidos. |
| Confirmar con cola llena | El controlador rechaza la admisión y conserva formulario, nombres, opciones y motivo. |
| Vaciar pendientes | Elimina sólo los pendientes; el activo pausado conserva su identidad. |
| Reanudar | Continúa el activo y luego los trabajos que siguen admitidos. |

Se mantienen ocho pendientes, cargos de entrada/FD, IDs crecientes, la barrera de
resultado/ACK y las validaciones de cada motor. No se reservan trabajos ni se
abre FS durante la consulta de disponibilidad para pintar. Cerrar, carga/refresh
pendiente, mutación aplicada y una revisión anterior sin drenar siguen bloqueando.
Una preparación de renombrado de sólo lectura no se confunde con una mutación
ya aplicada ni con una copia de la cola independiente.

La selección se valida al ejecutar antes de sanear. Marcas inválidas, ocultas o
fuera del filtro no se convierten en otra captura por fallback al foco. Espacio
conserva su política de ignorar el filtro retenido y usar las marcas del listado
raíz; el informe de tamaños no es autoridad para operar. La prueba exacta de
selección de la paleta, owners, generaciones y tokens FD no cambian.

El progreso se sitúa sobre MC, respetando el inspector realmente visible. En un
cuerpo estrecho distribuye los controles en dos filas y separa estado, contadores
y ruta; no fuerza un ancho que salga de440px con la barra lateral expandida.
El botón del último resultado también respeta MC y el aviso compacto. El fixture
usa las dimensiones reales del canvas para navegación/menú, y distingue entradas
del panel activo o de la columna activa para evitar actuar sobre otro propietario.

Fuente9ddd253:1306 pruebas normales (447core/105helper/754UI), tres históricas
ignoradas; Clippy all targets con warnings como errores, formato y release PASS.
Cola Wayland1793560078115479975, XvfbX111793560081082575235 e instalado
Wayland1793560381173496871: cada uno35capturas/5cierres normales/drain, cinco
perfiles, contenido y fuentes retenidas exactos. Cola llena y formulario retenido
se prueban en controlador/modelo; no se hizo un experimento de capacidad UI de8jobs.
Espacio de regresión Wayland1793560075965694320 y XvfbX11 aislado
1793560198200409125: cada29capturas/4cierres normales/15helpers conocidos ausentes
o reutilizados. El primer wrapper Xvfb de Espacio terminó1 por limpieza
“kill: No such process” aunque su recibo funcional fue PASS; quedó conservado,
y el mismo binario/fixture solo terminó0. Causa exacta no demostrada. Evitar dos
wrappers Xvfb con asignación automática a la vez no serializa los tests Rust.

Instalación1793560331250393795, cinco hashes instalados y cinco respaldos
verificados; appa715c17069d6aa85475d39016a0a725b09847a53befc95eda1b52dc47a41c6e6.
Recibos en `docs/evidence/queue-review/assessment.json`.

La inspección reveló un pendiente separado: a440 con lateral expandida el nombre
del doble panel queda sin ancho (`dual_view.rs`: max(r.w-116,0)); el control a1000
sí lo muestra. Las verificaciones compactas de este checkpoint acreditan controles
de cola, no legibilidad de nombres del panel. No es evidencia causal del fallo
X11 ancho histórico. Próximo: priorizar nombre sobre metadatos opcionales.
Los handlers privados y el framebuffer no acreditan input/clipboard del SO,
AT-SPI,2x, otros SO, deadlines de syscalls o vida global de helpers. El objetivo
completo, ROADMAP, anomalía X11 y freeze PSS/Swap/CPU/startup siguen activos;
no se añaden formatos de previa ni se reanudan experimentos de recursos.

Reconciliación2026-11-01: la reserva geométrica de nombres del pendiente anterior
se corrigió en NARROW-NAMES.md, fuente f6a735f; los recibos previos permanecen como
evidencia del estado anterior. Esto no resuelve causalmente la anomalía X11 ancha.
