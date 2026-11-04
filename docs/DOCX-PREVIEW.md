# Texto DOCX — 0.0.4, M21

**Aceptación funcional acotada PASS**:500 Rust, Clippy/release/fmt,10 casos wire
y escena real DOCX. Los FAIL ENOSPC anteriores se conservan como historia, sin
atribuir su causa. Instalación coherente se registra por separado. Freeze de
recursos/startup vigente: NOT_MEASURED, sin PASS global ni tag0.0.4.

## Implementación preparada

Space sobre.docx reutiliza helper efímero, supervisor cancel/timeout/reap,
TextDocument/selección/copy handler y State de último destino. Watch de padre+
época/generación y reconciliación visible ya existentes se aplican también a DOCX.
Request lleva Revision esperada; helper abre finalO_NOFOLLOW/NONBLOCK regular,
compara FD antes de leer y revalida FD/nombre después de ZIP/XML. OfficeText devuelve
revisión efectiva; Job y State la comparan antes de Ready. Padres symlink/mounts
siguen semántica SO: no es confinamiento de carpeta ni snapshot atómico.

Sólo cuerpo principal word/document.xml, UTF-8, namespaces Word normal/strict.
Conserva texto de runs, párrafos, tabs, saltos y texto visible de enlaces (inertes).
No muestra instrucciones de campos ni texto eliminado. Sin render fiel de Office,
headers/footers/footnotes, imágenes, enlaces activos, macros o recursos externos.
XLSX/PPTX/ODF, formatos binarios antiguos, DOCM y XML UTF-16 siguen pendientes.

Se reutiliza el listado ZIP32 propio. Ahora expone flags/CRC/offset local como
metadata, sin cambiar la advertencia del catálogo readonly. El helper lee UN
miembro en RAM: nombre local exacto, flags/método/tamaños concordantes, área de
payload antes del central, extras locales acotados/noZIP64, STORED/DEFLATE y
DataDescriptor opcional verificados. CRC y longitud de salida exactos. Cifrado,
duplicados, symlinks/tipos no regulares o métodos/flags desconocidos se rechazan.
No valida los demás payloads ni extrae archivos; nunca usa los nombres como paths
SO. Esto no completa la operación de descomprimir archivos del roadmap.

## Límites

Archivo regular≤8MiB; listado ZIP≤10000entradas/4MiB central/256KiB nombres ya
existente. XML del miembro≤1MiB declarado ANTES de asignar y salida real comprobada
por chunk4096B antes de append. CRC sobre los bytes decodificados completos. El
DEFLATE de flate2 fijado1.1.10 rechaza EOF incompleto (zio.rs:155–160), además de
comprobar total_in exacto; no se acepta sólo la longitud declarada.

XML streaming quick-xml, sinDOM:≤65536eventos/depth64/64atributos por elemento,
nombre≤256B/valor atributo≤4096B, sin DTD/PI/entidades desconocidas ni carga de
recursos. EntradaUTF8 y caracteres XML comprobados; texto visible≤256KiB/2000LF,
truncado explícito en fronteraUTF8. Se sigue validando el resto del XML tras
truncar presentación. Namespace/parser/allocator tienen overhead: estos límites
NO son un cap PSS. Wire≤2MiB antes de JSON, texto decodificado otra vez acotado.
El supervisor existente≤5s mata/recoge antes de entregar respuesta. RLIMIT_AS96MiB
del helper limita espacio virtual, no PSS ni presupuesto total150MB. Syscall
kernelD puede impedir reap/cierre: no se promete deadline offline ni ACK ficticio.

Sólo buffers/pipes: sin caché persistente, temporales de producto ni archivos junto
al original. El label de ruta puede representar bytes inválidos con reemplazo;
ruta/selección/IPC conservan bytes raw. Selección opera sobre texto extraído,
no sobre offsets XML ni layout original. El renderer de texto actual pinta como
máximo4096B por línea; el resto sigue en el documento para selección/copia, pero
la ventana horizontal completa de líneas largas sigue pendiente. No se afirma
shaping/graphemes exactos por retener Unicode. Clipboard real sigue pendiente por foco
Wayland/CUA; invocar Copy handler no acredita contenido del clipboard.

## Evidencia y bloqueo de validación

- Helper aislado66/66 PASS después de limpiar sólo caché dev propia; incluye8tests
  nuevos ZIP/XML. No mide consumo ni completa UI.
- Workspace compiló todos los binarios de test con DEV/TEST_DEBUG=0 e incremental0
  (flags por comando, no cambio permanente); core136/141 FAIL por5ENOSPC en
  rename/persistencia/source fixtures. UI/helper no llegaron a ejecutarse porCargo.
- Tras limpieza coordinador de ELF regenerables, dos de esos fixtures pasaron con
  binario existente y el tercero volvió a fallar ENOSPC en atomic_write(state.toml).
  Se detuvo el intento, sin otro rebuild ni limpieza ajena. No causa Btrfs exacta.
- Con esos binarios existentes,9tests exclusivamente en memoria PASS:3ZIP,
  4XML,2Job/State (CRC, compresión/bounds, DTD/entidades, Unicode/truncado,
  VerifiedA→dirtyB/ReadyAtardío y firma efectiva incorrecta). No son suite global.
- Primera compilación falló ENOSPC escribiendo rmeta. Un reintento posterior tuvo
  E0599 por editar protocolo mientras se compilaba: resultado descartado y árbol
  estable recompilado. Ambos logs conservados, sin atribuir esa carrera al motor.

