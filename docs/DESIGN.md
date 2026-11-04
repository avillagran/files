# files — diseño v0 (2026-10-26)

Navegador de archivos liviano, multiplataforma, inicialmente para Omarchy.

## 1. Por qué los demás pesan tanto (medido en esta máquina, una ventana, ~/)

| App | RSS | PSS (lo que realmente cuesta) | Stack |
|---|---|---|---|
| Nautilus | 221 MB | 167 MB | GTK4 (render GPU) + libadwaita + GIO/GVFS |
| Flea | 236–251 MB | 143–153 MB | **Quickshell = Qt Quick/QML** (UI) + backend Rust (`flea --backend`, 4 MB) |

Conclusión clave: Flea **no es Rust en la parte que pesa**. El binario Rust usa 4 MB.
Los 150 MB son el motor QML (JS engine + scene graph) y el contexto GPU. QML sirve
para prototipar visualmente, pero no para construir.

De dónde salen los MB en un gestor "moderno":
1. **Contexto GPU** (Mesa/Vulkan/GL): 30–80 MB anónimos solo por existir.
2. **Motor declarativo** (QML/JS, GTK4 render nodes, WebView): 40–100 MB.
3. **Iconos SVG** rasterizados en 5 tamaños × tema completo al arrancar: 10–30 MB.
4. **Thumbnails** y decodificadores (libjpeg, librsvg, gdk-pixbuf) residentes: 10–50 MB.
5. **Modelos no virtualizados**: un objeto/widget por archivo, 10k archivos = 10k widgets.
6. Daemons/indexers (tracker, gvfsd) que la app arrastra.

## 2. Presupuesto objetivo (PSS, ventana 1920×1080, carpeta de 10k archivos)

| Subsistema | Presupuesto |
|---|---|
| Toolkit + libs (parte file-backed) | ≤ 12 MB |
| Framebuffers (2 × 1920×1080×4, software, wl_shm) | ~16 MB (inevitable; escala con la ventana) |
| Modelo de directorio (10k entradas × ~120 B) | ~1.5 MB |
| Iconos (atlas único, rasterizado por tamaño en uso) | ≤ 2 MB |
| Caché de thumbnails (LRU con tope duro) | ≤ 8 MB, configurable, 0 si se apaga |
| Fuentes (FreeType, 2 caras cargadas) | ~3 MB |
| Heap restante | ~8 MB |
| **Total** | **~50 MB** ✅ · > 150 MB = herejía |

Nota honesta: en la pantalla 2370×2122 de esta máquina los dos framebuffers solos son
~40 MB. El presupuesto se define en 1080p y se mide con un test automático.

## 3. Reglas de ingeniería (no negociables)

- **Render por software** (CPU) por defecto. Sin GL/Vulkan. Repintado solo del área dañada.
- **Todo virtualizado**: solo existen las filas visibles (+1 pantalla de margen).
- **stat() perezoso**: `readdir` primero (nombre + tipo), `stat` solo de lo visible o al ordenar por tamaño/fecha.
- **Nada residente que pueda ser proceso hijo**: thumbnails, extracción, búsqueda profunda → helper que arranca, hace, muere.
- **MIME por extensión** (tabla propia, ~300 entradas); libmagic solo bajo demanda.
- **Iconos**: un set propio pequeño (~48 glifos) en formato vectorial simple, rasterizado al tamaño pedido y cacheado. Sin cargar el tema de iconos del sistema al inicio (opcional después).
- **Test de memoria en CI**: abrir ~/ con 10k entradas, medir PSS, fallar si > 60 MB.
- **Arranque < 100 ms** en frío.

## 4. Stack: opciones evaluadas

| Opción | PSS típico | Multiplataforma | Veredicto |
|---|---|---|---|
| QML / Qt Quick, GTK4, Tauri, Electron, Flutter | 120–300 MB | sí | ❌ descartados por peso |
| Qt6 Widgets (C++, sin QML) | 45–70 MB | excelente, look nativo | ⚠️ justo en el límite |
| GTK3 (C) | 35–60 MB | pobre en Win/mac | ⚠️ no multiplataforma real |
| **FLTK 1.4** (C++ o Rust via fltk-rs) | **10–25 MB** | Linux (X11+Wayland), Windows, macOS | ✅ **recomendado** |
| SDL3 + UI propia (C/Zig) + FreeType/HarfBuzz | 20–35 MB | sí (SDL3 trae clipboard, DnD, IME, diálogos nativos) | ✅ alternativa, 3× más trabajo |

**Recomendación: FLTK 1.4.** Da ventana, eventos, texto (Pango/Cairo en Linux, nativo en
Win/mac), clipboard, DnD, HiDPI, diálogos nativos, y widgets base (árbol, tabla, menús,
input) en ~1 MB de biblioteca. Toda la vista de archivos la dibujamos nosotros (como en
cualquier opción), así que el look Omarchy no depende del toolkit.

Lenguaje: **Rust + fltk-rs** (núcleo FS seguro, hilos para I/O) o **C++17 + FLTK**
(cero capas intermedias). Decisión pendiente del usuario.

## 5. Arquitectura

```
files/
  core/      sin dependencias de UI
    fs/      listado streaming, stat perezoso, sort, filtro, watcher (inotify/kqueue/RDCW)
    ops/     copiar/mover/renombrar/papelera, cola con progreso, cancelación, deshacer
    mime/    tabla extensión→tipo, apps por defecto (xdg / registro / LaunchServices)
    places/  XDG dirs, montajes, favoritos, recientes
    theme/   lee ~/.config/omarchy/current/theme (colors) y config propia (TOML)
  ui/        FLTK: ventana, barra de ruta, sidebar, vistas, panel dual, diálogos
  helpers/   thumbnailer (proceso aparte, cap de RAM), buscador recursivo
  tests/     memoria (PSS), rendimiento (100k archivos), ops
```

## 6. UI (ver mockups/v1.html)

