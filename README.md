# files 0.0.4 — en desarrollo

Navegador de archivos liviano en Rust + FLTK: UI propia detrás de `Painter`, render
por software, Grupos/Lista/Grilla/Columnas y treemap Espacio, con Nerd Fonts de Omarchy. MIT.

```sh
cargo build --release
./target/release/files ~/Downloads
cargo test --workspace
make memcheck
```

Requiere Rust stable, CMake, compilador C++ y bibliotecas de desarrollo de Wayland,
xkbcommon, Cairo/Pango y X11 para el fallback de FLTK. Se compila en `target/` y las
carpetas sintéticas viven en `~/.cache/files-test`. Cargo usa `.cargo/tmp` para
sus temporales y fixtures unitarios salvo un TMPDIR explícito del usuario; no `/tmp`.

- Escribe para filtrar; `Esc` limpia. `Ctrl+F` edita sin borrar la consulta,
  con cursor/selección y deshacer; `Enter`/`Tab` vuelven a la lista.
  [Edición del filtro](docs/FILTER-EDITING.md). `Ctrl+L` edita la ruta; `Enter` confirma.
  En la ruta: flechas/Home/End, selección con Shift, componentes con Ctrl,
  deshacer/rehacer de texto y botones confirmar/cancelar. [Contrato](docs/PATH-EDITOR.md).
- `Ctrl+Shift+P` o Acciones → Comandos: catálogo buscable de herramientas, alias
  y atajos, con razones de disponibilidad. [Contrato](docs/COMMANDS.md).
- `Ctrl+Shift+F` busca recursivamente: combina nombres, `ext:pdf`, `size:>10MiB`,
  `after:2026-01-01` y `before:2027-01-01` (fechas UTC). Botón Filtros/F4 para
  mínimo/máximo y fechas; Tab recorre campos. [Filtros](docs/SEARCH-FACETS.md).
  [Progreso visible](docs/SEARCH-PROGRESS.md) durante el recorrido; las rutas se
  publican al terminar y validar, sin porcentaje inventado.
  Consulta y criterios admiten cursor, selección, deshacer/rehacer y edición con
  mouse; Ctrl+F vuelve al campo y ↑/↓ recorren resultados. [Edición](docs/SEARCH-EDITING.md).
- Flechas seleccionan; `Enter` o doble clic navegan carpetas o delegan archivos a
  `open` del sistema, sin shell ni fallback. Si falta, error visible y alternativa
  PENDIENTE. `Backspace` sube. [Apertura y cierre](docs/OPEN-AND-CLOSE.md).
- `Ctrl+1/2/3/4/5`: Grupos/Lista/Columnas/Espacio/Grilla; Lista ordena nombre/tamaño/fecha.
- Espacio: treemap proporcional a bytes aparentes reales de archivos y carpetas,
  selección/inspector, entrar/volver y cálculo cancelable; es independiente de Columnas.
  [Límites y estados parciales](docs/SPACE-MAP.md).
- `Ctrl` alterna selección, `Shift` selecciona rango; `Ctrl+A` todo, `Ctrl+I` invierte.
  Filtra con `*.rs`/`ext:pdf,md` y `Ctrl+Shift+A` marca coincidencias y muestra todo.
  `Ctrl+Shift+D` limpia marcas y foco; menús de selección equivalentes.
  `Insert` marca/desmarca y avanza sin perder el lote; el pie muestra cuántos marcaste.
  `Home/End` va a los extremos; `Shift` extiende rango y `Ctrl` mueve el foco sin
  reemplazar marcas. `Ctrl+Shift` añade un rango. `PageUp/PageDown` recorre páginas.
- Columnas tipo Finder: selecciona carpeta para mostrar su contenido a la derecha,
  `←/→` cambia columna y `↑/↓` selección; `Shift+rueda` desplaza horizontalmente.
  Operaciones sobre la columna enfocada. Sustituye Espacial; layouts antiguos
  se conservan y sesiones legadas migran sin reutilizar coordenadas de iconos.
  [Contrato y límites](docs/COLUMNS-CONTRACT.md).