Logs: target/office-helper-first-test.log, office-helper-after-clean-test.log,
office-helper-stable-test.log, office-helper-debug0-test.log,
office-workspace-test.log, office-core-five-fixtures-after-coordinator-clean.log,
office-ui-memory-tests.log, office-parser-memory-tests.log, office-zip-memory-tests.log.
Auditoría estática de dependencias: evidence/0.0.4/office-dependency-audit.json.

scripts/office-ui-check.py está preparado, NO ejecutado: espera Revision exacta y
READY fresco; captura texto/selección, rawpath, replace preservando size/mtime,
borrado/recrear con callbacks suprimidos, rechazoCRC/XML/size/symlink y truncado.
Antes de aceptarlo faltan normalidad del host, suite/Clippy/release y escena real,
revisión visual y lifecycle sin hijos. No se instala ni etiqueta este WIP.

## Revisión de implementación M21 (2026-10-27)

- `core/archive_list.rs:100–214`: catálogo completo, límites previos a copias,
  duplicados marcados en ambas entradas; rechaza central truncado, sin lista parcial.
- `helper/zip_member.rs:22–170`: sólo miembro fijo, validación local/central,
  tamaño antes de reservar salida, expansión comprobada por chunk, CRC, consumo
  DEFLATE exacto y descriptor; no escribe miembros. El catálogo no acredita
  integridad de otros payloads.
- `helper/office_preview.rs:31–42`: revisión esperada contra FD real antes de
  ZIP/XML y revalidación FD/nombre tras parsear; `office_preview_job.rs:64–97`
  verifica id/revisión efectiva/caps antes de publicar. State comprueba además
  generación y revisión (`text_preview_state.rs:1173–1210`).
- `office_preview.rs:195–241`: regresión RED conservada en
  `target/office-xml-regression-red.log` reprodujo CDATA previo a raíz. Se rechaza
  CDATA fuera de raíz incluso vacío y declaración repetida/no inicial; BOM válido
  y comentarios exteriores siguen admitidos. No implica validación de todo OOXML.
- Office devuelve errores ordinarios por codec/XML/cap, no `SourceUnavailable`.
  State conserva la revisión esperada en Error y `needs_start=false`; reconciliar
  la misma revisión no lanza otro decode. Sólo cambio/invalidez real del origen
  permite otra solicitud. La política se observa en `verify_source:614–625`,
  rama Err:1318–1331 y dispatch:1337–1348; no se afirma prueba nativa del watcher.
- Supervisor compartido (`text_preview_job.rs:295–357`) acota wire antes de
  append, cancela/mata y espera al hijo directo antes del resultado. No introduce
  nietos. Kernel D mantiene la limitación documentada; Drop sólo cancela y el
  cierre normal debe seguir pump hasta ACK.

Tras recuperación de espacio comunicada por usuario, el coordinador ejecutó los
cinco fixtures exactos previamente afectados: PASS con binario existente,
`target/office-core-five-coordinator-restored-space.log`. La nueva suite completa
pasó 500 tests (141 core/67 helper/292 UI), Clippy workspace/all-targets
-D warnings y fmt PASS. Logs nuevos `office-workspace-restored-test.log`,
`office-clippy.log`, `office-fmt.log`; los FAIL ENOSPC anteriores siguen intactos.
Release/escena/instalación todavía pendientes al escribir esta revisión.

## Aceptación funcional después de recuperar espacio

- `target/office-workspace-restored-test.log`:500 Rust (141core/67helper/292UI)
  PASS. `office-clippy.log`: workspace/all-targets-Dwarnings PASS;
  `office-release.log` y `office-fmt.log` PASS. El primer release se interrumpió
  voluntariamente por build competidor ajeno; log separado preservado.
- `office-xml-wire-restored.json`:10/10 casos reales helper PASS, incluidos BOM,
  CDATA interior, prólogo/epílogo inválidos y declaración repetida/tras comentario.
- `office-ui-1793105654289278359.json`:6 framebuffers reales1000×650, Unicode,
  selección102B y truncado explícito256KiB; CRC/tamaño/DTD/symlink con errores
  frescos; nombres Unix0xFF lossless. Reemplazo conserva tamaño/mtime y cambia
  identidad; borrar/recrear invalida y recupera aun suprimiendo callbacks notify
  en fixture (reconciliación visible). Último destino gana; FDstamp exacto en Ready.
- Callback normal→cancel/drain→exit0,9PID/start registrados ausentes, zombies no
  aceptados. No acredita helper vivo justo al cerrar ni todos los lifetimes.
- Handlers internos/framebuffer, sin input SO/clipboard externo/Word layout fiel.
  Recursos/startup NOT_MEASURED; no se usa el pie PSS incidental como gate.

## Instalación coherente autorizada

Checkpoint fuente8032120 instalado en~/.local/bin con backup privado previo y
verificación SHA256 de los cinco artefactos (app/helper/HTML/licencias/stamp).
`installed-checkpoint-1793105752211923245.json` conserva rutas/hashes completos.
Files9703332b6c7d3767aedb769c07eb3080e6c2c07a7e23a9994b704f3b1cd3a6db,
helper1b06c98aa9037ee2ab1d461e874de08fab87823fcd0ef33f480b245a5ef230f5,
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
M20 reemplazado con respaldo; no ventanas ajenas controladas ni publicación.