Tomado de cada referencia:
- **Flea / Omarchy**: tema del sistema, tipografía mono, sidebar Lugares + Dispositivos, vistas lista/columnas/grilla, barra de estado con FS y espacio libre.
- **Finder**: columnas Miller, barra de ruta (breadcrumb) clicable y editable, "Ir a carpeta".
- **Windows**: acceso rápido / carpetas frecuentes, fijar favoritos, vista de inicio.
- **MC**: panel dual (F-tecla), barra de teclas F1–F10, filtro rápido al escribir, operar sin ratón.
- **Nautilus**: estrellas/favoritos, "N elementos" en carpetas, vista de recientes.

Vistas: Lista · Grilla · Columnas · Doble panel. Siempre virtualizadas.
Teclado: `Ctrl+L` ruta, `/` o escribir = filtro, `j/k` o flechas, `Enter` abrir,
`Backspace` arriba, `F5/F6/F7/F8` copiar/mover/mkdir/borrar, `F9` doble panel, `Tab` cambia panel,
`Ctrl+1/2/3` vistas, `Ctrl+T` terminal aquí, `Space` vista previa (helper externo).

## 7. Fases

1. **0.0.1 núcleo + lista**: abrir, navegar, ordenar, filtro, sidebar, breadcrumb, tema Omarchy, test PSS.
2. **0.0.2 operaciones**: copiar/mover/papelera/renombrar con progreso y deshacer, DnD, clipboard, watcher.
3. **0.0.3 vistas**: grilla, columnas, doble panel, teclas F.
4. **0.0.4 extras**: thumbnails (helper), búsqueda, abrir con, montajes, favoritos.
5. **0.0.5 Windows/macOS**: papelera nativa, apps por defecto, instaladores.

---

# Adenda 0.0.1 (2026-10-26, segunda ronda)

## A. ¿QML "optimizado"? Medido

App QML mínima (una ListView de 10k filas, sin lógica), Qt 6.12, esta máquina:

| Modo | RSS | PSS | Anon |
|---|---|---|---|
| Qt Quick con GPU (default) | 146 MB | 90 MB | 41 MB |
| Qt Quick `QT_QUICK_BACKEND=software` | 104 MB | 51 MB | 19 MB |

Es decir: **QML vacío ya está en el límite de 50 MB** con render por software, y en
90 MB con GPU. Cualquier función real (iconos, thumbnails, modelo, diálogos) lo lleva a
70–100 MB. Qt Quick Compiler reduce JS pero no quita el motor V4 ni el scene graph.
Conclusión: QML queda **descartado como runtime**. Sirve solo para bocetar.

## B. Alternativa "QML sin el peso": Slint (Rust)

Slint ofrece lo que gusta de QML (lenguaje declarativo `.slint`, propiedades reactivas,
previsualización en vivo) pero **compila el `.slint` a Rust nativo** en build: sin motor
JS, sin intérprete. Renderer por software incluido. Linux (Wayland/X11), Windows, macOS.
Licencia: GPLv3 **o** Royalty‑Free (gratis para apps de escritorio, sin copyleft) o comercial.

**Medido** (misma prueba que QML: ListView de 10k filas, ventana 900×600, Slint 1.x,
`backend-winit-wayland` + `renderer-software`, binario estático de 12.8 MB que solo enlaza
fontconfig/freetype):

| | RSS | PSS | Anon | Shmem (framebuffers) |
|---|---|---|---|---|
| Slint software | 31 MB | **19 MB** | 4 MB | 8 MB |
| QML software | 104 MB | 51 MB | 19 MB | 7 MB |
| QML GPU | 146 MB | 90 MB | 41 MB | 0 |

Slint vacío cuesta **2.7× menos que QML** en el mismo modo, y deja ~30 MB de margen
para la app real dentro del presupuesto de 50 MB.

## C. Decisión de stack (propuesta cerrada, falta tu OK)

**Rust + Slint (software renderer)** como camino principal:
- Rust es tan multiplataforma como C++ para esto (winit/Wayland/X11/Win32/Cocoa vía Slint).
- El núcleo FS en Rust puro (sin UI) es reutilizable si algún día cambia el toolkit.
- Plan B: **Rust + fltk-rs** (10–25 MB) si Slint tropieza en algo funcional (DnD, IME, diálogos nativos).
- C++ queda como opción solo si se quiere Qt Widgets (45–70 MB, justo en el límite).

## D. Tema Omarchy en tiempo real (verificado en este sistema)

- Fuente canónica: `~/.local/state/omarchy/current/theme/colors.toml`
  (es lo que lee `omarchy-theme-color`). También existe `~/.config/omarchy/theme.json`
  con `{"theme":"Hackerman"}` y `~/.config/omarchy/themes/<nombre>/`.
- Al cambiar tema, Omarchy ejecuta `~/.config/omarchy/hooks/theme-set.d/*` con el nombre
  del tema. **No necesitamos hook**: basta un watcher (inotify) sobre el directorio
  `current/theme` y releer `colors.toml` cuando cambie el symlink. Cero daemons.
- Fuera de Omarchy: el mismo parser lee un `colors.toml` propio en
  `~/.config/files/theme.toml`; en Windows/macOS se puede seguir el modo claro/oscuro del SO.

Mapa semántico `colors.toml` → UI:

| Clave | Uso en files |
|---|---|
| `background` | fondo de la ventana / área de contenido |
| `darker_background` | riel, inspector, omnibar, toasts |
| `lighter_background` | tarjetas de carpeta, hover, chips de ruta |
| `selection` | fila/tarjeta seleccionada |
| `muted` | bordes (con alpha), separadores |
| `foreground` / `bright_foreground` | texto normal / nombres |
| `dark_foreground` | metadatos, placeholders, iconos inactivos |
| `accent` | segmento actual de ruta, foco, progreso, botón primario |
| `blue` carpetas · `magenta` imágenes · `cyan` texto · `green` scripts · `yellow` comprimidos · `red` destructivo |

## E. UI v2 — "fuera de la caja" (mockups/v2.html)

Qué cambia respecto a Flea/Nautilus:
1. **Omnibar única**: la ruta son fichas clicables, y el mismo campo filtra al escribir,
   ejecuta comandos con `>` y salta a lugares con `:`. Desaparecen la barra de dirección,
   la caja de búsqueda y el breadcrumb separados.
