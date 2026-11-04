# Rutas Unix opacas en IPC y arranque — 0.0.4 (2026-10-27)

Las rutas de Request conservan strings JSON para Unicode válido. Si un PathBuf
Unix no es Unicode, se serializa como array de bytes y se reconstruye sin pérdida.
No se añade otro motor ni comando de imagen: PreviewImageChecked reutiliza este
adaptador y sigue comprobando expected_revision contra el FD real/Response.source.
Root y relative usan la misma representación, sin normalizar ni decodificar URLs;
las políticas openat/traversal de cada comando siguen aplicándose después.

Máximo4096B por ruta, NUL rechazado, tamaño comprobado antes de construir PathBuf
o crecer el buffer propio de bytes más allá del límite. La línea completa sigue
acotada64KiB antes de serde. Esto no afirma que el parser JSON no use allocations
intermedias. JSON strings antiguos no cambian y los helpers siguen admitiéndolos.
Otros SO reciben error explícito para ruta no Unicode hasta tener adaptador.
No se promete transporte de OsString en todas las respuestas/configs del proyecto.

La CLI usa args_os y acepta `--` como fin de opciones; una carpeta con bytes noUTF8
ya no causa panic en std::env::args. Valores de flags siguen siendo texto/números.
Labels pueden mostrar el carácter de reemplazo para bytes sin representación
Unicode; ese texto nunca reconstruye la ruta de operaciones. Editar manualmente
rutas opacas en omnibar y las demás superficies raw de UI permanecen por validar.

427Rust136core/46helper/245UI, Clippyworkspacealltargets-Dwarnings y release limpios.
Pruebas: ruta Unicode conserva string, ruta raw conserva bytes, root/relative raw,
no URLdecode/normalización, rechazo NUL/byte inválido/límite y SpaceMap request raw
conservando id+path. El primer test de SpaceMap aún esperaba rechazo de raw y falló;
se actualizó para exigir ida/vuelta exacta, no se suprimió el límite de longitud.

`evidence/raw-path/raw-path-ui-1793071898429792393.json` funcionalPASS: directorio
argv con0xFE y PNG con0xFF, Grilla/inspector y Space completos con patrón morado.
Response.source dev/ino/bytes coincide con stat del fixture; todos los RGB del
bitmap se comprueban. Dos capturas reales1000×650, callback→drain→exit0 y3identidades
PID/starttime de helpers ya no presentes. Handlers internos y framebuffer, NO
inputSO. Informe inicial1677948673706 con una captura sigue intacto; segunda
corrida añade la evidencia de Grilla requerida, sin medir recursos/startup.

serde_json sólo se añadió como dev-dependency de core, ya existente en lock/UI/
helper, sin nuevo paquete/licencia. Core sigue sin UI. Transporte no sustituye
la firma FD pendiente de recursos incrustados; tampoco prueba GUI raw de todos
los comandos ni Windows/macOS. WidgetTracker original sigue causa abierta con
protección verificada; FAIL globales históricos intactos. Recursos/startup
NOT_MEASURED por freeze; sin tag0.0.4/PASSglobal.


8003944 instalado conbackup+hash: files3b4f6e787172e46ab260bd029491caa63f6c973d84839b5d97cb4df59a3aa08c;
helper7222619952a58c93d2361568e8aa0beea8bee4ee2f3477cf3307720a4d9fac2d;
HTMLa5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
installed-checkpoint1793072387409896590. Próximo: identidad FD de recursos
incrustados; las rutas ya preservan bytes pero eso no certifica qué FD se leyó.
