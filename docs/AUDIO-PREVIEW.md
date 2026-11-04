# Metadatos de audio en Space

0.0.4, parcial. Space sobre FLAC, WAV y otros formatos admitidos muestra título,
autor, álbum, duración, códec, frecuencia, canales y tasa de bits declarada.
La vista permite seleccionar el texto y reproducir/pausar/buscar con ffmpeg/Pulse
a demanda; ver AUDIO-PLAYBACK para evidencia y límites. El clipboard Wayland
sigue pendiente de validación con foco.

## Proceso y límites

Usa ffprobe opcional del sistema como un hijo directo. No se enlaza ni se
distribuye su biblioteca, no hay daemon ni decodificador en la UI. El worker
abre un archivo regular final con O_NOFOLLOW/O_NONBLOCK y conserva su FD
a través de exec. Ruta absoluta de hasta 4096 B, fuente de hasta 8 GiB.
Los padres y mounts siguen la navegación normal del SO; no es un sandbox ni
un snapshot si el contenido cambia in-place.

Demuxers forzados por extensión: mp3, wav, flac, ogg/opus, mov para m4a, aac
y aiff. Sólo protocolos file/pipe y códecs de audio permitidos; referencias
externas MOV desactivadas. Metadatos JSON hasta 64 KiB antes de parsear;
duración finita hasta siete días, frecuencia hasta 384 kHz y hasta 32 canales.

Cada etiqueta se recorta a 2048 B en límite UTF-8 y sanea controles/bidi antes
de insertar una fila. Documento hasta 16 KiB y 256 líneas, con recorte visible.
No se interpreta HTML ni se abre una URL de las etiquetas. Una etiqueta que
exceda el wire completo puede causar rechazo antes de producir la vista.

Timeout de 5 s, CPU de 3 s, AS virtual de 256 MiB y FD acotados. Estos límites
no prueban PSS ni autorizan exceder el techo de 150 MB. State cancela y recoge
el job anterior antes de lanzar el último destino. Cierre distingue fase con
hijo (kill/reap obligatorio) de I/O sólo lectura aún sin ACK: ver actualización
siguiente. El timeout del supervisor no interrumpe una syscall FS atascada.

## Evidencia y pendientes

276 tests Rust (73 core, 33 helper, 170 UI), Clippy workspace all-targets con
-D warnings y release limpios. audio-ui-1793039367932469197.json pasa la escena
funcional: FLAC/WAV propios, campos reales y selección Unicode, etiqueta larga
recortada, cambios rápidos, archivo inválido/symlink y cierre normal tras ACK.
Método: handlers internos y framebuffer de una ventana real, sin input del SO.
Cierre antes del nuevo spawn registrado no prueba cancelación de ffprobe vivo.

Recursos y arranque NOT_MEASURED por la fase explícita del usuario. No se
extrapolan presupuestos de otros previews. Reproducción, waveform, carátulas,
otros formatos y plataformas continúan pendientes; P08 y P09 no están completos.

## Identidad y publicación — 2026-10-27

El watcher existente de medios cubre ahora audio seleccionado, además de imagen
y vídeo. Adjunta directorio antes de verificar, token/epoch invalidan callbacks
y resultados viejos, y reconciliación visible periódica detecta eventos perdidos.
No hay watcher por archivo ni I/O en draw/UI. Audio queda fuera de thumbnails:
se separa el predicado de imagen/vídeo del conjunto de fuentes vigiladas.

AudioJob exige expected Revision; abre el FD regular mediante media_source::open,
compara antes de ffprobe, hereda ese FD, y revalida descriptor y nombre después
del parseo. Devuelve la Revision efectiva junto al documento. State acepta sólo
con generación activa y firma esperada coincidentes; Ready conserva esa firma.
No se atribuye identidad a una ruta comprobada en otra apertura.

Reemplazo/modificación/borrado/rename ocultan metadatos y cancelan el job antes de
publicar de nuevo. No es hash ni snapshot atómico, ni confinamiento total:
O_NOFOLLOW final no impide symlinks en padres/mounts. Recuperación del padre usa
watcher/reprobe; no se promete invalidación instantánea sin evento ni cuando la
app está apagada. No hay caché de audio persistente ni temporales de producto.

AudioJob tiene gate ReadOnly/Helper/Stopped. Cierre marca closing bajo el mismo
mutex que admite spawn; permiso para abandonar ReadOnly impide cualquier hijo
futuro, mientras Helper exige reap. State.exit_allowed no altera is_drained:
si falta ACK se emite PREVIEW_EXIT_ALLOWED drained=false, nunca PREVIEW_DRAINED.
No join de syscall FS en UI; no se promete terminación instantánea de kernelD.
Tests controlados de fase/controller no sustituyen un montaje realmente atascado,
que sigue pendiente. Cambiar selección mantiene single-flight hasta ACK, sin
acumular threads o lanzar un reemplazo mientras el anterior está bloqueado.

458Rust141/58/259, Clippyworkspace-Dwarnings y releasePASS. Pruebas deterministas
A→replaceB antes de abrir, nombre cambiado después de probe, ReadyA tardío y
ReadyA bajo generación B, close readonly sin falso ACK y gate que rechaza spawn.

`audio-ui-1793083931390323451.json` PASS funcional con callbacks notify suprimidos:
8framebuffers1000×650; FLAC/WAV/Unicode/selección/límites/errores/rapid y fuente
same path+size+mtime reemplazada, modificación con mtime restaurado, delete/recreate,
rename away/back y padre reemplazado. Firma efectiva se compara con stat del
fixture fuera de UI; al borrar desaparecen los metadatos del framebuffer.
Preview cerrado no lanza helpers en la espera de reconciliación. Rootexit0/ACK,
11PID/start helpers registrados ausentes/reutilizados, Zmismoidentity rechazado;
no cobertura de lifetimes ni cancelación de ffprobe vivo demostrada al cierre.
Sólo handlers internos/framebuffer; no inputSO ni prueba de nitidez nativa2x.
Recursos/startupNOT_MEASURED por freeze; footer no usado. Audio sigue SIN
reproducción, y P08/P09global/otrosSO/clipboard y fallosbase siguen pendientes.