2. **Riel colapsado** de 52 px que se expande al pasar el mouse, en lugar de sidebar fija.
3. **Vista Grupos** por defecto: carpetas como tarjetas con conteo, tamaño y "hace 2 h";
   archivos agrupados por fecha (Hoy / Esta semana / Antes) con badge de tipo coloreado.
4. **Inspector** a la derecha: vista previa, metadatos y acciones. Reemplaza el menú
   contextual para quien usa mouse. `F9` lo convierte en **segundo panel** (modo MC).
5. **Vista Espacio**: treemap de tamaños calculado por un helper que muere al terminar.
6. **Operaciones como toast** con progreso, pausa, cancelar y deshacer. Sin diálogos modales.
7. **Atajos bajo demanda** (`?`), no barra F permanente; las F de MC siguen funcionando.
8. Tipografía proporcional (sans), filas de 40 px, esquinas 10–14 px. Mono solo en `kbd`.

---

# Adenda 0.0.2 (2026-10-26, tercera ronda): licencia MIT y bordes

## F. Restricción nueva: open source permisivo (MIT ideal)

Slint queda fuera: su licencia es GPLv3 o "Royalty-Free" propietaria, ninguna permisiva.
Se midieron las alternativas permisivas con la misma prueba (10k filas, ventana 900×600):

| Stack | Licencia | PSS | RSS | Observación |
|---|---|---|---|---|
| Slint (referencia, descartado) | GPLv3 / propietaria | 19 MB | 31 MB | |
| **fltk-rs** | MIT (FLTK: LGPL con excepción de enlace estático) | **27 MB** | 44 MB | binario 1.6 MB; dibuja con Cairo/Pango en Wayland |
| iced + tiny-skia | MIT | 142 MB | 159 MB | sin lista virtualizada en la librería: instancia 10k widgets; descartado |
| winit + softbuffer + tiny-skia + cosmic-text | MIT / Apache-2.0 / BSD-3 | 41 MB | 57 MB | UI 100 % propia; cosmic-text carga la base de fuentes del sistema (~20 MB anon) y softbuffer usa 14 MB de shmem. Optimizable, pero hoy pierde contra FLTK |
| QML software (referencia) | LGPL | 51 MB | 104 MB | |

## G. Decisión de stack (0.0.2)

**Rust + fltk-rs**, usando FLTK solo como capa de plataforma: ventana, eventos, texto,
portapapeles, arrastrar y soltar, HiDPI, diálogos nativos. **Toda la interfaz visible se
dibuja a mano** sobre el canvas de FLTK (`draw()` propios), igual que haríamos en cualquier
opción, así el look del mockup no depende de los widgets de FLTK.

Regla de arquitectura: la capa de dibujo expone una interfaz mínima (`fill_rounded_rect`,
`text`, `clip`, `image`). Si FLTK estorba, se cambia por tiny-skia + softbuffer sin tocar
las vistas. Esa es la salida de emergencia. Medido, el stack raw hoy cuesta 41 MB sin optimizar, así que FLTK es la opción más barata **y** la más madura entre las permisivas.

## H. Bordes: seguir a Omarchy, con preferencia propia

- Fuente viva: `hyprctl getoption decoration:rounding -j` → `{"int": 0}` en esta máquina
  (Omarchy por defecto usa 0; el toggle sin gaps también fuerza 0). Se consulta al arrancar
  y al recibir el hook de tema; sin daemons.
- Preferencia en la app: `corners = "omarchy" | "square" | "rounded"`. "omarchy" mapea
  `rounding = 0` a cuadrado y `rounding > 0` a redondeado escalando un factor `r` (0 → 1).
- Todo radio de la UI se expresa como `base_px × r`: ventana, omnibar, fichas de ruta,
  tarjetas, filas, botones, toasts, barra de progreso.
- **Excepción fija**: thumbnails, badges de tipo y la vista previa del inspector conservan
  siempre su radio (6–10 px). El usuario los quiere curvos aunque el resto sea cuadrado.

## I. Ajustes del mockup v3 (mockups/v3.html)

- Cabeceras de sección más pequeñas (10 px, peso 500, menos espacio vertical).
- Columna de tamaño con numerales tabulares y unidad alineada en su propia subcolumna,
  para que "774 kB" y "1.2 MB" queden en línea.
- Filas 2 px más compactas.
- Selector "Bordes: Seguir Omarchy / Cuadrados / Redondeados" como demostración de la preferencia.

---

# Adenda 0.0.3 (2026-10-26, cuarta ronda): ¿GPU?

Misma prueba (10k filas, 900×600), esta máquina (Asahi, Apple M2 AGX; OpenGL por hardware
vía Mesa/asahi; **sin ICD Vulkan de hardware instalado**, solo lavapipe por software):

| Stack | Render | Licencia | PSS | RSS | Δ vs mismo stack en software |
|---|---|---|---|---|---|
| fltk-rs | software (Cairo) | MIT | 27 MB | 44 MB | — |
| fltk-rs + GlWindow | OpenGL (EGL, contexto vivo, sin dibujar) | MIT | 42 MB | 57 MB | **+15 MB solo por el contexto GL** |
| egui/eframe + glow | OpenGL (EGL/asahi) | MIT/Apache | 62 MB | 71 MB | |
| egui/eframe + wgpu | Vulkan → cayó a **lavapipe (CPU)** | MIT/Apache | 68 MB | 78 MB | |
| QML Qt Quick (ref.) | GPU | LGPL | 90 MB | 146 MB | +39 MB vs QML software |

Lecturas:
1. **El contexto GPU cuesta 15–40 MB antes de dibujar un píxel** (driver, shaders, pools de
   buffers). Es la mitad del presupuesto total.
2. Los toolkits GPU-first permisivos (egui, iced-wgpu, Floem, Xilem/Vello, GPUI, Makepad)
   parten de 60–70 MB. Ninguno baja de 50 MB con una ventana vacía.
3. wgpu es realmente multiplataforma (Vulkan/Metal/DX12/GL), pero en una máquina sin
   Vulkan por hardware cae a lavapipe: **render en CPU con el coste en RAM de la GPU**.
