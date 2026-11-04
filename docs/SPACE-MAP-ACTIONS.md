# Acciones de archivos en Espacio

El objetivo completo sigue activo. Este checkpoint elimina las restricciones
indiscriminadas de Espacio en la paleta y la barra MC para las acciones que ya
admitían los motores y las rutas de teclado, inspector o menú.

| Acción | Disponibilidad y autoridad |
|---|---|
| Copiar, mover, crear ZIP, extraer | Abren la revisión existente; no ejecutan por abrirla. Otra copia en cola no impide revisar el siguiente trabajo. Preparaciones previas deben entregar su ACK. |
| Renombrar, crear archivo/carpeta, Papelera, propiedades, deshacer/rehacer | Conservan las barreras de mutación/copia activa y los límites del motor. Propiedades admite una marca o un foco, sin exigir un foco ajeno a la marca. |
| Previa y ruta | Usan el foco existente y los handlers existentes; no se añaden decodificadores ni formatos de previa. |
| Buffer privado Copy/Cut/Paste | Conserva la captura FD, revisión y controles del buffer de files. No acredita publicación ni recepción del clipboard del SO. |
| Abrir | Además exige pertenencia a la proyección vigente, como la navegación del mapa. Enter y doble clic pasan por la misma comprobación. |
| F5 y Ctrl+R | F5 mantiene el recálculo de Espacio. MC y dispatcher comparten f5_action; Ctrl+R ahora despacha Action::Refresh, incluida la solicitud explícita de reintentar vigilancia. |

`map_actions` comparte disponibilidad entre paleta, MC, menú, inspector y el
despacho de Action en el mapa. Pintar no recorre las marcas, clona nombres ni
consulta el filesystem; Abrir usa la caché acotada de la proyección. Ejecutar
comprueba una sola vez las marcas antes de sanear/capturar. Una marca inválida u
oculta rechaza la acción, conservando selección y foco; no cae sobre otro archivo.
Los controles deshabilitados explican el motivo; la paleta vuelve a comprobar
owner, generación, revisión/ocultos del mapa y su prueba exacta de selección.

Las operaciones usan las marcas existentes del listado raíz, o el foco si no hay
marcas. Pueden incluir marcas heredadas de otra vista ausentes de un informe
acotado. No se amplían a otros objetos del informe. All/Invert siguen limitados
a la proyección; Clear deja selección y foco vacíos. El filtro retenido se ignora
sólo en Espacio. Un informe parcial o detenido no impide revisar nombres conocidos
del listado; sus bytes/errores no son permisos, existencia ni autoridad para operar.
Los nombres OsString y los preflight/FD/revisiones de cada motor se conservan.

La paleta incorpora la previa existente. Los tamaños por lote y el ordenamiento
incompatible con el mapa conservan sus restricciones. Native clipboard/input,
AT-SPI, 2x, otros SO, recursos/startup y el resto del ROADMAP siguen pendientes;
el freeze de recursos/startup y la pausa de nuevas previas se mantienen.

Aceptación de fuente 6d9c5dd y fixture 924f8d4: 1301 pruebas normales
(447 core,105 helper,749 UI), tres históricas ignoradas; Clippy all targets con
warnings como errores, formato y release PASS. Wayland1793557957164468049,
XvfbX111793557960140989528 e instalado Wayland1793558055069373867 pasaron cada
uno 29 capturas, cuatro cierres normales con drain y comprobación de las15
identidades helper registradas (14mapa, una previa de texto existente).

El fixture verifica copia, renombrado, creación de carpeta, movimiento preparado,
Papelera y Undo sobre archivos propios; propiedades, ZIP y Copy/Cut/Paste privado
se verifican como lectura o revisión/cancelación. XDG_DATA_HOME se aísla por app.
Los primeros dos intentos28capturas fallaron al no crear su directorio padre de
Papelera: precondición documentada del motor; corregir el fixture bastó con el
mismo binario en ambos backends. Se conservan esos FAIL y el error de profundidad
de macro JSON del fixture (corregido separando grupos, sin elevar límites globales).

Instalación1793558033383070484, cinco hashes instalados y cinco respaldos
verificados; app2cae3b73db4ec56a3ab928d974e585b6d9a2c18537364ff1100722eb8d72d842.
Recibos: `docs/evidence/listing-model/space-map/actions/assessment.json`.
La ruta textual CopyPath conserva la implementación previa to_string_lossy:
no se acredita entrega lossless de rutas no UTF8 al clipboard del SO.
