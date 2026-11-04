# Ocultos, menú contextual y ayuda inferior — 0.0.4

Decisión del usuario (2026-10-26): **un único estado global persistido**
`show_hidden`, false si no existe en config.toml. El botón de Espacio,
Preferencias y Ctrl+H modifican exactamente ese estado; no hay overrides por
carpeta/sesión. Se guarda con corners/window_mode mediante actualización tipada
que preserva campos desconocidos y escritura atómica; el cierre espera ACK.
Preferencias marca la opción activa y Espacio cambia Mostrar/Ocultar ocultos.

## Identidad, filtro y selección

En Linux/Unix se comprueba el primer byte de OsStr, sin convertir con pérdida ni
hacer I/O en UI. Grupos, Lista, Espacial y Espacio comparten la política. Fuzzy no
revela ocultos. Al ocultar, se podan lote/foco/anchor; se invalidan hits/hover/drag,
preview y tamaño de inspector. Operaciones sanitizan de nuevo la selección y
rechazan hits ocultos antiguos. Una operación ya confirmada bloquea el toggle
hasta su ACK, para no alterar silenciosamente el lote en curso. Refresh tampoco
restaura selección oculta. Los nombres raw conservan su identidad; sólo etiquetas
pueden mostrar sustituciones UTF8. No se implementa atributo Hidden de Windows.

Espacio reutiliza el informe real completo: no vuelve a escanear al cambiar el
filtro. Áreas se normalizan por bytes completos positivos **visibles**; el total
aparente del informe sigue incluyendo hijos/descendientes ocultos y se etiqueta.
Las carpetas visibles incluyen sus descendientes ocultos. Parciales/desconocidos
siguen fuera del denominador, sin áreas falsas ni porcentajes sobre otro total.

## Espacio y referencia v3

Bloques contiguos con separación fina, color estable por nombre tomado del tema,
contraste calculado para texto y foco/selección; nombre arriba, tamaño abajo.
Header compacto con Volver/Actualizar/Cancelar según estado, botón de ocultos y
resumen real. Más superficie útil y menos texto técnico; sin «0 MB residentes».
Los límites de scan/report y las limitaciones siguen en SPACE-MAP.md.

## Menú contextual

Botón derecho se intercepta antes de move/resize, selección/doble clic y apertura.
Dentro del lote lo conserva; fuera selecciona el target; fondo elimina selección
anterior y sólo ofrece acciones de carpeta. Sólo usa Actions existentes: abrir el
foco, Space, renombrar, copiar, Papelera, Propiedades; fondo crear archivo/carpeta,
ocultos, actualizar. Propiedades múltiple y operaciones ocupadas deshabilitadas.
No hay entradas que simulen funciones pendientes. Menú acotado al viewport,
Nerd Font + etiquetas; Up/Down/Enter, Esc/clic fuera, Shift+F10/tecla Menú.
No se presenta el target enfocado como apertura masiva del lote.

## Texto nativo eliminado y marquee

Se elimina **en origen** `canvas.set_label(...)`; Frame conserva label vacío.
La ayuda se dibuja en su región propia de la bottom bar, entre conteo y FS/free;
errores/guardados/operaciones conservan sus zonas y prioridad. Tooltip contiene
la frase completa. Un reloj de estado sin hilos duerme dos segundos en extremos,
avanza como máximo 25 veces/s y rota sólo tras completar el recorrido. Sin
interacción/foco, oculto o sin overflow no pide repaint. El loop usa daño User1
limitado a los 32px de barra; no introduce un loop global de 60fps.

Wayland no ofrece aquí consulta portable de minimización: el guard usa
shown/visible y foco de seat ya almacenado, conservador (también pausa al salir
el puntero). Minimización/gestos nativos y AT-SPI no están validados. La fixture
puede fijar min=max sólo bajo --ui-smoke + FILES_TEST_FIXED_SIZE para comparar
1000×560; producción Normal no impone flotante ni geometría al compositor.

## Validación y límites de evidencia

`python3 scripts/hidden-context-ui-check.py` crea datos propios en
~/.cache/files-test, no /tmp. Dos instancias verifican default false, los cuatro
modos, raw UTF8 inválido, toggle visible/preferencias/acción de Ctrl+H sincronizados,
persistencia true→reabrir→false, selección podada, áreas/denominador/total,
menú de lote/fondo, apertura de formularios existentes y callback/drain/exit0.
Capturas son framebuffer real FLTK a 1000×560 con tema actual. Los comandos
invocan handlers compartidos; **no son clic derecho ni teclas del SO**. El avance
del marquee usa reloj inyectado; se comparan bytes por encima de la barra.
Las pruebas puras cubren throttle, pausas, no overflow, resize y clocks invertidos.

Se preserva el primer FAIL: smoke_test aún exigía order.len==entries.len e
incluía ocultos en la expectativa. Se corrige la expectativa al conteo visible.
La primera captura funcional sin logging de hijos no acredita cleanup del árbol;
se conserva como evidencia de UI con sidecar y se exige logging/identidades en
la escena final. También se preserva un FAIL de suite por ETXTBSY del ejecutable
fixture de open: las pruebas usan ahora intérprete instalado inmutable y nombre
raw del script como argv, sin cambiar la ruta de producción ni ignorar errores.

Recursos/startup **NOT_MEASURED** por freeze. Footer PSS principal no incluye
Swap/hijos y no aprueba presupuesto. Baseline/arranque/clipboard históricos siguen
fallidos o pendientes; no tag 0.0.4 ni PASS global. El roadmap completo continúa.

Validación final: 388 Rust (126 core / 44 helper / 218 UI), Clippy workspace
all-targets -D warnings y release PASS. Cuatro tests de open se repiten tras
estabilizar su fixture de intérprete, sin cambiar código productivo. Escena
`hidden-context-ui-1793059768952742165.json` PASS: ocho capturas 1000×560,
dos instancias salida0 tras callback/drain, dos identidades helper observadas y
recogidas, unknown field conservado. Marquee con reloj inyectado cambia bytes
sólo en bottom32px; no prueba animación/foco/minimización del compositor.
SHA files101ecb706ed100681e45b21078b160b2d5afba014025d19454dd1586a2298e55,
helper4f7910cf27b1723b25d17a208b9c2287858decfb0e65abf23c534305aad620c0.