4. Un gestor de archivos no necesita GPU: el compositor ya compone en GPU; nosotros
   repintamos por software solo la región dañada. Las "animaciones" del mockup (hover,
   riel que se expande, toasts) son interpolaciones de 150–250 ms que la CPU dibuja sobrada.

**Decisión 0.0.3: sin GPU en v1.** La capa de dibujo queda abstraída (`Painter` con
`rect`, `rounded_rect`, `text`, `image`, `clip`) y se añade un backend GL/wgpu **opcional**
solo si aparece un caso real (p. ej. grilla de miles de thumbnails con zoom fluido), siempre
activable por preferencia y nunca por defecto. Objetivo de la v1 con FLTK software: ≤ 35 MB
PSS con 10k archivos, para dejar margen a thumbnails y operaciones.

---

# Adenda 0.0.4 (2026-10-26): shaders opcionales, estilo omarchy-t3

Referencia: `~/Work/omarchy-t3-code/ui/assets/shaders/` (GLSL compilado a `.qsb` para Qt):
- `threadaura.frag` — aura por fila: cometa orbitando el borde (working), borde que respira
  con destellos (attention), estrella fugaz al cambiar de estado (burst). Varios estilos.
- `reveal.frag` — transición mostrar/ocultar por región con `progress` 0..1: nebulosa,
  warp con separación RGB, pliegue de acordeón.
- `starfield.frag` — fondo procedural de estrellas con paralaje, titileo y meteoros.
- `activity.frag` — brillo gaussiano para texto de actividad.

Patrón común: **fragment shader por región rectangular**, uniformes `time/clock`,
`progress`, `style`, colores del tema y coordenadas del panel; opcionalmente un `sampler2D`
con el contenido ya pintado debajo (`source`).

## Cómo encaja en files sin pagar GPU por defecto

```
trait Painter {                      // backend software (FLTK/Cairo) y backend GPU (futuro)
    fn rect / rounded_rect / text / image / clip ...
    fn effect(&mut self, region: Rect, id: EffectId, u: &Uniforms) -> bool; // false = no soportado
    fn capture(&mut self, region: Rect) -> Option<TextureId>;             // `source` para shaders
}
```

- **Backend software**: `effect()` devuelve `false`. Cada efecto declara un **fallback CPU**
  barato: aura → borde con color de acento; reveal → fade+slide 180 ms; starfield → nada
  (fondo plano); activity → sin brillo. La app es completa sin GPU.
- **Backend GPU** (opcional, preferencia `effects = off | on`, por defecto `off`):
  wgpu o GL. Toda la UI pintada por CPU se sube como textura una vez por frame dañado y
  los efectos se dibujan como pases de fragment shader sobre/bajo esa textura. Los shaders
  se escriben en WGSL o GLSL (portables: `.frag` → `naga` los traduce), y **rust-gpu es
  viable aquí** si se prefiere escribirlos en Rust, aislado en el crate del backend GPU.
- Coste esperado al activar efectos: +15–40 MB (contexto). Aceptable porque es opt-in y
  se muestra en la barra de estado ("RAM 58 MB · efectos").
- Mismo set de uniformes que omarchy-t3 (`time`, `progress`, `style`, colores de
  `colors.toml`) para poder portar `threadaura` y `reveal` casi tal cual.

Casos de uso previstos: aura en operaciones en curso (fila copiándose = comet working;
conflicto = attention), reveal al abrir inspector/segundo panel, starfield opcional en el
fondo de la vista Grupos cuando la carpeta está vacía.

---

# Adenda 0.0.5 (2026-10-26): vista previa, red, copia, búsqueda

Regla transversal: **el proceso principal nunca carga decodificadores, pilas de red, modelos
ni índices**. Todo eso vive en `files-helper <subcomando>`, un segundo binario del mismo
repo que arranca bajo demanda, habla con la app por stdio (JSON por líneas + blobs por
memoria compartida/archivo temporal) y muere al terminar o tras N segundos ocioso.

## J. Vista previa (Space)

- `Space` abre un panel flotante sobre la ventana (sin diálogo modal); `Space` o `Esc` lo
  cierra; flechas navegan sin cerrarlo. Tecla `i` sigue siendo el inspector fijo.
- Lo renderiza `files-helper preview <ruta>` y entrega un bitmap por memoria compartida
  (o texto ya formateado). Al cerrar, el helper muere: **0 MB residentes** por previsualizar.
- Tipos v1: imágenes (crates `image`/`zune-*`, downscale al tamaño del panel, nunca la
  imagen completa en RAM), texto y código (resaltado ligero, primeras 2 000 líneas), Markdown,
  PDF (página N via `mutool draw` si existe, si no `pdfium-render` opcional), vídeo (frame +
  duración via `ffmpeg` si existe), audio (metadatos), archivos comprimidos (listado),
  carpetas (conteo + tamaño). Office: primero texto extraído; render fiel después.
- Multiplataforma: la lista de renderizadores se resuelve en runtime según herramientas
  presentes; sin dependencia dura de ninguna.

## K. Red (SMB, SFTP, WebDAV, FTP, NFS)

- Capa `Vfs` en `core`: `list`, `stat`, `open_read`, `open_write`, `rename`, `remove`,
  `mkdir`, `watch?`. Backends: `local` (en proceso) y los de red, **todos en
  `files-helper net`**, para que TLS, SSH y libsmbclient no vivan en el principal.
- SMB: libsmbclient via FFI (LGPL, enlace dinámico; en Windows se usa UNC nativo, en macOS
  `smb://` del sistema como alternativa). SFTP: `russh` + `russh-sftp` (Apache/MIT, puro
  Rust). WebDAV: `reqwest`. FTP: `suppaftp`. NFS: montaje del sistema, no cliente propio.
- Descubrimiento opcional: mDNS (`mdns-sd`) y WS-Discovery para SMB. Credenciales en el
  llavero del sistema (`keyring` crate: Secret Service / Keychain / Credential Manager).
- URLs en la omnibar: `smb://nas/fotos`, `sftp://user@host/ruta`. Aparecen en el riel bajo
  "Red" con estado de conexión. Las operaciones de copia pasan por el mismo motor (sección L)
  con streaming, sin descargar a temporal salvo que el destino lo exija.
