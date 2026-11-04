# Miniaturas visibles — 0.0.4 en desarrollo

Grilla e inspector muestran miniaturas PNG/JPEG/WebP y un frame inicial de vídeo.
Se solicitan sólo la selección y hasta32 fuentes visibles, nunca toda la carpeta.
Nombre Unix noUTF8 viaja como bytes, sin conversión con pérdida. La miniatura del
inspector es la misma entrada que dibuja Grilla; pulsarla abre Space. El vídeo
se reproduce con controles en Space, no en cada celda. No hay disco nuevo de caché.

## Pipeline y memoria local

Un worker reutilizable con un pedido/resultado activo: abrir FD/revisión fueraUI,
reutilizar bitmap si coincide con la revisión anterior, o invocar el motor existente
con expected_revision y comprobar source efectivo. Imagen usa helper compartido;
JPEG12/24MP baseline reduce IDCT primero y muestrea a160px después. Progresivos
que exceden32MiB de coeficientes siguen rechazados: P02 permanece parcial.
Vídeo comparte FFprobe→FFmpeg del snapshot existente y escala a160px; sin shell,
sin red, sin procesos anidados. No decoder nuevo ni dependencia/licencia nueva.

El watch de padre no recursivo reutiliza el worker de MEDIA-LIFECYCLE: modo origen
para Space y modo directorio para miniaturas. Attach antes de autorizar demanda,
época/token y ACK del job impiden aceptar resultados viejos. Cualquier evento
invalida conservadoramente y vacía los RGB; una sonda FD por recurso visible se
programa al caducar su prueba1s para pérdidas de eventos. Desde la corrección de parpadeo del 2026-11-04, durante una renovación
periódica se conserva la última imagen confirmada como snapshot visual; no
constituye prueba fresca ni autoridad de operación. Las invalidaciones reales
y los errores siguen retirando el bitmap. Una syscall bloqueada
no implica deadline garantizado. No symlink_metadata/stat/readdir desde draw/UI.
O_NOFOLLOW final no confina symlinks de padres. Linux verificado; adaptadores
restantes pendientes. FIFO/directorio/symlink final no se leen como imagen.

LRU de propietario único, acceso por préstamo durante draw: no Arc fijados fuera
sin cobrar, ni clonación RGB por ocurrencia. Capacidad de Vec RGB (incluye cabecera
wire drenada) + ticket activo máximo80896B comparten4MiB; como máximo32entradas.
Reserva ANTES de decode y se mantiene hasta ACK/insertar o descartar. Claves de
caché64KiB contadas por capacity+estructura; demanda64KiB comprobada por referencias
antes de clone/join. El viewport de muestreo es único y reutilizado, máximo76800B,
separado explícitamente del ledger RGB. Thumbnails redondeados8px incluso con
corners square: máscara CPU con cobertura de borde aplicada a la imagen real,
incluido letterbox. Painter sigue siendo software y no crea widgets por celda.

Caché se libera al ocultar demanda/navegar/abrir Space. Preview, du y Espacio
esperan drenaje de miniaturas antes de otro helper; se difiere du si hace falta.
Full Space no reutiliza todavía esta LRU: la excluye para evitar duplicación de
caché, conserva sus límites anteriores y buffers de pipeline/scratch separados.
Esto NO unifica aún todos los formatos/frames en un presupuesto global, ni prueba
PSS+Swap del árbol50MB/150MB. P09 sigue parcial. No se usan números del footer
(PSS principal) para aprobar. Recursos/startup NOT_MEASURED por freeze explícito.

Los fallos de codec quedan asociados a revisión: revalidar un archivo sin cambios
no lanza otra vez el decoder cada segundo. Borrado/fallo retira su bitmap; reemplazo
con mismo nombre/mtime/size necesita la nueva identidad. Un evento perdido sobre
una entrada fuera de demanda se detecta al reusar; no se promete borrado instantáneo
sin evento. RAM desaparece al salir; no TTL/recolector de disco ni barrido de cachés
ajenas. Guardar originales en Papelera es operación del usuario, no caché de media.

La puerta de visibilidad usa shown/visible_r FLTK: cancela media y cambia token,
no hace nuevas decodificaciones ocultas; al volver sondea de nuevo y vídeo vuelve
pausado en la posición previa. Tests de handler, no prueba nativa de minimización.
Worker readonly imposible de interrumpir puede impedir terminar proceso en kernel:
no join desdeUI ni nuevos hilos para rodearlo. Antes de permitir salida en fase
readonly, un gate prohíbe cualquier spawn futuro; fase helper espera cancel/reap.
ACK de la sonda y permiso para cerrar se distinguen, sin falso drain completo.

## Evidencia funcional