- F2 renombra con preview, patrones/secuencia/literal/caso; Ctrl+Z/Shift+Z deshace/rehace.
  Ctrl+N crea archivo; F7/Ctrl+Shift+N crea carpeta, sin sobrescribir.
  [Límites y validación del renombrador](docs/RENAME.md).
- Riel52px expandible; inspector con metadatos y **Calcular tamaño** recursivo
  cancelable; Ctrl+Shift+Espacio calcula las carpetas filtradas. Acciones permite
  pausar siguientes cálculos, reanudar y reintentar sólo errores sin repetir totales
  válidos. `Ctrl+Alt+Espacio` abre el gestor con filas, errores desplazables y
  controles por intento. [Gestor](docs/SIZE-MANAGER.md), [cola y límites](docs/FOLDER-SIZES.md). Usa un helper efímero sin seguir
  symlinks; no es treemap ni bloques físicos.
- F9 alterna doble panel; Tab cambia foco fuera de inputs/modales. Cada panel conserva
  ruta, historial, filtro y selección. F5/F6 proponen el destino opuesto para revisar,
  con una sola cola compartida. [Alcance y persistencia](docs/DUAL-PANE-CONTRACT.md).
- F5 copia con cola, pausa/cancelación y diagnóstico de errores/rescate; Ctrl+R recarga
  preservando selección. F6 mueve por rename nativo o EXDEV de un archivo regular
  sin hardlinks con journal y recuperación explícita; árboles/lotes EXDEV pendientes.
- Space abre texto, imágenes, Markdown con recursos locales, HTML experimental,
  PDF multipágina, GIF animado bajo demanda, vídeo con audio opt-in, metadatos y reproducción de audio, resumen de carpeta o
  catálogo ZIP sin extracción. Esc/Space cierra y espera recoger el helper.
  P reproduce/pausa vídeo/audio; M alterna audio de vídeo, silenciado por defecto.
  PDF: T alterna Página/Texto extraído seleccionable; Ctrl+PgUp/PgDn cambia página,
  Ctrl+rueda amplía el raster.
  [Límites y evidencia](docs/ROADMAP.md). Clipboard Wayland con foco sigue pendiente.
- F2: renombrador con tokens/secuencia y reemplazo literal o regex explícito,
  capturas y preview antes de Aplicar. [Sintaxis y límites](docs/RENAME-REGEX.md).
- Papelera (Acciones o Supr): envío local y Deshacer sin reemplazar nombres ocupados,
  hasta 64 elementos por lote. Shift+Supr no elimina permanentemente.
  [Alcance y recuperación](docs/TRASH-CORE-NOTES.md).
- Alt+Enter o Acciones → Propiedades muestra metadatos reales y permite preparar
  permisos r/w/x de un objeto propio, con confirmación por FD y rechazo de cambios
  externos. [Alcance Linux y límites](docs/FILE-PROPERTIES.md).
- Preferencias (, Ctrl+,): ventana Normal por defecto, Pantalla completa,
  bordes Omarchy/cuadrados/redondeados y tema. Acciones (, F10): operaciones sobre
  archivos. Preferencias atómicas conservan campos desconocidos.
- Header libre para mover y cuatro grips interiores para resize nativo Wayland en
  modo Normal; no se impone flotante. [Modos y límites de input](docs/WINDOW-MODES.md).

Tema: `~/.local/state/omarchy/current/theme/colors.toml`; fuera de Omarchy,
`~/.config/files/theme.toml`. `FILES_THEME=/ruta/colors.toml` permite probar una paleta.
`FILES_CONFIG_DIR=/ruta/aislada` cambia sólo la configuración de files y conserva la
resolución de fuentes del escritorio. Sin hooks ni daemon; TOML parcial conserva
la última paleta válida. El contorno exterior sigue administrado por Hyprland.

