# Candidato para preview HTML

Investigación del coordinador, 2026-10-26. No es una dependencia adoptada ni una
función implementada; no se ha medido su consumo en files.

## Evidencia primaria

- [litehtml README](https://github.com/litehtml/litehtml/blob/master/README.md):
  motor de layout HTML/CSS, independiente del backend gráfico. Soporte parcial de
  HTML/CSS; no promete equivalencia con un navegador moderno. Strings UTF-8.
- [Licencia](https://github.com/litehtml/litehtml/blob/master/LICENSE): BSD de tres
  cláusulas. El README identifica Gumbo como Apache-2.0; comprobar la versión
  transitiva exacta y su licencia antes de incorporar código.
- [document_container](https://github.com/litehtml/litehtml/blob/master/include/litehtml/document_container.h):
  callbacks de fuentes, texto, imágenes, CSS, enlaces, viewport y recorte. La app
  debe suministrar dibujo y resolución de recursos.

## Propuesta de integración a evaluar

- Aislar parser/layout en helper efímero. Adaptador C++ pequeño con interfaz C
  acotada; compartir sólo comandos de dibujo limitados o bitmap de viewport.
- Fuente Nerd Font predeterminada como el resto de files. No enumerar todo el
  catálogo ni mantener el documento completo en el proceso principal.
- Sin motor JS. El host debe rechazar scripts, navegación automática, recursos
  remotos y protocolos activos; no inferir seguridad sólo del nombre del motor.
- Resolver CSS/imágenes mediante capacidades locales restringidas al directorio
  permitido. Comprobar canonicalización, symlinks, URIs y TOCTOU; bloquear escapes.
- Limitar entrada, nodos, profundidad, CSS, píxeles decodificados, tiempo y salida
  antes de asignaciones grandes. Cancelar y recolectar el helper al cambiar archivo.
- Markdown puede convertirse a HTML acotado y reutilizar el mismo renderer;
  extracción de texto sola no cumple preview renderizado.

## Aceptación necesaria

Fixtures con encabezados, listas, tablas, enlaces e imágenes relativas; CSS y
HTML difíciles deben degradarse de forma visible. Probar bloqueo de red/scripts
y escapes. Captura real, selección/copiar, scroll, cancelación y mediciones del
árbol completo. Comparar latencia y memoria con límites del proyecto antes de
elegirlo. Mantener fuente/versión/checksum reproducibles y avisos de licencia.
