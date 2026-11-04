# Ventana y menús (0.0.4)

Prioridad explícita del usuario, 2026-10-26. Sustituye la ventana Linux sin borde
forzada: **Normal** es ahora la preferencia por defecto y usa una ventana estándar
con decoraciones FLTK/libdecor, administrada por el compositor. No se solicita
flotante, panel, overlay, always-on-top ni una regla global. Un compositor con
layout tiled puede decidir que no procede mover/redimensionar esa ventana.

Preferencias (botón Nerd Font engranaje, Ctrl+,) y Acciones (botón Nerd Font rayo,
F10) son menús distintos y excluyentes. Sus nombres semánticos y tooltips son
«Preferencias» y «Acciones». El canvas tiene una etiqueta con ambos atajos; esto
no acredita un árbol AT-SPI/screen reader nativo, pendiente en accesibilidad.
Acciones sólo contiene abrir/preview/copiar/renombrar/crear/deshacer/rehacer.
Preferencias contiene modo de ventana, bordes de interfaz y tema; las vistas
siguen accesibles en el header y Ctrl+1/2/3. Esc cierra cualquiera de los menús.

Modos soportados por FLTK:

| Clave config.toml | Etiqueta | Comportamiento solicitado |
|---|---|---|
| normal | Normal | Decorada, sin maximizar ni pantalla completa; compositor decide placement |
| fullscreen | Pantalla completa | Pantalla completa nativa; conserva Preferencias para salir |

Sólo una elección explícita modifica `window_mode`; redimensionar/reubicar desde
el compositor no reescribe esa elección. Modo y `corners` se guardan juntos en
una única escritura atómica en el trabajador existente, conservando campos/tablas
desconocidos. ACK por revisión, reintento tras error y drenaje al cierre. Cambios
mientras hay escritura pendiente coalescen al último snapshot de preferencias.
El tema sigue independiente, al igual que el factor visual de bordes.

## Header y grips

Normal tiene cuatro targets interiores de 24×24px y mínimo de ventana 700×440.
Los controles del header y las fichas/edición de ruta siempre ganan el hit test.
El resto libre de sus primeros 58px solicita movimiento; el contenido de archivos
nunca es zona de ventana. Diagonales NW/SE y NE/SW para grips; `grab`/`grabbing`
del tema de cursor Wayland si existen, con fallback Hand/Move de FLTK. Overlays,
menús, diálogo, selección espacial en arrastre o cierre bloquean estos gestos.

El puente C++ propio usa las estructuras reales de **FLTK fijado 1.5.23/API1.4.5,
ABI1.4.0**; no adivina offsets ni modifica registry. Comprueba backend Wayland,
ventana mostrada propia, FL_PUSH izquierdo, belowmouse de esa ventana, foco de
puntero/seat/serial actuales antes de `libdecor_frame_move/resize`. No conserva
ni fabrica seriales. Limpia pushed/estado como el driver FLTK tras la solicitud;
los configure nativos aplican el resize y anclaje de esquina opuesta. No hay bucle
de geometría ni fallback Hyprland. El compositor coalesce los configure, sin
transportar el timer QML50ms del panel de referencia.

Los cursores comprueban foco propio antes de tocar el seat. Un retorno exitoso
significa solicitud enviada, **no** que el compositor la haya aceptado. En otros
backends el gesto custom informa capacidad no disponible; los modos y decoraciones
estándar siguen vía FLTK. Movimiento/resize nativos fuera de Linux pendientes.

## Compilar el adaptador

`cc` MIT/Apache ya presente en lockfile pasa a build-dependency directa. fltk-sys
idéntico también es build-dependency sólo para ordenar generación de headers;
no nuevo backend/runtime. FLTK no exporta metadata Cargo de include/source:
`build.rs` localiza su fuente de versión fijada ya descargada por Cargo y sus
headers generados en este perfil; rechaza fuentes ambiguas o configuraciones
que difieran en config.h **o** FL/fl_config.h (sólo se excluyen los dos strings
FLTK_DATADIR/FLTK_DOCDIR, que cambian con la ruta del artefacto host/target). Override para vendor/entornos
especiales: `FILES_FLTK_SOURCE_DIR` (raíz cfltk/fltk) y `FILES_FLTK_CONFIG_DIR`
(raíz generada con ambos headers). No descarga ni edita dependencias.
Este acoplamiento privado debe revisarse al actualizar FLTK; no se declara
compatibilidad con un FLTK instalado arbitrario ni cross-build Linux validado.

## Evidencia

`scripts/window-ui-check.py` usa targets/handlers internos y framebuffer real;
lee hyprctl clients para observar modos, nunca lo usa para mover/flotar/resize.
Comprueba default, persistencia entre procesos, campos desconocidos, exclusión
de menús, zonas de controles/header/cuatro esquinas, cierre normal tras ACK.
La fixture no llama el puente de gestos: input SO/move/resize/cursores reales
siguen pendientes por controlador CUA sin ventanas. Recursos/startup NOT_MEASURED
por freeze; no se extrapola el pie PSS principal de capturas a árbol/presupuesto.

La primera escena falló al esperar Maximizada: la regla real de Omarchy
`default/hypr/windows.lua` suprime eventos maximize para todas las apps.
El adaptador FLTK marcaba maximize_active, pero hyprctl seguía fullscreen=0.
Ese estado cliente no acredita el modo compositor. Se retiró el modo extra
antes de la entrega; sólo se exponen Normal/Pantalla completa verificados.
No se cambió la regla ni se usó un dispatcher sobre la ventana activa. Se
conserva window-ui-failure-1793041926965059030.json y sus capturas.

Aceptación funcional: window-ui-1793042474907145360.json PASS en tres procesos,
unknown fields preservados y salidas0 ACK. Capturas finales: window-preferences-
1793042473963081012.png, window-actions-1793042474075297111.png, window-normal-
restored-1793042474691886649.png. Dimensiones cliente845×1061/1728×1117, no
decoración exterior capturada ni inputSO. Fallo fixtureclosing anterior preservado
en window-ui-failure-1793042207114132046.json y corregido sin relajar lifecycle.