La última base medida antes de los previews nuevos sigue **FAIL**:
56,743936 MB pico y 41,455616 MB estable del árbol PSS+SwapPss (10k/1080p).
Arranque externo: 90,644 ms p50 y 151,978 ms máximo, 7/10 bajo 100 ms;
**FAIL del gate estricto**. No se extrapolan esos datos al binario actual.

La fase actual autoriza integración/capturas y mantiene suspendidas las medidas
de recursos/arranque. Los previews instalados tienen evidencia funcional, presupuesto
NOT_MEASURED y ningún PASS global ni tag 0.0.4. El pie PSS principal de una
captura no mide el árbol ni Swap; el clip de vídeo además introduce readbacks.

Método, datos y límites: [VALIDATION](docs/VALIDATION.md). Capturas reales del
framebuffer: [Hackerman](docs/evidence/0.0.2/hackerman.png),
[Tokyo Night](docs/evidence/0.0.2/tokyo-night.png),
[Espacial tras reabrir](docs/evidence/0.0.2/spatial-reopen-1x.png),
[du en inspector](docs/evidence/du/inspector.png). Escenas usan handlers internos:
los eventos físicos del SO siguen pendientes por falta de controlador compatible.

```sh
python3 scripts/tabs-ui-check.py --columns
python3 scripts/tabs-ui-check.py --sessions
python3 scripts/du-ui-check.py
```

Las capturas y scripts de Espacial0.0.2 son históricos: Columnas los sustituye.
Recursos/startup siguen congelados; no ejecutar benchmarks en esta fase.


Ejecuta benchmarks de forma serial, sin compilaciones ni capturas concurrentes.
`du` tiene límites/supervisión: [DU-HELPER](docs/DU-HELPER.md). Shaders GLES reales
están portados y dibujados en fixtures hardware, **todavía sin integración Painter**.
Movimiento, conflictos/journal, fechas, previews avanzados, resto de vistas/red
y adaptadores Windows/macOS continúan en la [matriz completa](docs/ROADMAP.md); módulos aislados
no se consideran features entregadas. Sólo Linux aarch64/Wayland está probado.

HTML experimental requiere `make html-helper` para preparar el adaptador opcional
junto a release (fuentes fijadas/verificadas, sin instalación global). PDF usa
pdfinfo/pdftoppm y vídeo/audio ffprobe/ffmpeg del sistema cuando existen.

[Diseño aprobado](docs/DESIGN.md) · [Mockup v3](mockups/v3.html) ·
[Licencias](docs/DEPENDENCIES.md).

Versionado: cada entrega incrementa sólo el último número:0.0.1,0.0.2,0.0.3…
Cambiar los dos primeros requiere autorización del usuario.

Ocultos desactivados por defecto: Espacio, Preferencias y Ctrl+H comparten la
misma opción global persistida. Menú contextual para lote/elemento/fondo y ayuda
en la barra inferior; Espacio usa bloques de color y tamaños reales como v3.
Ver [alcance y evidencia funcional](docs/HIDDEN-CONTEXT-AND-STATUS.md).

Grilla: botón Nerd o Ctrl+5, zoom con Ctrl+rueda/±, selección y navegación por
filas visibles. Miniaturas visibles PNG/JPEG/WebP/vídeo y caché compartida con inspector;
hasta32fuentes bajo demanda.
[Contrato y evidencia](docs/GRID-VIEW.md).

Atrás/Adelante: botones Nerd junto a la ruta o Alt+flechas. Historial por pestaña
acotado. Ctrl+T abre, Ctrl+Tab cambia, Ctrl+W cierra y Ctrl+Shift+T reabre;
hasta ocho pestañas por instancia. Sin ruta en argv restaura la última sesión;
una ruta explícita no restaura ni autoguarda. Preferencias → Guardar sesión
habilita guardado explícito; `--no-session` lo desactiva para esa instancia.
Otra ventana mantiene la sesión en sólo lectura mientras exista un dueño. La omnibar rechaza pegados mayores de4096B
antes de copiarlos. [Historial](docs/NAVIGATION-HISTORY.md) y [pestañas](docs/TABS-CONTRACT.md) y [sesión](docs/SESSION-CONTRACT.md).