- Sin GVFS ni KIO: son daemons permanentes y solo Linux.

## L. Copia hiper-optimizada

Motor `ops` en el proceso principal (hilos, no helper): necesita progreso fino y cancelación
inmediata; su memoria es un pool fijo de buffers (máx. 4 × 4 MB).

Orden de intentos por archivo:
1. **Clonación CoW** cuando origen y destino están en el mismo FS que la soporte:
   Linux `FICLONE` (btrfs, XFS, bcachefs), macOS `clonefile`/`copyfile(COPYFILE_CLONE)`
   (APFS), Windows ReFS block clone. Instantáneo y sin ocupar espacio extra.
2. **Copia en kernel** sin pasar por espacio de usuario: Linux `copy_file_range`
   (también entre FS en kernels modernos), macOS `fcopyfile`, Windows `CopyFileEx`.
3. **Streaming** con buffers de 1–4 MB, lectura adelantada y, para archivos grandes,
   `posix_fadvise(SEQUENTIAL)`/`O_DIRECT` opcional; io_uring en Linux cuando esté disponible
   (feature opcional).
- Paralelismo adaptativo: muchos archivos pequeños → N workers (N = cores, tope 8);
  pocos grandes → un stream por dispositivo destino. Se mide el throughput y se ajusta.
- Archivos dispersos (`SEEK_HOLE/SEEK_DATA`), xattrs, tiempos, permisos, enlaces simbólicos.
- Mismo dispositivo → `rename` (mover es instantáneo). Entre dispositivos → copia + verificación
  opcional (xxhash) + borrado.
- Reanudable, pausable, cancelable; cola única visible en el toast; deshacer (mover → mover
  de vuelta; copiar → borrar lo copiado; borrar → papelera propia según spec freedesktop /
  Recycle Bin / macOS Trash).
- Métrica objetivo: igualar `cp --reflink=auto` y superar a Nautilus en árboles de muchos
  archivos pequeños (donde el paralelismo manda).

## M. Búsqueda

Tres niveles, todos desde la omnibar con prefijo, resultados fusionados en una sola lista:

| Prefijo | Qué | Cómo | Dónde corre |
|---|---|---|---|
| (nada) | filtro instantáneo de la carpeta actual | en memoria, fuzzy (`nucleo`) | principal |
| `/texto` | nombres en todo el árbol o el disco | índice propio (ver abajo) o `plocate`/`fd` si existen | `files-helper find` |
| `?texto` | contenido literal/regex | motor de `ripgrep` (`grep` crates, MIT) con límites | `files-helper grep` |
| `~frase` | semántico, "la factura del notebook de septiembre" | embeddings + índice vectorial | `files-helper semantic` |

Índice de nombres: solo para raíces que el usuario marca (Home por defecto). Archivo en
disco (`~/.local/share/files/index/`), estructura trigramas + tabla de rutas; actualización
incremental por inotify/FSEvents/ReadDirectoryChangesW mientras la app está abierta y
re-escaneo al arrancar. Nunca se carga entero en RAM: mmap y consulta por páginas.

Semántico (EmbeddingGemma):
- Modelo: **EmbeddingGemma 300M** (Google), formato GGUF, ejecutado por `llama.cpp`
  (`llama-server --embedding` o `llama-embedding`), que ya está en esta máquina y corre en
  Linux/macOS (Metal)/Windows. Alternativa sin llama.cpp: `candle`/`ort` dentro del helper.
- El modelo **no se distribuye con la app**: se descarga bajo demanda al activar la función
  (licencia Gemma Terms of Use, no es MIT; aceptación explícita en Preferencias).
- Pipeline en `files-helper semantic`: extraer texto (txt/md/código/pdf via `mutool`/
  `pdf-extract`/office via `zip`+XML), trocear ~512 tokens con solape, embeber, guardar en
  índice vectorial en disco (`usearch` o HNSW propio; Apache). Consulta: embeber la frase,
  top‑k por coseno, re-ordenar con nombre/fecha. Las carpetas indexadas se eligen en
  Preferencias; el helper muere al terminar y el modelo sale de RAM con él.
- Coste: ~400–700 MB de RAM **solo mientras indexa o consulta**, en otro proceso. El principal
  sigue en su presupuesto. Se muestra en la barra de estado mientras ocurre.

## N. Helpers (resumen)

`files-helper {preview|thumb|du|net|find|grep|semantic}` — un binario, varios procesos.
Protocolo: una línea JSON por petición/respuesta; bitmaps y blobs por `memfd`/archivo
temporal. Cada helper tiene tope de RAM (`RLIMIT_AS` en Linux, job objects en Windows) y
tiempo de vida ocioso configurable (por defecto 30 s).

## Fases (actualizadas)

1. 0.0.1 núcleo + vista Grupos + tema vivo + test PSS.
2. 0.0.2 operaciones con motor de copia (clonación, kernel copy, paralelismo), papelera, deshacer.
3. 0.0.3 vistas restantes, inspector, segundo panel, vista previa con Space (helper).
4. 0.0.4 búsqueda: filtro, `/` índice de nombres, `?` contenido.
5. 0.0.5 red: SFTP y SMB primero, WebDAV/FTP después.
6. 0.0.6 semántico con EmbeddingGemma; thumbnails; vista Espacio.
7. 0.0.7 Windows y macOS; efectos GPU opcionales.

---

# Nota de versionado (2026-10-26)

Regla global del usuario: las versiones avanzan lento (0.0.1, 0.0.2, …, 0.0.25, …). Las
fases llamadas aquí 0.0.1…0.0.7 se leen como hitos **0.0.1, 0.0.2, …**; el número exacto lo
fija cada entrega. Cualquier salto a 0.1.0 o 1.0.0 se propone y lo autoriza el usuario.

# Hallazgos de implementación (0.0.1, 2026-10-26)

- **Grupos implica ordenar por fecha.** El `readdir` inicial conserva sólo nombre y
  tipo; un trabajador carga los metadatos para clasificar por fecha después de la
  primera pintura. Mientras tanto hay un grupo transitorio «Por fechar». En Lista
  por nombre sólo se consultan las entradas visibles; ordenar por tamaño/fecha
  solicita el resto. No se crean widgets por entrada.
