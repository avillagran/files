# Vídeo: fotograma, duración y seek

P07 parcial, 0.0.4. Space sobre MP4/MOV/MKV/WebM/AVI/MPEG abre una vista con
fotograma y duración, línea de tiempo clicable y pasos de cinco segundos.
La entrega inicial era un snapshot; el checkpoint de reproducción añade
play/pausa software y audio opt-in mediante el mismo proceso. No promete tasa
de frames de UI, sincronización A/V ni presupuesto.

## Ejecución

Usa ffprobe y ffmpeg del sistema si están disponibles, según DESIGN J.
No se enlazan ni redistribuyen esas bibliotecas/binarios; no se añade crate,
codec, GPU ni daemon al proceso principal. Si falta una herramienta aparece
capacidad no disponible. El job lanza hijos directos secuenciales y recoge cada
uno antes del siguiente; nunca usa shell ni helper que lance nietos.

Linux abre en worker un archivo regular final O_NOFOLLOW/O_NONBLOCK, hasta8GiB,
ruta absoluta4096B; hereda sólo ese descriptor y usa /proc/self/fd/N. Conserva
inode al renombrar/reemplazar path, no snapshot de cambios in-place. Padres
symlinks y mounts se conservan como navegación SO, no sandbox de carpeta.

Se fuerza demuxer por extensión (mov,matroska,avi,mpegts,mpeg), protocolos
file,pipe; sin listas/HLS/concat, referencias MOV externas desactivadas.
Codec allowlist, max64Mi píxeles/16384px, duración finita≤7d, metadata64KiB.
Rotaciones ortogonales/SAR acotado admitidos por cálculo de display; falta
validación visual exhaustiva de vídeos rotados/anamórficos. FFmpeg software
-hwaccel none, un hilo decode/filter, salidaRGB exacta≤960×540×3. Sólo una
asignaciónwireRGB se mueve al Bitmap; no historial de fotogramas.

State retiene sólo último seek por revisión; cancela y drena antes de lanzar
reemplazo. Libera frame anterior antes de decodificar otro. Cerrar recoge ACK
antes de salir. Un error de herramienta/archivo se muestra en la vista.

## Límites y aceptación

Timeout4s ffprobe/8s ffmpeg, CPU8s/proceso, FD acotados y core0. RLIMIT_AS768MiB
contiene espacio virtual de librerías del sistema, **NO permite ese PSS** ni
prueba techo150MB. -max_alloc32MiB limita cada asignación, no la suma ni
memoria del proceso. Decodificadores y contenedores no son sandbox del SO.
Consumo total app+CLI+Swap pendiente: NOT_MEASURED por fase explícita de usuario.
No ejecutar nuevos recursos/startup hasta liberar fase; no extrapolar otros
previews. Escenarios4K/8K, codecs complejos y reproducción aún sin aceptación.

`python3 scripts/video-ui-check.py` crea MatroskaFFV1 sintético640×360, rojo
0–4s y azul4–8s. Captura primer frame, seek5s y clictimeline2s, verificaRGB
real decodificado y revisión fresca, cambio rápidoúltimo destino, errorsbad/
symlink y cierre0trasACK. Sólo handlers+framebuffer de ventana real; no inputSO
ni clipboard. Fixtures propias en ~/.cache/files-test, sin datos personales.

Seek aEOF conserva un intervalo de frame según avg_frame_rate/r_frame_rate;
si no hay tasa válida usa margen1s. Posición mostrada es la solicitud efectiva
normalizada, no el PTS probado del frame. VFR/últimoPTS requieren más evidencia.

## Reproducción software sin audio

P/Reproducir/Pausar usa un único ffmpeg directo, entrada -re ysalida fps12.
Un buffer parcial y un slot de último frame reemplazable enworker; la UI
retiene sólo subitmapactual yreutiliza unviewport. No historial indefinido.
El cap2buffers delworker NOincluye UI/viewport/copiaFLTK/buffersWayland ni
memoria decoder delhijo. Frames no mostrados se reemplazan, noencolan.

Pausa cancela/recoge decoder; reanudar abre otro desdeposiciónestimada. Seek
pausa reproducción yvuelve a snapshot; P reanuda. Cambiararchivo/cerrar drena
antesdespachar nuevo. Contadorframe se reinicia para cada decoder: un Ready
retenido no acredita el nuevo proceso. Posición=seek+sequence/12 estimada,
NOtimestampPTS/A-Vsync. 12fps esconfiguración, no rendimientomedido.

Primerframe/stall5s, duración+10s ycuotasdepipesjustas; stderrretenido64KiB
contrascenderdrenado, EOFparcialrechazado, kill+wait antesdeFinished. CPU
2×restante+10s(max1día),AS768MiBvirtual/max_alloc32MiB noaprobaciónPSS. Material
sparse/VFR/codecsexigentes puedenfallar; audio yhardware siguenpendientes.

