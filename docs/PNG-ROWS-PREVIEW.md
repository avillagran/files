# PNG reducido por filas — 0.0.4 (2026-10-27)

Unidad funcional verificada en Linux/Wayland; P02 sigue parcial. El adaptador usa png0.18.1, ya transitivo de image y con
licencia MIT/Apache; se añade sólo el edge directo en helper/Cargo.lock, sin
paquete nuevo. No engine nativo ni GPU. La ruta PNG de producción reutiliza este
mismo decoder por filas; WebP conserva la ruta anterior limitada4Mipíxeles.

8MiB comprimido,8192 por eje/40Mipíxeles fuente,1MiB de metadata y65536chunks.
Decoder::Limits32MiB se mantiene como best-effort de sus propias reservas,
NO cap de asignador/PSS total. Nunca se llama next_frame ni se reserva el raster
completo de fuente para decode/deinterlace. Salida RGB<=1024²/3MiB y mapa
sourceY→ejeDestino<=8192u16/16KiB. Reader sólo recibe una fila/scratch; fila bruta
máxima8192×4×2=64KiB. Compressed input se presta a Cursor, no se duplica.

RGB/gray/alpha8 y16bit, paleta/tRNS y sub8gray se expanden antes de samplear.
16bit redondea a8bit con la misma escala de image, no descarta simplemente el
byte bajo. Se comparte la composición sobre checkerboard y las funciones de
orientación/fit con la ruta anterior. Ocho orientaciones escriben directamente
en su posición final, sin segundo RGB. Adam7 compara cada descriptor de pasada
con el schedule esperado; filas vacías/estrechas y muestras seleccionadas se
cubren sin construir un buffer full-frame de deinterlace. Se exige completar
exactamente todos los píxeles de salida antes de devolver Preview.

Preflight busca eXIf incluso detrás de IDAT para conocer orientación antes de
muestrear. Decode valida CRC, Adler32 y errores CRC ancillaries; finish valida
el final antes de publicar. Texto/ICC se omiten (sin color-management completo),
metadata raw limitada1MiB. APNG se rechaza explícitamente como pendiente; no se
lanzan frames ocultos. Trailing padding tras IEND conserva compatibilidad anterior
bajo el límite total8MiB, no se presenta como contenido validado del PNG.

Source FD esperado/efectivo, raíz de recursos, epochs, latest-wins, cancel/kill/reap,
LRU y pipe son los existentes. No I/O/decode en draw/UI ni cache persistente.
Los caps locales NO acreditan50MB ideal/150MB techo del árbol. Helpers y viewport/
backend tienen costes separados; fase de recursos/startup sigue congelada.

## Pruebas

8regresiones de PNG: coincidencia exacta con fullreference anterior en tipos8/16,
paletas/tRNS/sub8gray, ocho orientaciones, Adam7odd/narrow y eXIf tardío,
CRC/truncación/metadata grande/APNG,12MP generado y decodificado por filas,
source/output cap. Wholeworkspace453=141core/58helper/254UI y Clippy-Dwarnings
PASS. El test de bomba antiguo tenía IHDR truncado y esperaba texto4Mip; ahora
IHDR completo8192² rechaza40Mip antes de CRC/reserva. El receiver acepta PNGgrande
sólo con RGBwire acotado, igual que JPEG; WebPgrande continúa rechazado.
La presentación de imágenes/PDF/vídeo y bitmaps incrustados comparte ahora el
radio8px CPU de thumbnails, como exigía adendaH: antes paint() general pasaba0.
El test de letterbox comprueba ambas rutas; no depende de corners de ventana.
Se conserva una sola reserva de sampling, sin nueva imagen para la máscara.
Footer cambia a Vista reducida/Resolución original según píxeles del bitmap,
no el texto técnico anteriorPNG/WebP≤4Mip que ya no describe la ruta PNG.
Release limpio y escena image-ui-1793081848018085943.json PASS funcional:
9framebuffers1000×650; PNG9MP→1024²,12MP→1024×768,24MP→1024×682,
EXIF tardío6→682×1024 y Adam7 conzoom2. Se comprobaron firmaFD efectiva y
al menos4colores del bitmap; alfa, corrupto, JPEG/WebP pequeño, cambio rápido y
zoom conservan pruebas. Compositor escala2/ventana física2000×1300, pero captura
1000×650: NO acredita nitidez nativa2x. Revisión visual12MP y24MPorientado muestra
checker/colores, bordes redondeados y controles sin solape.

Cierre normal0 tras ACK/drain de preview; las12identidadesPID/start registradas
están ausentes o reutilizadas, nunca se acepta Z del mismo PID/start. Esto NO es
cobertura de todos los lifetimes. El cierre se solicitó con preview pendiente;
el gate de fuente bloqueó el arranque de ese helper, por tanto cierre DURANTE
un decode PNG activo sigue NOT_VALIDATED. No prueba inputSO.

Recursos/startupNOT_MEASURED por freeze; el PSS del pie no aprueba presupuesto.
Sin nuevo gate del árbol, sin tag0.0.4/PASSglobal. WebPgrande/APNG, color ICC,
calidad de reducción, P09 global entre formatos y otrosSO siguen pendientes.
SHAfiles3dc74314523f4862f919d9f66779855fde502fb0006b6ed5cb2886e9ef4628f0;
helper187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.

