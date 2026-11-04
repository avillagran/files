# Vídeo/GIF: cerrar durante sondeo readonly — 0.0.4 (M17)

Snapshot ystream seleccionados reutilizan readonly_gate deaudio/PDF/HTML.
Antes de abrir/verificar el archivo están enReadOnly; la admisión de cada child
comparte mutex concancel/closing y pasa aHelper. Closepuede permitir salir con
readonly todavía sinACK porque esa misma puerta prohíbe todochild posterior.
No join enUI ni hilo nuevo para cada cambio mientras se espera respuesta: la
intención sigue latest-wins ysingle-flight.

Helper NO concede ese permiso: requiere kill/wait delsupervisor. Snapshotvuelve
aReadOnly después delsupervisor común (reap en todos los resultados); entre
ffprobe yffmpeg se vuelve a admitir bajo elgate. Streamvuelve aReadOnly tras
try_waitSome(status) alEOF odespuésdelwait final de cleanup. Sólo entonces pueden
correr checks finales readonly yconceder permiso. WorkerStopped/resultado canal
se mantienen separados: exit_allowed NO es is_drained, ni promesa de reapfalso.

Alcerrar con workerretenido la app emite FILES_PREVIEW_EXIT_ALLOWED drained=false,
no FILES_PREVIEW_DRAINED. Elflagclosing impideprocesos tardíos aunque una llamada
readonly retorne después delpermiso. El puentePDEATH Linux existente conserva
su alcance para children propios. La ruta de miniatura mantiene su supervisor
previo; esta unidad aplica la nueva puerta al preview seleccionado.

**Límite abierto:** una syscall bloqueada durante la faseHelper (por ejemplo
revalidate entreframes mientras el childexiste) sigue requiriendo cancelación/
reap. No se afirma interrupción de kernelD ni terminación inmediata de un grupo
con hiloD; aislamientoI/O, filesystemofflinereal yrecuperación de mount siguen
pendientes. No se permite ocultar helpersvivos detrás de un ACK ficticio.

487Rust(141core/58helper/288UI), Clippyworkspace/all-targets-Dwarnings/releasePASS.
`python3 scripts/video-readonly-close-check.py`: reporte1793099666110053045PASS,
dosapps/framebuffers1000×650, roles snapshot/stream conholdSIMULADO30s enworker
readonly, sinmutexheld. Callbackcompartido→exit0 conreadonly_pending=true/
drained=false/no_future_helper=true, ningúnchildadmit deeserole despuéshold.
Elharnessnoespera30s:nojoin. No esunmontajeFUSE real, D-state, cierreSO ni latencia
medida. Elhook sólo existe conpareja FILES_TEST_PREVIEW=1 + rolfixture, inerte
normalmente. Es simularpendiente, no cancelar syscallreal ni alegar ACK.

Regresiónvideo-play-ui1793099692852727198PASS6FB1000×650, GIFsnapshot/rojoazul/
pause/resumeEOFsinerror/thumbnailinspector yclosecon streamobservadovivo→ACK0;
10loggedPID/startgone/reused strictZfalse, no lifetimecoverage. MismosSHAapp/helper.
Sin inputSO/SuperW/clipboard/native2xsharpness/PSS/CPU/startup: NOT_MEASUREDfreeze.
Compilación intermedia E0061 quedóen target/video-readonly-test.log; finalclean.
