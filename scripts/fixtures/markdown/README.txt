Fixture P04; no constituye evidencia de aceptación por sí sola.

Abrir overview.md en Space con ventana de 1000x650 y una ventana más estrecha.
Revisar al menos una captura superior y otra tras scroll; registrar escala real.

Verificar:
- Título, énfasis, tachado y código diferenciados; Markdown no se muestra como fuente.
- Tabla con tres columnas/alineaciones, sin solapamiento con el siguiente bloque.
- Lista ordenada empieza en 3; lista anidada y tareas distinguen su estado.
- Selección de la frase Unicode copia texto visible, sin delimitadores Markdown.
  La última a lleva U+0301; el emoji incluye U+200D. No cortar UTF8 ni perder marcas.
- Resize refluye el documento y mantiene selección lógica. Scroll alcanza el final.
- Código conserva TAB y texto literal; línea larga no desborda el panel.
- palette.png es un recurso local 96x64: tres franjas RGB, sin red.
- guide.md abre localmente; retorno funciona. Fragmentos requieren navegación real.
- Destinos externos/traversal no generan carga automática ni aperturas implícitas.
- HTML incrustado se dibuja literal según política actual; no ejecutar ni tratar
  como implementación de la vista HTML renderizada P05.
- Esc/cambio rápido/cierre pendiente drenan jobs y liberan recursos.

Distinguir eventos internos/framebuffer de input SO/clipboard real. Medir recursos
por separado de capturas/build, con PID/start de helpers, Swap y límites vigentes.
Imágenes embebidas/enlaces/fragmentos que aún no estén implementados se reportan
pendientes; no convertir el texto alternativo en prueba de imagen renderizada.
