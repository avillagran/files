# Medios bajo demanda: identidad, invalidación y caché — 0.0.4

Prioridad explícita del usuario, 2026-10-26. Después de cerrar Ayuda, previews y
miniaturas visibles preceden más funciones periféricas. Se reutilizan helper de
imagen/IDCT, vídeo FFprobe/FFmpeg, binary pipe y controllers latest-wins existentes.
Freeze recursos/startup continúa: presupuestos reales de esta unidad NOT_MEASURED.
Este documento distingue contrato objetivo de unidades todavía no implementadas.

## Identidad y publicación

Clave: ruta absoluta **raw** (no conversión UTF8 con pérdida), tipo de recurso
(imagen reducida/frame de vídeo y tamaño/posición), identidad dev+ino y revisión
size+mtime_sec/nsec+ctime_sec/nsec. Atime no invalida por lecturas propias. Sólo
archivos regulares, FD O_NOFOLLOW/NONBLOCK/CLOEXEC. O_NOFOLLOW protege sólo el
componente final: los symlinks de padres y mountpoints se siguen; no es un
confinamiento de rutas ni sandbox. Otros SO necesitan adaptador,
no se fabrican dev/ino cero. Identidad sola no es hash ni snapshot de contenido.

El FD que se lee/decodifica debe coincidir con la firma esperada, estable antes y
después de leer/decode. Antes de publicar se revalida también el nombre sin seguir
symlink: reemplazar con el mismo nombre no reutiliza el objeto antiguo. Una firma
anterior tomada en otro open NO basta para certificar el FD real del decoder.
Cualquier discrepancia libera RGB/frame y entrega cancelación/error, no media vieja.
IPC imagen checked envía expected_revision y Response::Image devuelve source del
FD real. La UI exige coincidencia con expected y su generación antes de Ready;
vídeo snapshot/stream aplica el mismo contrato en su worker con FD pinned.
Los paths siguen raw en modelo/sonda. RAW-PATH-WIRE (2026-10-27) añade
strings Unicode compatibles y bytes Unix a Request; imagen completa con root y
nombre opacos verificada. Adaptadores Windows/macOS y otras superficies siguen
pendientes.
En sistemas con timestamps gruesos o cambios concurrentes no hay garantía de
snapshot atómico; eventos observados nunca pueden dejar un resultado obsoleto
aceptado por una generación posterior.

## Eventos y reconciliación

Un watcher no recursivo por directorio activo, no por archivo. Como máximo los
padres de carpeta visible y preview activo; eventos acotados y overflow/error
invalidan conservadoramente el conjunto, sin acumular paths. Adjuntar watch y
sondear FD/metadata se hacen fuera de UI, mediante un worker reutilizable y un
único destino/pedido pendiente; syscall bloqueado no genera otro hilo al navegar.

La época monotónica nunca se limpia al terminar una sonda. Callback/resultados
incluyen token de destino; se adjunta el watcher ANTES de abrir el origen y sólo
se publica Verified si token/época siguen estables. La UI vuelve a comprobar época
antes de aceptar y antes de dibujar (sólo atomics/slots, sin I/O). Reconciliación
visible cada1s y reenganche del padre antes de sondear cubren parent rename/reemplazo
y eventos perdidos; sin evento no se promete invalidación instantánea.

Evento recibido: descartar inmediatamente imagen/frame/UI del origen, cancelar
generación activa y drenar/reap antes de otro decode. Reconciliar al abrir/reusar,
y acotadamente para origen visible si se pierden eventos. Borrado muestra estado
no disponible, modificación vuelve a cargar bajo demanda, reemplazo cambia firma.
Operaciones propias invalidan tras ACK antes de refresh; rename/move invalida
origen/destino, sin migrar sólo por nombre. Al navegar se desconecta directorio
anterior y último destino gana. Recursos Markdown incrustados requieren integrar
sus padres/firma por recurso; no se dan por cubiertos por el watcher del documento.

## Demanda, LRU y helper único

Grilla solicita únicamente índices visibles (prioridad selección), nunca readdir
completo para precargar imágenes. Inspector solicita sólo selección; Space usa
preview grande y vídeo reproduce/seek con el backend existente. Miniatura inicial
de vídeo es un frame, no un loop de reproducción en todas las celdas. Salir de
Grilla/ocultar inspector/cerrar Space elimina demanda y cancela trabajos no usados.
Sólo un helper/decoder activo entre previews/du/treemap/miniaturas; pedido siguiente
espera ACK/reap, no se oculta consumo en hijos paralelos.