`python3 scripts/video-play-ui-check.py` verificaframesreales, rojo→azul,
pausa/drenaje, resume/EOF ycierreduranteprocesoobservadovivo(PID+starttime).
Clip son24snapshotsframebuffer tomadosaproximadamente4fps yensambladosdespués
decerrarapp, no grabaciónOScontinuani prueba de12fps/CPU/latencia. Recursos/
startup siguenNOT_MEASUREDporfaseusuario, sinreplicarbenchmarks.

272Rust/Clippy/releasePASS; video-play-ui1793038888536010466 funcionalPASS
conPID/starttime deffmpegvivo ydesaparicióntraspausa/EOF/cierre. PrimerFAIL
1793038705143512044porsequenceanterioraceptadoenresumeconservado; se reinicia
sequence yexigeframe/spawnnuevo.

Captura video-playing-blue1793038886785315859 muestrafooterprincipal76.6MiB
tras24readbacksin-process. Esobservacióndelharness, UNVALIDATEDparaatribuir
alreproductor odecidirpresupuestoPSS+Swapárbol. No se midióplaybacksinclipni
capturassolas; experimento sóloalfinalizarsuspensión de recursos. No atribuir
+70MB aRGB/Painter sinexperimento; asignacionesporframe sonacotadas.

## Devolución de buffers UI — 2026-10-27

La UI entrega al mismo job el Vec RGB que acaba de desplazar. El slot único
alterna Ready(frame) o Recycled(rgb); el lector consume ambos como próxima
asignación de ensamblado. No hay cola/canal adicional ni historial. Un frame
nuevo que ganó la carrera nunca se reemplaza por RGB reciclado; en ese caso
se descarta el buffer devuelto. Se verifican len esperado y capacity máxima
antes de retenerlo. Recycled no publica Ready ni avanza posición/secuencia.

Esta mejora de ownership no acredita ahorro de PSS/CPU ni resuelve la cifra
del footer bajo capturas. Freeze intacto, sin nuevas mediciones. El contrato
continúa parcial: audio/VFR/PTS/caché global/otrasplataformas pendientes.

454Rust141core/58helper/255UI, Clippyworkspace-Dwarnings y releasePASS.
Test modelo100handoffs mantiene tres punteros entre assembler/slot/display;
State comprueba devolución del bitmap desplazado. No equivale a conteo de
asignaciones de toda la app ni medición de ahorro/latencia.
video-play-ui-1793082933186610779PASS funcional con --no-clip:4framebuffers
1000×650, rojo→azul, pause/drain, resume/EOF y close con stream observado vivo.
9PID/starthelpers registrados ausentes/reutilizados tras exit0, Zmismoidentity
noaceptado; no lifetimecoverage. El clip de24readbacks se omite sólo para esta
escena funcional: NO experimento discriminante de memoria, no lectura del pie.
El FAIL1793082885016655179 permanece con sidecar: comando no encontró entrada
y luego mostró carpeta; trigger noestablecido. Repetición activa el aislamiento
de input ambiente ya existente para fixtures, mismo binario/handlers; no prueba
causa de ese fallo ni inputSO. El viejo footer76.6MiB sigue UNVALIDATED.


## Audio opt-in en vídeo — 2026-10-27 (M11)

La vista abre silenciada; Audio/M activa una pista compatible por elección
explícita. Cambiar durante reproducción cancela y recoge el proceso anterior
antes de reemplazarlo. Pausa, seek, cambio de fuente y cierre conservan ese gate.
No crea un segundo player: el mismo ffmpeg entrega RGB por pipe y audio PCM a
Pulse. Comparten el formateador de salida, validación y allowlist de audio
seleccionado; no hay PCM/cache en disco ni nueva dependencia enlazada.

ffprobe inspecciona hasta32 pistas con índices absolutos únicos0..255 dentro de
64KiB de metadata. Se selecciona el primer vídeo no attached_pic y la primera
pista de audio; si ésta no es compatible se muestra capacidad no disponible,
sin saltar silenciosamente a otra. Los dos outputs usan sus índices reales,
no presuponen vídeo0/audio1. Selección manual de pistas sigue pendiente.

Un fallo de fuente marca un estado tipado desde el worker, oculta RGB antes
del ACK y exige nuevo token/watch/probe. Un error de dispositivo con fuente
válida conserva el fotograma verificado y muestra aviso; desactivar audio
permite reintentar reproducción silenciada. No se decide por analizar strings
del error. Aviso visible saneado a512B, diagnóstico completo acotado en State.