Ctrl+Alt+B o Preferencias activa la barra opcional de operaciones MC, con botones
Nerd y teclas F1/F2/F3/F5/F6/F7/F8/F10. [Contrato](docs/OPERATION-BAR.md).

F1 o Preferencias → Ayuda muestra los atajos disponibles con páginas y cierreEsc.
[Validación y límites](docs/SHORTCUT-HELP.md).


Las previews seleccionadas de imagen/vídeo invalidan los píxeles al cambiar,
borrar o reemplazar el original. Revisión comprobada en el FD del decoder y
reconciliación fuera de UI. Miniaturas de Grilla/inspector con LRU acotada ya visibles; caché global entre
formatos/Space y mediciones siguen pendientes.
[Contrato, evidencia y límites](docs/MEDIA-LIFECYCLE.md).

[Miniaturas, invalidación y límites reales](docs/MEDIA-THUMBNAILS.md).


Rutas Unix no Unicode: arranque por args_os e IPC por bytes, incluida imagen
completa en Space. [Evidencia y límites](docs/RAW-PATH-WIRE.md).

Recursos Markdown: [firma de raíz y FD efectivo](docs/RELATIVE-SOURCE.md),
con reemplazos entre sonda y decoder rechazados. [invalidación y reconciliación](docs/DOCUMENT-MEDIA.md) incorporadas para
Markdown local; recursos/startup siguen sin nuevas mediciones.

[JPEG progresivo de 12/24 MP](docs/JPEG-DC-PREVIEW.md): preview reducido1/8
con el mismo límite de coeficientes; aún sin nuevas mediciones de recursos.

[PNG de 12/24 MP por filas](docs/PNG-ROWS-PREVIEW.md): reducción sin raster
completo, alfa, Adam7 y orientación EXIF. Capturas funcionales verificadas;
recursos/startup sin nuevas mediciones por freeze.

[Preescucha de audio](docs/AUDIO-PLAYBACK.md): reproducir/pausar y buscar con
Pulse/ffmpeg del sistema, a demanda y con fuente verificada. Sin inicio automático;
la posición es estimada y la aceptación de recursos sigue pendiente.

Mover: F6, menú contextual o inspector → revisar destino → confirmar en cola.
Movimiento nativo en el mismo filesystem, sin sobrescritura, hasta128 raíces.
Entre filesystems (EXDEV), un archivo regular sin hardlinks usa copia verificada,
publicación sincronizada y journal antes de borrar el origen. Acciones →
Movimientos interrumpidos permite revisar y finalizar/restaurar explícitamente.
Árboles/lotes/hardlinks EXDEV siguen pendientes. [Contrato y evidencia](docs/EXDEV-JOURNAL-CONTRACT.md).

Nuevas mejoras de previews están pausadas; se priorizan funciones y UI:
doble panel, cortar/pegar, DnD, operaciones y SFTP montado.

[Columnas tipo Finder](docs/COLUMNS-CONTRACT.md): cadena con foco/origen de
operaciones independientes del hijo visible, poda de ramas y ventana deslizante
sin límite de profundidad. Sustituye Espacial; sesión antigua tag3 migra sin
coordenadas de iconos y nuevo tag5 conserva scroll. Framebuffer y copia real
verificados, sin inputSO/liveResize/mediciones nuevas.


Ctrl+C/Ctrl+X/Ctrl+V y los menús permiten copiar/cortar/pegar en un
**portapapeles de archivos de files, interno a esta ventana**. Pegar muestra
fuente y destino antes de admitir la operación; Cut conserva pendientes tras
conflicto o cancelación. En campos editables Ctrl+V pega texto del sistema.
MIME de archivos entre aplicaciones y DnD siguen pendientes: [contrato y
validación](docs/FILE-CLIPBOARD-CONTRACT.md). No se reanuda un Cut al abrir sesión.