Objetivo de primera caché: RAM compartida, RGB retenido global4MiB, máximo32
entradas y256KiB de claves/metadata; miniaturas≤160px, preview grande actual≤1024px.
Cuenta capacity del Vec (incluye wire drenado), no sólo len. Un propietario con
préstamos comparte Grilla/inspector en M2, sin RGB clonados ni pines externos;
un recurso fijado por la UI sigue cobrado aunque se quite del LRU. Antes del helper
se reserva el máximo transiente permitido y se evictan no fijados; si no cabe,
se espera/libera otro recurso, no se decodifica primero y rechaza después.
Scratch de viewport y bytes de decoder/helper se registran aparte: estos límites
NO acreditan50MB ni150MB del árbol. No aumentar el techo ni cortar funciones a
placeholders para convertir un objetivo ideal en PASS.

## Disco, temporales y cierre

Primera unidad nueva: RAM/pipes exclusivamente, sin caché persistente ni archivos
junto al original. Imagen/video actuales usan buffers/pipe; PDF usa proceso/pipe
con FD abierto. Las capturas de evidencia son del harness, no caché de producto.
Sin caché de disco propia no se inventa un recolector de archivos ajenos ni se
borra ~/.cache de otras apps. Prueba debe verificar que no se crean ficheros de
producto tras éxito/error/cancel/cierre. Cerrar solicita stop al worker readonly sin join en UI: una syscall sobre montaje
puede ser imposible de interrumpir. Se registra si llegó ACK; si no llegó, la
la terminación del proceso solicita recuperar su hilo/FD y NO equivale a sonda
drenada. Una tarea en estado kernel ininterrumpible puede demorar incluso la
terminación del proceso: no se garantiza deadline de cierre offline ni se crea
un worker de reemplazo para rodearlo. Esto
no relaja el ACK/reap de helpers externos antes de salir.

Crash libera RAM/FD; no quedan miniaturas
nuevas en disco. Si se añade disco posteriormente: subdirectorio privado0700,
manifest por identidad/firma, temporales exclusivos, publicación atómica y cuota/TTL,
limpieza de huérfanos al iniciar y recuperación de staging tras crash. Nunca se
promete eliminación instantánea mientras files está apagado.

## Entregas y aceptación observable

1. **M1 implementado/verificado Linux, alcance seleccionado**: firmas FD y watcher/reconciliación cancelable del
   preview seleccionado imagen/vídeo; ocultar media antigua al borrar/modificar/
   reemplazar/renombrar, incluidos cambios propios, sin nueva caché persistente.
2. **M2 implementado/verificado Linux en Grilla/inspector**: miniaturas visibles Grilla + inspector, LRU/tickets globales
   y reserva previa, motores existentes a salida reducida y caché compartida con
   Space. MEDIA-THUMBNAILS documenta alcance real: RGB compartido Grilla/inspector,
   LRU4MiB/tickets y exclusión al abrir Space; caché entre documentos y todos los
   buffers/formatos globales siguen pendientes. No contar iconos como miniaturas.
3. **M3 pendiente**: pruebas de cuotas/entradas fijadas/eventos perdidos/rápida
   navegación/cierre/error y orfandad según almacenamiento realmente usado;
   imágenes/frames desactualizados nunca aceptados por una nueva generación.
4. **Medición pendiente por freeze**: PSS+Swap árbol, picos/estable/CPU/latencia/
   liberación y vídeo sin capturas simultáneas; GPU aparte si aplicara. Los FAIL y
   UNVALIDATED históricos permanecen intactos, sin PASSglobal ni tag0.0.4.

Fixtures propias: borrar externo y desde files, mismo path reemplazado preservando
mtime/size, modificación in-place, rename/move, cancel A→B→C, close activo, revisión
perdida por watcher/overflow, LRU por capacity y referencias fijadas. Framebuffers
con colores identificables antes/después, clipboard/inputSO separado, instalar
sólo checkpoint funcional validado con backup y SHA app/helper/HTML coherentes.

## Evidencia M1 — 2026-10-26

410 pruebas Rust (132 core,45 helper,233 UI), Clippy workspace/all-targets con
-D warnings y release limpios. Pruebas deterministas: Verified(A)→replace(B) antes
de open del helper rechaza A; Ready(A) tardío tras dirty/Verified(B) no se publica;
callbacks/resultados de destinos viejos no cambian la época nueva. Un mutex breve
serializa token/época contra callback; nunca contiene I/O. Inotify real prueba
reemplazo, borrado y cambio de identidad del padre; sondeo/reattach ocurre fuera UI.

