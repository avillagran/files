# Espacio: treemap de tamaños

Referencia aprobada `mockups/v3.html#v-map`, distinta de **Espacial** (posiciones
manuales de iconos). Botón Nerd **Espacio** o Ctrl+4 calcula los hijos inmediatos
con un helper efímero; las carpetas incluyen sus archivos descendientes. Un clic
selecciona el objeto para inspector/operaciones; Enter/doble clic entra en una
carpeta real, Volver/Backspace sube. Actualizar vuelve a escanear; Cancelar y salir
de la vista descartan el informe y cancelan/recogen el helper. La omnibar navega
normalmente; el filtro actual no se aplica al mapa y se indica explícitamente.

Los bloques parten de bytes **aparentes de archivos regulares por nombre**,
recursivos en carpetas, no bloques físicos ni propiedad exclusiva de disco.
Hardlinks cuentan por nombre; archivos sparse conservan su longitud aparente.
Symlinks y objetos especiales no se siguen y contribuyen cero, visibles en la
lista sin área. No aparece «0 MB residentes» ni consumo inventado del mockup.
El pie PSS existente es sólo del principal; no acredita el presupuesto del árbol.

## Proporción y estados

El layout puro divide por bytes completos positivos sobre el eje más largo,
con áreas proporcionales y límites compartidos. No crea un peso mínimo para
cero/desconocidos/error/parciales. El painter redondea límites a píxeles y separa
visualmente cada bloque2px: esos márgenes/etiquetas no son precisión física de
cada píxel. Bloques diminutos también son accesibles en lista. Etiquetas recortan
con clip propio; nombres se convierten con pérdida únicamente para mostrar,
identidad/hits usan bytes lossless y el índice de la generación de listado.

Un nodo parcial tiene `bytes=None` y `observed_bytes` como límite inferior; se
excluye del denominador de las áreas. El total medido suma sólo nodos completos,
el observado incluye límites inferiores. PARCIAL y su causa no son cargas en
curso: cero/desconocidos/error/parciales aparecen en lista separada. Vacío muestra
0B sin bloque ficticio. Error del helper no mantiene el informe anterior como
si fuera actual. La selección muestra estado/bytes/conteos reales en inspector,
sin disparar un segundo du. Metadatos/diagnósticos muy largos quedan recortados;
copiar/export completo y tooltip de cada bloque todavía pendientes.

## Helper, lifecycle y límites

Linux: root final O_DIRECTORY|O_NOFOLLOW, readdir/fstatat/openat por FD, sin seguir
symlinks finales/intermedios descendientes. Apertura inicial de ancestros tiene
resolución normal; `link/.` y mountpoints no son una sandbox. No snapshot: contenido
puede variar in-place durante cálculo. Identidad de un directorio se revalida al
abrirlo. Sólo retiene resultados/nombres de hijos inmediatos:

- 2048 nodos, nombre4096B, nombres agregados256KiB; diagnósticos512B/nodo y64KiB
  agregados (incluye motivo del informe), antes de clonarlos.
- 1M entradas observadas, profundidad128 y15s cooperativos. Las carpetas/archivos
  incompletos nunca declaran bytes exactos. Omisiones desconocidas son None,
  no un cero inventado; Some(0) exige EOF de enumeración raíz.
- Request64KiB, respuesta4MiB, writer/reader acotados antes de ampliar buffer.
  Preconteo de nombres canonical antes de serde y validación de cantidad/bytes,
  unicidad/totales/conteos/status después. No parser JSON sandbox general.
- Un State/worker/hijo: último destino deseado, cancelar y consumir ACK/reap antes
  de reemplazar en otro pump. Timeout supervisor20s, kill/wait fuera del hiloUI.
  Salir/cerrar espera drenaje, sin depender de Drop. Preview y du no se lanzan en
  paralelo al helper de Espacio; un preview solicitado espera su cancelación.

Un syscall de FS no interrumpible puede retrasar kill/reap; no se presenta la
cota cooperativa como deadline duro de montaje offline. Se retiene un solo job,
sin acumular hijos/threads tras navegación repetida. ParserprotocolJSON requiere
ruta raíz UTF8; error visible para raíz noUTF8, nombres hijos raw sí admitidos.
Adaptadores de escáner para otros SO están explícitamente pendientes. Las cotas
de nombres/wire/FD/objetos no equivalen a PSS total ni a caché LRU P09.

## Verificación

370 Rust (121core/44helper/205UI), Clippy workspace/all-targets -Dwarnings y
release PASS. Helper CLI real9casos PASS en helper-wire1793057735444805998:
8:4:2, nombresraw,symlinkraíz/trailing/file/relative/missing rechazados,mismatch yvacío.
space-map-ui1793057823344999138 funcional PASS: áreas8:4:2, selección/inspector,
entrar/volver, cero/vacío/parcial/error, viewport real por Normal/fullscreen,
cancelación de recorrido20k, salir a Lista y cierre callback con helper vivo.
11identidades PID/start observadas, todas recogidas tras salida0 y drain; no
sampler de memoria/CPU. Los tamaños son longitudes aparentes de fixtures sparse.

Método: handlers/targets visibles y framebuffer FLTK, navegación y modos por
handlers compartidos; sin eventos SO. La fixture ignora entrada nativa ambiental
explícitamente (sólo con FILES_TEST_ISOLATE_INPUT=1 y --ui-smoke), evitando que
interacción externa navegue fuera de sus datos. App normal no cambia por esta flag.
La primera corrida falló por subcomando no registrado y recibió interacción
ambiental fuera de fixtures (lectura de listado/metadatos; sin mutación); se
preserva el FAIL, se registró space_map y se aisló la escena. La siguiente falló
al buscar Cancelar después de terminado (correctamente no visible); ahora cancela
un recorrido activo. También se conservan fallos iniciales de validación/clippy.

SHA app6dcb6c9b0079d04aa4d3d59477d91570e6922e020d6e03802b239a2c8f17655a,
helper737eb1dc23cdd90192c9742762c4c5cb87bca4c45521c51e89bf0259c5b50463.
Freeze recursos/startup NOT_MEASURED. V09 funcional visible aceptada con límites;
presupuesto total, rootraw/otrosSO/offlinemounts siguen pendientes, sin PASSglobal.
Integración posterior: ocultos globales defaultfalse/botón sincronizado, proyección
visible con totalinclocultos, estilo v3 y menú contextual en
HIDDEN-CONTEXT-AND-STATUS.md. Este informe anterior conserva su build/evidencia.


## Cambios posteriores a la observación

SPACE-MAP-CHANGES.md documenta avisos de cambios/interrupción en la raíz sin
recálculo automático, alcance no recursivo de vigilancia y recálculo explícito
por F5/Recalcular que renueva también el listado de nombres/hits. Datos completos
no implican actualidad ni instantánea. Aceptación fuente/instalación en ese contrato.