- Las tarjetas muestran conteo de hijos inmediatos y suma de tamaños de archivos
  inmediatos. No representan tamaño recursivo: `du` sigue como protocolo sin
  implementación. Los enlaces simbólicos no se recorren durante el conteo.
- **FLTK sólo dibuja por software**, sin `enable-glwindow`, con `no-images` para no
  incorporar decodificadores de thumbnails a la UI. Las bibliotecas de texto del
  sistema sí enlazan libpng de forma transitiva (Cairo/FreeType); no se decodifican
  imágenes de archivos en este hito. La barra de estado se invalida por su región;
  los cambios de navegación/layout invalidan el lienzo. No hay animaciones continuas.
- Linux usa una ventana sin decoración de cliente, de acuerdo con el mockup. Evita
  el trabajo y la superficie de libdecor por ventana; dejó diez arranques consecutivos
  bajo 100 ms en la medición guardada. El compositor sigue administrando mover,
  redimensionar y cerrar. No se modificaron reglas persistentes de Hyprland.
- **Límite de FLTK/Wayland en el contorno exterior:** su máscara nativa usa Cairo
  A1 (un bit, sin anti-aliasing alfa fino). La prueba experimental mostró picos de
  48–54 MiB durante redimensionados; también hubo una carrera al fijar la geometría
  desde Hyprland, por lo que ese pico no permite atribuir un coste aislado al recorte.
  No se incorpora esa ruta experimental en 0.0.1; falta resolver anti-aliasing y
  validar su presupuesto de forma aislada. **El contorno exterior lo administra
  Hyprland**; la preferencia square/rounded/omarchy se aplica a la geometría interior.
  Los badges y el recuadro del inspector conservan radios fijos. Es una limitación
  explícita para revisar con otro backend o una corrección upstream.
- El watcher observa `current/theme`, sus padres y el destino resuelto. Tras cambios
  atómicos vuelve a adjuntar las vigilancias; debounce de 100 ms, cola acotada de una
  notificación y conservación de la última paleta válida ante escritura parcial.
- Licencias CC0/ISC/Unicode-3.0: excepciones específicas autorizadas por el usuario;
  ver `DEPENDENCIES.md`. No se amplió la política para otras dependencias.
- CUA 0.28.2 no descubre ventanas en este Hyprland 0.56.2 (`Found 0 windows`;
  identificador Wayland rechazado como `stale`). Las escenas verifican el modelo de
  UI dentro de una ventana Wayland real. No equivalen a prueba de entrega de clics
  del SO. Las capturas finales salen del framebuffer real de FLTK, sin superponer
  otras ventanas ni incorporar su contenido.
- DnD, portapapeles de archivos, thumbnails, vista previa de contenido, operaciones,
  red, búsqueda recursiva y helper `du` real siguen fuera de este hito. El inspector
  muestra tipo/metadatos y apertura; su recuadro representa el tipo, sin decodificar.
- Compilación y prueba de escritorio realizadas en Linux aarch64/Wayland. Windows,
  macOS, X11 y entrega de eventos de ratón por controlador nativo quedan sin probar.

# Actualización explícita del usuario (2026-10-26): alcance y Nerd Fonts

El alcance posterior a 0.0.1 incluye todo el plan y capacidades modernas, shaders
reales opt-in incluidos; la matriz y aceptación observable están en ROADMAP.md.
La tipografía sans proporcional anterior queda sustituida por **Nerd Fonts** en
toda la UI, también toolbar/riel/inspector/espacial. Se resuelve la fuente monospace
Nerd de Omarchy mediante una consulta acotada fontconfig, con fallback legible fuera
de Omarchy, sin enumerar/rasterizar el catálogo y sin cambiar configuración global.
El techo de 150 MB comprende principal y helpers; 50 MB es objetivo, no permiso
para omitir coste de hijos/GPU. El modo EmbeddingGemma de 400–700 MB permanece
bloqueado por incompatibilidad de presupuesto, sin frenar otras capacidades.


## Hallazgos de implementación0.0.2 (2026-10-26)

- Nerd Fonts reemplaza la sans previa por instrucción explícita. Se resuelve la
  familia monospace del host con una consulta fontconfig; glifos rasterizados en
  framebuffer real. Fixtures aíslan sólo FILES_CONFIG_DIR para conservar ese mapping.
- Persistencia espacial y bordes: límites previos a asignación, validación de datos
  anteriores, temp exclusivo+rename/sync, snapshots Arc coalescidos hasta ACK. Fallos
  sobreviven navegación/cierre; aliases usan identidad canónica resuelta fuera UI.
  Mover/renombrar carpeta requiere futura migración de identidad path+dev+ino.
- El cierre espera guardar o descarte explícito, y cancelar/recoger helper. Esto no
  aporta journal durable frente a kill/power loss ni aislamiento de escritores hostiles.
- El sampler mide árbol PSS+Swap y registra salidas confirmadas durante lectura.
  Completo significa lecturas verificables de supervivientes, no cobertura de lifetimes;
  máximos muestreados siguen siendo límites inferiores, sin GPU. Detalle VALIDATION.
- Calcular tamaño usa helper du efímero FD/no-follow con límites y supervisor; muestra
  tamaño aparente por nombre, no bloques físicos ni snapshot. Treemap sigue pendiente.
- La geometría observada2x y captura FLTK lógica acreditan layout, no toda la nitidez
  física del compositor. El contorno exterior y eventos SO conservan límites anteriores.
- GLES3 hardware Apple M2 Pro comprobado; cuatro shaders MIT portados/dibujados en
  fixtures. Aún no hay efectos reales integrados en Painter ni presupuesto ON/OFF app.
- Arranque estricto sigue FAIL(max111,423ms sin sampler); no se oculta con mediana.
  Objetivo ideal50MB incumplido en Grupos100k(55,59MB), no techo150MB ampliado.