`evidence/media-lifecycle/media-lifecycle-ui-1793064138443049803.json`: PASS funcional
acotado, cuatro framebuffers reales1000×650. Rojo→azul con mismo nombre/size/mtime,
modificación verde, borrado, rename away/back, padre reemplazado, A→B→C, vídeo
snapshot/stream y borrado durante reproducción. Ocultar preview no lanzó otro
helper durante el intervalo de reconciliación; Papelera ocultó media antes de
mutar y confirmó ACK éxito/Undo. Root exit0 tras ACK;13 identidades PID/starttime
de helpers registradas ya no vivas. ACK del probe readonly=true en esta corrida;
no prueba offline/kernel atascado ni cobertura temporal completa de procesos.

Primer informe1793064084657403710 queda FAIL intacto: instrumento omitía variable
que habilita marcador ACK y esperaba nombre original en Trash (usa nombre único).
Sidecar explica la corrección; limpieza de emergencia de ese FAIL no es lifecycle
PASS. Los RGB se comprobaron en State contra el patrón y revisión efectiva, además
de capturar framebuffer. Handlers internos, no inputSO. Footer PSS principal no
aprueba presupuesto; árbol/Swap/CPU/arranque NOT_MEASURED por freeze. No caché de
producto creada en source/config/data de fixture; Trash conserva el original por
semántica de Papelera, no por caché. No auditoría del filesystem completo.

Pendientes: miniaturas Grilla/inspector, caché global LRU/tickets, embedded Markdown,
raw-path IPC imagen, otros SO, minimización/inputSO y recuperación offline con
syscall bloqueada. No P02/P07/P09 completos ni PASSglobal/tag0.0.4.


M2: ver MEDIA-THUMBNAILS y escena1793069315038209135: miniaturas reales, raw IPC,
reconciliación con notify suprimido,45/2/10helpersgone, cierres0/0/1(backend-loss
esperado). WidgetTracker panic previo tiene causa abierta; guardas de superficie
eliminada verificadas con destrucción propia, no compositor.423Rust/Clippy/release,
sin nuevas métricas; P09global y nativeinput siguen abiertos.

2026-10-27: RAW-PATH-WIRE verifica CLI args_os/IPC/imagen completa raw Unix,
427Rust y escena1793071898429792393PASS. Esta corrección sustituye el pendiente
de IPCrawimagen de la entrega M2; no la firma FD de imágenes incrustadas.

2026-10-27: RELATIVE-SOURCE añade firma efectiva de raíz/texto y sonda→decoder
checked para incrustadas Markdown. Reemplazo entre sonda y open se rechaza antes
de leer; root/nombre/file se revalidan tras decode. No cubre aún cambios después
de publicar ni todos sus padres: watcher/reconcileembedded sigue pendiente.

Seguimiento 2026-10-27 M4 DOCUMENT-MEDIA: worker existente acepta manifiestoMD,
watch antes de sonda final y candidato oculto hasta token/época verificados.
Intenciones ausentes sin RGB y reconciliación1s recuperan recreación/raíz;442Rust
y escena1793076116463397564PASS2instancias con/sin callbacks,5capturas/26helpers
registrados porinstancia. Stopreadonlyfalse/true separado dehelperACK; NOjoinUI,
no drenaje falso. Recursos/startupNOT_MEASURED; P09global/otrosformatos pendientes.

## Audio seleccionado — 2026-10-27
AudioJob incorpora el mismo watcher/token/epoch/reconcile, expected FD Revision
y firma efectiva en Ready. Rechaza probeA/openB y publicación tardía de otro
original; no se añade audio a thumbnails ni motor/cache de reproducción.
AUDIO-PREVIEW registra458Rust/Clippy/release y audio-ui1793083931390323451PASS
con callbacks suprimidos, reemplazo/delete/rename/padre y11loggedhelpersgone.
Gate de cierre ReadOnly/Helper separa permiso de salida y ACK; no joinUI ni
futuros spawns tras cerrar readonly. KernelD/offline real aún no probado.
Sin confinamiento de padres ni snapshot atómico; recursos/startupNOT_MEASURED.


M12 (2026-10-27): PDF seleccionado adopta expectedRevision obligatoria, FD real
compartido pdfinfo/pdftoppm, revalidación intermedia/final y Bitmap.source. Watch
antesprobe/epochs/reconcile se reutilizan; sourceflag tipado fuerza token nuevo.
473Rust/Clippy/release y pdf-ui1793092913810767063 PASS funcional con callbacks
suprimidos/replace/inplace/delete/parentrecover/23loggedhelpersgone/cierre0ACK.
readonly_gate compartidoaudio/PDF: permisoexitreadonly no equivale a drain; no
join/UI ni nuevosworkers ante syscallstuck, no prueba kernelD. PDF-PREVIEW detalla
límites; HTML y demás previews aún no heredan automáticamente este contrato.


