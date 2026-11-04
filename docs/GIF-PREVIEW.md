# GIF bajo demanda — 0.0.4 (parcial)

Space abre un snapshot inicial, sin autoplay. P/Reproducir usa el mismo motor
software de vídeo, pausa/seek/EOF y single-flight; no otro decoder enlazado,
paquete, daemon o caché en disco. Al finalizar conserva el último frame y permite
repetir por elección explícita. Se fuerza `-ignore_loop 1` en ffprobe, snapshot y
stream: el loop infinito del archivo no crea un trabajo permanente.

Perfil compartido entre snapshot/miniatura/stream: GIF regular≤8MiB comprimidos,
4Mi píxeles por frame, duración finita≤300s; salida960×540RGB24 o miniatura160px.
Formato/códec forzados `gif`, whitelist exclusiva (no amplía codecs de otros
contenedores); protocolosfile/pipe, bytecap antesprobe, pixelcap pasado al decoder
yverificado en metadata. FD esperado/efectivo, generación ywatch/reconcile de
medios existentes. Padres symlink/mounts siguen navegaciónSO, no confinamiento,
snapshot atómico ni sandbox. Límites locales/AS768MiB NO equivalen a PSS del árbol.

RGB es opaco, no se promete transparencia/disposal complejo o fidelidad de todos
los GIF. Cadencia12fps es configuración, posición estimada por secuencia, no PTS
ni reproducción exacta de delays. GIF grandes/irregulares y zoom siguen pendientes.
La miniatura del inspector visible es estática de primer frame: al cerrar Space
puede pedir un snapshot; eso NO es reiniciar el stream oculto. Grilla usa el mismo
selector/helper/cache, pero GIF en grilla no se capturó en esta unidad.

485Rust(141core/58helper/286UI), Clippyworkspace/all-targets-Dwarnings/releasePASS.
`python3 scripts/video-play-ui-check.py --gif --no-clip` crea GIF640×360 de8s,
rojo→azul, con extensiónNETSCAPE2.0 y loopcount0 comprobados en bytes.
video-play-ui1793098465429205562 funcionalPASS6framebuffers1000×650:
snapshot pausado/red, play/red→blue/pausa/resumeEOF sin backendnotice, no stream
tras cerrar Space, inspector160×90 rojo con firmaFD comprobada; nuevo play y
cierre con streamPID/startvivo→ACK→exit0,10loggedhelpersgone/reused, Znoaceptado.
No lifetimecoverage, inputSO/clipboard, native2xsharpness, rendimiento ni presupuesto.
Sin24readbacks/clip; recursos/startupNOT_MEASURED porfreeze, no0.0.4tag/PASSglobal.

FAIL1793098062479122223 se conserva: comprobaba ausencia de TODO helper después
ClosePreview, aunque el inspector seguía visible y demandaba miniatura. Log sólo
mostraba probe/frame nuevos, no stream; corregido contar stream y validar por
separado la miniatura visible con snapshotFD/dimensiones/pixel y captura real.
La verificación EOF ahora exige notice=None además de pausa/drain; error backend
no puede contar como final normal. Compilación intermedia importfaltante/unused
Path conservada en target/gif-final-test.log/gif-test.log, final limpio.

## M18 — encuadre interactivo, 2026-10-27

`VIDEO-ZOOM.md` extiende el mismo visor a vídeo/GIF: zoom1–8×, arrastre,
Ctrl+rueda y+/-/0. Se conserva encuadre entre frames/pausa/seek; sólo tres números
sobreviven al nuevo snapshot, no una copia del raster. Cambiar origen o revisión
lo reinicia. La línea de tiempo conserva prioridad y las flechas buscan5s.
Esto sustituye la limitación anterior de zoom pendiente; precisiónVFR/PTS,
rotación exhaustiva, GIFalpha/disposal/delay y recursos continúan pendientes.
Freeze recursos/startup vigente.
