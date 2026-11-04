# Migración de teclado a lib-fltk-omarchy v0.0.2

Revisión del tag `v0.0.2` / `210e0d64` el 2026-10-28. La dependencia y el
adaptador de tema se actualizan; el router `focus::install()` todavía **no se
activa** en Files. No se afirma anillo de foco ni conformidad keyboard-first.

Files dibuja Grupos, Lista, Grilla, Columnas, rail, pestañas y ambos paneles en
un solo `Frame`; los hits son registros del Painter, no widgets FLTK con foco
independiente. Su manejador existente entrega Tab al panel doble, Ctrl+Tab al
historial de pestañas y flechas a selección/grilla/Columnas. El router de la
biblioteca ofrece flechas a `focus::scope`, pero Tab recorre widgets reales;
un único canvas no representa las dos paradas del panel doble. Activarlo sin
migrar el canvas o quitar Tab ahora perdería navegación validada. La migración
requiere controles/paneles enfocables de la biblioteca con cursor virtual y
selección por ID, no copiar un segundo router en Files. Solicitado en
[issue #3](https://github.com/avillagran/lib-fltk-omarchy/issues/3#issuecomment-6477303133).

- Ctrl+Tab/Ctrl+Shift+Tab son atajos existentes de pestañas; el router deja pasar
  modificadores. La integración debe evitar un segundo TabBar que procese el
  mismo atajo y asegurar orden de overlays, omnibar y cierre de pestaña.
- Ctrl+, y F10 abren menús separados; F1 ayuda. B3 y Shift+F10/Menu conservan
  selección y target del panel. Se mantienen como estaban hasta tener PopupMenu
  con cierre observable y límites al viewport.
- v0.0.2 devuelve foco al cerrar y retiene Tab dentro de overlays. Sin embargo,
  `PopupMenu::size` sigue calculando altura completa sin scroll ni clamp y
  `open/open_at` devuelven `()`, sin handle/callback de descarte. Por instrucción
  del usuario no se adopta todavía: [issue #1](https://github.com/avillagran/lib-fltk-omarchy/issues/1#issuecomment-6477297872),
  [issue #4](https://github.com/avillagran/lib-fltk-omarchy/issues/4#issuecomment-6477300247).
- Omnibar, B3, acciones globales, navegación de vistas y grilla siguen usando
  sus handlers existentes; no se introdujo un nuevo manejador de foco ni Tab.
  La validación TOML previa al reload sigue hasta resolver issue #5. El puente
  nativo libdecor no se sustituye por IPC Hyprland.

Aceptación futura: controles reales separados, orden de Tab/Shift+Tab ofrecido
por la biblioteca, anillo visible sólo con teclado, scope de paneles, pruebas
con `focus::press` además de escenas de Files y evidencia SO si está disponible.
Sin entrada SO, esas pruebas no demuestran teclado físico ni compositor.

## Revisión v0.0.7 — 2026-10-28

El tag `3f5978a` incorpora `focus::host`/KeyboardPanel para varias paradas en
un canvas y PopupHandle con cierre observable y viewport con scroll. Esos
bloqueos de API ya no aplican; la integración en Files y la equivalencia de
selección por ID, tabs, omnibar, acciones y cierre aún deben implementarse y
probarse. El router todavía no se activa sólo por cambiar el pin. `Theme::try_load`
y `reload` estrictos permiten retirar la validación local en un paso separado.
W02/libdecor sigue activo hasta una prueba real de gesto en Files. GPU permanece
apagada. No afirmar input SO ni anillo en Files con los fixtures del toolkit.

`Theme::try_load`/`reload`/`last_error` ya reemplazan la validación duplicada
local. El watcher Files sigue gestionando eventos y el override `FILES_THEME`;
la biblioteca decide si una recarga del tema Omarchy es válida.

### PopupMenu: integración y atajos

La aplicación usa PopupMenu para Preferencias, Acciones y contextual; el dibujo
anterior queda sólo para escenas antiguas de smoke que aún lo requieren. La
escena interna `toolkit-menu-ui-1793228594559034966.json` comprueba clamp,
scroll, los cuatro motivos de cierre, F1, F10, Ctrl+, y Ctrl+j. El primer fallo
de v0.0.8 (`toolkit-menu-ui-1793228420725853726.json`) se debió al aislamiento
del propio fixture: éste bloqueaba la entrega al canvas. La excepción sólo se
habilita durante la llamada sintética. El fallo histórico de v0.0.7 permanece
en `toolkit-menu-ui-1793227807327186226.json`; el issue #6 fue cerrado por la
biblioteca. Estas escenas no acreditan input SO. W02 sigue activo y recursos y
arranque siguen NOT_MEASURED.

### Pin v0.0.8 — 2026-10-28

El tag v0.0.8 fija `7d20e19e94133654f7453682d04a4099036a9c4b`,
con una sola copia de fltk/fltk-sys 1.5.23 y GPU desactivada. El guard de
headers continúa activo; la evidencia está en `fltk-omarchy-v008-headers.json`.
El pin por sí solo no activa el adaptador experimental de PopupMenu: el paso
(c) requiere verificar los atajos y su integración antes de adoptarlo.

### KeyboardPanel: notificación de foco pendiente

En v0.0.8 `focus::host` mantiene la región activa internamente; `set_active`
repinta, pero no notifica a Files el cambio causado por Tab. Files necesita
conmutar sincrónicamente el modelo activo para que ruta, barra de herramientas
 y operaciones pertenezcan al panel enfocado. Consultar `focused_panel()` más
 tarde no garantiza ese orden frente al siguiente evento. Se solicitó una API
 de notificación en [issue #7](https://github.com/avillagran/lib-fltk-omarchy/issues/7).
 Es revisión estática; no acredita entrada SO. Hasta resolverlo se conserva el
 comportamiento existente, sin activar un segundo manejador de Tab.

### Pin v0.0.9 — 2026-10-28

El tag v0.0.9 fija `c2ab990c8293bcf93c6a69fba016b57b05430c33` y aporta
notificaciones de foco del panel (issue #7 cerrado). La migración de Files a
KeyboardPanel todavía debe conectarse y validarse; el cambio de pin no activa
por sí mismo el router. El guard de headers y el lock se registran en
`fltk-omarchy-v009-headers.json`; GPU permanece desactivada.

### Superficies al recargar tema

Files toma el tema Omarchy activo por defecto; la opción Tokyo Night es una
alternativa local explícita. Las superficies de Painter derivan ahora del mismo
Theme validado que la paleta, evitando conservar colores del tema previo tras
una recarga. La biblioteca todavía no expone `darker_background`,
`lighter_background` y `bright_foreground` de colors.toml; se utilizan tokens
existentes de popup/control mientras [issue #8](https://github.com/avillagran/lib-fltk-omarchy/issues/8)
solicita esos valores exactos. No implica equivalencia exacta con cada tema.