M13 (2026-10-27): HTML seleccionado usa expected/efectivaRevision, FD heredado
al adaptador en vez de reopen de path y readonly_gate compartido. Ready exige
generación/firma, dirty/error retira display list.476Rust/Clippy/release,
fd-contract1793093345381869451 y html-ui1793093483630467339 PASS scoped: notify
suprimido/replace-inplace-delete-parentrecover/14loggedhelpersgone/cierre0ACK.
HTML-ENGINE-PROBE detalla límites; aproximación de texto/recursos/enlaces/parser
no quedan completos por esta unidad. FreezeNOT_MEASURED intacto, sin cache disco.


M14 PDFtexto usa el mismoFD/gate/watcher y outputs excluyentes bajo demanda.
Capacidad de tamaño estable PDF/HTML yaNO emite flag SourceUnavailable/reprobe:
error terminal hasta nueva revisión/reapertura, evita loop deworker/token.
480Rust/Clippy/release ypdf-ui1793094189643854652 PASSscoped. No nuevas medidas;
PDF-PREVIEW detalla textoextraído/selección ylímites, no geometríaPDF/clipboardSO.


M17 VIDEO-SOURCE-CLOSE: snapshot/stream seleccionados añaden sharedreadonly_gate
como audio/PDF/HTML, admisión mutex contra closing ypermiso readonly≠workerACK.
487Rust/Clippy/release yholdsimulado1793099666110053045PASS2casos/2FB/exit0con
DRAINEDfalse/noadmisióntardía, NOkernelDreal. RegresiónGIF1793099692852727198PASS
6FB/10helpersgone/activecloseACK0. FaseHelperconrevalidatebloqueada noresuelta,
no promesa de terminacióninstantánea deD-state, miniaturassupervisorprevio.
FreezeNOT_MEASURED; SourceDoc detallaalcance, noP09global ni nativeSuperWproof.

## M18 — zoom/pan vídeo y GIF, 2026-10-27

VIDEO-ZOOM: mismoViewport1–8×/Ctrlrueda/+/-/0/arrastre, framesinvalidanpíxelssin
resettransform; seekliberaRGB yretiene3floats, nuevoorigen/revisiónresetea.
488Rust141/58/289+Clippyworkspacealltargets-Dwarnings/release/fmtPASS. Escenas
video-play-ui1793101116101351845(GIF11FB/12helpers) y1793101152229152975(MKV9FB/10)
PASS1000×650/zoom2pan48/36/conservadoframes-paused-seek-resumeEOF/reopenreset/
activeclose0ACK/loggedPIDstartgoneZfalse. Handlers internos, noinputSO/clipboard/
sharpness2x/lifetimes, no detallefotográficoprobado por coloresuniformes.
No clip24frames ni PSS/Swap/CPU/startup porfreeze; P07/P09siguenparciales.

## M19 — navegación visible de previews, 2026-10-27

PREVIEW-NAVIGATION: botones Nerd anterior/siguiente+Alt↑/↓ compartenAction,
ordenvisible/ocultos/filtro; sinUII/O/preload. Extremo conserva estado/revisión;
cambio real reutiliza selección/generación/cancel/drain yúltimodestino.
488Rust141/58/289+Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
preview-neighbor-ui1793101665648168454PASS5FB1000×650/PNG-WebP-rawPNG0xFF/
hiddenexcluido/FDsourceypíxeles/errorreal+volver/rapidúltimogana/callback0ACK/
7loggedhelperIDsgoneZfalse. Hit target/nombreaccesible verificados; noinputSO/
AT-SPI/clipboard/lifetimecoverage/activehelperclose enestaescena.
Freeze recursos/startupNOT_MEASURED; no playlist/autoplay ni completitudP09.

## M20 — giro no destructivo del visor, 2026-10-27

PREVIEW-ROTATION: botones Nerd/R/ShiftR giran sólo vista de imagen/PDF/vídeo/GIF;
muestreo directo del RGB original, sin copia rotada ni archivos modificados.
Transform numérico conserva giro entre frames/seek yPDFpáginas/texto-retorno;
nuevoorigen/revisión/reset0 lo elimina. AyudaF1 añade atajos reales de medios.
489Rust141/58/290+Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
preview-rotation-ui1793102443334515413PASS14FB1000×650/asimetría4cuadrantes con
píxeles framebuffer comprobados/giroCW-CCW-reset/video-GIFplayseek/PDFpágina2+
textoretorno/originalesbytesintactos/callback0ACK/21loggedhelpersgoneZfalse.
Sin inputSO/native2x/AT-SPI/lifetimes/activehelperclose enestaescena ni prueba de
colorfotográfico;14readbacks funcionales NOmedición aislada de consumo.
Recursos/startupNOT_MEASUREDfreeze; noP02/P06/P07/P09global ni tag0.0.4.
