# Motor HTML experimental con vista visible

2026-10-26. P05 parcial: Space muestra layout HTML/CSS real mediante un adaptador
nativo efímero de litehtml y una display list de Painter. No hay navegador ni
JavaScript. El adaptador opcional se construye y prepara junto a release, sin
instalación global ni dependencia nueva de Cargo. No completa P05: métricas
aproximadas, recursos/enlaces bloqueados, presupuesto aún NOT_MEASURED.

## Fuentes inmutables y licencias

Se fija litehtml al commit `6fd7aaa63833c913cf1a0336e46fb2e027e4c656`
(fecha del commit upstream: 2026-10-25). Se descargó, verificó y compiló este archivo fijado:

| Artefacto | SHA256 |
| --- | --- |
| [Archivo del commit](https://codeload.github.com/litehtml/litehtml/tar.gz/6fd7aaa63833c913cf1a0336e46fb2e027e4c656) | `82cfdc8347ebb61fbf308e29ebcb6ea5f82e894dd6a013c928deacc0446a471e` |
| [LICENSE de litehtml](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/LICENSE), BSD-3-Clause | `71bd488e77ee821bf38d89e2b8a9c5ebe687553fcae9cefa00a96499792a3e23` |
| [LICENSE de Gumbo incluido](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/src/gumbo/LICENSE), Apache-2.0 | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |
| [VERSION.txt de Gumbo](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/src/gumbo/VERSION.txt), `0.14.1 (2026-09-27)` | `2490fbe7cb742a7ae9334d17bef9c0026e5d64b498dd198fd96da9b532241652` |
| [document_container.h](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/include/litehtml/document_container.h) | `fcfe5c2d82d1f45c787a950c40732177564b942c4e2ed68dce8c94c9974515af` |

El archivo contiene 241 archivos regulares y 4.146.028 bytes expandidos. Gumbo
forma parte del mismo árbol fijado: el build no necesita buscar otra versión.
Digest de ese subárbol:
`e0a132ae902189e6972b8d8256951edd5bb7327119fe88b04d7f738dbb33b5f0`.
Para reproducirlo, ordenar los archivos regulares bajo `src/gumbo` por ruta
POSIX relativa y alimentar SHA256 con ruta, NUL y los 32 bytes del SHA256 de su
contenido. Identifica la copia incluida, incluso si difiere de Gumbo upstream.

La [configuración CMake fijada](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/CMakeLists.txt)
compila ambos motores. Se deshabilita su opción de testing porque utiliza otro
repositorio mediante ExternalProject. El probe no añade Cairo, GTK, Qt,
FreeType, IJG ni codecs. FTL no se asume equivalente a una licencia BSD
autorizada.

La licencia superior no basta para clasificar todos los avisos incluidos:

- `src/strtod.cpp` contiene un permiso UC/Sun distinto del texto estándar
  MIT/Apache/BSD autorizado. **No se compila**: el runner crea otra copia en
  caché, aplica `exclude-strtod.patch` con `--fuzz=0` y agrega nuestro
  `strtod_bridge.cpp` MIT, que llama a `std::strtod`. Conserva los originales y
  sus avisos. CMake rechaza una lista de fuentes que aún incluya el archivo
  original. El ejecutable efímero fija `LC_NUMERIC=C` antes del parser, sin
  cambiar configuración del escritorio ni del proceso principal de files.
- `src/gumbo/src/utf8.c` tiene encabezado Apache-2.0 y un aviso interno de
  Hoehrmann al que falta el párrafo de garantía de MIT. Se consultó la
  [fuente primaria del autor](https://bjoern.hoehrmann.de/utf-8/decoder/dfa/):
  publica licencia MIT completa. Su variante mejorada coincide con las 364
  entradas de `utf8d` y el cuerpo de `decode` incluidos en Gumbo, ignorando
  únicamente espacios del cuerpo. Se preserva el permiso completo en
  `scripts/fixtures/html-engine/THIRD-PARTY.txt`. La evidencia de hashes y
  comparación está en `hoehrmann-source.json`; no se inventa una excepción
  nueva. El código envolvente de Gumbo conserva Apache-2.0.

El reemplazo numérico comprueba al arrancar cada proceso fixtures para
decimales, signo, exponente, precisión, `endptr`, cero negativo, entrada inválida,
overflow y underflow/ERANGE. No se asegura equivalencia con todo el parser
numérico upstream: valores especiales, notación hexadecimal y casos CSS
difíciles necesitan revisión. El puente sólo es válido con locale numérico C
controlado por el helper; no debe imponer locale al proceso de UI.

## Adaptador concreto y salida

`scripts/fixtures/html-engine/probe.cpp` implementa la interfaz abstracta de la
versión fijada. Entrega HTML a `document::createFromString`, llama a
`render(width)` y registra callbacks de `document::draw` para toda la altura del
documento, finita y de hasta 1.000.000 px. El ancho visible y la altura real del
viewport siguen alimentando CSS y media queries; el clip de dibujo usa la
altura completa para que el host pueda hacer scroll sin relanzar el helper. El
layout procede de litehtml; no se convierte HTML a Markdown ni se considera
extracción de texto como preview renderizado.

La API actual usa `create_font(const font_description&, const document*,
font_metrics*)`, valores `pixel_t` flotantes, callbacks separados para imagen,
relleno sólido y tres gradientes, y una pila de clips. Otro tag requiere
revisar los overrides; un ejemplo antiguo no prueba compatibilidad.

La salida es una lista JSON acotada de texto con caja, RGBA, tamaño y estilo;
rectángulos sólidos; y push/pop de clip. Litehtml calcula flujo, espaciado CSS,
tablas, títulos y media queries. Se cuentan explícitamente las aproximaciones:
bordes sólidos simplificados, clips rectangulares, gradientes no dibujados e
imágenes bloqueadas con placeholder. Un PASS del probe no completaría estas
funciones.

El runner consulta sólo `fc-match monospace` para obtener la familia configurada,
con fallback legible `monospace`, sin enumerar ni rasterizar el catálogo.
**Las métricas son aproximadas**: avance 0,6 × tamaño por escalar Unicode,
incluidos combining marks y ZWJ. Fuerza la familia configurada y limita tamaños
a 8–64 px. No acredita shaping, segmentación grapheme, cobertura de glifos,
ausencia de tofu, selección ni imagen rasterizada. La integración final debe
usar medición Nerd Font coherente con Painter, no estos avances para hit tests.

## Política del host y límites

La entrada es un archivo local regular, no-follow y nonblocking de hasta 32 KiB.
Se verifica su tamaño antes de asignar el buffer y durante la lectura por si
crece. Se rechazan NUL y UTF-8 inválido. El límite es experimental, no una
decisión definitiva sobre documentos HTML admitidos por files.

Los callbacks no contienen I/O de red ni apertura de recursos. `import_css`
vacía la hoja solicitada; links de CSS, imágenes, cambios de base y navegación
se cuentan y se rechazan. Las imágenes tienen tamaño placeholder fijo. No
existe un motor de ejecución JS ni se abre un navegador externo. La fixture
incluye script, iframe, URL activa, imagen remota y CSS importado. La escena
funcional de la app verificó contadores de bloqueo y ausencia del sentinel JS;
no acredita sandbox del sistema operativo ni toda variante de HTML hostil.

| Recurso | Límite del probe / alcance |
| --- | --- |
| HTML | 32 KiB antes del parser |
| Fuentes | 64 handles vivos antes de insertar |
| Texto de callback | 4096 bytes UTF-8 antes de expandir JSON |
| Creación de elementos | 4096 callbacks; no cuenta todos los nodos internos de texto |
| Comandos | 8192; 2 MiB de texto total reservados una vez |
| Clips | profundidad 32; pila equilibrada al terminar |
| Viewport | ancho 128–1600, alto 128–1200 |
| Coordenadas y extensión | finitas; valor absoluto ≤1.000.000 |
| Pipes | stdout ≤2 MiB + 16 KiB; stderr ≤64 KiB |
| Proceso | timeout monotónico 5 s; CPU 3 s; espacio virtual 64 MiB desde la UI (128 MiB en runner aislado); 32 FD; sin core dump |

**document_container no proporciona presupuesto de asignación del parser.**
En [document.cpp](https://github.com/litehtml/litehtml/blob/6fd7aaa63833c913cf1a0336e46fb2e027e4c656/src/document.cpp),
`createFromString` usa Gumbo con opciones y allocator por defecto antes de
invocar los callbacks de elementos. El límite del host no impide asignar ese
árbol inicial; una excepción puede conservar asignaciones intermedias hasta
salir el proceso. CSS y layout tampoco tienen un cap agregado demostrado.
Límites de entrada, tiempo y espacio virtual contienen parte del riesgo, pero
**RLIMIT_AS no es PSS** y no verifica el presupuesto de files. Producción
requiere límites de allocator/nodos/profundidad en el parser o un modo
restringido con alcance y consumo medidos explícitamente.

Ningún URL de este experimento abre archivos locales. Recursos futuros deben
resolverse una vez contra una raíz autorizada y abrirse por componentes sin
symlinks mediante la política del helper existente. Hay que definir también
`<base>`, redirects, `url()`, fuentes, data URI, imports y refresh. El filtro
léxico no es sandbox; hardlinks y puntos de montaje necesitan alcance explícito.

## Integración y reproducción funcional

El primer build falló por comparar `pixel_t` con un entero. Se conserva
`docs/evidence/html-engine/probe-1793035151738003920.json`; `.value()>0.0f`
corrige el contrato de la versión fijada. CMake Release posterior compila y
enlaza sin warnings. La UI ha ejecutado el adaptador real, incluyendo sus
fixtures numéricas de arranque, a través del supervisor común.

```sh
cargo build --release
make html-helper
python3 scripts/html-ui-check.py
```

`make html-helper` verifica/compila fuentes inmutables en caché y prepara
`target/release/files-html-helper`, licencia y stamp. No instala globalmente.
`stage-html-helper.py` rechaza fingerprint de fuente o SHA256 de binario que
no coincidan. Sin ese ejecutable, Space muestra capacidad no disponible.
La distribución/instalación automática del adaptador y otros SO están pendientes.

El runner `--prepare/--build/--run` sigue disponible para trabajo aislado;
`--run` NO se usó en este checkpoint. Conserva fuentes originales y fallos,
compila un trabajo CMake y escribe evidencia con timestamp exclusivo. Ningún
comando de recursos/arranque se ejecutará mientras siga la fase explícita del
usuario de sólo integración/capturas.

Rust valida el wire antes de construir TextDocument: 2MiB+16KiB, preconteo de
comandos y estructuras JSON, schema/status, viewport exacto solicitado, hasta
8192 comandos, clips balanceados≤32, coordenadas finitas, fuentes8–64 y texto
384KiB/2001 líneas. Cancelación/timeout recoge el hijo antes de publicar; State
mantiene un único job y sólo la última revisión. Resize cancela el anterior y
recalcula CSS al ancho nuevo; la selección no se preserva entre esos layouts.

La escena usa handlers internos y framebuffer real, no eventos SO/clipboard:
CSS/tabla/estilos, selección visible, media query1000→800, recursos bloqueados,
scroll hasta FINAL_HTML_VISIBLE, rechazo32KiB, cambios rápidos y cierre0 tras
ACK. 252 tests Rust(73core/33helper/146UI), Clippy workspace all-targets
-Dwarnings y release limpios. Evidencia y hashes app/adaptador en
`docs/evidence/0.0.4/html-ui-*.json` y capturas `html-*.png`.

**Límites visibles confirmados:** avance0.6×tamaño por escalar difiere de la
fuente real: se recortan glifos CJK y el final de algunas celdas (`archivo.md`).
Selección es proyección de comandos en orden de dibujo, no DOM ni shaping.
El negro de texto se adapta al tema sin analizar todo fondo CSS; documentos
con fondos claros explícitos pueden perder contraste. Bordes/alpha/clips son
aproximaciones; no hay gradientes, imágenes HTML ni enlaces funcionales.
No se afirma seguridad arbitraria del parser ni cumplimiento PSS. Recursos y
startup NOT_MEASURED por fase; siguen FAIL base/arranque/clipboard históricos.

## Posible contrato C ABI

Propuesta, todavía sin símbolos exportados: documento opaco y ninguna STL
cruzando el límite.

```c
typedef struct files_html_document files_html_document;
typedef int (*files_html_measure)(void *user, const unsigned char *utf8,
    size_t bytes, unsigned font_id, float *advance);
typedef int (*files_html_emit)(void *user, const files_html_command *command);
int files_html_create(const unsigned char *html, size_t bytes,
    const files_html_limits *limits, const files_html_host *host,
    files_html_document **out);
int files_html_layout(files_html_document *, unsigned width, unsigned height);
int files_html_draw(files_html_document *, float scroll_y);
void files_html_destroy(files_html_document *);
```

Punteros prestados durante la llamada, errores como enteros, excepciones
contenidas en C++, longitud explícita de cada texto y callbacks que pueden
rechazar trabajo. El helper debe ser single-flight, cancelable y recolectado.
Selección e hit tests requieren rangos del texto renderizado y IDs estables,
separados de los offsets del HTML fuente.

No bloquear la UI mediante callbacks de medición entre procesos. El helper
necesita un backend de fuentes permitido y auditado que coincida con Painter,
o métricas/shaping reutilizables acotados. Las métricas aproximadas del probe no
pueden convertirse silenciosamente en contrato final.

Quedan pendientes métricas reales/shaping, contraste CSS completo, preservación de
selección tras resize, enlaces/recursos locales, allocator del parser y pruebas de
HTML/CSS profundo o malformado, crash y
medición del árbol principal+helper: PSS+SwapPss estable/pico muestreado, CPU,
latencia y arranque. No se declara HTML completo ni presupuesto aprobado.


## Identidad efectiva de HTML seleccionado — 2026-10-27 (M13)

El worker Rust recibe expectedRevision del watcher/probe existente, abre archivo
regular final NOFOLLOW/NONBLOCK y exige coincidencia antes de renderizar. Su límite
32KiB precede spawn/parser; no se lee contenido en UI. El adaptador ahora admite
`--source-fd N WIDTH HEIGHT FONT`, N3..255 validado antes de leer. Hereda exactamente
el FD abierto, sin dup a un índice fijo ni reopen de path. El modo path anterior
se conserva para fixtures CLI, pero files usa sólo FD. Pread no cambia el offset
compartido; fstat comprueba regular/cap antes del buffer32KiB+1. No nueva dependencia.

Tras reap y decode de la display list, el worker revalida el mismo FD y nombre;
Ready lleva Revision efectiva, State exige firma esperada y generación actual.
Dirty/borrado oculta lista y selección anteriores, cancela, rearma token y espera
nuevo probe estable. Watch antes probe/epoch monotónico/reconcile1s se reutilizan,
sin watcher por archivo ni threads nuevos acumulados. Source error tipado separado
de fallo del parser. Readonly_gate audio/PDF compartido: helper exige reap, cierre
readonly puede permitir salida sin join de syscall ni fingir ACK. No prueba kernelD.

O_NOFOLLOW protege sólo último componente; symlinks padres/mounts siguen navegación
SO. Ni revisiones ni watcher proporcionan snapshot atómico, hash de contenido,
sandbox o detección instantánea garantizada. Scripts/red/CSS/imágenes/navegación
externas siguen bloqueados. Las métricas monoespaciadas escalares aproximadas y
recortes de CJK/celdas siguen visibles; esta unidad no declara shaping correcto.

476Rust(141core/58helper/277UI), Clippyworkspace/all-targets-Dwarnings/releasePASS.
Native build probe1793093259627217246 registra fingerprint binario nuevo;
fd-contract1793093345381869451 PASS: FD mantiene A después de reemplazar nombre
porB, offset compartido intacto, CLIlegacy lee B, rechazos FD/FIFO/>32KiB sin
salida parcial. Tests Rust VerifiedA/openB/finishA-after-replace y ReadyA tardío
trasDirty/B, además de readonlyadmission y cap anterior a child.

html-ui1793093483630467339 funcionalPASS10FB1000×650 yresize800×500 con notify
suprimido: firmaFD y texto efectivo de reemplazo mismo size/mtime, inplace con
mtime restaurado, delete/recreate, renamefile/parentrecover, hidden sinhelper;
selección/estilos/tabla/scroll/recursosbloqueados/error32KiB/symlink/rapid/cierre0ACK.
14loggedPID/startgone/reused, Znoaceptado; no lifetimecoverage ni helpervivoalclose.
SHA appbb32f9725d40d6327c66897aa0a1595c7a2c113e28fd81206f41f0a83a83575c;
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Handlers internos/framebuffer real, no inputSO/clipboard/native2xsharpness.
Recursos/startup NOT_MEASUREDfreeze; valores incidentales del pie no son gate
ni medida del árbol. P05/P09 siguen parciales; no0.0.4tag/PASSglobal.