- Regresión final0.0.2: baseline árbol+Swap40,81MiB pico,28,63MiB estable; FAIL del
  umbral de análisis40MiB. Se conserva evidencia anterior. Lectura estática FLTK
  Wayland1.5.23 permite pools simultáneos y retención de current_pool grande
  (10MB mínimo o2×framebytes), pero no prueba causalidad. Diagnósticos de mappings
  tienen acceptance=false; Size no es PSS. Sin parche global ni supuesto ahorro.
  Usuario suspende nuevas mediciones0.0.2 y autoriza continuar integración0.0.3;
  rendimiento pendiente no se convierte en PASS por cerrar checkpoint.

## Hallazgos de implementación0.0.4 (2026-10-26)

- fltk-rs1.5.23 `app::wait_for` convierte retorno de C a i32 y acepta sólo0/1;
  backend Unix devuelve número de FD listos de poll, legítimamente>=2. Fixture
  aislada de dos pipes: raw2/callbacks2/errno0, wrapper Rust lo rechazaría. UI ahora
  llama la misma función C enlazada mediante wrapper local, acepta raw finito>=0,
  captura errno inmediato, reintenta sóloEINTR con pausa1ms y conserva errores reales.
  Error fatal cancela/drena operaciones/helpers antes de devolver fallo al binario.
  Sin parche de registry ni dependencia nueva. Traza en escena confirma raw4/3
  exitosos; retorno del fallo original no se instrumentó y no se inventa causalidad.
- Clipboard de Wayland necesita foco/serial de compositor. Fixture sin foco publica
  set_selection serial0 y wl-paste devuelve Nothing is copied; selección interna
  exacta no equivale a clipboard externo probado. Se conserva FAIL y falta repetir
  con foco real de la fixture, sin sustituir app::copy por proceso externo.
- Preview de texto dibuja fuente con primitiva literal de Painter y mapa byte→display
  para tabs/controles; preserva selección/copias de bytes originales. Sólo documento
  activo acotado, sólo líneas visibles; render por línea limitado4096B con aviso.
  HTML/Markdown fuente no se considera preview renderizado, que continúa pendiente.

- P05 experimental: litehtml/Gumbo nativo entrega comandos limitados a Painter,
  sin browser/JS/red. No se usa código UC/Sun strtod fuera de allowlist; bridge
  MIT aislado. document_container se llama después de que Gumbo crea su árbol:
  cap de callbacks NO es cap previo de parser. RLIMIT_AS64MiB de UI no es PSS.
  Métricas escalares0.6×size muestran recorte CJK/celdas; no se declara shaping ni
  preview HTML completo. Falta allocator/recursos/contraste/packaging y presupuesto.
  Fase actual sólo integración/capturas: sin nuevas mediciones hasta liberación.

## Decisión explícita de ventana (2026-10-26, 0.0.4)

Usuario sustituye el borderless Linux forzado por Normal por defecto, gestionada
por el compositor sin imponer flotante/panel/overlay. Preferencias y Acciones
separados con Nerd Fonts; modos normal/pantalla completa persistidos
independientes de corners/tema. Header libre y grips interiores24px solicitan
move/resize Wayland con serial real, sin robar controles ni tocar config global.

Hallazgo: fltk-rs no expone resize interactivo Wayland. Adaptador C++ local contra
headers privados del pin FLTK usa libdecor, validando foco/ventana/evento/serial;
no se parchea registry. Build sin metadata de headers exige discovery acotado de
la fuente fijada y configs idénticas o override explícito. No prueba de gesto SO
ni accesibilidad AT-SPI; límites y verificación en WINDOW-MODES.md. Se mantiene
freeze de recursos/startup y ningún PASSglobal de0.0.4.

### Prioridad explícita del usuario — 2026-10-26
Tras Propiedades: investigar cierre Super+W en bind efectivo/compositor/libdecor,
sin modificar configuración global; apertura por comando `open` del sistema con
argv directo, sin shell ni fallback xdg-open (alternativa pendiente si falta).
Integrar después Espacio `#v-map` como treemap proporcional de bytes reales con
helper cancelable, selección/inspector y navegación: distinto de Espacial y du.
No reproducir el texto ficticio «0 MB residentes» del mockup. Freeze de recursos y
arranque continúa; sólo integración, pruebas funcionales y capturas en esta fase.

### Ocultos globales y Espacio — decisión explícita 2026-10-26

Ocultos por prefijo punto desactivados por defecto. Espacio, Preferencias y Ctrl+H
controlan el MISMO estado global persistido; sin overrides de carpeta/sesión.
Selección/operaciones excluyen elementos ocultados. Treemap normaliza áreas entre
hijos visibles; total aparente y carpetas incluyen descendientes ocultos y se indica.
Estilo se acerca a v3 mediante bloques de tema contiguos y header compacto.
Frame label nativo vacío elimina ayuda detrás del contenido; tips pasan a bottom
bar con clip/repaint regional/pausas. Contextual real B3 usa Actions existentes.
Detalles, límites de foco/minimización y evidencia en HIDDEN-CONTEXT-AND-STATUS.md.
Freeze recursos/startup continúa, sin nuevas métricas ni PASSglobal de0.0.4.


### Medios bajo demanda: publicación e invalidación (2026-10-26)

Prioridad explícita del usuario: previews/miniaturas antes de más funciones
periféricas. MEDIA-LIFECYCLE conserva stack y motores: firma dev/ino/size/mtime/
ctime del FD real, petición imagen checked y source efectivo, generación UI,
watch de padre con época monotónica y reconciliación fuera UI. O_NOFOLLOW final
no confina symlinks de padres. Un worker readonly atascado no se reemplaza por
más hilos ni se join en UI; ACK de esa sonda se distingue de helpers reap. Kernel
ininterrumpible puede demorar terminación; offline sigue pendiente. M1 Linux
seleccionado verificado funcionalmente; LRU/miniaturas/embedded/otrosSO y métricas
siguen abiertos, sin disco nuevo ni ampliación de presupuesto.


### Miniaturas y superficie eliminada (2026-10-26)

