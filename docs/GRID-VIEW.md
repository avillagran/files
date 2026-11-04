# Grilla de iconos — 0.0.4

Botón Nerd Grilla o Ctrl+5. Distribución automática de archivos/carpetas, distinta
de Espacial (posiciones manuales persistidas) y Espacio (áreas de tamaños).
Sólo se dibujan las filas que intersectan el viewport; no crea posiciones ni
widgets ni un bitmap por entrada. El índice ordenado/filtro global se reutiliza,
con stat perezoso de visibles y tamaños reales cuando llegan. Iconos son Nerd
Font por tipo, **no miniaturas**: P09/thumbnails continúan pendientes.

- Botones −/+ y Ctrl+rueda/Ctrl+±: zoom65–180%, independiente del layout Espacial.
  No escribe posiciones ni zoom Espacial. Zoom de Grilla permanece durante la
  instancia; persistencia por carpeta/sesión sigue pendiente V12.
- Flechas recorren columnas; Shift extiende rango, PageUp/Down por filas,
  Home/End primero/último y reveal por fila. Clic/Ctrl/Shift comparten selección,
  Enter/doble clic navega/abre por acción existente; contextual compartido.
- Al resize se recalculan columnas/rango visible sin duplicar nombres ni retener
  un objeto por celda. Nombres largos se recortan visualmente; identidad raw
  permanece en el listado. Ocultos siguen el MISMO global persistido.
- El cambio de vista invalida hits/hover/drag/visible antes de la siguiente pintura.
  Header compacto conserva controles separados; tooltips/atajos nombran la Grilla.

Pruebas puras ejercitan rango visible100k/extremos/resize/zoom y painter recorder
sobre1000 entradas, selección/rango/reveal/hidden, sin tocar disposición Espacial.
No equivalen a presupuesto app100k. Recurso/startup NOT_MEASURED por freeze.
Método de la escena: handlers internos/hits visibles, request propio de tamaño
bajo flag test, captura framebuffer y callback real FLTK; NO eventosSO.

Fallos preservados: fixture du ejecutable recién escrito produjo ETXTBSY en suite
paralela. La prueba usa intérprete /bin/sh instalado inmutable y supervisor real;
producción mantiene Command(helper). Captura inicial mostró clipping de tinta
Nerd mayor que su advance monoespaciado: región de icono ahora se recorta a celda,
no al advance. Resize por fullscreen fue rechazado por max=min de fixture fija;
se usa tamaño explícito de la propia fixture y se verifica framebuffer800×500.
Otro FAIL encontró READY inicial previo al repaint de smoke_test: ahora exige
paint_revision posterior a la invalidación de hits, sin aceptar estado viejo.

Validación final: 391 Rust (126core/44helper/221UI), Clippy workspace/all-targets
-Dwarnings y release PASS. `grid-ui-1793060477438092576.json` funcional PASS:
162 nombres visibles + dos ocultos, ocho capturas reales, zoom100→132%, rango,
último elemento revelado, contextual, carpeta/volver, toggle raw, resize propio
800×500→1000×560 y callback/drain/exit0. No se pidió helper de preview/du/treemap;
se prueba lifecycle del principal, no cleanup de un árbol con hijos hipotéticos.
SHA files e8c61f3cbf1659387d7c73b5fecd6e80d8ccc0cb8284765f240cc8a0eea3c836;
helper4f7910cf27b1723b25d17a208b9c2287858decfb0e65abf23c534305aad620c0.
No miniaturas, no recurso/startup nuevo, sin tag0.0.4/PASSglobal.
