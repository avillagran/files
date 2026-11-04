# Catálogo ZIP de sólo lectura

0.0.4, parcial. Space sobre .zip muestra el directorio central real: nombres,
tamaños declarados comprimidos/sin comprimir, método, modo Unix y etiquetas
de symlink, cifrado, ruta insegura o nombre duplicado. No extrae, descomprime
ni ejecuta entradas. No comprueba registros locales, payload o CRC. Un catálogo
consistente no certifica que se pueda extraer de forma segura.

## Límites antes de asignar

Parser propio en core, sin dependencia nueva; se ejecuta sólo en files-helper.
Busca EOCD en los últimos 65557 B y exige final/comentario exactos, monodisco
y ZIP32. ZIP64, trailers y firmas centrales adicionales están explícitamente
fuera de soporte. Comprobación aritmética de offsets y longitudes, hasta
10000 entradas y 4 MiB de directorio antes de reservarlo. Nombres de hasta
4096 B y 256 KiB agregados antes de copiarlos a entradas/índice de duplicados.
Esos caps no incluyen todos los contenedores ni el PSS del proceso.

Los nombres conservan bytes exactos. La presentación escapa controles,
backslashes y bytes no UTF-8; nunca se usa esa cadena visual para abrir un
archivo. Duplicados son iguales por bytes, sin prometer casefold ni equivalencia
Unicode del filesystem. Rutas absolutas, padres, drives y backslashes se marcan
como inseguras; ningún nombre interno llega al filesystem.

Helper Linux abre archivo regular final O_NOFOLLOW/O_NONBLOCK, hasta 8 GiB,
y usa ese FD durante la lectura; padres/mounts y cambios in-place no son un
snapshot. Salida de texto hasta 128 KiB/2000 líneas con enumeradas/total y aviso
visible. Wire hasta 1 MiB en supervisor de 5s; valida id/tipo/encoding/texto.
AS96MiB del helper no es PSS. State cancela y recoge antes del siguiente job.

## Evidencia

288 tests Rust (79 core, 37 helper, 172 UI), Clippy all-targets -D warnings y
release limpios. zip-ui-1793040518449068984.json funcional PASS: catálogo real
con Unicode, symlink, traversal/drive y duplicados; nombre no UTF-8 mostrado
como raw\xFF.bin; ZIP vacío; 2500 entradas con listado truncado a 130846 B;
malformado y 10001 entradas rechazados; último destino/cierre normal tras ACK.
El warning inicial de Clippy se conserva en evidence/zip-preview/initial-clippy.log.

Método: handlers internos y framebuffer real, sin input SO/clipboard. No se
creó ningún archivo de las entradas. Cierre antes del nuevo spawn registrado
no demuestra kill de helper activo. Recursos/startup NOT_MEASURED por fase.

Crear/extraer, protección del destino, límites de expansión, TAR/otros formatos,
ZIP64, validación del contenido, Office y presupuesto siguen pendientes. Esta
entrega no completa Z01 ni P08/P09.

## 2026-10-31 — Shared streaming foundation (source only)

ARCHIVE-OPERATIONS-CONTRACT.md documents archive_member and archive_extract
read-only planning. Catalogue display remains observational (no payload CRC).
Office uses the shared bounded decoder; no new preview feature or installed change.
Creation/extraction destination execution/UI remain pending; do not infer them
from a codec or plan test. Parser additionally retains central needed-version
for local-header agreement in the shared member reader.