M29a añade el puente Wayland y «Pegar del sistema» (Ctrl+Shift+V), todavía sin
validación MIME entre aplicaciones. Codec/pipe/State y rechazo sin gesto real
verificados; DnD/KDE/multi-origen pendientes. Ver docs/NATIVE-TRANSFER-CONTRACT.md.
No nuevas métricas mientras el freeze siga vigente.

El receptor DnD COPY y la revisión mediante pipe privado tienen pruebas y
capturas; la interoperabilidad Wayland entre aplicaciones continúa sin validar.
Sender, KDE y varios padres siguen pendientes. Detalles y licencias del hook
FLTK en docs/NATIVE-TRANSFER-CONTRACT.md y vendor/fltk-wayland-transfer/.

Buscar (Ctrl+Shift+F) combina nombres/metadatos con contenido UTF8 literal.
Texto/F6 conserva un borrador independiente, con límites y líneas de coincidencia.
[Contrato de contenido](docs/CONTENT-SEARCH.md).

Favoritos y lugares: **Ctrl+D** guarda la carpeta actual y **Ctrl+Alt+D** o la
estrella Nerd abre el acceso rápido. Filtra etiquetas, navega con flechas/Enter y
edita una etiqueta con F2; Supr quita el favorito sin borrar su carpeta.
[Contrato y límites](docs/BOOKMARKS.md). No sustituye al buscador recursivo.

El destino de Copiar/Mover admite cursor, selección con mouse/teclado, edición en
medio de la ruta y Ctrl+Z/Ctrl+Y. Editar invalida la revisión preparada; mover el
cursor la conserva. Las rutas raw capturadas sólo se sustituyen completas con
Ctrl+A. [Contrato del editor](docs/DESTINATION-EDITOR.md).

En Copiar puedes activar **Omitir existentes (F4)** para continuar con las otras
raíces del lote. Viene desactivado en cada diálogo; no mezcla carpetas ni
sobrescribe. El resultado separa publicaciones y omisiones con rutas solicitadas.
[Contrato de conflictos](docs/COPY-CONFLICTS.md). La opción no se aplica a Mover; resolver conflictos reemplazando o renombrando
sigue pendiente.

Para **copiar con otro nombre**, abre F5 y pulsa **F2**: cambia el nombre destino
del elemento actual. **Alt+↑/↓** o los botones Nerd recorren la selección;
**Ctrl+L** vuelve a la carpeta destino. El botón restaurar recupera el nombre
original del plan, sin renombrar archivos. También permite duplicar en la misma
carpeta. [Contrato y límites](docs/COPY-NAMES.md). Sólo Copiar por ahora; no
reemplaza ocupantes y rechaza nombres de destino duplicados antes de encolar.

Ctrl+L → `>comando` o `:lugar` → Enter abre el catálogo filtrado. Elige luego la acción/destino; no se ejecuta la primera coincidencia automáticamente. `./>nombre` y `./:nombre` siguen siendo rutas literales. Véase docs/OMNIBAR-CATALOGS.md.

Búsqueda recursiva: las coincidencias aparecen durante el recorrido con etiqueta PROVISIONAL. Puedes abrir/localizar una observación explícitamente; cancelación, error o consulta nueva retiran candidatos. El informe final valida la raíz y conserva el archivo seleccionado. Límite500 explícito, sin índice ni nueva cache. Ver docs/SEARCH-BATCHES.md.

El buscador de contenido muestra un fragmento observado de hasta256 bytes con el match marcado, también mientras busca. Elipsis señalan recortes; espacios/controles se normalizan sólo al mostrar. No se abre una previsualización ni se vuelve a leer el archivo. Ver docs/SEARCH-SNIPPETS.md.
