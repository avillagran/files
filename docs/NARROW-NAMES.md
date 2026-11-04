# Nombres antes de metadatos en paneles estrechos

La evidencia previa de QueueReview mostró nombres sin ancho en doble panel a440px
con lateral expandida: `max(r.w-116,0)`. En Lista la reserva fija255 dejaba apenas
30px y podía situar metadatos sobre el espacio útil. El control a1000 mostraba los
nombres. Este problema geométrico es distinto de la anomalía X11 ancha histórica.

`list_columns` calcula las celdas sin asignar memoria ni consultar el FS:

- El icono y los márgenes conservan su espacio; el nombre recibe el resto.
- Lista muestra tamaño sólo si deja160px para nombre; fecha requiere también ese
  mínimo, tamaño y separaciones. Si no cabe, se omite la celda, sin mostrar0 ni
  transformar lo desconocido en conocido. No se formatean los metadatos omitidos.
- Doble panel muestra su tamaño opcional sólo si deja128px para nombre. A440 con
  lateral expandida usa el ancho disponible para nombres en ambos paneles.
- Las celdas no se solapan ni salen del área. No se cambian alturas, scroll,
  virtualización, orden físico/visible, selección, owners, revisiones ni rutas raw.

Ordenar por nombre, tamaño o fecha sigue disponible aunque la celda de datos esté
oculta. Las cabeceras compactas conservan los tres targets y muestran iconos o
iniciales N/T/F, con la flecha del criterio activo. Si hay espacio, Nombre vuelve
a mostrar su título. Los nombres siguen usando la presentación escapada común;
no se decodifican etiquetas ni se convierten con pérdida los targets de operación.
Un nombre arbitrariamente largo aún puede recortarse: priorizar su celda no es
prometer lectura íntegra de4096bytes en un panel pequeño ni aceptación AT-SPI.

Fuente f6a735f/fixture031aa00:1309 pruebas (447core/105helper/757UI), tres
históricas ignoradas, con concurrencia normal; Clippy all targets con warnings
como errores, formato y release PASS. Wayland1793562545435935498,
XvfbX111793562548431018683 e instalado Wayland1793562954848124239 pasaron cada
uno27capturas/2cierres normales/drain: Lista y doble panel a1000×560 y440×440,
lateral colapsada/expandida, etiquetas raw, foco/marcas/owner conservados al
ordenar, orden independiente por panel y revisión de copia sin encolar.
Inspección visual de ambos paneles compactos y Lista confirma las etiquetas
escapadas cortas. Cola de regresión Wayland1793562485302474500 pasó35capturas/
5cierres normales en sus cinco perfiles.

Los primeros dos intentos fallaron por esperar1000×650 en fixture-wide; helper
y PNG IHDR indican1000×560. Corregir la expectativa bastó con el mismo binario
en ambos backends. Se retienen los FAIL. El lint inicial del decoder hex del
fixture se corrigió con as_chunks acotado, sin suprimir warnings ni serializar.
Instalación1793562789004828933, cinco hashes instalados y cinco respaldos
verificados; app8f88cc804da339095b0a626579b05b24bc94caa49ff39113119faead6addc1b3.
Evidencia en `docs/evidence/narrow-names/assessment.json`. El goal
completo, otras tareas del ROADMAP y las anomalías históricas siguen activos.
Freeze absoluto PSS/Swap/CPU/startup; no se añaden formatos de previa. Handlers y
framebuffer no acreditan input/clipboard del SO,2x, otros SO ni vida global de
workers/helpers.