El proceso hijo define MALLOC_ARENA_MAX=2; no modifica entorno global. El
contrato funcional AV con Pulse fallaba mmap con arenas predeterminadas y pasó
con2 bajo el MISMO AS768MiB. Ver video-audio-arena-contract.json y FAIL
1793091361377293413 preservado. Esto sólo permite crear el contexto bajo la
contención virtual actual: NO demuestra PSS menor, ahorro de CPU o gate150MB.

470Rust(141core/58helper/271UI), Clippyworkspace/all-targets-Dwarnings y release
PASS. video-play-ui-1793091780741012397 funcionalPASS con fuente sintética8s,
audioFLAC en índice0 y vídeoFFV1 en índice1: silenciado inicial, hot-toggle con
ACK/PID anterior ausente, rojo→azul, pausa/resume/EOF y cierre stream activo.
5framebuffers1000×650; 9PID/start registrados gone/reused, Znoaceptado.
La salida Pulse nativa usa PCM cero: NO audibilidad, capturaPCM, syncA/V ni
seek sample-exact probado. Geometría2x no acredita nitidez nativa2x.

video-play-ui-1793091967178711219 funcionalPASS: socket Pulse privado ausente,
error visible con frame retenido, retry silenciado/frames/EOF/cierre0ACK;
6framebuffers y9loggedhelpersgone. FAIL1793091839762673007 conservado: fixture
configuraba PULSE_SERVER después de Popen; corregido antes, mismo binario.
Regresión audio separado audio-ui-1793092217283190449 PASS9framebuffers/
14loggedhelpersgone/cierreplayeractivo0ACK, mismo SHA app/helper.

Escenas por handlers internos y framebuffer FLTK real, NO inputSO ni clipboard.
Sin clip24readbacks, no experimento de memoria. Recursos/startup NOT_MEASURED
por freeze. P07/P09 continúan parciales: VFR/PTS, codecs grandes/rotación,
plataformas, política global de caché y gate total pendientes. FAIL base y
footer76.6MiB histórico intactos; sin tag0.0.4 ni PASSglobal.


M15 AUDIO-PCM: sourceWAVsample-defined→FLAC880/video12s, monitorprivado confirma
880 alactivar audio después pausa/seek0; defaultmute comprobado después preámbulo.
1793096793305882328PASS2FB/8loggedhelpersgone/root0ACK/sinkpropio retirado/defaultigual.
No A-Vsync/PCMclock/físico ni hot-toggleconPCM acreditado por esta escena (hot-toggle
conzeroPCM yaM11); recursos/startupNOT_MEASUREDfreeze.481Rust/Clippy/release y4Python.


M16 GIF-PREVIEW: perfilGIF8MiB/4Mipix/300s, demuxer/códecgif exclusivo eignore_loop1
en snapshot/stream/miniatura; sin nuevoengine.485Rust/Clippy/releasePASS y
video-play-ui1793098465429205562PASS6FB/10helpersgone/activeclose0ACK/singlepass,
inspector160×90primerframeFD; GIFgrilla/alpha/delayexact/zoom ybudgetpendientes.
FAILharness1793098062479122223 conservado: ClosingSpacepermiteminiatura visible,
no stream. EOFinstrumento ahoraexigenoticeNone, no errorbackend comoEOFnormal.
FreezeNOT_MEASURED, sin ahorroPSS ni nitideznativa2x/inputSO.


M17 VIDEO-SOURCE-CLOSE separa readonlyexitpermission yFinishedACK en snapshot/
stream; childadmission comparte mutex conclose, Helperexigereap.487Rust/Clippy/
release ydosrolesholdsimulado1793099666110053045PASS; normalGIFregression
1793099692852727198PASS6FB/10helpersgone/activecloseACK0. No kernelDreal ni garantía
sobre syscall dentrofaseHelper. Sin joinUI, no falsoDRAINED ni métricasnuevas.

## M18 — encuadre interactivo, 2026-10-27

`VIDEO-ZOOM.md` extiende el mismo visor a vídeo/GIF: zoom1–8×, arrastre,
Ctrl+rueda y+/-/0. Se conserva encuadre entre frames/pausa/seek; sólo tres números
sobreviven al nuevo snapshot, no una copia del raster. Cambiar origen o revisión
lo reinicia. La línea de tiempo conserva prioridad y las flechas buscan5s.
Esto sustituye la limitación anterior de zoom pendiente; precisiónVFR/PTS,
rotación exhaustiva, GIFalpha/disposal/delay y recursos continúan pendientes.
Freeze recursos/startup vigente.

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

## 2026-11-04 — autoplay, mute global y calidad de origen

Ver `VIDEO-AUTOPLAY.md`: reemplaza el inicio pausado/silenciado, el tope
960×540, el escalado bilinear y la salida fija de 12fps descritos arriba.
