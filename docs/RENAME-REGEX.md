# Reemplazos regex en el renombrador

El formulario de F2 permite alternar explícitamente reemplazo literal y regex.
El modo literal sigue siendo el valor inicial. El patrón general conserva
`{name}`, `{ext}`, `{dotext}`, `{n}` y `{n:03}` para prefijos, sufijos, extensión y
secuencia. El reemplazo se aplica al nombre sin su última extensión, antes del
patrón general y del cambio de mayúsculas/minúsculas. Un dotfile sin otro punto
se trata como nombre completo. La extensión sólo cambia al modificar el patrón.

Ejemplo: reemplazar `photo(?<n>[0-9]+)` por `imagen-${n}` transforma
`photo1.txt` en `imagen-1.txt`. `$1` usa un grupo numerado, `${nombre}` uno con
nombre y `$$` produce un dólar. Como en Rust regex, `$1_suffix` toma la referencia
completa más larga; usa `${1}_suffix` para separar el grupo del sufijo. Una captura
inexistente o no participante produce texto vacío. Regex vacío coincide en las
fronteras UTF-8. Propiedades `\p`/`\P`, clases Unicode (`\w`, `\d`, `\s`/fronteras) y
casefold Unicode no están activados: las tablas requieren una licencia distinta
de la allowlist y no se amplían las excepciones por inferencia. Usa rangos
explícitos como `[0-9]` o `[áéíóú]`; dot y literales UTF-8 sí funcionan. Las clases
ASCII pueden usarse en un grupo `(?-u:...)` cuando el patrón siga siendo válido
para texto UTF-8. Los cambios de caso del selector existente siguen usando Rust
fuera de la expresión. Look-around y backreferences no están soportados;
se muestran como error, sin plan aplicable.

El plan se calcula en el worker de mutaciones existente. No modifica archivos;
Aplicar sólo acepta un plan preparado actual y vuelve a verificar identidades y
destinos mediante el motor transaccional. Cambiar el modo o editar invalida la
revisión anterior. Colisiones, destinos ocupados, patrón inválido, exceso de
expansión y cancelación no entregan un plan parcial. Aplicar queda inhabilitado
cuando no hay cambios preparados. Deshacer/Rehacer usa el mismo historial seguro.

## Límites explícitos

El regex se compila una sola vez por lote. Patrón/reemplazo tienen máximo 4096 B en
la API y cada campo editable de UI máximo 2048 B. Sólo se habilita `std`, sin tablas Unicode del crate. El builder limita NFA a 1 MiB y
profundidad 64; máximo 64 grupos y 256 segmentos de sustitución. Los motores
opcionales DFA/backtracking/perf de regex quedan desactivados. Una búsqueda o
compilación individual acotada no se interrumpe dentro de la biblioteca; se revisa
cancelación antes/después y entre matches, segmentos y entradas.

La sustitución recorre primero slices prestados y suma la expansión hasta 4096 B,
antes de crear el String de salida. La segunda pasada construye sólo ese resultado
acotado. Se conservan los límites del planner (100k entradas por lista, 4096 B por
nombre, 8 MiB agregados de input/output por separado) y el preflight UI anterior a
clonar selección. Los errores de regex se formatean con un writer de máximo 1024 B
UTF-8. Estas cotas no son una medición de PSS ni una garantía de latencia total.

Fechas de archivos, normalización exacta por filesystem, renombrado masivo con
presupuesto integrado y journal durable siguen pendientes de R02/R03. No se
convierte con pérdida un nombre seleccionado no UTF-8.

## Validación

La suite integrada y `scripts/rename-regex-ui-check.py` registrarán resultados de
esta unidad. La escena es sólo funcional: lote de dos, vista previa, errores,
Aplicar/Deshacer/Rehacer, integridad de bytes y cierre tras ACK. Usa handlers
internos y framebuffer real, sin input del SO ni presupuesto de lote 100k.
Recursos y arranque permanecen NOT_MEASURED por suspensión explícita del usuario.

324 tests Rust (106 core/37 helper/181 UI), Clippy workspace/all-targets
`-D warnings` y release limpios. Escena final
`rename-regex-ui-1793046339654903640.json` funcional PASS: lote2, preview sin
mutación, regex inválida y expansión rechazada sin plan, Aplicar/Undo/Redo/Undo con
bytes exactos, sin staging residual y salida normal0 tras ACK. SHA256 app
`5a9b4e38cf6f469efd92951b248376fba15d660b6741a43490725da97aa89d8d`.

Primera escena `1793046051262606727` conserva su PASS de filesystem; revisión
visual encontró diagnóstico multilínea oculto en un pie de una línea. Sidecar
marca ese FAIL visual. Nuevo resumen de dibujo acotado conserva el diagnóstico
original y la captura nueva muestra motivo y Aplicar deshabilitado. No se
sobrescribieron capturas previas ni benchmarks históricos.

`scripts/rename-plan-check.py` ahora prepara un fixture Cargo que depende del
core real y sus features; el antiguo `rustc #[path]` omitía la nueva dependencia.
Python compila sintácticamente; el benchmark adaptado no se ejecutó por freeze y
no se extrapolan sus mediciones históricas al binario actual.