M2: mismo motor imagen/vídeo a160px, caché owning con préstamos Grilla/inspector,
capacity+reserva antesdecode,32entradas y demanda visible acotada; sin disco nuevo.
Radio8px CPU sobre imagen real (incluido letterbox), independiente de corners.
WidgetTracker FLTK eliminado produjo un panic aislado sin backtrace: causa abierta,
no atribuir a cleanup. Guardas al volver del wait/flush detectan ventana/canvas
eliminados y cancelan/drenan por salida de fallo sin usar su puntero. Fixture de
C++destrucción propia/helpervivo verifica esa ruta, NO Super+W/inputSO ni causa del
fallooriginal. No parche registry, no cambio stack. MEDIA-THUMBNAILS distingue
cachelocal de presupuesto global/PSS y mantiene freeze y pendientes.

### Recursos incrustados ligados al FD (2026-10-27, 0.0.4)

RELATIVE-SOURCE/DOCUMENT-MEDIA: rootFD/fileFD esperado antesdelectura yefectivo
enrespuesta; candidatoMDoculto hasta watcher/probefinal/token/época estables.
Un solo worker readonly existente,32directorios/8recursos, intencionesausentes
para reconciliar recreaciónsinRGBviejo. Watchopcionales pueden recurrir a1s
reconcile ysu diagnósticoUIindividual queda pendiente. Rootaliasdirectopreservado,
contextorelativoNOFOLLOW, no sandbox ni snapshotatómico. StopreadonlysinjoinUI
se distingue dereaphelpers; stopACKfalse se conserva. Métricasfreeze intacto,
sin ampliarobjetivo/techo ni declararP09global/otrosformatos completos.

2026-10-27 — Hallazgo de audio en Space: el prototipo ffplay devuelve exit0
incluso ante cierto fallo de dispositivo, y su EOF nativo tras seek agotó timeout
mientras dummy pasó; causa interna no atribuida sólo por esa diferencia. La
salida actual usa ffmpeg/Pulse del SO, mismoFD/supervisor, stderr acotado estricto
y controles propios. Pulse no creó contexto bajo AS512MiB y el fixture sí pasó
con AS768MiB (como vídeo), sin cambiar el techo REAL150MB ni medir PSS. Evidencia
funcional silencio/EOF/cancel/ACK en AUDIO-PLAYBACK; recursos/startup congelados.


2026-10-27 — Vídeo audio opt-in: mismo ffmpeg directo con outputs RGB/Pulse,
muted por defecto. Contexto Pulse con AV falló mmap bajoAS768MiB/arenasdefault;
MALLOC_ARENA_MAX2 sóloenchild permite contexto conservandoAS768. Contrato
funcional no equivale a PSS/CPU/techo150MB. Source failure tipado exige nuevo
watch/probe, error de salida conserva frame verificado y permite mute/retry.
VIDEO-PREVIEW documenta evidencia y límites; freeze sin nuevas mediciones.


## Decisión del usuario — Columnas sustituye Espacial (2026-10-27)

El usuario indica que Espacial no agrega valor y pide reemplazarla por Columnas
Miller tipo Finder. Espacio/treemap permanece. Selector Ctrl+3/ayuda/CLI pasan a
Columnas, --spatial conserva alias compatible. Sesión tag3 legado → Columns con
zoom/pan/scroll de iconos descartados; tag5 nuevo conserva scroll de Columnas.
Layouts anteriores se conservan sin borrar ni reutilizar coordenadas como
columnas. COLUMNS-CONTRACT detalla origen de operaciones en columna enfocada,
virtualización y ventana deslizante de hasta8 listados sin límite de profundidad.
Previews nuevas pausadas, métricas congeladas; doble panel queda siguiente.


## Implementación M27 — doble panel MC (2026-10-27)

F9 convierte inspector en segunda lista viva y Tab cambia foco fuera de inputs.
Un Model + Browser movible, dos listados virtualizados y un worker secundario
transferido exclusivamente desde/hacia Columnas. Una cola/journal/medios/AuxIo.
F5/F6 capturan destino raw opuesto revisable; IDs (panel,tab), selección/query/
historial/scroll independientes. F9off libera datos invisibles; snapshots3MiB
conjunto y listado32MiB local, sin PSSclaim. Persistir ambas mitades, input SO y
gates reales siguen pendientes; DUAL-PANE-CONTRACT delimita la entrega.


### 2026-10-27 — M28 clipboard privado y frontera MIME

CtrlC/X/V archivos usa captura raw/FD en AuxIo, diálogo revisable y cola existente;
Cut parcial consume sólo publicaciones confirmadas por generación y subset.
Primera entrega interna a ventana, O06 parcial: no sustituye MIME SO/DnD. En
Linux fijado la consulta Rust text/plain pasaba CString pero Wayland compara
punteros; puente local usa Fl::clipboard_plain_text sin tocar registry. Escena
texto propio inmediato y tras eventloop verifica callback; foco/serial externo
sigue sin validar. Para M29 sólo gesto/ventana propia y serial real; no data-control
para sortear foco ni leer/apropiar clipboard sin acción. Freeze/previews pausadas.


### 2026-10-27 — M29a clipboard nativo y coexistencia DnD (parcial)

Bridge propio wl_data_device/listeners, GesturePermit de serial/foco reales y
epoch invalidada antes de borrow Model; transporte FD bounded en AuxIo. Capas
codec/pipe/State/UIrechazo verificadas; MIME SO NO validado, sin data-control ni
fabricar foco. FLTK1.5.23 Wayland negocia DnD/roundtrip en callbacks ajenos aun
con handler false; no adoptar doble-device como prueba de coexistencia. Estudiar
hook opt-in reproducible bajo repo antes enter/motion/drop, default y cleanup
ajenos preservados. M29/O06 siguen abiertos, KDE y multi-origen pendientes.
Previews pausadas, métricas congeladas; NATIVE-TRANSFER-CONTRACT.md delimita caps.


### 2026-10-27 — hook FLTK y receptor DnD parcial

Patch opt-in de la TU Wayland pinned, fuente+licencia FLTK y guard fuerte de
enlace por hash, sin editar registry; receptor COPY y destino raw fijado con
revisión antes cola. 642tests y fixture privado/verificación visual aprobados
en alcance; no hay prueba de compositor ni DnD entre apps. Sender/KDE/varios
padres/O06 completos siguen pendientes. Ver NATIVE-TRANSFER-CONTRACT.md.
