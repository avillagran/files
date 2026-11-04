# Vida de helpers propios — 0.0.4 (Linux)

owned_child::arm añade PR_SET_PDEATHSIG=SIGKILL antes de exec. Comprueba el PID
del padre antes/después de prctl para rechazar la carrera de parent death. El
hook post-fork no asigna memoria ni toma locks/hace I/O y se añade a los hooks
FD/rlimits existentes. La señal pertenece al thread creador en Linux: cada
supervisor mantiene ese trabajador hasta reap.

Se aplica sólo al supervisor común de texto/imagen/Markdown/HTML/PDF/ffprobe/
audio, streaming de vídeo, du e informe Espacio. Las apps abiertas por el usuario
con open conservan su vida independiente; no hay interceptor global de procesos.
Sigue siendo obligatorio cancel/kill/wait normal y ACK antes de reemplazo/salir;
esta señal cubre la terminación abrupta del creador. No crea un daemon/reaper en
producto, no interrumpe kernelD ni asegura a nietos/setuid exec. Windows/macOS
equivalentes pendientes. No es journal/rollback durable de tareas de archivos.

## Pruebas

464Rust141core/58helper/265UI, Clippyworkspace-Dwarnings y releasePASS. Test de
prctl después de exec confirma SIGKILL y PID padre, y que el umask previamente
configurado sigue vigente (pre_exec anterior no sobrescrito).

`helper-crash-1793088449742352360.json` PASS: fixtures propias,20k descendientes
vacíos y dos medios sintéticos; audio/Pulse silencioso, vídeo, du y Espacio. En
cada caso identidadPID/start estaba viva inmediatamente antes de matar SÓLO
PID raíz; no kill de grupo ni señal manual al helper. El subreaper PRIVADO del
script adopta/recoge los cuatro y verifica WTERMSIG=9/identidad desaparecida.
No prueba que PID1/T3 recoja orphans con la misma latencia: producto no instala
subreaper. El kill puede ocurrir después de spawn y antes de progreso útil.

Tres framebuffers845×1071 (tiled normal, no tamaño1080p ni nitidez2x) muestran
ventana propia antes de kill y relanzada después. Hashes de dos medios y conteo
20k descendientes se conservan; no hash de cada descendiente/atributo. Relaunch
reusa config privada, obtiene medios verificados y cierra0conACK. Ninguna operación
de escritura de archivos estaba activa; no prueba recovery de copy/rename.

Regresión normal audioPulse1793088666411517841PASS9FB/14loggedhelpersgone y vídeo
1793089019986522162PASS4FB/9loggedhelpersgone, play/pause/seek/EOF/cierreactivo.
No se rescata un FAIL por cleanup manual; el script sólo lo usa tras fallo y no
lo convierte enPASS. InputSO/audibilidad/PCM/kernelD/offline/grandchildren siguen
sin aceptación. Recursos/startupNOT_MEASURED porfreeze; no se usa footerPSS ni
se modifica ideal50MB/techo150MB.