423 Rust (132core/46helper/245UI), Clippy workspace/all-targets -Dwarnings y release
limpios. Cache exige ticket, cuenta capacity, límite32/LRU, rechazo sobredimensión,
respuesta tardía tras dirty descartada, error memoizado por revisión, preflight de
entrada grande, prueba expirada noReady, worker reutilizado/ACK y gate readonly vs
helper. Imagen letterboxed redondeada y scratch reutilizado se prueban con recorder.

`evidence/thumbnails/thumbnails-ui-1793069315038209135.json`: PASS de escenario,
tres instancias serializadas y cinco capturas reales1000×650. Primera:7 recursos
visibles, JPEG12MP→160×120, filename raw, RGB compartido con inspector, frame vídeo,
Space→volver, sin nuevo decoder en intervalo estable, replace/modify/delete/TrashACK,
cambio de carpeta y callback con helper vivo→exit0. Segunda: callbacks notify
suprimidos deliberadamente, reconciliación FD cambia rojo→azul→callback exit0.
Tercera: borrar C++ widget propio con helper vivo→guard y drenaje→exit1 esperado,
sin panic.45/2/10identidades de helpers registradas ya no existen tras salida;
concurrencia observada1/0/1 no es cobertura completa de lifetimes ni presupuesto.
Handlers internos y framebuffer, NO inputSO ni Super+W real.

FAILs anteriores y sidecars se preservan: marcador Close ausente en instrumento,
ESRCH legítimo al salir helper, comando antes de primer paint. En ese último caso
hubo además un panic WidgetTracker eliminado sin backtrace. Su causa original
sigue abierta; corrida idle20s con backtrace no lo reprodujo. Guardas nuevas no
usan ventana/canvas eliminados y cancelan/drenan por ruta de fallo; la fixture
verifica esa protección, no la causa ni compositor. No afirmar que Super+W quedó
validado. Fallo inesperado del backend no es recuperación durable de prefs/tareas
tras crash: X02 sigue pendiente; errores/rescate de operaciones se imprimen.

Pendientes: caché compartida entre documentos/Space, embedded Markdown/HTML,
PDF/audio, otras vistas/pestañas/paneles, HiDPI nativo,
minimización/inputSO/offline y medición del árbol sin capturas/builds concurrentes.
Los FAIL/UNVALIDATED globales previos permanecen; sin tag0.0.4 ni PASSglobal.


Checkpoint2a4c95e instalado por usuario con backup y hashes verificados:
files644d3ed6a9dc18a463af9ddb5dbb1fbabae78ac4bfa7a6fb7549c234ba5b60c6;
helper76d279c3ac70bdbfdae4c8e82b12a27f09ef28808ab73c946dc52002f143b5ff;
HTMLa5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
`evidence/0.0.4/installed-checkpoint-1793069978470793539.json`. Conocidos abiertos
incluyen causa original de WidgetTracker; protección verificada no sustituye su
investigación. No publicación ni tag nuevo. Siguiente: ampliar identidad/cache a
recursos incrustados y resolver transporte raw de imagen completa, manteniendo
la prioridad de medios y el resto de ROADMAP abierto.


2026-10-27: RAW-PATH-WIRE cierra transporte de imagen completa raw Unix y CLI
args_os, conservando strings compatibles.427Rust/Clippy/release y escena
1793071898429792393PASS con directorio0xFE+archivo0xFF. Pendiente embedded
y caché global se mantiene, al igual que la causa original del WidgetTracker.

## PNG/JPEG grandes visibles — 2026-10-27

`python3 scripts/thumbnails-ui-check.py --large-codecs --normal-only`: escena
1793082861888227321PASS con PNG24MP por filas y JPEG24MP progresivoDC1/8
reducidos a160px. Usa motores/FDproof/cache existentes; no implementación de
codec/caché duplicada. Snapshot incluye source_width/height yformat para exigir
6000×4000/PNG,JPEG antes de capturar.4framebuffers1000×650, Grilla/inspector
comparteRGB, replacement/delete/raw/Trash/navigate/Space/cierreactivo.
44PID/starthelpers registrados ausentes/reutilizados, rootexit0; no lifecycle
coverage total. Este comando omite repetir las fixtures previas de notify
suprimido/widgetloss, no las sustituye.454Rust/Clippy/releasePASS.
Caps locales unchanged, no nueva cache disco; recursos/startupNOT_MEASUREDfreeze.
APNG/ICC/WebPgrande/P09global/otrosSO/nitidez nativa2x permanecen pendientes.


M16 GIF-PREVIEW reutiliza primerframe de vídeo/160px yLRU existente. Escena
1793098465429205562PASS inspector160×90rojo, fuente640×360/firmaFD/pixel real y
captura visible despuésde cerrar Space (últimoframeSpaceeraazul). Es demanda
visible legítima, no loop oculto; grillaGIF no capturada, no afirmar verificación
visual de esa superficie aún.485Rust/Clippy/release, sin recursos/startup porfreeze.
