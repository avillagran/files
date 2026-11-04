# Giro de la vista sin modificar archivos — 0.0.4

M20 añade dos botones Nerd Font en el header de imagen/PDF/vídeo/GIF.
R gira90° a la derecha yShift+R a la izquierda; nombres accesibles aclaran
que sólo se gira la vista.0/Ajustar vuelve a zoom1×, pan0 y giro0. Girar conserva
zoom y pone pan0 para no arrastrar el recorte de la orientación anterior.

El muestreo mapea coordenadas del viewport directamente al RGB original, con
ancho/alto intercambiados para90/270°. No se asigna una copia rotada del origen,
ni se modifica el archivo, EXIF o PDF. El único raster visible sigue acotado a
1160×800×3bytes; la clave de contenido incluye el giro. Roundings se aplican a la
imagen orientada. No es edición destructiva ni una preferencia persistida.

La transformación numérica (3floats y1byte de giro) sobrevive a nuevos frames y
seek. PDF conserva zoom/pan/giro entre páginas y al pasar por texto extraído;
el nuevo raster ajusta el pan a sus límites. Otro archivo/revisión y cierre lo
reinician. No se conserva un historial RGB ni se comparte el giro con miniaturas.
La orientación EXIF/metadata ya aplicada por el decoder se conserva como base;
este giro manual se añade después.

AyudaF1 incorpora los atajos de navegación, zoom, giro, reproducción, audio yPDF
que ya tienen handlers. Texto/código/HTML/Markdown no muestran botones de giro.

Una prueba con matriz asimétrica3×2 comprueba los4mapeos y longitud del único
raster; pruebasState conservan giro entre frames/seek/páginas y reset de origen.
489Rust (141core/58helper/290UI), Clippyworkspace/all-targets-Dwarnings,
release yfmt PASS. Clippy intermedio rechazó la tupla de clave demasiado compleja;
se sustituyó por SampleKey con campos explícitos, sin silenciar el lint. Logs
inicial/corregido conservados en target/preview-rotation-*.log.

`preview-rotation-ui-1793102443334515413.json`: PASS funcional,14framebuffers
1000×650 de imagen, vídeo, GIF yPDF con4cuadrantes asimétricos. Se comprueban
píxeles reales del framebuffer (tolerancia8/255) contra el orden esperado, giro
horario/antihorario/reset, frames de reproducción/seek, página2 y retorno del texto
PDF. Reabrir otro origen reinicia el giro. Hash de bytes de todos los originales
idéntico antes/después. Root exit0trascallback/ACK;21identidades PID/start de
helpers registradas ausentes/reutilizadas y Z no aceptado. Sin helper vivo
observado al cerrar, ni cobertura de lifetimes/AT-SPI/inputSO/clipboard/native2×.
Se inspeccionaron imagen0/90°, vídeo en reproducción yPDFpágina2; botones Nerd
visibles, sin tofu/colisión de header evidente. No es prueba de fotografías,
precisión cromática o rendimiento de reproducción.

Esta escena hace14readbacks funcionales en proceso, no un experimento aislado de
memoria ni clip. No inferir consumo del footer incidental. Recursos/startup NOT_MEASURED por freeze;
ningún límite local equivale a PSS global ni resuelve los FAIL históricos.

Checkpointe537ce1 instalado con backup`/home/avillagran/.cache/files-test/installed-backups/1793102536282364022`,
hashes app/helper/HTML verificados en`docs/evidence/0.0.4/installed-checkpoint-1793102536363937562.json`.
files`a682485cf39a00fe2695a4b020338c55d156277f4b5ef08ba01d3964fb73fc3a`. Ventanas existentes no controladas.
