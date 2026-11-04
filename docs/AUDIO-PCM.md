# PCM en sink privado — 0.0.4 (verificación funcional)

`python3 scripts/audio-pcm-ui-check.py` es una escena funcional, no benchmark.
Usa Pulse del usuario por socket Unix local, un null sink de nombre aleatorio
`files_test_*` y su monitor explícito. No cambia dispositivo predeterminado ni
configuración del escritorio. El único override de salida del producto requiere
`FILES_TEST_PREVIEW=1` y `FILES_TEST_AUDIO_SINK=files_test_*`, ASCII64B, sin rutas,
server ni otros nombres. Se pasa `-device` a ffmpeg: si falta el sink, error sin
fallback a altavoces. No opera interfaces gráficas ni necesita foco CUA.

Genera WAV sample-defined:96000muestrascero+480000tono a48kHz/mono/s16, después
FLAC440Hz y vídeoFFV1+FLAC880Hz sin filtros de tiempo;12s se comprueban en metadata
de ambos. Fixtures privadas del cache, nunca archivos de usuario.
El monitor parec48kHz/estéreo/s16 usa pipe RAM acotado1MiB por ventana, sin archivo
PCM. Comprueba señal no nula y energía de tono correspondiente, silencio al pausar
y vídeo silenciado inicial después del preámbulo, luego880Hz al activar audio
tras pausa/seek0 en el mismo child de
vídeo. El sink-input se valida por PID de ese ffmpeg y sink propio antes de
analizar muestras. Señal, muestras y energía son criterios de contenido audio,
NO PSS/CPU/latencia/rendimiento ni prueba de audibilidad física/A-Vsync/PCMclock.

Pausa/callback de cierre usa handlers compartidos del app; framebuffer real no
acredita inputSO. Después verifica root0/StateACK y ausencia de identidades de
helpers registradas (mismoPID/start en estadoZ falla), recoge recorder, revalida
propiedad del módulo antes de retirarlo y confirma default sin cambio. Sólo ese
módulo y esos procesos propios son cleanup; no se modifica ningún sink ajeno.
SIGTERM/errores guardan evidencia parcial y ejecutan finally; no se garantiza
cleanup de fixture si el supervisor es terminado con SIGKILL ni serveroffline.
El producto no crea esos módulos: son infraestructura temporal de prueba.

481Rust(141core/58helper/282UI), Clippyworkspace/all-targets-Dwarnings/releasePASS;
4PythonPurePASS discriminan tonos/silencio/datos incompletos. Captura final
`audio-pcm-ui-1793096793305882328.json` funcionalPASS2FB1000×650: tono440 y880,
pausa/defaultmuteRMS0, rutas porPID en sink propio confirmadas. Callback con
streamPID/startvivo→StateACK→root0;8loggedhelpersgone/reused, Znoaceptado.
Recorderreap/modulepropioausente/defaultsin cambio al terminar. No lifetimecoverage.

Fallos históricos conservados:1793095899920132809 JSONmódulos sinindex impidió
cleanup; módulo único recuperado por tipo+nonce+IDshort y retirada verificada.
1793095941649589586 parenthyprctl usaba firmaobsoleta aunque childcorrecto;
1793096007907294698 faltó Actionaudio explícita.1793096114698595496 mismatch
ruta inicial sin registrar suID: causa ABIERTA, no asegurar que eraPA_INVALID_INDEX
ni emitir afirmación de audibilidad física. Ahora PA_INVALID_INDEX es pendiente,
IDs reales distintos siguenrechazando yse registra target.object/IDprivate.
Zero-route1793096246104965614PASS sólo silenciototal. PCM1793096434340845151PASS
histórico congeneradorlavfi cuya duración10.042667 difería deintención12; sidecar
retira exactitud/preambleclaim. Se usa WAV explícito en reporte final, mismoSHAapp.

Build inicial ENOSPC.pyc ycancelación propia130 preservados; limpieza sólo cache
incrementalrepo yretryCARGO_INCREMENTAL0 pasó. Noatribuir ahorroPSS/CPU ni causa
única de compilaciónlenta por esta comparación; buildsajenos no fueron tocados.
Fallos de capturas/cierre, cleanup o contenido no se convierten en PASS. Recursos/startup
NOT_MEASURED por freeze; límites virtuales de FFmpeg no acreditan techo150MB.
