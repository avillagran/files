Fixture adicional; no es prueba funcional ni de memoria por sí sola.

Empezar abriendo ../contextual-links.md desde el listado principal, NO abrir esta
guía directamente: la raíz autorizada es el directorio del documento inicial.
Mantener esa raíz al navegar a contextual/guide.md y nested/detail.md.

Esperado:
- Retornos ../ y ../../ válidos, sin salir de la raíz original.
- Fragmentos #regreso/#detalles saltan al título correspondiente.
- Enlaces outside.md rechazan el escape antes de solicitar un helper.
- La imagen corresponde al mismo palette.png 96x64 de tres franjas RGB.
- Cambiar rápidamente entre documentos no permite publicar bitmaps obsoletos.
- Medir por separado del framebuffer; no contar alt como imagen renderizada.
- La política de symlinks del helper sigue vigente tras normalización contextual.
