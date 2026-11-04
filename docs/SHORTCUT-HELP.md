# Ayuda de atajos — 0.0.4

F1 o Preferencias → Ayuda de atajos abre un panel propio de Painter con38 filas
estáticas. Navegación, vistas, selección/operaciones y preview/configuración;
sólo atajos implementados, sin anunciar SFTP, F6/mover u otras funciones futuras.
Acciones sigue siendo operaciones sobre archivos, separada de Preferencias.
Ayuda no ejecuta atajos listados al hacer clic en sus textos.

Flechas/rueda y PageUp/Down desplazan; Home/End llegan a extremos. Cerrar/Esc/F1
vuelve a la UI anterior conservando ruta/selección. Help elimina hits subyacentes,
contextual y gestos de ventana no se activan; filas se dibujan sólo dentro del
viewport. Reflow acota página al resize. No I/O, helpers, timers o caché de datos.
No reemplaza accesibilidad AT-SPI/IME/i18n, ni completa todos F1–F10/terminal aquí.
Clipboard sigue teniendo la limitación de foco/serial de validación anterior.

401Rust129/44/228, Clippyworkspace/all-targets -Dwarnings y release PASS.
help-ui1793061761953895411 funcionalPASS: botón visible Preferencias y acciónF1,
páginas/extremos, cuatro framebuffers1000×560/800×500 por constraint de fixture,
hits archivos ausentes, ruta/selección preservadas, cerrar/reabrir/Esc ycallback
con ayuda abierta→drain→exit0. Método: handlers internos/framebuffer real, no
teclas/rueda del SO. No helpers de preview/du/map solicitados: lifecycle raíz.
Recursos/startupNOT_MEASURED por freeze. Sin tag0.0.4/PASSglobal.
