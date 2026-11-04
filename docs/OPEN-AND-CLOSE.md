# Apertura y cierre de ventana

Decisión explícita 2026-10-26: los archivos se delegan a `open` del sistema en
Omarchy/macOS. `Command::new("open").arg(absolute_path)` conserva el argumento
lossless; ninguna shell, intérprete de ruta o fallback `xdg-open`/opener. Directorios
reales siguen navegando dentro de files; un symlink se entrega a `open`, sin asumir
que su destino sea carpeta en el hilo UI. Ruta absoluta ≤4096bytes/sin NUL se
comprueba antes de construir el trabajo; sólo una apertura pendiente. Spawn/wait
van en un worker y errores son visibles. Exit0 acredita salida del launcher,
no que haya aparecido un visor. Hoy `open` no está en PATH en este host: capacidad
no disponible y alternativa **PENDIENTE**, sin instalar ni emular el comando.

El launcher no es un preview/helper administrado: no se mata la app externa al
cerrar files y no se espera su vida para cerrar la ventana. Un `open` personalizado
que no salga puede retener ese único worker hasta que termine el proceso files;
no se acumulan intentos ni se promete timeout. Tests usan programa fixture propio,
no alteran PATH global ni ejecutan datos como comandos.

## Super+W: hallazgos y límites

La copia instalada `~/.local/bin/files` observada conserva SHA256
`c91b9cf20a7f1fa4b48b2e9cbdd6c5b8ebb0302efd9210f7da207577fe295972`,
igual al checkpoint de carpetas anterior a `3e6cf88`. Esa versión fuerza
`set_border(false)`. En FLTK1.5.23 fijado, `Fl_Wayland_Window_Driver.cxx`1136–1138
registra un callback vacío `xdg_toplevel_close` para la ventana UNFRAMED. Es un
mecanismo estático concreto que puede explicar ignorar cierre del compositor;
no es una reproducción de la pulsación reportada. El código actual usa borde
normal gestionado por compositor, cuya ruta libdecor `handle_close`1024–1037
entrega FL_CLOSE/callback. No se parcheó registry ni configuración global.

`hyprctl binds -j` efectivo muestra Super+W press/release `__lua1164/1165` en la
observación guardada, no el dispatcher stock descrito por texto. omarchy-x-panel
aplica su propio binding periódico: su detector compara `function:` mientras este
Hyprland presenta `__lua`, posible carrera de rebind NO reproducida como causa.
No se cambiaron esos binds ni el plugin. Sólo se observa la ventana/PID propio.

`FILES_CLOSE_TRACE=1` registra callback FLTK → solicitud → estados de drain →
DRAINED. Espera mutaciones/rollback/ACK, previews, du, cola de copia y guardados
atómicos antes de ocultar; errores/rescate o fallos de persistencia permanecen
visibles para resolución explícita. Una fixture `win.do_callback()` acredita la
segunda mitad de la ruta, **no evento compositor ni Super+W**. La ruta real de
entrada sigue pendiente si CUA no enumera la ventana. La skill jev-computer-use
prohíbe improvisar otro control de pantalla cuando falta target; no se sustituyó
por teclas globales, ventana activa ni dispatcher sobre ventanas ajenas.

## Evidencia

345 tests Rust (115core/37helper/193UI), Clippyworkspace/alltargets-Dwarnings y
release PASS. Última regresión específica open_job4PASS. Escena
open-close-ui-1793056978667673268.json funcional PASS tres procesos: host ausente,
launcher fixture con argv absoluto literal Unicode/espacios/$() y symlink, exit7
visible, directorio navega sin launcher, cada salida0 por callback/request/drain.
Ningún visor real acreditado. Capturas anteriores conservaron PASS de disco pero
FAIL visual por recorte del motivo; sidecar preservado, resumen ahora muestra
«open ausente · alternativa PENDIENTE» y «open falló · código7» completos.

CUA volvió a devolver0 ventanas mientras hyprctl observaba el PID/address propio.
Super+W/compositor sigue PENDING_OS_INPUT, sin bypass ni cambios de binds. SHA app
`81c53b8add62fa941fac3fc9f11f30f9af8441e5d31f6a885dc1db3ad9f186b7`. Freeze recursos/startup NOT_MEASURED;
no usar footer PSS como presupuesto del árbol.

Instalado bajo autorización explícita en ~/.local/bin: checkpoint bdaa5f3,
files81c53b8add62…, helper64f7133b20ab…, HTMLa5c37981b28b…; hashes completos
verificados en installed-checkpoint-1793057085303837728.json, backup de los cinco
artefactos/metadatos. HTML recipe/source stamp validado antes de staging. Ninguna
ventana ajena o anterior cerrada, no instalación global. Esta actualización retira
el ejecutable instalado borderless viejo; no convierte Super+W en prueba PASS.

## Shared terminal request slot (2026-11-01)

Terminal requests share the bounded existing external-app launcher with `open`;
file argv/no-fallback policy and FILES_OPEN_OUTCOME trace remain unchanged.
Terminal has its own typed outcome trace/status notice and leaves operation rescue
reports untouched. See TERMINAL-HERE for the ASCII broker/raw-directory bridge,
explicit service ownership and validation boundaries. No viewer fallback added.
