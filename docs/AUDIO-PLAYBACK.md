# Reproducción de audio — 0.0.4 (parcial)

La salida actual ffmpeg/Pulse está verificada funcionalmente en Linux.
Recursos/startup NOT_MEASURED; audibilidad y posición PCM no validadas.

## Prototipo conservado (histórico)

Prototipo ffplay: controles reales P/Reproducir/Pausar/seek, sin autoplay,
sourceFD esperado y gate/helper único de AUDIO-PREVIEW. Documento/selección
conservados al buscar; cancelación/ACK antes de reemplazo, ocultar libera child.
Common supervisor tiene clock de spawn y stderr estricto8KiB/diagnóstico4KiB:
ffplay puede devolver exit0 ante audio-open-failed (fixture SDL disk con parent
privado inexistente, sin generar audio en disco). No se acepta ese estado comoEOF.

463Rust141/58/264, Clippyworkspace-Dwarnings/releasePASS.
UI1793085963942149325PASS sólo SDL dummy/silencio:14loggedhelpersgone, play/pause/
seek requested/EOF/delete/callback activo. NO prueba audio audible ni posiciónPCM.
Sistema nativo1793085882916399311FAIL: play/pause/seek llegan, EOFtrasseek vence
timeout. La diferencia con dummy no identifica por sí sola la causa en SDL/driver.
NO se instala esta unidad ni se presenta como reproducción nativa completa.
Instalado sigue metadataFDca8e41fd10be (1c06920).

FAIL1793085547229903243porfirmaHyprlandheredada obsoleta conservado+sidecar;
fixture selecciona instancia única para suWAYLAND_DISPLAY, sin foco/configglobal.
FAIL1793085633746085621argv -infbuf0trataba0comoarchivo; corregido-noinfbuf.
Ambos históricos/imagenes inmutables.

Siguiente adaptación usa salida Pulse del ffmpeg del sistema ya requerido para
vídeo, conservando supervisor/FD/controles. Sin nuevo decoder enlazado, daemon,
archivo cache ni paquete instalado. Recursos/startupNOT_MEASUREDfreeze.

## Salida actual: ffmpeg → Pulse

La producción conserva el controlador y reemplaza sólo la salida del prototipo
por un ffmpeg directo, con Pulse del SO (compatible con PipeWire-Pulse en Linux).
No usa ffplay/SDL ni crea otra ventana. Sin daemon propio, dependencia Cargo
nueva, shell ni cache PCM en disco. Fuente FD pinneado/esperado, demuxer/códec
permitidos, MOV sin referencias externas, salida forzada Pulse, estéreo s16/48kHz
y volumen0.5. No acepta destinos de archivo arbitrarios. El null muxer existe
sólo con dos flags explícitos de fixture; no es fallback de producto.

Pausa/seek cancela y recoge antes de reiniciar desde posición solicitada; no
son control de un reloj PCM ni seek sample-exact. Timeline no roba selección.
Pausa/EOF conserva metadatos, error de dispositivo permite reintentar; fuente
inválida destruye los metadatos previos. Cerrar/ocultar cancela, no autoplay al
volver. El reloj UI empieza al spawn y es estimado; no acredita sonido audible.

Se conserva stderr error-only8KiB y texto saneado4KiB, drena exceso y exigeEOF
en ambos pipes más reap. Ningún exit0 con diagnóstico se presenta como éxito.
ChildCPU2×restante+10s(hasta1día), wallrestante+10s, entrada8GiB, max_alloc32MiB,
AS virtual768MiB como vídeo. Probe funcional silencioso512MiB falla creando
contexto Pulse/mmap y768MiB acepta; eso NO es PSS ni modifica ideal50MB/techo150MB
del árbol. Sin nuevos PSS/Swap/CPU/startup porfreeze; aceptación de recursos
pendiente. No aceptar una reproducción larga que exceda el techo cuando se mida.

Waveform, carátulas, volumen UI, posiciónPCM/latencia, otros formatos y SO,
crash/kernelD/offline y P09global continúan pendientes.

## Evidencia de la salida actual

463Rust141core/58helper/264UI, Clippyworkspace-Dwarnings/releasePASS.
`audio-ui-1793086873981565834.json` PASS con salida Pulse real del sistema y
fuente PCM nula de6s:9framebuffers1000×650, metadatos/UTF8/recorte/errores/rapid,
P/play-pausa solicitado por Action, seek por handler de puntero sin robar la
selección, reanudar y EOF después del seek. No hay eventos de teclado/punteroSO,
no grabación audible ni lectura de muestras a la salida. Posición UI estimada.

Ocultar recoge player y no reinicia; borrar durante playback oculta metadatos y
cancela/recoge; restaurar obtiene pruebaFD nueva y permite reproducir. Callback
FLTK propio con PID/start deplayer observado vivo→ACK→exit0;14identidades
registradas ausentes/reutilizadas, Zmismoidentity noaceptado. No cobertura de
lifetimes completos ni prueba de Super+W, kernelD/offline o nitidez nativa2x.

FAILPulse1793086437393143500 mantiene OOM/mmap con AS512; contrato silencioso
512/768 y sidecar registran cambio virtual, no ampliación del presupuesto real.
FAILnativoffplay yPASSdummy del prototipo quedan inmutables como antecedentes;
no se reutilizan para aceptar el backend actual. No nueva medición de memoria,
CPU, arranque ni GPU; todos los fallos históricos del proyecto permanecen.


2026-10-27 M11: validación Info/codec y construcción de salida Pulse compartidas
con vídeo opt-in, sin duplicar player. Regresión standalone
 audio-ui-1793092217283190449 PASS9FB1000×650/14loggedhelpersgone strictZfalse/
callbackplayeractivo0ACK, SHA app52cde7a05957/helper187c939eb981. Fuente silenciosa:
no audibilidad/PCM/recursos. Ver VIDEO-PREVIEW para el gate de dos outputs.


M15 2026-10-27: AUDIO-PCM añade evidencia de señal real (no sólo zeroPCM):
1793096793305882328PASS sinkprivado localUnix/440Hz, pausaRMS0 yvídeo880optin,
2FB/8loggedhelpersgone/callbackstreamvivo0ACK/recorderreap/moduleabsent/defaultigual.
481Rust+4Python+Clippy/releasePASS. Physicalspeakers/A-Vsync/PCMclock no verificados;
fallos históricos incluida rutaIDinicialdesconocida preservados. FreezeNOT_MEASURED.
