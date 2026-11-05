# Dependencias de 0.0.2

El código de files es MIT. Las dependencias Rust se resuelven en `Cargo.lock`.
La revisión de `cargo metadata --format-version 1` encontró:

- MIT, Apache-2.0, BSD o una alternativa MIT/Apache: dependencias habituales,
  incluidas FLTK Rust, serde, TOML y opener.
- FLTK C++: LGPL con excepción explícita de enlace estático, admitida en el diseño.
- Excepciones concretas **autorizadas por el usuario el 2026-10-26**:
  `notify` → CC0-1.0; `inotify` y `inotify-sys` → ISC;
  tablas de `unicode-ident` → Unicode-3.0 junto a MIT/Apache-2.0.

Las expresiones duales `Unlicense OR MIT` y `Zlib OR Apache-2.0 OR MIT` se usan
bajo su alternativa MIT. No se incorporaron dependencias GPL ni propietarias.
Las excepciones anteriores no autorizan ampliar la lista para otras dependencias.

En Linux, FLTK usa las bibliotecas de plataforma ya instaladas (Wayland,
Cairo/Pango, fontconfig/FreeType, X11 como fallback). No se distribuye un runtime
GTK/QML, un servidor web, decodificadores, una pila de red ni modelos.

Referencias primarias: [fltk-rs](https://github.com/fltk-rs/fltk-rs),
[licencia de FLTK](https://www.fltk.org/COPYING.php),
[notify](https://github.com/notify-rs/notify),
[inotify-rs](https://github.com/hannobraun/inotify-rs),
[unicode-ident](https://github.com/dtolnay/unicode-ident).


0.0.2 reutiliza serde_json/libc ya presentes para protocolo/supervisión de helper;
no añade familias de licencia. Los cuatro shaders portados conservan MIT y fuentes/
hashes en assets/shaders/README.md. Sus pruebas EGL usan bibliotecas del sistema;
la app sigue sin contexto GPU por defecto y la integración real se auditará al llegar.

## 0.0.4: helper de imagen (preparación)

`files-helper` añade `image = 0.25.10` con `default-features = false` y sólo
`png`, `jpeg`, `webp`. El árbol **seleccionado** normal/build está guardado en
`docs/evidence/image-preview/dependency-audit.json`; no activa Rayon, AVIF, EXR,
TIFF, GPU ni red. Se revisaron los manifests originales de la versión resuelta.

Alternativas elegidas: image/image-webp/png/flate2/fdeflate/crc32fast/cfg-if/
num-traits/quick-error/autocfg/bitflags bajo MIT o Apache-2.0; zune-core y
zune-jpeg bajo MIT/Apache, no su alternativa Zlib; bytemuck y miniz_oxide bajo
MIT/Apache; byteorder-lite bajo MIT (no Unlicense); adler2 bajo MIT/Apache;
simd-adler32 bajo MIT; moxcms y pxfm bajo BSD-3-Clause/Apache-2.0. No se añade
ninguna excepción de licencia.

**zlib-rs 0.6.8**, sólo Zlib, aparece como dependencia **opcional no seleccionada**
en Cargo.lock por flate2/png. No está habilitada, compilada ni enlazada; no se
considera autorizada. La evidencia usa `cargo tree -p files-helper -e normal,build`
y los logs reales de build, no un recorrido ingenuo de `cargo metadata.resolve`
(que también puede incluir aristas opcionales débiles). Un cambio que seleccione
ese backend necesita revisión y autorización previa de esa licencia.

Detalle de formato/bounds/limitaciones y fuentes primarias en
[IMAGE-PREVIEW.md](IMAGE-PREVIEW.md). Estas dependencias residen sólo en el
helper; su PSS y el del árbol de app integrado todavía deben medirse por separado
antes de aceptar P01/P02. Un helper funcional no completa la UI de imágenes.

JPEG release upgrade: image features now only `png,webp`; helper-only path
`vendor/jpeg-decoder`0.3.2, MIT OR Apache-2.0 (Apache option used), default features
OFF, no Rayon. Exact archive/hash/licenses/originalsrc and applied source patch are
in SOURCE.json/PATCHES.md; independent provenance review vendor-source-review.json.
The local manifest declares historicalasmjs for modern check-cfg without changing
source. No IJG/TurboJPEG is linked or authorized. image's zuneJPEG adapter is no
longer selected by this release; previous audit belongs to the earlier candidate.

## Markdown nativo (0.0.4)

`pulldown-cmark =0.13.5`, defaults desactivados, licenciaMIT. Transitiva nueva
`unicase2.10.0` MIT/Apache2; bitflags ymemchr ya permitidos. Se usan eventos nativos,
no el featureHTML serializador ni CLI/getopts. Manifests/licencias leídos del
registry local; checksums fijados en Cargo.lock. No nuevo motor browser ni GPU.

## Adaptador HTML opcional (0.0.4 experimental)

No añade crates. litehtml6fd7aaa y Gumbo incluido se compilan estáticos en un
ejecutable efímero separado, BSD3/Apache2. Archivo fijado82cfdc83… y licencias,
fuentes/recipe/hashes en HTML-ENGINE-PROBE y scripts/fixtures/html-engine.
La rutina UC/Sun strtod original NO se compila: patch sin fuzz y bridge MIT
propio. Aviso MIT completo Hoehrmann preservado tras verificar tabla y decoder
contra fuente primaria. Ninguna excepción nueva. `make html-helper` sólo prepara
artefactos locales en target/release; falta empaquetado portable y presupuesto.

## Herramientas de vídeo del sistema

P07usa ffprobe/ffmpeg opcionales ya instalados mediante procesos directos,
según DESIGN J. No se redistribuyen/enlazan esos ejecutables ni sus codecs;
no hay nueva dependenciaCargo/descarga/instalación. Licencias de bibliotecas
compiladas de files siguen allowlist. Paths/version/SHA de herramientas
usadas se registran en video-ui JSON; ausencia se presenta como capacidad
no disponible. Su consumo forma parte del futuro gate total, no se oculta
por ser proceso externo. Reproducción de vídeo software y audio posterior documentadas en VIDEO-PREVIEW/AUDIO-PLAYBACK; no nuevos paquetes Cargo.

## Herramientas PDF del sistema

P06usa pdfinfo/pdftoppm opcionalesmedianteejecutablesdirectos. No se enlazan
ni redistribuyenPoppler/librosPDF ni seincrementan dependenciasCargo. Su
licencia/actualización pertenece al paqueteSO; ninguna excepciónnueva para
códigodistribuido de files. Falta adaptaciónportable/capacidadsin herramientas
yPSSárbol; paths/versiones/SHA deescenafuncionalenpdf-ui JSON.

## Adaptador de gestos de ventana (0.0.4)

cc1.10.0 MIT/Apache y fltk-sys1.5.23 (ya presentes en lock) pasan a dependencias
de build directas para compilar el bridge C++ MIT local contra headers reales.
No bibliotecas de runtime nuevas ni GL/GPU; licencia FLTK y excepción estática
existentes. Detalles de pin/API/ABI y fuentes/config en WINDOW-MODES.md.

## Renombrador regex (0.0.4)

`regex = 1.13.1`, defaults OFF y sólo `std`, con regex-automata 0.4.18 y
regex-syntax 0.8.11 bajo alternativa MIT (también Apache-2.0). El árbol de features
seleccionado está en `evidence/rename-regex/`; `aho-corasick` 1.1.5 aparece en lock
bajo MIT/Unlicense pero no se activa en este árbol normal.

La revisión de los archivos originales detectó `regex-syntax/src/unicode_tables/
LICENSE-UNICODE` con términos Unicode 1991–2018. No es la excepción específica de
unicode-ident y no se autorizó implícitamente: las features de esas tablas quedan
DESACTIVADAS. El motor sigue aceptando UTF-8 escalar/literales/rangos; las clases,
propiedades y casefold Unicode muestran error de capacidad. La unificación del
workspace añade sólo `dfa-search` de automata por bstr/opener ya existentes, no
activa tablas de syntax ni DFA-build/hybrid/perf. Hashes/manifests/decisión en
`dependency-audit.json`. No nueva excepción, descarga o instalación global.

## Apertura de archivos — 2026-10-26
Se retiró opener y sus dependencias bstr/normpath al adoptar `open` del sistema
por instrucción explícita. No dependencia nueva ni fallback. También desaparece
la feature dfa-search transitiva de regex-automata antes aportada por bstr;
regex sigue std/NFA/syntax sin tablas Unicode no autorizadas. El apartado previo
es histórico de ese checkpoint; árbol actual en evidence/open-close/regex-features.txt.

## PNG por filas — 2026-10-27
`png = 0.18.1`, MIT OR Apache-2.0, ya transitivo de image; se añade sólo
la dependencia directa en helper. Sin nuevo paquete ni decoder nativo/GPU.
Reader por filas/Adam7 y límites descritos en PNG-ROWS-PREVIEW; caps locales
no equivalen al presupuesto PSS del árbol.

Audio seleccionado: producción usa el mismo ffmpeg del SO con salida Pulse,
no enlaza ni redistribuye ffplay/SDL. El prototipo ffplay quedó sólo en historia
git/evidencia por su EOF nativo fallido. ffprobe/ffmpeg paths/version/SHA actuales
en audio-ui1793086873981565834; ausencia/salida no disponible son errores visibles.
Consumo del proceso pertenece al gate total pendiente, AS768MiB virtual NO PSS.


GIF M16 no añade crate ni motor enlazado: usa sólo demuxer/códecgif del ffmpeg/
ffprobe del sistema ya opcionales, con whitelist porformato. No redistribuye
los CLI. Caps de GIF-PREVIEW yvalidación funcional no acreditanPSSárbol.

## DOCX en preparación (0.0.4)

quick-xml=0.41.0 MIT, defaults off, sólo dependencia memchr ya existente.
flate2=1.1.10/crc32fast=1.5.3 pasan a dependencias directas del helper; flate2
defaults off/rust_backend mantiene miniz_oxide permisivo, NO zlib-rs. Árbol
normal/build offline/locked y hashes de manifest/licencia en
evidence/0.0.4/office-dependency-audit.json. Sin nueva familia de licencia,
DOM/browser/network/daemon. Esto es auditoría estática, NO aceptación de UI/PSS.

## EXDEV source inventory (0.0.4, M25 WIP)

SHA256 incremental: `sha2 =0.10.9`, defaults off, feature std. Ocho paquetes
nuevos en lock: sha2, block-buffer, cpufeatures, crypto-common, digest, typenum
MIT/Apache2; generic-array MIT; version_check MIT/Apache2. Declaraciones y
archivos LICENSE locales comprobados en exdev-dependency-declarations.json;
la coordinación verificó independientemente las mismas declaraciones. Sin nueva
excepción. No implica auditoría completa de source ni presupuesto de app.
No se selecciona sha2-asm; árbol normal/build se comprueba antes del checkpoint.


## M29a — MIME Wayland (0.0.4 parcial)

Sin cambios de Cargo.lock ni nuevas dependencias. Bridge C++ local MIT compila
contra headers privados FLTK1.5.23 ya pinned y Wayland ya enlazado; static_assert
ABI/API exige revisión al actualizar. No redistribuye gestor externo ni usa
data-control. FLTK conserva licencia/excepción de enlace ya documentada. Posible
hook DnD futuro exige fuente/parche reproducible de ese pin, no editar registry.


## M29b — hook Wayland FLTK1.5.23

Se incluye la TU upstream y patch acotado en vendor/fltk-wayland-transfer bajo
licencia FLTK LGPL con excepción estática y COPYING íntegro; modificaciones FLTK
se distribuyen bajo esos términos, no MIT. SOURCE.json fija SHA archivo crate,
fuente, patch y licencia. No nuevas crates, licencias ni red; wrapper CMake sólo
modifica fuente generada en target. Guard y documentación de rebuild en README
del vendor. La compilación no demuestra interop nativa ni consumo de memoria.

## 2026-11-04 — vendor lib-fltk-omarchy 1ff1adf

La actualización del toolkit (tooltips Omarchy, animaciones, RTL/i18n)
añade al lockfile `unicode-bidi 0.3.18` y `unicode-segmentation 1.12.0`,
ambos `MIT OR Apache-2.0`, sin archivos ni términos de licencia Unicode
adicionales (revisados LICENSE-* y fuentes). `pulldown-cmark =0.13.5` sigue
resolviendo con el requisito ≥0.13.5 del toolkit.
