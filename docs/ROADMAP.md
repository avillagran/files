# files — matriz de implementación completa

Matriz reconciliada parcialmente en NODE 2026-11-01; historial desde2026-10-26. El alcance ampliado sustituye el límite antiguo de 0.0.1.
El diseño visual aprobado sigue siendo `mockups/v3.html`; Rust/FLTK software y
licencias de `DEPENDENCIES.md` se conservan. No se publica, instala globalmente ni
se crea remote por inferencia. Las entregas sólo avanzan 0.0.2, 0.0.3…

Estados: **pendiente**, **implementado** (código sin aceptación integral),
**implementado parcial**, **verificado** (aceptación con evidencia), **bloqueado**.
Ningún menú, placeholder, contrato o core sin acceso UI equivale a feature completa.
Los estados verificados de 0.0.1 tienen los límites de `VALIDATION.md`; los nuevos
scenarios de árbol total deben volver a verificarlos. Esta matriz es fuente de avance,
no una afirmación de que las capacidades pendientes ya existen.

## Entregas de trabajo

1. **0.0.2**: espacial visible/persistida + selección/zoom/arrastre; corregir índices
   obsoletos y preferencias atómicas; gates completos de árbol y timeouts.
2. **0.0.3**: operaciones seguras con UI y renombrado transaccional/preview.
3. Entregas siguientes: paneles/pestañas/Miller/treemap, previews/helpers, búsqueda/
   archivos, montajes/red, shaders reales opt-in y adaptadores. Se dividen en hitos
   verificables sin cambiar minor/major; no se detiene el trabajo al cerrar uno.

## Matriz de cobertura y aceptación

| ID | Fuente | Capacidad | Aceptación observable | Estado | Evidencia / bloqueo actual |
|---|---|---|---|---|---|
| A01 | DESIGN 1–4, A–C,F–G | Rust/FLTK software, licencias | Release sin GL por defecto; auditoría lockfile y dependencias | verificado | 0.0.1: DEPENDENCIES.md, evidence/licenses.json |
| A02 | DESIGN 2–3 | PSS base, presupuesto total | 10k/1080p árbol completo; objetivo 50 MB decimales; techo 150 MB; baseline gate 60 MiB y análisis >40 MiB; PSS+Swap | implementado parcial | último baseline10k FAIL56.743936MB pico/41.455616MB estable PSS+Swap; escenario copia35.503104MB no sustituye baseline. Freeze sin nuevas medidas; no GPU/lifetimes completos |
| A03 | DESIGN 3 | Virtualización/listado/stat | 100k entradas sin widgets por archivo; primer lote sin statvfs; sólo stat visible salvo orden fecha/tamaño | implementado parcial | core/listing.rs; riesgo mounts offline pendiente |
| A04 | DESIGN 3 | Arranque frío | Timeout real; p50/p95/max desde spawn; fail si cualquiera >=100 ms | implementado parcial | última serie externa FAIL: p50 90.644308ms, máximo151.978267ms,7/10 bajo100ms; freeze sin nuevas medidas |
| A05 | DESIGN 5,K | VFS/plataformas | Interfaces locales y remotas; I/O bloqueante fuera de UI; cancelación no acumula workers offline | implementado parcial | core/listing.rs; contratos sin backends completos |
| A06 | DESIGN 5,N | Helpers efímeros | JSONL versionado/acotado; cancelar selección; matar/recolectar árbol; límites y PSS total medidos | implementado parcial | du visible22cef80/1cb4880: supervisor/cancel/reap y árbol100k35.11MB; preview_textbc8ed30 sólo CLI; faltan demás helpers |
| A07 | DESIGN 3 | MIME/iconos | 365 extensiones; glifos Nerd reales y fallback legible, sin catálogo completo | verificado | core/mime.rs; 0.0.2/spatial-check.json + capturas FiraCode; sustituye set vectorial por instrucción del usuario |
| T01 | DESIGN D | Tema vivo Omarchy/fallback | Watcher rearmado ante symlink/replace; TOML parcial conserva paleta; mapa semántico visible | verificado | evidence/ui-check.json, hackerman.png, tokyo-night.png |
| T02 | DESIGN H–I | Bordes y geometría | Square/rounded/Omarchy interior, badges siempre curvos, resize estable; contorno exterior documentado | implementado parcial | 0.0.2: persistencia atómica conserva TOML; contorno exterior sigue en Hyprland, framebuffer lógico no prueba nitidez nativa 2x |
| T03 | ampliación | Preferencias/sesión atómicas | Read-modify-write conserva claves desconocidas; reemplazo atómico y archivo inválido preservado con error | implementado parcial | persistence.rs + sesión de tabs versionada/ownership y V12 ACK/retry implementados. OWNER-LOCK-RELEASE ebfbd46: creatorPID unlock frente a FD heredados,1290Rust/forkred→green/UI fuente+instalada preferencias9FB3exits/favoritos11FB4exits/sesión6FB8exits scopedPASS. Recuperación general de tareas/otrosSO/gates integrales pendientes |
| V01 | DESIGN E,I | Omnibar ruta/filtro | Breadcrumbs, Ctrl+L, fuzzy, Esc; navegación grande→vacía antes de draw sin índices obsoletos | verificado | 0.0.2: tests generation/visible/get seguro; escena UI real. Teclado/ratón del SO siguen en U03 |
| V02 | DESIGN E,M | Omnibar comandos/lugares | Prefijos > y : ejecutan comandos/ubicaciones reales, ayuda y errores visibles | implementado parcial | COMMANDS: catálogo46 por Ctrl+Shift+P/Acciones y dispatch existente; escena privada11FB scopedPASS. Prefijos CtrlL >/: conectados a catálogos filtrados (OMNIBAR-CATALOGS,823Rust/5FB scoped); completado inline y protocolo/inputSO pendientes |
| V03 | DESIGN E,I | Grupos | Tarjetas con hijos inmediatos/tamaño/edad; Hoy/semana/antes; columnas numéricas alineadas | verificado | evidence/hackerman.png |
| V04 | DESIGN 6 | Lista y orden | Nombre natural/tamaño/fecha, carpetas primero, asc/desc y ocultos persistentes | implementado parcial | Lista/natural verificados2ead92c; ocultos globales defaultfalse/persistencia y cuatrovistas integrados, HIDDEN-CONTEXT-AND-STATUS; eventosSO pendientes |
| V05 | DESIGN 6 | Grilla | Iconos/miniaturas virtualizados, selección y zoom; acceso UI | implementado parcial | GRID-VIEW+MEDIA-THUMBNAILS: botónNerd/Ctrl5/zoom/rango/hidden/context;7miniaturas imagen/vídeo reales+inspector rawunix/160px/LRU/tickets,423Rust yescena1793069315038209135PASS. Persistencia/otrosvistas/HiDPI/PSSpendientes; freezeNOT_MEASURED |
| V06 | DESIGN E/decisión2026-10-27 | Columnas/Miller | Cadena Finder navegable/virtualizada, poda ancestro, teclado y operaciones sobre foco | verificado parcial | M26 COLUMNS-CONTRACT: 596Rust/Clippy/release/fmt; columns-ui1793124416464738573 PASS6FB/cadena-poda/foco/copia real/normal0ACK/initial800×440. Profundidad12 y migración tag3→Columns/tag5 nuevo unitarias. LISTING-COLUMNS-REFRESH ac02206: vigilancia hasta8raíces, recarga secuencial con worker compartido, rawselection/anchor/viewport, tickets obsoletos y rama ausente;1278tests yWayland/XvfbX11 scopedPASS. Cadena completa persistida/inputSO/liveResize pendientes |
| V07 | DESIGN E | Doble panel | F9/Tab, navegación independiente y operaciones entre paneles | verificado parcial | M27 DUAL-PANE-CONTRACT: dos listas vivas/IDs(panel,tab)/worker secundario exclusivo;603Rust+Clippy/release/fmt, dual-ui1793126826929914048 PASS8FB/raw F5-F6/cola al cerrar panel/normal0ACK/initial800×440. Filtros por panel visibles. Persistir ambas mitades, otras vistas/inputSO/recursos pendientes |
| V08 | decisión2026-10-27 | Espacial sustituida por Columnas | Migrar selector/ayuda/sesión conservando datos antiguos; no confundir con treemap | sustituida por usuario | COLUMNS-CONTRACT/V06 en curso; layouts antiguos preservados, sin afirmar dos features completas |
| V09 | DESIGN E | Vista Espacio treemap | Helper du cancelable, tamaño real árbol, hit tests de áreas, navegar y liberar helper | implementado parcial | SPACE-MAP: Ctrl+4/Espacio real, áreas de bytes completos, inspector/navegación/empty-partial-error/resize/cancel y cierrehelper vivo funcionalPASS1793057823344999138;370Rust/Clippy/release. Estilov3/botónocultosglobal/denominadorvisible ytotalinclocultos en HIDDEN-CONTEXT-AND-STATUS. SPACE-MAP-CHANGES a3f5094: avisos de raíz/interrupción sin recálculo automático, botón renueva listado e informe (F5 dispatcher corregido después en d05e51c), scope no recursivo visible, filas sin área hasta3;1281tests/Wayland+XvfbX11+instalado scopedPASS. Límites2048nodos/256KiBnombres; SPACE-MAP-NAVIGATION d05e51c: navcache/reveal/clamp/Page/CtrlShift/filas65×45 sin inflarárea, MC440 con nombres y F5contextual/tooltip/ayuda, hitsrevocados;1284tests/Wayland+XvfbX11+instalado scopedPASS. All/Invert/Clear/MCInsert y operaciones/rootraw posteriores integrados en SPACE-MAP-ACTIONS.md y evidencia de MapMarks/MapActions; la proyección limita All/Invert, no convierte tamaños en autoridad. OtrosSO/offline/recursos/gates nativos pendientes, freeze intacto |
| V10 | ampliación | Pestañas e historial | Crear/cerrar/reabrir tabs; back/forward por tab y restauración de sesión | verificado parcial | M22 TABS-CONTRACT: ocho activas/cerradas y estados por tab, 513Rust/UI9FB. M23 SESSION-CONTRACT: metadata durable versionada/raw, lock lifetime/readonly, argv/no-session, invalid-futuro preservado, ACK; 527Rust/Clippy/release/fmt y tabs-session-ui-1793111266679001110 PASS6FB/8procesos. Recursos/startup NOT_MEASURED; inputSO/otrosSO pendientes |
| V11 | DESIGN 5–6 | Lugares/montajes | XDG/montajes, favoritos/recientes navegables y persistidos; errores/desmontar seguro | implementado parcial | XDG/proc mounts + BOOKMARKS hub CtrlD/CtrlAltD,64 favoritos persistidos/raw, etiquetas y merge cooperativo;793Rust/11FB scoped PASS. OWNER-LOCK-RELEASE ebfbd46: release PID-owner explícito, forks red→green y11FB4exits source+installed. Native/mounts/gestión y aceptación global pendientes |
| V12 | ampliación | Preferencias por carpeta | Vista/orden/zoom/posiciones recuperadas sin escribir en carpetas personales; ocultos GLOBAL por decisión usuario, no override por carpeta | implementado parcial; NODE funcional | `folder_views` + AuxIo/ACK/retry/close; Guardar/Olvidar, restore/tab/dual guards;18FB/6exits X11+Wayland scopedPASS en NODE. NODE instalado record1793515028382395392/5hashes, installed9FB3exit0; OSinput/2x/otrosSO y posiciones completas pendientes; `FOLDER-VIEWS-CONTRACT.md`; OWNER-LOCK-RELEASE ebfbd46 corrige retención de flock tras drop por FD heredado, sin invalidar CAS/busy;9FB3exits source+installed |
| U01 | ampliación | Selección múltiple | Ctrl/Shift, rango, seleccionar todo/invertir, marquee; todas las vistas y operaciones coherentes | implementado parcial | 0.0.2: Ctrl/Shift, rango, todo/invertir, drag conjunto; faltan marquee y futuras vistas/operaciones |
| U02 | DESIGN 6,E | Teclado/ayuda/contextual | Atajos completos F1–F10, menú contextual, ayuda visible y terminal aquí | implementado parcial | ContextualB3/ShiftF10/Menu, F1 paginado, navegación/selección y catálogo46 con comandos deshabilitados/razón. COMMANDS y evidencia1793404858332671715; handlers/framebuffer. TerminalHere implementado/verificado parcial en TERMINAL-HERE.md (broker real y cwd raw); QUEUE-REVIEW.md unifica revisión con cola activa/MC. Perfil MC completo, accesibilidad e inputSO pendientes |
| U03 | ampliación | Accesibilidad/IME/HiDPI | Texto compuesto/clipboard editable, foco perceptible, tamaños 1×/2×, controlador nativo verificable | implementado parcial | CUA 0.28.2 cero ventanas; escenas internas no son eventos SO |
| U04 | ampliación | Abrir con/propiedades/permisos/enlaces | Elegir app, metadatos reales, chmod seguro, symlink/hardlink con validación y no sobrescritura | implementado parcial | FILE-PROPERTIES: Alt+Enter/Acciones, metadatos FD y chmod explícito no recursivo,341Rust/Clippy/release y escena funcional PASS. Abrir con/enlaces/ACL/xattrs/múltiples/otrosSO pendientes; open del sistema y bug cierre priorizados |
| O01 | DESIGN L | Copia/movimiento | Reflink→kernel→stream, bytes/metadatos/symlinks correctos, no clobber; comparar throughput cp | verificado parcial | COPY-UI/BENCH copia previa, baseline10k FAIL56.744MB conservado. M24 MOVE-CONTRACT: rename nativo F6/context/inspector, revisar→confirmar token FD/cola global/no-clobber/128roots/FD budget; 537Rust/Clippy/release/fmt y move-ui1793113502303715506 PASS6FB/inode-metadata-xattrs/raw/symlink/refresh/cancel-close. M25 EXDEV archivo regular único nlink1 verificado +journal/recuperación explícita, 586Rust/Clippy/release y exdev-ui1793122272433112203 PASS3casos reales63→75/10FB/close0ACK. Árboles/hardlinks/lotes EXDEV, ACL-xattrs copia general/undoMove/resume general pendientes  Copia visible SFTP explícita: COPY-PUBLICATION/revisión+cola; nativo4roots/raw/mtime/colisión/cancel-parciales, sin fallback ni copy-delete. |
| O02 | DESIGN E,L | Cola/progreso/control | Toast accesible con progreso, pausa/reanudar/cancelar y cancelación entre archivos | implementado | copy_job/copy_state y toast/formularios;171tests, captura controles corregida, cancelar tras progreso3280archivos, ACK/cierre y cleanup observados; techo del escenarioPASS35.503MB con hueco salida, baselineFAIL; resume durable pendiente |
| O03 | DESIGN L | Conflictos/recuperación | Omitir/reemplazar/renombrar explícitos; journal durable, reanudar tras crash y limpieza parcial | verificado parcial | M25 WAL single-file EXDEV y recuperación explícita por fase/proof; procesos nuevos gaps publish/delete/restore core, UI publish/restore reales PASS. Conflictos generales, árboles/lotes/hardlinks y otras tareas pendientes; no powerloss probado |
| O04 | DESIGN L | Papelera/restaurar/deshacer | Adaptadores SO; restaurar origen sin clobber; deshacer transaccional probado en fixtures | implementado parcial | Linux home-Trash mismo FS, Acciones/Supr y Undo visibles; lote limitado 64, FD pinned/no-clobber/recuperación explícita. Fixtures y tests en TRASH-CORE-NOTES; otros volúmenes/SO, catálogo persistido, redo/journal y recursos pendientes |
| O05 | DESIGN 6 | Crear archivos/carpetas | UI editable y creación exclusiva, Unicode/errores y no sobrescritura | implementado parcial | 0.0.3: UI real con creación exclusiva y bytes/colisiones/reservados/cancel probados; rename-ui.json/rename-resources-normal-exit.json, lote2; undo creación pendiente |
| O06 | DESIGN 6 | Clipboard/DnD | Copiar/cortar/pegar archivos nativos y DnD local/entre paneles, selección múltiple | parcial verificado | M28 buffer raw interno a ventana, Cut parcial/retry/ACK, escena clipboard-ui1793130549985863820; MIME SO/DnD real pendientes |
| R01 | ampliación | Plan puro renombrado | Tokens, secuencia, literal/case; colisiones/ciclos; límites antes de expansión | implementado | rename_plan.rs exportado180002d, bounds8b57971: 9 tests; sin UI, no feature completa |
| R02 | ampliación | Renombrador visible | Preview antes/después, prefijo/sufijo, extensión y fechas; regex, errores y confirmación explícita | implementado parcial | F2 real, tokens/secuencia/literal/case desde0.0.3; regex UTF-8 escalar/capturas con caps y preview/apply/undo en0.0.4 (RENAME-REGEX). Fechas, propiedades/casefold Unicode y presupuesto lote masivo pendientes; recursosNOT_MEASURED por fase |
| R03 | ampliación | Transacción renombrado | Revalidar identidad, staging exclusivo, ciclos, rollback/deshacer y no clobber | implementado parcial | rename_transaction.rs exportado:20 tests incluidos Cargo, rollback/cancel/undo-cancellable/diag cap; 0.0.3: escena UI+disco lote2 preview/apply/undo/redo/cierre ACK PASS; recursos35.06MB muestreados, no100k/journal durable |
| P01 | DESIGN J | Space/inspector lifecycle | Space/Esc, flechas cambian preview; cancelación por generación y helper liberado al cerrar | implementado parcial | State único texto/imagen, generation/cancel/drain/cierre verificados por188tests y escenaimagePASS; inputSO y clipboard textoFAIL, recursosimagenUNVALIDATED |
| P02 | DESIGN J | Imágenes | Decode reducido con límites de píxeles/memoria, zoom y LRU duro; thumbnails a demanda | implementado parcial | helperPNG/JPEG/WebP1024²/4Mipíxeles, alfa/EXIF/zoom/pan y escenaUI realPASS; árbol observado53.69856MB pero5helperssinmuestra→UNVALIDATED; baselineJPEG12/24MP IDCTreal UI/recurso42.36288MB muestreado con2helpers verificados; M2MEDIA-THUMBNAILS LRUGrilla/inspector+160pxJPEGBaseline/PNG/WebP/vídeo funcionalPASS423Rust; JPEG-DC-PREVIEW445Rust/10vendorlib/Clippy/release yUI1793078251643976150PASS progresivos12/24MP DC1/8 conigualcap32MiB, nointegridadAC. PNG-ROWS-PREVIEW453Rust/Clippy/release yUI1793081848018085943PASS PNG12/24MP porfilas/Adam7/alfa/EXIF; sinfullsourceRGB. WebPgrande/APNG/calidadchroma/globalcache/otrosSO/PSSnuevo pendientes |
| P03 | DESIGN J | Texto/código | Primeras 2000 líneas acotadas, resaltado, selección/copiar y encoding/errores | implementado parcial | preview_text helperbc8ed30:256KiB/2000LF,UTF8/UTF16BOM, binarios/FIFO rechazados;8tests/helper y8fixtures. Space/selección UI992fe04, captura fuente real; clipboard SO FAIL sinfoco, recurso143eb6d UNVALIDATED helpers noidentificados; resaltado/renderHTML/MD pendientes |
| P04 | DESIGN J + ampliación | Markdown renderizado | Estilos/listas/tablas/enlaces locales controlados e imágenes acotadas; no texto fuente como preview | implementado parcial | MARKDOWN-PREVIEW: estilos/listas/tablas/selección/resize, bitmaps localesArc4MiB y resolvercontextual anteshelper;242Rust/Clippy/release,escenaembedded1793034469432638103PASS. RecursosincrustadasNOT_MEASURED poruserfase; anteriorMDUNVALIDATED43.178MB2/8helpers;P09LRU/graphemes pendientes |
| P05 | ampliación | HTML renderizado seguro | Layout real con scripts/red desactivados; archivos relativos restringidos y límites | implementado parcial | HTML-ENGINE-PROBE: litehtml pinned+Gumbo en helper opcional, Painter/selección/resize/scroll/cierre reales;252Rust+Clippy/release,escenaHTML funcionalPASS. Métricas aproximadas con recorte visible; recursos/enlaces bloqueados, allocator/packaging/contraste y mediciones pendientes, NOT_MEASURED por fase  M13FDheredado/Revisionefectiva+watch/reconcile476Rust/Clippy/release yhtml-ui1793093483630467339PASS10FB/14helpersgone/notify-suppressed/replace-delete-parentrecover; recursosNOT_MEASUREDfreeze |
| P06 | DESIGN J | PDF multipágina | pdftoppm/pdftotext detectados, navegar páginas, cancelar y medir árbol | implementado parcial | PDF-PREVIEW: pdfinfo/pdftoppm directos secuenciales FDpinned, páginas/zoom/pan visibles;266Rust+Clippy/release, pdf-ui1793038247910320945 funcionalPASS; error instrumental original conservado. M12expectedFD/Revisionefectiva+watch/reconcile1s473Rust/Clippy/release yescena1793092913810767063PASS8FB/23helpersgone/notify-suppressed/replace-delete-parentrecover; readonlyexit≠ACK. M14Textoextraído porpáginaT/pdftotext+selecciónASCIIhandler+UTF8unit480Rust/Clippy/release/pdf-ui1793094189643854652PASS12FB/29loggedhelpersgone; selección sobregeometríaPDF/Unicodefontcoverage/complejos/password yrecursosNOT_MEASURED porfase, otrosSO pendientes |
| P07 | DESIGN J + ampliación | Vídeo | Frame/duración y reproducción/seek viable; ffmpeg/ffprobe/mpv detectados, clip y liberación medidos | implementado parcial | VIDEO-PREVIEW: ffprobe/ffmpeg directos secuenciales, FDpinned, bitmap960x540/seek/línea de tiempo;260Rust+Clippy/release, video-ui1793037769557008693 funcionalPASS. Reproducciónsoftware12fpsconfigurada/play-pausa/EOF/cierreACTIVOPID yclipframebuffer4fps funcionalPASS272Rust; audioopt-in M11 mismoffmpeg/Pulse mute inicial;470Rust/Clippy/release yescenas1793091780741012397+1793091967178711219PASS índices1/0/hot-toggleACK/backendretry/close, audiozeroPCMsin audibilidad/sync; budgetNOT_MEASURED porfase. Footer76.6MiB bajo24readbacks es UNVALIDATED paraatribuir aplayback;VIDEO-PREVIEW bufferUIdevueltoalslotúnico Ready/Recycled454Rust/Clippy/release yescena1793082933186610779PASS4FB/pauseEOF/closeactivo9helpersgone,NO ahorroPSSafirmado; M18VIDEO-ZOOM488Rust yescenasGIF1793101116101351845/MKV1793101152229152975PASS zoom2pan48/36/frames-pausa-seek-resume-reopenreset; VFR/4K/rotación visual pendientes |
| P08 | DESIGN J | Audio/archivos/carpetas/Office | Metadatos/reproducción acotada; listado comprimidos, conteo; extracción texto Office limitada | implementado parcial | AUDIO-PREVIEW: Space metadatos reales FFprobe, FLAC/WAV/Unicode/recorte y cierre funcionalPASS276Rust/Clippy/release. AUDIO-PREVIEWFDexpected/efectivo+watch/reconcile yReadyrevision458Rust/Clippy/release; audio-ui1793083931390323451PASS8FB/notify-suppressed/replace-delete-rename-parent/11loggedhelpersgone, cierreReadOnlypermiso≠ACK probado encontroller. AUDIO-PLAYBACK463Rust/Clippy/release yUI1793086873981565834PASS Pulse nativo/silencio9FB/play-pausa-seek-EOF/hidden-delete/closeactivo14loggedhelpersgone. PrototipoFFplaynativoEOFFAIL/dummyPASS preservado, backend sustituido; AS768virtual noPSS. Waveform/carátulas, Spacecarpetas DUestricto visible/279Rust yfolder-ui1793040015107090730PASS; ZIP32catálogo readonly visible/288Rust/zip-ui1793040518449068984PASS; DOCX M21: DOCX-PREVIEW,500Rust/Clippy-release-fmtPASS,10wire yoffice-ui1793105654289278359PASS textoUnicode/selección/truncado/FDidentity/reconcile/cierre0ACK9loggedhelpersgone; FAILENOSPC históricos conservados; otrosOffice/audibilidad/posiciónPCM pendientes; recursosNOT_MEASURED porfase |
| P09 | DESIGN J,N + prioridad medios | Caché/limits | Identidad de FD/revisión, invalidación visible, LRU por bytes y helpers; medir PSS+Swap árbol | implementado parcial | MEDIA-LIFECYCLE M1: imagen/vídeo seleccionados checked-FD+época/token/inotify/reconcile1s, 410Rust/Clippy/release y escena1793064138443049803 funcionalPASS;13helpersgone/exit0. M2MEDIA-THUMBNAILS Grilla/inspectorLRU4MiB+ticket/160px/rawbyteIPC,notify-suppressedFDreconcile,423Rust/Clippy/release yescena1793069315038209135PASS. RELATIVE-SOURCE rootFD/fileFD esperado+efectivo434Rust/Clippy/release ywire1793074260681793818+UI1793074268202431350PASS acotados; DOCUMENT-MEDIA442Rust/Clippy/release+escena1793076116463397564PASS2instancias notify/reconcile/rootrecover, watcherantesprobesfinales ycandidateoculto; readonlystopfalse/true separadohelpersACK. MEDIA-THUMBNAILS escena1793082861888227321PASS PNG24MP/JPEGprogressive24MP160px Grilla/inspector/replace/delete/Trash/Space/cierre44loggedhelpersGone, sin medidas. Audioseleccionado FDexpected/efectivo ywatch/reconcile458Rust+escena1793083931390323451PASS, ReadOnlyexitallowed≠drained. SpaceLRU/globaltodosformatos/otrosSO pendientes; RAW-PATH-WIRE CLI+imagenfullrawUnix427Rust/escena1793071898429792393PASS; panicWidgetTrackercausaabierta/guardcancel-drainverificado; sin disco nuevo. Recursos/startupNOT_MEASURED freeze |
| Z01 | ampliación | Archivos comprimidos | Crear/listar/extraer desde UI; traversal, symlinks, expansión/cantidad/cuotas protegidas | implementado parcial | ZIP-PREVIEW: Space catálogoZIP32 real/count/sizes/nombresraw/flags, helper efímero, 288Rust+Clippy/release yescenafuncionalPASS; Foundation source-only ARCHIVE-OPERATIONS-CONTRACT: shared streamingCRC/quotas+namespace/local-span plan,1068Rust/Clippy/release/fmt y30helperwire PASS; ExtractZIP32 regular/dirs UI: revisión AuxIo→Aplicar→cola global, pausa/cancel/ownerACK/raw/sync/no-clobber;1093Rust+Clippy/release/fmt y1793491842976117966 scopedPASS7PNG. Mismo presupuesto, planes grandes pueden rechazarse; RetryUI cleanup/sync en resultado retenido: scoped1793496524673844308 PASS5PNG, confirmación/pausa/rechazoCola/tokenoriginal/foreignpreservado; CrearZIP32 regular/dirs/enlacesliterales UIrevisada→mismaCola,DEFLATEstream64KiB/CRC/raw/UTC2s/rwx,no-clobber;1114Rust+Clippy/release/fmt y1793498375771482261 scopedPASS7PNG1000/440. Journaldurable/ZIP64/TAR/extractlinks/otrosSO pendientes. Extraccióne0dd847 histórica; creación INSTALADAe5970ff/app20db1571eb91,record1793498441702024461,5+5hashesPASS. RecursosNOT_MEASURED porfase |
| S01 | DESIGN M | Búsqueda nombres | /texto recursivo/índice on-demand, filtros tamaño/fecha/tipo, navegar resultados, cancelar | implementado parcial | RECURSIVE-SEARCH: Ctrl+Shift+F/helper acotado, filtros de nombre/ext/tipo y tamaño/fecha UTC (SEARCH-FACETS), padres/abrir/localizar/raw/ocultos locales y estado parcial explícito; escena 1793393137003734053 PASS scoped. Progreso de contadores implementado (SEARCH-PROGRESS); lotes visibles PROVISIONAL implementados con SearchBatches (837Rust/7FB+17FB scoped, SEARCH-BATCHES); SEARCH-CALENDAR F8 Hoy/Ayer/7fechas locales con reloj capturado y UTC explícito,937Rust/26FB/helperIPC scopedPASS. Raíz navegable SEARCH-SCOPE INSTALADAb405554/app35c2c4003224, menúNerd/Alt↑→←, historial8/32KiBrutas sin snapshots, criterios conservados/Enterexplícito/helperACK;1121Rust y1793499783655547501 scopedPASS8PNG1000/440. Paginación F9 acotada en SEARCH-PAGES y presets1/24/168h verificados parcial; índice/orden global/períodos personalizados-naturales/otrosSO/eventos-recursos pendientes |
| S02 | DESIGN M | Contenido/regex | ?texto en helper, límites bytes/resultados/tiempo, filtros y navegación | implementado parcial | CONTENT-SEARCH: Texto/F6 y content: literal UTF8 exacto, streaming KMP acotado, FD/revisión, líneas/raw/counters/parciales;780Rust/4CLI/24FB scoped PASS. Fragmentos256B/marked UTF8 en resultados implementados (SEARCH-SNIPPETS,843Rust/20+7FB scoped); regex avanzada y guiada implementadas/verificadas parcialmente en CONTENT-REGEX-CONTRACT.md y GUIDED-REGEX-CONTRACT.md; literal KMP preservado, regex∅/EOF/multilínea acotada y criterios guardados v1/v2. Unicodecasefold/otras codificaciones/índice/gates nativos/recursos pendientes |
| S03 | DESIGN M | Índice incremental | Raíces autorizadas, mmap/páginas, watcher incremental, reconstrucción/errores | pendiente | sin índice |
| S04 | DESIGN M | Semántica compatible | Alternativa cuyo árbol completo respete techo; no descargar/lanzar Gemma 400–700 MB | bloqueado | DESIGN M reconoce 400–700 MB: incompatible con techo 150 MB |
| C01 | ampliación | Checksums/duplicados | UI con hashes comparables, cancelación y memoria acotada, confirmar acciones destructivas | implementado parcial | SHA256 FD/revisión/chunk64KiB+lote128/8KiBnombres, helperúnico/progreso/ACK, panelcaptura/F5/F6/cancel/copy/comparación;1188Rust/Clippy/release/fmt y X11+Wayland10FB/2exit0drain/6loggedhelpersgone scopedPASS. INSTALADO5da9265/app4279419032a9/helper48e017b4b4b2, record1793518010519338180/5backups+5hashes; installed7FB/2exit0/toolkitpopupPASS. `CHECKSUMS-CONTRACT.md`; finderrecursivo integrado: tamaños→SHA/NOFOLLOW/hardlinksexcluidos/partialexplícito/ACK;1198Rust+3ignored/Clippy/release/fmt y X11Wayland14PNG/4exit0/8loggedhelpersgonePASS. INSTALADO79a906e/app5e3f5e851a49/helperfaee512719ea,5backups+5hashes y installed7PNG/2exit0/4loggedgonePASS (`DUPLICATES-CONTRACT.md`); paginación/gruposgrandes implementada follow-up:1200Rust/Clippy/release/fmt+X11Wayland16PNG/4exit0/8loggedgonePASS;4096rawmatches/directwire5.4MB. follow-upINSTALADOa37cbca/appd0401cd1c43e/helper006fdb1a8f3a,5backups+5hashes,installed8PNG/2exit0/4loggedgonePASS; revisión grupos/Ubicar integrada follow-up:1203Rust+Clippy/release/fmt yX11Wayland24PNG/4exit0/12loggedgonePASS; INSTALADO2ec389f/app441261c2b883/helper006fdb1a8f3a,5backups+5hashes,installed12PNG/2exit0/6loggedgonePASS. Etiquetasfilasraw/control implementadasfed0c70,1206Rust/Clippy/release/fmt yX11Wayland32PNG/4exit0/12gonePASS; INSTALADOfa24e8e/appbc9871535945,5backups+5hashes,installed16PNG/2exit0/6gonePASS (`NAME-LABELS-CONTRACT.md`); headerlabelsINSTALADO64b7a26/app0e902ec5f9ef,1207Rust/Clippy/release/fmt y X11retry+Wayland38PNG/4exit0/12gone/installed19PNG/2exit0/6gonePASS; capturaX11inicialnombresizqausentes/causaabierta (`path-labels/validation/render-assessment.json`); otraspaths/revisiónmutaciones pendientes. Accionesdestructivas y globalgates pendientes |
| C02 | ampliación | Comparar/sincronizar carpetas | Dry-run muestra diferencias/conflictos, ejecución explícita sin pérdida, journal/deshacer según operación | comparación y dry-run UI verificados parcial; Apply pendiente | DIRECTORY-SYNC-UI.md/CONTRACT y evidence/directory-sync-ui: comparación preservada; Update/Mirror A→B/B→A y Merge, conflictos/decisionesPending por fila, coberturaancestros, filtros/propuestas/blockedglobal, report/rawLocate; único lector/admisión/cancel/rootcloseACK.1396Rust489/112/795+3ignored/Clippy/fmt/release; Wayland/X11 sync19FB6roots12readers cadauno y comparación14FB6roots7readers cadauno. Sin Apply/manifestFD/journal/reemplazo-retiro recuperable/cola/recovery/undo; FULLgoal/gatesglobales abiertos |
| N01 | DESIGN K + ampliación | SFTP como montaje nativo | Conectar/desconectar y ruta SO visible a otras apps; sshfs/FUSE bajo demanda, UI capacidad y loopback | verificado parcial | SFTP controls1754f83 instalado; actor/registro/recovery/OpenFD/close-retained nativos UI PASS. COPY-PUBLICATION review explícito visible+omisión fechas enlaces,925Rust y native1793457917645506527 PASS copias/raw/metadatos/cancel/no-clobber. Default atomic/overall SSHFS FAIL preservados. Sin sshfs global; perfiles locales SFTP-PROFILES verificados (32/128KiB, borrador explícito); SFTP-REPORT F4 captura legible/wordwrap/páginas/copyrequest verificados; catálogo montajes/PID-gap/entregaSO del informe/diagnóstico extendido/offline/kernelD/otrosSO/recursos/nativeinput pendientes; N01 no cerrado |
| N02 | DESIGN K | SSH seguridad/reconexión | Claves/host-key verificadas, llavero, ubicaciones guardadas; offline/reconnect y desmontaje seguro | pendiente | sin hosts reales ni secretos autorizados |
| N03 | DESIGN K | SMB/UNC | Montaje/adaptador SO y UI, loopback/fixtures, capacidad y errores explícitos | pendiente | no GVFS/KIO permanente |
| N04 | DESIGN K | WebDAV/FTP | Backends/helpers y mounts donde SO soporte; credenciales seguras, límites y errores | pendiente | sin backend |
| N05 | DESIGN K | NFS/dispositivos | Usar montajes SO, capacidad disponible/offline, espacio/errores sin bloquear UI | pendiente | listado /proc mounts solamente |
| G01 | DESIGN GPU/adenda shaders | Backend efectos opt-in | effects=off default; GL sólo ON, renderer hardware comprobado; rechazo software | implementado parcial | egl-capability.json: Apple M2 Pro GLES3.2 real; shaders compilados/render fixtures fuera app. Falta ventana/Painter/lifecycle/perf |
| G02 | DESIGN shaders | Aura real | Portar threadaura con estados working/attention/burst; comparación ON/OFF, fallback CPU | pendiente | Painter::effect=false es stub, no feature |
| G03 | DESIGN shaders | Reveal real | Transición por región/source, cancelar/oculto, fallback CPU; clip comparativo | pendiente | sin backend |
| G04 | DESIGN shaders | Starfield real | GL procedural, reposo/oculto detiene loop, captura/clip y coste total/GPU | pendiente | sin backend |
| G05 | DESIGN shaders | Activity real | Glow shader por región, uniforms y theme, fallback sin glow, medido | pendiente | sin backend |
| X01 | DESIGN fases | Windows/macOS | Adaptadores paths/apps/trash/mounts, build y pruebas en hosts disponibles; distinguir portable vs probado | pendiente | Linux aarch64 histórico y NODE Linux x86_64 con aceptación funcional acotada y runtime local verificado; Windows/macOS sin host/pruebas confirmados |
| X02 | ampliación | Sesión/crash/watchers | Restauración de tabs/tareas, persistencia atómica, refresh FS y errores sin perder selección | verificado parcial | M23 SESSION-CONTRACT: sesión de tabs versionada con lock interproceso/readonly/ACK/invalid-futuro preservado, UI8procesos PASS. MEDIA-LIFECYCLE: watches+reconciliación/expectedFD para previews y thumbnails scoped; watcher general incorporado después en LISTING-AUTO-REFRESH y LISTING-COLUMNS-REFRESH ac02206: browser/tab/path+columnowner estables, hasta8raíces/16MiB, serialACK/defer/viewport/closeACK, validación funcional acotada Wayland/XvfbX11. M24 refresh source/dest por ID/FD currentpaths, conserva selección y cancela todos antes de barreras. M25 journal EXDEV archivo regular y catálogo/recovery explícito por identidad verificados; recuperación general de tareas/árboles/lotes, SpaceMap recursivo vivo/otrosSO/inputnativo pendientes; avisos de raíz y recálculo explícito en SPACE-MAP-CHANGES a3f5094; recursos NOT_MEASURED |
| Q01 | gate cada entrega | Sampler proceso completo | Identidad PID/starttime, reparenting, incomplete nunca PASS, máximo muestreado/estable y Swap | verificado | 32 tests Python + baseline ready. complete verifica supervivientes, no cobertura temporal; salidas durante lectura explícitas y métricas no legibles invalidan |
| Q02 | gate cada entrega | 10k/100k/navegación | PSS+Swap total, picos muestreados/estable, CPU idle/load, latencia UI, FDs/hilos/procesos y liberación | implementado parcial | 10k/100k loaded y layouts verificados en0.0.2; max/estable/Swap/idleCPU/FD/hilos/cleanup; faltan navegación repetida/latencia/multitab/panel |
| Q03 | gate cada entrega | Operaciones/red/previews/efectos | Fixtures aislados de datos personales; throughput/cancelación/offline y ON/OFF; sin builds/capturas concurrentes | implementado parcial | du árbol100k/cancel/reap observado35.11MB;10kUNVALIDATED hijo rápido. Operaciones/red/previews/shaders en app aún sin gates |
| Q04 | reporte/journal | Evidencia por hito | Commit, features completas/pendientes, métricas, 2–4 imágenes Markdown reales y clips; envío auto al coordinador | implementado parcial | Reportes enviados con commits y framebuffer; checkpoint0.0.2 etiquetado;0.0.3 escena rename/creación y recursos35.06MB; arranqueFAIL150.31ms, no PASS global |

| U05 | omarchy-app-dev | i18n ES/EN | Locale automático, selector y cadenas de todas las vistas/errores traducidas | pendiente | No contar UI española como i18n completa |
| T04 | Instrucción usuario 2026-10-26 | Nerd Fonts Omarchy | Resolver monospace Nerd sin catálogo, glifos reales/fallback, overflow 1x/2x | verificado | FiraCode en spatial-check.json; glifos revisados sin tofu evidente, resize/1x/2x; captura lógica, nitidez física 2x pendiente |

## Instrumentación y riesgos recibidos

- `process_metrics.py` / `test_process_metrics.py`: entregados por coordinador,
  sampler con identidad y reparenting. Picos son máximos **muestreados**, no exactos;
  procesos que viven entre muestras pueden escapar. No incluye GPU ni CPU final de
  desaparecidos. No hacer PASS si `complete=false`.
- `rename_plan.rs`: plan puro del coordinador, tokens/secuencia/literal/case y
  colisiones. Ejecución/rollback, regex/fechas y UI todavía pendientes. Benchmark
  standalone no representa la app; 100k ~87 ms exige trabajador y medir preview.
- `navigate`: corregido en 0.0.2; invalida visible/hits/hovered por generación y usa
  acceso comprobado antes del siguiente draw. Tests grande→vacía/índice fuera de rango.
- `set_corners`: corregido en 0.0.2; read-modify-write atómico, conserva claves;
  valores deseados pendientes/errores sobreviven hasta ACK y se reintentan al cerrar.
- `path.is_dir()` y `statvfs` síncronos pueden bloquear en mounts offline. Un
  AtomicBool no interrumpe syscalls; aislar probes potencialmente bloqueantes y
  limitar concurrencia/recuperación, sin ocultar procesos o memoria.
- Orden natural integrado y liberado por coordinador (2ead92c).

## Límites específicos de la disposición espacial (0.0.2)

- Identidad canónica resuelta en worker; aliases comparten cola y revisiones.
  Renombrar/mover carpeta cambia identidad basada en path+dev+ino: migración pendiente
  en el motor de operaciones. Los guardados no son journal durable frente a kill/power loss.
- Layout máximo4MiB serializado/16384 posiciones manuales, validación y presupuesto
  antes de hexificar/copiar; nombres lossless. Versión futura/corrupta no se sobrescribe.
- Cola de dos snapshots inmutables compartidos, una escritura en vuelo; coalescing,
  reintento y ACK exacto. Cola llena impide navegar hasta poder retener el cambio.
- Fallos permanecen visibles incluso al cambiar de carpeta; cerrar espera sin bloquear
  UI y ofrece reintentar/seguir/cerrar descartando explícitamente los pendientes.
- Threads de listado/sondas limitados a4; primer lote precede statvfs. No resuelve
  interrupción de syscall offline: aislamiento de proceso/recuperación de mount pendiente.

## Prioridad explícita de ventana — 2026-10-26

| ID | Capacidad | Aceptación observable | Estado | Evidencia / límite |
|---|---|---|---|---|
| W01 | Normal por defecto y modos | Ventana estándar; normal/pantalla completa, reabrir conserva elección sin perder campos | verificado funcional | window-ui-1793042474907145360:3procesos/default/fullscreen/normal/camposdesconocidos/cierreACK; recursosNOT_MEASURED |
| W02 | Header/grips nativos | Controles excluidos, cuatro targets24px, serial real, mínimos700×440 y cursores propios | implementado parcial | Bridge pinned FLTK/libdecor; contrato geométrico, gesto SO pendiente CUA |
| W03 | Preferencias/Acciones | Menús excluyentes, sólo configuración vs operaciones, Nerd/tooltip/nombres/atajos | verificado funcional parcial | window-ui1793042474907145360: targets/handlers/framebuffer y separación; nombresemántico/tooltip, árbol accesibleSO no probado |

Freeze recursos/startup conservado; no reabrir benchmarks para este bloque.

Native input recheck2026-10-27: evidence/0.0.4/native-inventory1793087442753417655
usa nuevo compositor pero cua-driver0ventanas confixture propio mapped antes/
después. SininputSO/focoalternativo; reglaJev y aprobaciónfocopendiente intactas.

OWNED-HELPERS (2026-10-27): Linuxparent-deathSIGKILL en4spawnsupervisores,
464Rust/Clippy/release yhelper-crash1793088449742352360PASS4roles/directowned
helpers reap9 trasroot-onlykill, relaunch0+ACK. Regresionesaudio1793088666411517841/
vídeo1793089019986522162PASS. No completaX02taskjournal niP09global/otrosSO/
inputSO/recurso; testsubreaper no es daemon de producto ni pruebaPID1.


M11 VIDEO-PREVIEW: audio opt-in/Pulse mismo child yFD, mute por defecto/M,
índices absolutos de pistas, backendnotice/retry sin invalidar fuente válida;
sourcefailure tipado ocultaRGB yrearma token.470Rust/Clippy/release ydosescenas
funcionalesPASS másregresiónaudio standalone. Arenaschild2bajoAS768virtual permite
contexto, no gatePSS. P07/P09parciales; recursos/startupNOT_MEASUREDfreeze.


M12/M13 (2026-10-27): PDF y HTML seleccionados comparten watcher/token/epoch/
reconcile y expected/efectivaRevision de FD. PDF usa mismoFD en ambos CLI, HTML
heredaFD en vezreopen. Dirty ocultaRGB/displaylist y readonly_gate compartido
conaudio conservaexitpermiso≠ACK.473/476Rust+Clippy/release yescenasPASSscoped,
no textoPDF/shapingHTML/globalcache ni recursos/startup porfreeze.


M15 AUDIO-PCM (2026-10-27):481Rust+4Python/Clippy/release, realPCM440audio y880
vídeooptin en sinkPulseNULLpropio, silencio pausa/defaultmute,2FB/callbackvivo/
8loggedhelpersgone/modulecleanup/defaultigual. Reporte1793096793305882328PASSscoped,
fallosruta/harnesshistóricosconservados; nofísico/A-Vsync/clock/PSS/CPU/startup.
No completaP07/P08/P09global ni levanta freeze.


M16 GIF-PREVIEW: GIFanimado Space bajo demanda/singlepass(ignore_loop1), mismo
engineFD/generaciones/streamyminiaturas,8MiB/4Mipix/300s.485Rust/Clippy/release,
1793098465429205562PASS6FB/10helpersgone/activeclose0ACK eInspector160×90red/firmaFD.
CodepathGrillaGIF compartido sólo unit, no captura; alpha/disposal/delayexact
yPSSpendientes. No completaP02/P07/P09global ni levanta freeze.


M17 VIDEO-SOURCE-CLOSE: readonlygateaudio/PDF/HTML extendidoaselectedvideo/GIF
snapshot/stream, closepermiso≠ACK, admisión bajo mutexclosing yreapHelperobligado.
487Rust/Clippy/release+holdSIMULADO1793099666110053045PASS2roles/2FB/exit0FALSEdrain,
regresiónGIF1793099692852727198PASS6FB/10helpersgone/activecloseACK0. KernelDreal/
I/O bloqueada dentrofaseHelper/otrosSO yP09global siguenpendientes; freezeintacto.

## M18 — zoom/pan vídeo y GIF, 2026-10-27

VIDEO-ZOOM: mismoViewport1–8×/Ctrlrueda/+/-/0/arrastre, framesinvalidanpíxelssin
resettransform; seekliberaRGB yretiene3floats, nuevoorigen/revisiónresetea.
488Rust141/58/289+Clippyworkspacealltargets-Dwarnings/release/fmtPASS. Escenas
video-play-ui1793101116101351845(GIF11FB/12helpers) y1793101152229152975(MKV9FB/10)
PASS1000×650/zoom2pan48/36/conservadoframes-paused-seek-resumeEOF/reopenreset/
activeclose0ACK/loggedPIDstartgoneZfalse. Handlers internos, noinputSO/clipboard/
sharpness2x/lifetimes, no detallefotográficoprobado por coloresuniformes.
No clip24frames ni PSS/Swap/CPU/startup porfreeze; P07/P09siguenparciales.

## M19 — navegación visible de previews, 2026-10-27

PREVIEW-NAVIGATION: botones Nerd anterior/siguiente+Alt↑/↓ compartenAction,
ordenvisible/ocultos/filtro; sinUII/O/preload. Extremo conserva estado/revisión;
cambio real reutiliza selección/generación/cancel/drain yúltimodestino.
488Rust141/58/289+Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
preview-neighbor-ui1793101665648168454PASS5FB1000×650/PNG-WebP-rawPNG0xFF/
hiddenexcluido/FDsourceypíxeles/errorreal+volver/rapidúltimogana/callback0ACK/
7loggedhelperIDsgoneZfalse. Hit target/nombreaccesible verificados; noinputSO/
AT-SPI/clipboard/lifetimecoverage/activehelperclose enestaescena.
Freeze recursos/startupNOT_MEASURED; no playlist/autoplay ni completitudP09.

## M20 — giro no destructivo del visor, 2026-10-27

PREVIEW-ROTATION: botones Nerd/R/ShiftR giran sólo vista de imagen/PDF/vídeo/GIF;
muestreo directo del RGB original, sin copia rotada ni archivos modificados.
Transform numérico conserva giro entre frames/seek yPDFpáginas/texto-retorno;
nuevoorigen/revisión/reset0 lo elimina. AyudaF1 añade atajos reales de medios.
489Rust141/58/290+Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
preview-rotation-ui1793102443334515413PASS14FB1000×650/asimetría4cuadrantes con
píxeles framebuffer comprobados/giroCW-CCW-reset/video-GIFplayseek/PDFpágina2+
textoretorno/originalesbytesintactos/callback0ACK/21loggedhelpersgoneZfalse.
Sin inputSO/native2x/AT-SPI/lifetimes/activehelperclose enestaescena ni prueba de
colorfotográfico;14readbacks funcionales NOmedición aislada de consumo.
Recursos/startupNOT_MEASUREDfreeze; noP02/P06/P07/P09global ni tag0.0.4.


## M21 — texto DOCX verificado, 2026-10-27

DOCX-PREVIEW: Space cuerpo principal Word UTF8 por ZIP32 validado+XMLstream,
CRC/DEFLATE/descriptor, límites previos a asignación, sin extracción/recursos.
Expected/effectiveRevision del FD y generación; errores codec/cap no disparan
SourceUnavailable/reintento de revisión estable. CDATA fuera raíz y declaraciones
no iniciales rechazadas con regresión RED→GREEN y10wirecases reales.
500Rust141/67/292+Clippyworkspacealltargets-Dwarnings/release/fmtPASS;
office-ui1793105654289278359PASS6FB1000×650/Unicode102Bselección/truncado256KiB/
rawUnixFD/replaceigualsize-mtime/delete-recreate/notify-suppressed/callback0ACK/
9loggedhelpersgoneZfalse. No Wordlayoutfiel/otrosOffice/clipboardSO/inputSO ni
helpervivoexactamentealclose. Recursos/startupNOT_MEASUREDfreeze, históricos
ENOSPC/baseline/arranque/clipboard intactos; noP08global ni tag0.0.4.
Siguienteunidad visible V10 pestañas e historial por pestaña: TABS-CONTRACT.


## M22 — pestañas de la instancia, 2026-10-27

TABS-CONTRACT: ocho activas y ocho cerradas recuperables, IDs monotónicos,
historial/orden/filtro/vista/zoom/pan/scroll y selección raw por pestaña. Un Model,
un listado/worker; snapshots presupuestados antes de clonar, máximo3MiB guardado
más reservas documentadas. Global hidden y cola de copia conservan su semántica.
Undo/copia refrescan su carpeta capturada; volver a inactiva relista.
513Rust (141/67/305), Clippy workspace/all-targets -Dwarnings, release y fmt PASS.
tabs-ui-1793109776417107918 PASS9FB, A/B independientes y cambio rápido, hidden
podado sin seleccionar destino invisible, rawUnix, máximo/reapertura, resize
800×500/440 con metadata y última pestaña close0ACK/helper PIDstartgone sin zombie.
Tres FAIL previos conservados y corregidos, no rebajar assertions. Handlers/Actions
y framebuffer; no inputSO/clipboard ni helper vivo exacto al cierre.
V10 parcial: sesión durable pendiente; recursos/startup NOT_MEASURED por freeze.
Siguiente bloque: sesión durable versionada/acotada de estas pestañas, sin
restaurar tareas ni operaciones automáticamente; doble panel/Miller, move/journal,
red montada, shaders app e i18n siguen abiertos en la matriz.


## M23 — sesión durable acotada, 2026-10-27

SESSION-CONTRACT: formato binario versionado/raw, validate prestado previo a
materializar, encoder simétrico y límites4MiBwire/1MiBactiva/3MiBguardadas. Un
writer advisory lock lifetime en AuxIo existente; otra ventana readonly sin
takeover. Argv explícito sin restore/autosave salvo Guardar; --no-sessiondisabled.
Inválido/futuro preservado; respuesta tardía no pisa intención/cierre. Buffer
retenido hasta ACK, error bloquea reintento por tick, postrename Unsynced explícito.
527Rust141/67/319+Clippy/release/fmt PASS. tabs-session-ui1793111266679001110
PASS6FB/8procesos normales0ACK/save→restore/estado A-B/rawclosed/argv/readonly/
nooverwrite/invalidfuturebytes. Sin inputSO/clipboard/recursos ni crashresume tareas.
Siguiente unidad priorizada por usuario: mover F6/contextual/inspector en cola
global, mismoFS y EXDEV separados; contrato identidad/no-clobber/cancel antes
publicar/borrado sólo tras copia verificada. Journal durable/conflictos siguen
explícitamente pendientes; red montada/shaders/doble panel/i18n no se abandonan.


## M24 — mover nativo con revisión, 2026-10-27

MOVE-CONTRACT: F6/contextual/inspector, revisar por AuxIo y confirmar token de
padres+rootsFD al encolar. Hasta128roots,256FD tokens admitidos incluyendo
activo/ACK, revisión adicional130FD; cargo metadata y rutas antesadmisión.
RenameNoReplace, source literal/raw y control de ancestros por FD antesApply;
published/verified explícito ante carrera, no cleanup de entrada no verificada.
No rollback global: raíces previas quedanmovidas tras cancel/error.
537Rust148/67/322+Clippy/release/fmt PASS. move-ui1793113502303715506PASS6FB,
revisión/pausa sin publicar, cuatro roots con inodes/bytes/metadata/xattrs,
refresh A/B sin perder foco target, colisión no-clobber, /procEXDEV preflight
rechazado y origenconservado, cancel/closepausado0ACK. Handlers/noinputSO;
canceltrasroots sólo callbackcore; recursos/startupNOT_MEASUREDfreeze.
O01/O03 siguenparciales: siguiente EXDEV manifiesto verificado, attrs/durabilidad
y journal seguro; no fallback copy+delete ni UndoMove actual. O06 no avanza.

## M25 en curso — transferencia EXDEV y recuperación, 2026-10-27

Núcleo compilado: inventario/hash/metadata efectivo, WAL single-root y executor
regular nlink1 con staging/publicación durable y eliminación individual. Recovery
explícito reabre FDs y renueva barreras, no reanuda en silencio.191corePASS +
Clippy core limpio; transferencia pública FS63→75 y tres escenarios de proceso
nuevo PASS (salida73 en gap publish/delete/restore). Detalles/límites/evidencia en
EXDEV-JOURNAL-CONTRACT y exdev-core-foundation.json. No UI EXDEV, no instalación,
no O01/O03/X02 completo: árboles/hardlinks/lotes/recuperación visible siguen WIP;
recursos/startup NOT_MEASURED por freeze. M24 instalado intacto.


## M25 — checkpoint visible, 2026-10-27

Sustituye el WIP anterior para EXDEV de un archivo regular nlink1 y recuperación
explícita. 586Rust193/67/326, Clippy/release limpios; escena
exdev-ui-1793122272433112203 PASS3casos FS63→75/10FB/close0ACK/stages ausentes.
Catálogo legible origen→destino, prueba de intención y ownership hasta admisión;
controles en ventana inicial440×440 completos, textos largos recortados. Live
resize UNVALIDATED y FAILs/sidecar preservados. EXDEV-JOURNAL-CONTRACT incluye
contabilidad local, no medición global. Recursos/startup NOT_MEASURED freeze.

Prioridad explícita del usuario: PAUSAR ampliaciones de previews/medios, conservar
lo instalado y continuar funciones/UI. Siguiente unidad visible: doble panel,
después cortar/pegar y DnD sobre cola global, ampliar operaciones y SFTP como
montaje del SO. Shaders app, i18n y demás matriz permanecen abiertos. No cerrar
O01/O03/X02 por este scope single-file; árboles/hardlinks/lotes requieren ledger.


## M26 — Columnas sustituye Espacial, 2026-10-27

Prioridad explícita usuario ejecutada: Ctrl+3/selector/--columns y alias--spatial
→ Miller/Finder; Espacio/treemap permanece. Un Model +worker activo y otro worker
lookahead reutilizable, cachés sin motores. Ocho listados retenidos con ventana
deslizante (no límite de profundidad), poda por ancestro y foco/origen separados.
Tag3 legado migra sin zoom/pan/scroll de iconos, tag5 nuevo conserva scroll.
Layouts antiguos no borrados. V06 parcial verificado, V08 sustituida por decisión.
596Rust194/67/335+Clippy/release/fmt y escena1793124416464738573 PASS6FB/copia bytes/
poda/normal0ACK/initial800×440. Menú inactivo diferido y cancelable con prueba de
nombre raw/generación; payloads/modificadores/profundidad12/migración en tests.
FAILs previos conservados; lock journal owner explicitunlock sóloPIDcreador con
regresión FDduplicado RED→GREEN. No inputSO/liveResize/recursos/startup/globalPASS.
Doble panel siguiente: F9/Tab, origen capturado, F5/F6 destino opuesto revisado,
cola/journal/supervisores únicos. Previews nuevas pausadas y freeze intacto.


## M27 — doble panel MC visible, 2026-10-27

F9/Tab y navegación independiente con origen/destino capturados F5/F6. Un Model
activo+Browser movible, una cola/journal/media/AuxIo; worker secundario compartido
por transferencia exclusiva con Columnas. Grupos Tabs con IDs(panel,tab),3MiB
metadata conjunto incluido oculto, datos32MiB locales sin PSSclaim. F9off libera
listados invisibles; cierre panel no cancela cola. Sesión sólo grupo enfocado
con aviso explícito, persistencia dual pendiente.603Rust194/67/342+Clippy/release/
fmt y dual-ui1793126826929914048 PASS8FB/copia y move raw/normal0ACK/initial800×440.
No inputSO/liveResize/recursos/startup/globalPASS. Previews nuevas pausadas.
Sigue cortar/pegar y DnD sobre motores existentes, ampliar operaciones y SFTP
montado; EXDEV árboles/hardlinks/lotes, shaders app/i18n y resto siguen abiertos.


## M28 — Copiar/Cortar/Pegar visible, 2026-10-27

CtrlC/X/V contextual/Acciones y destino raw revisable sobre cola única. Captura
AuxIo/Source FD + tokens efectivos Native/EXDEV, subset original para retry,
Control vivo y cancelación tipada; Cut consume sólo confirmados por generación.
Presupuesto conjunto clipboard2MiB/FDs conjuntos locales, no claim de PSS.
624Rust203/67/354+Clippy/release/fmt, escena1793130549985863820 PASS8FB/2normal0ACK,
Copy0xFF→dest0xFE, Cut2 parcial/retry/cancel/reemplazo, buffer nuevo conserva ACK.
Const MIME FLTK canónica corrige consulta Rust falsa por puntero: texto propio
inmediato y tras eventloop exactos, NO prueba de foco/serial ni clipboard externo.
Menús inicial800×440 sin toast; FAILs previos/sidecar preservados. O06 parcial:
M29 MIME nativo/DnD con gesto/serial real y bytes bounded, contrato antes wiring.
No data-control como fallback de foco. Previews pausadas y freeze intactos.


## M29 — MIME/DnD nativos activo, contrato previo

Contrato NATIVE-TRANSFER-CONTRACT.md: wl_data_device/puente con gesto+serialreal,
proxies/listeners propios coexistiendo con FLTK, offers/tokens y FD bounded antes
alloc; no FL_PASTE URItexto ni data-control para foco. Codec raw exactuna vez,
destino capturado, cola/proofs existentes. DnD externo inicialmente COPY negociado;
finished no autoriza borrar ni duplica delete, Move interno por ACK propio.
M29a WIP: codec/bridge clipboard/AuxIo/revisión implementados,637Rust+Clippy/fmt
aprobados; eventos/MIME SO no validados. Un padre común; KDE/multi-origen y DnD
externo pendientes (FLTK negocia en paralelo; requiere hook opt-in reproducible).
M28 privado no cierra O06. Previews nuevas pausadas/freeze intactos, resto ROADMAP no abandonado.


## M29b — receptor DnD en integración, 2026-10-27

Hook FLTK1.5.23 opt-in por ventana propia, patch reproducible y guard de enlace
por hash; receptor COPY/URI y destino por panel/columna sin robar foco. 642Rust
(208/67/367),3ignored; Clippy/fmt/release exit0. Test C++ proxies simulados y
clipboard-ui1793135944759735086 scoped PASS12PNG/2root0ACK: pipe privado→
AuxIo→revisión→cancel/retry→cola/copia raw; origen/selección conservados.
Native Wayland/inputSO/interop UNVALIDATED, sender/KDE/multi-padre/otrosSO
pendientes; O06 sigue parcial. Freeze recursos/startup y pausa previews intactos.


## M29c — emisor DnD parcial, 2026-10-27

Emisor COPY en ventana propia: guarda el permiso del press original, espera un
umbral de 8 px, captura nombres raw al pulsar y verifica Source/codifica en
AuxIo. Antes de solicitar `start_drag` comprueba ventana/seat/epoch/serial
original, expiración y botón izquierdo; liberar o cancelar invalida antes de
tomar Model. Una selección múltiple conserva su lote al arrastrar y el clic
sin arrastre se resuelve al soltar. La reserva conjunta incluye clipboard
deseado, prueba, payload y fuente publicada; se mantiene hasta ACK o cleanup
(delimitado también por plazo). La fuente anuncia sólo COPY; `finished` o
`cancelled` nunca borran archivos ni consumen Cut. El drop propio reutiliza
revisión y cola existentes.

648 tests workspace (208 core, 67 helper, 373 UI; 3 ignorados), Clippy/fmt/
release PASS. Fixture C++ usa proxies simulados: COPY/serial/surface en helper
ya admitido, rechazos de permiso/epoch/plazo y `finished` marca cleanup; no
acredita grab/foco del compositor ni teardown completo. Escena
`clipboard-ui-1793136958438850356.json` PASS acotado: 13 framebuffers, Source
y AuxIo reales, despacho final simulado, Cut conservado y dos cierres con ACK.
No hay input SO ni interop Wayland probado; O06, KDE, multipadre y adaptadores
siguen abiertos. Primera escena `clipboard-ui-1793136933634122245.json` FAIL:
pegado de texto propio tras event loop no llegó. Repetición PASS no explica ni
borra el fallo; clipboard estable global sigue sin acreditar. Recursos y
arranque NOT_MEASURED (freeze); previews nuevas pausadas.


## M29d — KDE correlacionado, 2026-10-27

La importación KDE solicita `text/uri-list` y `application/x-kde-cutselection`
de la misma oferta y época, con un ACK único tras validar intención, Source y
destino. Dos pipes se drenan concurrentemente con plazo común de 3 s; URI
limitada a 1 MiB y metadata exactamente un byte `0` o `1` con EOF. Error,
cancelación, límite o cambio de oferta rechazan todo sin convertir Cut en Copy.
El máximo por fase del puente es 8 FD: cuatro envíos más cuatro extremos
transitorios; tras abrir quedan dos lectores, que se cierran antes de capturar
Source. Otros workers mantienen reservas independientes.

652 tests workspace (208 core, 67 helper, 377 UI; 3 ignorados), Clippy/fmt/
release PASS. Fixture C++ con proxies simulados verifica dos MIME de una oferta
y rechazo al cambiar selección entre EOFs. Siete tests de transporte incluyen
ambos órdenes, metadata inválida, límite, deadline y cancelación después de EOF
URI. Escena `clipboard-ui-1793137466717410666.json` PASS acotado: dos pipes
privados, ACK simulado, Cut abre revisión Mover, cancelar conserva origen, dos
framebuffers y salida0 tras ACK. Input/interop KDE reales siguen UNVALIDATED;
multipadre, DnD real y O06 siguen abiertos. El FAIL intermitente de texto
`clipboard-ui-1793136933634122245.json` permanece sin explicación. Recursos y
arranque NOT_MEASURED por freeze, previews nuevas pausadas.

## Adopción lib-fltk-omarchy — tema, 2026-10-28

`lib-fltk-omarchy` v0.0.1 (`50a42927`) alimenta los colores del Painter,
la preferencia Corners y el radio de badges. El watcher de Files valida los archivos antes de pedir la recarga al toolkit;
el override de fixtures `FILES_THEME` conserva el comportamiento anterior. La fuente Nerd Font resuelta se reaplica al
recargar. No se usa AppWindow: se conserva el puente nativo libdecor de Files.
Los colores específicos de superficies aún usan valores de Files; escala
Tipográfica/espaciado y estados del toolkit no están aplicados a todo Painter.
PopupMenu, OperationToast, TabBar y ShortcutHelp siguen pendientes de adopción
con paridad visual y funcional. Multipadre sigue WIP en `e9ebdcd`.

Primera compilación: headers normalizados coherentes y una sola copia de
fltk/fltk-sys 1.5.23 (`docs/evidence/0.0.4/fltk-omarchy-first-build-headers.json`).
Escena framebuffer con acciones internas y tema privado: recarga por rename,
font.base leído y Corners square conservado; input SO no probado. 668 pruebas
workspace aprobadas, 3 ignoradas; Clippy/fmt/release PASS. Recursos y arranque
NOT_MEASURED por freeze.

### Choques de componentes toolkit v0.0.1

Se conserva el menú Painter actual hasta que PopupMenu limite y desplace filas
cuando no caben y exponga cierre/foco: [issue #1](https://github.com/avillagran/lib-fltk-omarchy/issues/1), [issue #4](https://github.com/avillagran/lib-fltk-omarchy/issues/4).
TabBar, ShortcutHelp y OperationToast mantienen las implementaciones existentes
por scroll de pestañas, límites/Home/End de ayuda y etiquetas/localización de
toast: [issue #2](https://github.com/avillagran/lib-fltk-omarchy/issues/2).
Selección múltiple, tablas/árboles virtuales, PopupCard, animación y gesto nativo
se solicitaron en [issue #3](https://github.com/avillagran/lib-fltk-omarchy/issues/3).
No se sustituye el puente libdecor por IPC Hyprland. Estos son pendientes de
paridad del toolkit; no se declararon componentes adoptados.

### Recarga del toolkit y escritura parcial

Hasta que [issue #5](https://github.com/avillagran/lib-fltk-omarchy/issues/5)
permita al toolkit rechazar TOML inválido, Files mantiene su watcher y valida
los archivos antes de solicitar `theme::reload`. La última paleta dibujada se
conserva si la lectura o el parseo fallan. La validación y la lectura interna
del toolkit no son atómicas frente a un escritor concurrente. La escena
`toolkit-theme-ui-1793181731296656632.json` verifica escritura truncada,
framebuffer retenido y posterior reemplazo de tema; no valida eventos SO.
669 tests (223 core/67 helper/379 UI; 3 ignorados), Clippy/fmt/release PASS;
recursos y arranque NOT_MEASURED.

## lib-fltk-omarchy v0.0.2 — 2026-10-28

Pin git `210e0d64`, FLTK/fltk-sys únicos 1.5.23 con features existentes;
guard de headers y evidencia `fltk-omarchy-v002-headers.json` PASS. Tema y
validación de TOML siguen; libdecor permanece. 670 pruebas workspace, tres
ignoradas, Clippy/fmt/release PASS. Escena de tema en framebuffer PASS y
`toolkit-v002-focus-fixture.png` prueba el anillo con `focus::press` en una
ventana **aislada de prueba del toolkit**, no en Files ni con teclado SO.

El router del toolkit aún no está activado en Files: un solo canvas contiene
ambos paneles y el Tab existente; se requieren controles enfocables de panel y
cursor virtual antes de retirar esa ruta sin perder atajos. Migración y choque
con omnibar, vistas y menús: `FLTK-OMARCHY-KEYBOARD-MIGRATION.md`, issue #3.
PopupMenu sigue sin scroll para filas que exceden viewport y sin API pública
de cierre/aviso; foco previo sí se restaura en v0.0.2. Issues #1/#4 comentados;
menús previos conservados. Recursos/arranque NOT_MEASURED; no PASS global.

## 2026-10-30 — Usability, local search and directory sizes

New user priority covers both MC/keyboard workflows and ordinary mouse use.
`USABILITY.md` records the broader programme without reducing ROADMAP scope.
Local name filtering now supports AND terms, exclusions, full-name globs,
literal phrases, extension alternatives and entry kinds with bounded parsing.
Search/clear hits, Ctrl+F, MC-style Insert and F3/F8 aliases reuse production
handlers. Ctrl+Space exposes directory-only calculation with entry counts;
refresh invalidates previous results. Directory traversal avoids redundant
metadata reads for directory/link entry kinds, and fuzzy scoring no longer
allocates a lowercase copy of each Unicode filename. These are source-level
changes, not a measured performance claim.

679 Rust tests, Clippy/release/fmt and a six-framebuffer functional scene pass;
see USABILITY for exact evidence and installation hashes. S01 recursive filename
search remains pending, as do size/date/content facets, per-row folder sizes,
complete MC navigation profile and native input acceptance. Resource/startup
freeze and preview expansion pause stay active.

## 2026-10-30 — Recursive filename search

Second usability slice: Ctrl+Shift+F, Actions/background menu and a dedicated
keyboard/mouse panel with captured root, local hidden scope, basename+parent,
open/locate and bounded virtualized rows. FD-anchored no-follow Linux traversal
stays on the root mount; results use raw IPC paths and explicit partial states.
One shared helper supervisor and cancellation/ACK admission; root close drains.
Modal DnD/file-clipboard/popup rejection and generation-safe text paste added.

688 Rust tests and Clippy/release/fmt pass, eight framebuffer final scene scoped
PASS; see RECURSIVE-SEARCH.md for reports, caps and limitations. Resource/startup
NOT_MEASURED; new previews still paused. Content/date/size search, pagination,
ignore rules, size rows/progress and other ROADMAP work stay open. No global PASS.

## 2026-10-30 — Measured folder rows and queue

Ctrl+Space/Ctrl+Shift+Space, inspector and menus now drive a bounded current-root
queue, one shared admitted helper, streaming partial counts and measured rows.
List/Grid/Groups/Columns/dual matching-root displays and size sorting consume
observations, with unknowns last. Refresh/owner/navigation/mutation invalidation,
late cancel and final-after-reap semantics are documented in FOLDER-SIZES.md.
External deep changes still require refresh; allocated space/hardlink dedup,
recursive cache watchers/mount policy/offline syscall handling remain separate.
Resource/startup freeze and preview expansion pause remain active.

## 2026-10-30 — Selection tools and MC focus

Filter-to-mark/show-all Ctrl+Shift+A, Ctrl+Shift+D true empty operation selection,
Actions/context equivalents and marked footer counts supplement Insert/Ctrl+A/I.
Invert updates one set; All keeps focus; refresh restores a surviving marked focus
when a filter removes its old name. Selection remains current visible scope with
raw operation names and existing owner/budget rules. SELECTION-TOOLS.md records
validation. Classic MC full profile, general keyboard/accessibility/native input
and the broader UI/tools/search/operations ROADMAP remain open; freeze unchanged.

## 2026-10-30 — Coherent keyboard navigation and viewport rendering

Home/End, pages, Shift ranges and Ctrl independent focus now share a browser
handler across List/Groups/Grid/Columns/dual. Groups model order follows its painted
date sections; five derived boundaries replace per-paint index vectors and full
section scans, while List indexes visible rows directly. Focus strips, partial-row
reveal and stale/empty anchors are addressed. KEYBOARD-NAVIGATION.md records limits.
Full MC profile, command catalog/palette, accessibility/native input and overall
operations/search/memory/startup goals remain open; no resource freeze lifted.

## 2026-10-30 — Editable path field

Cursor/selection/scalar-safe midline insertion/delete, text undo/redo, mouse
selection/component, horizontal scroll and confirm/cancel buttons replace
append-only path editing. Clipboard target includes cursor revision; raw/control
paths use explicit bounded local URIs, exactly-once user confirmation decode.
PATH-EDITOR.md records caps and gaps: graphemes/bidi/native IME/context menus/
completion/filter-caret/platform acceptance and overall resource/startup remain
open. No new preview work, worker, timer, dependency or global config change.

## 2026-10-30 — Search size/date predicates

Recursive search accepts checked apparent-file size ranges/comparisons and UTC
modification date boundaries, composable with existing name/ext/type/negations.
Metadata is requested only after cheap candidate filtering; ordinary queries do
not add lstat calls. Unknown metadata is excluded even under negation and marks
results partial. SEARCH-FACETS.md records semantics and limits. Relative/local
dates, unit/calendar pickers, content search, streaming progress/index and global
performance/native-input acceptance remain open. No resource freeze lifted.

## 2026-10-30 — Visual search criteria

The search drawer adds bounded minimum/maximum sizes and UTC since/before fields,
mouse targets and F4/Tab/ShiftTab/CtrlF/CtrlA/Enter/Esc keyboard policy. Criteria
AND with advanced text and survive collapse; clear preserves the advanced query.
Compilation is bounded before helper admission, and a separate edit epoch prevents
paste redirection across inputs without discarding in-flight work on focus alone.
SEARCH-FACETS records exact semantics. Rich caret/IME, picker controls and local/
relative dates, streaming/content/index search and overall native/resource goals
remain open; previews paused and resource/startup freeze unchanged.

## 2026-10-30 — Discoverable commands

COMMANDS adds a 46-action fuzzy catalog opened by Ctrl+Shift+P or the Actions
menu, keyboard pages/mouse rows and explicit disabled reasons. Dispatch reuses
normal operations/queues and revalidates owner/generation/exact selection; stale
painted targets carry an input epoch. Selection backing is <=64KiB, query256B,
no file-name clones or per-paint full selection scans. Native menus and compact
fallback retain separate preferences/actions. Registry matching does not execute
shell text. This is not a full MC profile/terminal, arbitrary command launcher,
new transfer engine, native-input or resource/startup acceptance; the full roadmap
and pause/freeze remain intact.

## 2026-10-30 — Bounded live search counters

SEARCH-PROGRESS adds a separate legacy-compatible NDJSON search_stream command,
monotonic provisional counters and one latest slot using the existing supervisor.
No paths/actions before final root validation and reap. Open/Locate/Stop reflect
availability; Enter waits for current work and refreshes completed empty searches.
A40k private fixture verifies positive progress and cancel/new-intent plus previous
controls/raw/partial behavior. Streaming result batches, index/content/pagination,
platform/offline/native-input and resource/startup acceptance remain pending.

## 2026-10-30 — Size queue pause and selective retry

Acciones and command catalog expose Pause next/Resume/Retry failed sizes. Pause
finishes the active helper and gates the next admission; retries preserve pause,
good totals and manually cancelled rows. Queue/root/ACK contracts unchanged.
Paused rows and status counters take precedence over rotating help/free-space;
compact440 status remains separate from incidental selfPSS. Catalog now49 actions,
unavailable controls retain reasons.748Rust/Clippy/release/fmt and final ten-FB
scoped size scene in FOLDER-SIZES.md. Earlier clipped footer preserved as visualFAIL.
No new engine/dependency/worker or memory improvement claim. Native input/global
resource/startup/snapshots/mounts/offline/size catalog detail and full ROADMAP open;
freeze and preview pause unchanged.

## 2026-10-30 — Rich recursive search draft editing

Query+four criteria now share path scalar-editor mechanics: caret/ranges, Ctrl-word,
Delete/undo/redo, mouse point/drag/double component and horizontal viewport. Input
vs result focus keeps Home/End/Enter predictable, Tab/CtrlF returns to input;
cursor changes preserve valid results/worker but invalidate late paste targeting.
Bounded point/exact-text cache avoids width measurements on progress/caret-only
paints; binary pointer lookup, no timing/PSS claim. SEARCH-EDITING.md records754Rust,
Clippy/release/fmt and24FB final scoped scene, known CJK/micro-hint limits.
Ordinary filter still pending this editor; IME/graphemes/native/global resource
acceptance and restROADMAP remain open. Freeze/newpreview pause not lifted.

## 2026-10-30 — Local filter text focus

Browser filter now uses bounded Cursor/line_field geometry: CtrlF retains/selects
query, live middle edits and per-field undo, text mouse selection and async paste
epochs. Text vs file shortcuts split, Enter/Tab return list, Backspace in list goes
parent, CtrlShiftA/D MC selection kept. Path pencil remains mouse-accessible with
query; pane/tab queries persist while ephemeral edit context/history drops.
758Rust/Clippy/release/fmt and final12FB+samebinary24FB search regression documented
in FILTER-EDITING.md with3fixtureFAILs/guide visualFAIL and preserved evidence.
No native/global resource acceptance, new engine or measured saving; freeze and
preview pause unchanged. Minimum informational toast, other responsive widths,
IME/graphemes and full tools/operations/search/size/ROADMAP remain open.

## 2026-10-30 — Folder-size queue tool

Actions/Ctrl+Alt+Space/catalog expose the existing queue as a mouse/keyboard tool:
per-row cancel/retry, pause next admission, stop all, failures-only retry, visible
batch and wrapped diagnostics. Borrowed records, monotonic attempt IDs and captured
pane/tab/root/generation/epoch reject obsolete controls and old completions.
764 Rust tests plus Clippy/release/fmt; scoped14FB manager and11FB catalog scenes on
the same final binary. Details and limits in SIZE-MANAGER.md/VALIDATION.md.
No new scanner/cache/dependency or measured resource claim; native-input and global
resource acceptance, snapshots/offline/mount policy and broader ROADMAP remain
open. Resource/startup freeze and new-preview pause preserved.

## 2026-10-31 — Optional MC operation strip

Persistent Preferences/Ctrl+Alt+B/catalog toggle exposes eight existing F-key
actions to mouse users too. Reserved viewport space, short labels at440, full
accessible names, text-focus guard before pointer blur, current owner/generation
and existing operation preflight; no extra engine/payload cache. See
OPERATION-BAR.md and VALIDATION.md for scoped checks. Full keyboard/native/global
resource acceptance and the remaining operations/search/SFTP/shaders/i18n roadmap
stay open; preview expansion and resource/startup freeze stay unchanged.

## 2026-10-31 — Literal UTF8 content search

Recursive search now combines names/metadata with one exact positive content
criterion. Texto/F6 preserves separate drafts; advanced content syntax remains.
Bounded streaming KMP/UTF8 validation, same-FD metadata+revision postcheck, explicit
binary exclusions/limited/changed partials, first match line/byte. Same supervisor/
cancel/ACK, raw paths and output caps; no new service/dependency/index/cache.
780 Rust+Clippy/release/fmt,4 release CLI cases and24FB final scene scoped PASS;
CONTENT-SEARCH.md records reports/bounds/limits. Regex/case folding/encodings/
semantic/index/pagination/native/global-resource acceptance remain open, as do
operations/SFTP/shaders/i18n/restROADMAP. Freeze and new-preview pause unchanged.


## 2026-10-31 — Favourites and quick folders

CtrlD/CtrlAltD/Nerd star/catalog53 share one bounded favourites/XDG/current-tab
history hub. Labels only, raw paths, existing async navigator and AuxIo worker;
cooperating-window lock/reload/merge, monotonic row IDs/exact expected tuple,
64KiB wire64rows, explicit captured retry/omit and published-unsynced state.
793 Rust+Clippy/release/fmt; four-process11FB scene and same-binary11FB catalogue
regression scoped PASS. See BOOKMARKS.md/VALIDATION.md for preserved failures and
bounds. Native input/global resources/mount management/other platforms remain;
no reduction of operations/SFTP/shaders/i18n/restROADMAP, freeze/previews unchanged.


## 2026-10-31 — Copy/Move destination editing

The operation review field now shares bounded Cursor/Geometry: keyboard and
pointer selection, midline Unicode edits/undo, horizontal reveal and per-dialog/
caret/draft paste epochs. Source capture/preflight/global queue/no-clobber stay;
actual destination edit discards a prepared move, caret motion retains it.
Raw/control-containing captured destinations remain bytes until complete explicit
replacement; display escapes/truncates and cannot be parsed back into a path.
See DESTINATION-EDITOR.md/VALIDATION.md for scoped evidence. General conflict
skip/replace/rename/journal and multi-origin/EXDEV tasks are still open; this is
no completion of O01/O03. Resource/startup freeze and preview pause unchanged.


## 2026-10-31 — Explicit Copy Skip Existing

Copy default Stop retained; per-dialog Nerd/F4 Skip continues independent roots
without overwrite/merge or applying to Move. Kernel publication collision may
skip only after clean identity-aware staging cleanup; other errors/rescue stop.
Complete skipped/published counters and shared bounded requested-path details,
unchanged4MiB queue/2MiB ACK headroom.806Rust+Clippy/release/fmt; final8FB owned
scene scopedPASS, initial compact440 visualFAIL preserved and corrected. See
COPY-CONFLICTS.md/VALIDATION.md. O03 remains partial: replace/rename, interactive
per-root decisions, Move, durable multi-root recovery and native/global-resource
acceptance remain; restROADMAP/SFTP/shaders/i18n/freeze/preview pause unchanged.


2026-10-31 Copy destination names: per-root F2 editor/Alt arrows/Ctrl+L and same-folder duplication delivered with scoped9FB/818tests. Raw sources immutable; actual selected FD verified before copy. O03 remains partial: general replacement/Move naming/durable recovery pending; resource/startup freeze and previews pause unchanged. See COPY-NAMES.md.

2026-10-31 Folder-size manager order: button/S Queue→Largest→Name, complete measurements only (unknowns last), selected identity and queue order preserved.820Rust/17FB scoped PASS; SIZE-MANAGER.md. Resource/startup freeze and native/global acceptance remain open.

2026-10-31 Search incremental WIP: borrowed scanner prefix/first-hit and bounded core Candidates/Batch32/500/256KiB/exactfinal reconciliation validated (830workspace+6codec tests,Clippy). NOT yet protocol/UI/live-result acceptance; S01 remains partial. Installed omnibar checkpoint unchanged; freeze/previewsPAUSA. See SEARCH-BATCHES.md.

### 2026-10-31 — Copy Keep Both (scoped)

Copy F4 now cycles Stop/Skip/Conservar ambos, reset per dialog; Move unchanged.
Numeric no-clobber publication reuses staged payload after late collisions,
reports actual paths and complete renamed totals. Raw names/compound extensions/
same-folder duplicate,1000 candidates/name255/path4096 bounded.853Rust269/85/499
(+3ignored), Clippy/release/fmt PASS. keep-both-ui1793436602618559696 PASS7FB/
root0ACK; conflicts1793436615203947340 PASS8FB/root0ACK. Internal handlers and
real framebuffer only. Original harness FAIL retained. COPY-KEEP-BOTH documents
limits; overwrite/merge/Move conflicts/durable general recovery remain open.
Resource/startup NOT_MEASURED freeze; previews paused; no global PASS.

### 2026-10-31 — Sizes manager → folder navigation

Alt+Enter and Nerd folder button open selected size row through existing browser/
history; stamped context+selectedID, no UI FS probe/new scanner/cache. F1 updated.
855Rust269/85/501+3ignored/Clippy/release/fmt PASS. sizes-open1793437301816876363
PASS5realFB1000×650/440²(all5inspected),32B largest keyboard/raw0xFF mouse/history/
stale disabled/source hashes/root0drainACK. Original harness menu-route FAIL
preserved. No native input/helper-lifetime/resource/global acceptance; freeze and
preview pause intact. SIZE-MANAGER documents ordinary path replacement semantics.

### 2026-10-31 — Search display order

F7/Nerd Ruta→Nombre natural→Tipo, selectionphysical≠displayposition; samepath
kept on sort andprovisional→final, arrows/HomeEndPage followvisualindices. Cache
invalidated onpopulation evenaddressreuse; no report/pathclone/helper/stat/cache.
858Rust269/85/504+3ignored/Clippy/release/fmtPASS. search-order1793437888618938167
PASS5realFB(all5inspected)/42hits/raw440/root0ACK/1loggedhelpergone strictZno;
batchesregression1793437899122910460PASS7FB/6gone/root0ACK. Originalprepaint
harnessFAILpreserved+sidecar. SEARCH-ORDER records transientnaturalkeys/cost;
reverse/size/date/relevance/paging-index/native/resourceglobalpending. Freeze and
previewpause/historicalFAILs intact.

### 2026-10-31 — Real SFTP/FUSE matrix, compatibility FAIL

SFTP-NATIVE-MATRIX: SSHFS3.7.6 private build (no globalinstall/vendorlink), private
loopbacksshd/internal-sftp/fixturekeys+knownhost; realkernelmount/read/raw/relative
link/externalcat/write/busy-unmount/ordinaryunmount→absent/SSHFS0 PASS. Final
1793442486810260624:6loggedPIDstartgone strictZno/0identityerrors, copyEngineFAIL
EINVAL/0published/cleanstage/sourceintact; directrenameNOREPLACEerrno22; exclusive
links rejectoccupants butdifferentvirtualinodes. OverallFAIL/exit1, originals
retained+classification sidecar. No defaultcopyproof weakened. Need remotevisible-
exclusivepublisher withpartial/rescue semantics beforeworker/UI mountactivation;
notimplementedyet. Installed86bf3ff/f0df843preparationunchanged; no globalN01PASS/
input/nativeGUI/resource/startup/wholelifetimes/daemon/offlineproof. Freeze intact.

### 2026-10-31 — Explicit visible SFTP copy adapter

Core VisibleExclusive opt-in retains partial destinations and reports acknowledged,
completed and uncertain roots; no rollback/source deletion or default-policy change.
Six new tests cover tree/link omission, cancellation, root substitution, numbered
collision and child collision. Native SSHFS report1793444281194564656:
visible_adapter PASS; default stagedcopy FAIL errno22 and overallFAIL/exit1 retained.
No GUI/mountworker/install, native input or resource/startup claim. See
SFTP-NATIVE-MATRIX for scope and explicit omission of unsafe symlink timestamps.

### 2026-10-31 — Mount transaction foundation, not Connect

Pure sftp_lifecycle single-flight ticket/immutable intent, admitted spawn,
late-cancel mount retained, foreground exit reconciled, unreadable table uncertain,
exact-identity explicit disconnect and absence-before-next-transaction. No process
worker/mountpoint provenance/restart journal/UI activation yet. Review retains
port+probe and recheck revokes it; presentation shows port. 885 workspace tests
PASS +5 final controller tests, Clippy/fmt; no new release/install/GUI/resource
acceptance. Full SFTP-MOUNT-CONTRACT and ROADMAP remain open.

### 2026-10-31 — Mountpoint FD provenance, SFTP still open

Owned0700 parent and exclusive random point pinned/revalidated, mount coverage
blocks child inspection before IO, no auto-delete/chmod. Core5newtests plus
realSSHFS heldFD/unmount verification PASS; 890workspaceRust/Clippy/fmtPASS.
Not restart journal/actualworker/ConnectUI/hostilewriter guarantee. Installed
preparation unchanged; next wire process admission/lifetime and visible state.
Freeze and new-preview pause remain; complete ROADMAP remains active.

### 2026-10-31 — Native SFTP process engine

Connection core now executes fixed SSHFS argv and ordinary unmount, with bounded
sealed diagnostics, owned process reap, cancel/retained state and remote root
serving proof. Native full/owner-lifetime/strict-hostkey cases PASS scoped; source
namespace-onlyReady flaw found/fixed, raw failures preserved.895Rust/Clippy/release/
fmtPASS. No UI activation/new install: durable registration/lock/restart recovery,
AuxIo admission/close ACK and controls are next. OS mount survives graceful lab
owner exit and serves a freshly created remote file, not yet GUI-close proof.
N01/resource/global gates remain open; freeze/preview pause/full ROADMAP unchanged.

### 2026-10-31 — Durable SFTP registry and cross-process observation

Private anchored single-transaction registry16KiB, lifetime advisory lock/read-only
second opener, raw paths+Target/port+provenance+generation+PID/start+mount signature.
Atomic publication distinguishes Unsynced/Uncertain; retry only syncs. Unknown/
invalid/future/FIFO/symlink/changed records preserved. Active identities retained;
Absent publish requires fresh own-namespace absence+original point+process reap.
908Rust312/85/511+3ignored,13registrytests/Clippy/release/fmtPASS; syncEIO injected,
not powerloss. Native1793450438576487018 registry observation PASS: owner alive
readonly→owner gone exact mount/process match, fresh server file readable; after
LAB unmount reader reportsNoMount/Missing without rewriting history.7loggedIDs
gone/0errors/survivors, no lifetime coverage. DefaultcopyFAIL/overallFAIL retained.
Still NO AuxIo/actual-window close/recovered disconnect controls/Connect UI/new
install/resources/startup gates. Full ROADMAP active; freeze/previewsPAUSA intact.

### 2026-10-31 — Explicit recovered SFTP disconnect

New core controller never signals/waits original SSHFS or adopts a Child. Current
registry/preimage/lease/privateparent/fullmount revalidated, durable intent before
ordinary unmount child, fresh check after sync; no force/lazy/delete. Read-only,
stale, live/zombie prior unmount and unknown PID-gap reject before child. Busy
retained with diagnostic; retry/completion preserve identity and reap guards.
No-op closed reconciliation avoids another journal write.913Rust317/85/511+3ignored/
5newtests/Clippy/release/fmtPASS. Native1793453782020109493 readonly denial +
new-owner busy→retry→durableAbsent PASS,14loggedgone/0errors/survivors; first
1793453058010388848 preserved. Pythonprivate reaper handled originalserver, not
controller; not PID1/GUI/crashgap/lifetime/inputSO/resourceproof. DefaultcopyFAIL/
overallFAIL retained. Still no shared executor/UI/install. Freeze/previews and
complete goal/ROADMAP remain active.


### 2026-10-31 — SFTP product executor and controls

Core Connection/Store/recovered controller now wired to a lazy fixed transport
worker with bounded1request/1reply and single flight, separate from AuxIo saves.
Explicit reviewed connect; fresh/recovered serving-proof Open; ordinary disconnect,
reconcile and sync buttons/keyboard; polling/reaping while modal hidden. Close
revokes future work and awaits mutation ACK, without claiming blocked readonly I/O
was drained. Registered OS mount survives window exit. Failed publication retains
child/registry and blocks automatic replay; known Absent allows another connection.
Inspection does not hold a writer lease. Missing host sshfs remains visible and no
automatic global install is performed. One registry slot, no connection catalog.

See SFTP-PREPARATION.md and evidence/sftp-native/ui-validation for current logs,
scoped native UI evidence and independent install verification. Native proof uses
actual loopback SSHFS/FUSE with private keys/known_hosts/config and product handlers;
no OS input or resources/startup experiment. Existing default staged-copy FAIL is
preserved; visible-exclusive copy policy needs an explicit product review before
wiring. N01/full ROADMAP and hard/ideal resource goals remain open; previews paused.

Validation:919Rust318core/85helper/516UI(+3existingignored), Clippyworkspace/alltargets-Dwarnings, release/fmt PASS. Native `sftp-loopback-1793455941407692094.json` has UI+freshConnect+mount PASS with8PNG1000×650/3root0;25loggedPID/start identities gone,0readerrors/survivors, not lifetimecoverage. Fixture parent reaped the old SSHFS, product only its own children. DefaultcopyFAIL/overallFAILexit1 are unchanged. Initial recovered UI mismatch1793455368964965907 and its sidecar preserve wrong Registered-vs-Retained expectation; no broadening to arbitrary states. Final negative-tool/440² scene1793455971563736333 has6PNG, no registry creation/source mutation/root0. No native input, clipboard or PSS/Swap/CPU/startup measurements. Long diagnostic lines can still clip; a full copyable transport report remains pending.

Final lifecycle follow-up:921Rust319core/85helper/517UI(+3ignored) and Clippy PASS. Closing increments a shared admission epoch; cancelling close resumes only NEW epoch requests, never revives a queued revoked command. Added deterministic test. Registry Lease now explicitly unlocks only in its acquiring PID, including construction-error paths: deterministic forked-FD test proves child cannot unlock parent, and owner Drop allows a new opener while child retains inherited FD. The prior concurrent new_owner.writable() failure remains in ui-validation/sftp-executor-close-epoch-tests.log; which child caused that historical failure is UNKNOWN. No serialization/relaxed assertions/rerun-only resolution. Current evidence references are listed in ui-validation/checks.json.

Final release UI acceptance: native `sftp-loopback-1793456384121839709.json`, product controls/freshConnect/mount PASS8PNG/3root0; negative-capability and440² `sftp-preparation-ui-1793456409825960118.json` PASS6PNG/root0.25loggedPID/start identities gone/0errors/survivors; not whole lifetimes. Native overall/defaultcopy FAIL remain. Source/log/binary hashes and scopes in ui-validation/checks.json; supersedes previous display-only scene1793455941407692094 for installation. All original reports retained.

Installed source1754f83 through `evidence/0.0.4/installed-checkpoint-1793456472288022118.json`;5installed+5backups independently rehashed exact. Appab1d438d58784c133eeb7bb7c30641bf75c5c0012996121b012a7fc34ef4d911/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. No sshfs global installation/config/source publication/tag; host capability still reports missing sshfs.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Opciones in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925Rust319/85/521+3existingignored, Clippyworkspace/alltargets-Dwarnings/release/fmt
PASS. Native1793457917645506527 UI/copy/mount PASS16framebuffers incl440² and
1000×560;4copiedroots(raw0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expectedFAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves57partial
files and original3000files intact.24loggedPID/start identities gone/0errors/survivors,
not lifetimes;3roots exit0 after normal callback. Native defaultcopy/overallFAIL
exit1 retained. Final copy-names UI1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visualFAIL preserved+sidecar;
explicit bounded lines fixed and finalframebuffer inspected. No source delete,
automatic removal of visible partials, powerloss/nativeinput/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.

Installed source `e58f576`, report `evidence/0.0.4/installed-checkpoint-1793458216633312563.json`;5installed+5backups independently SHA256 exact. Appfee00a4d1260bdd2163dd84798eafc6e1d3f3626d3de7feb86a812ecd53e611c/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Fixture rwx+mtime equality checked specifically a.txt with whole-second time; byte hashes also cover tree child/raw file, literal symlink payload and outside-target mtime. No nanosecond/ACL/xattr/ownership fidelity claim. No source push/tag/global sshfs install.


### 2026-10-31 — Captured local search calendar

F8/Nerd calendar: Hoy/Ayer/hoy y seis días previos, process-local calendar resolved
once on existing AuxIo with reference captured on choice. Dates populate fixed
UTC bounds and require explicit Search. Date-only UTC syntax unchanged; strict
YYYY-MM-DDTHH:MM:SSZ extension, mixed bounds compare instants. Latest/epoch/close/
reopen/edit guards; readonly ACK separate from helper reap, no per-click thread.
Content editor preserved after ACK, stale paste and overflow reject without
changing criteria. No per-result DU or added metadata calls beyond existing lazy
metadata predicates. See SEARCH-CALENDAR.md for DST/error and platform scope.

937Rust324core/85helper/528UI(+3ignored), Clippyworkspace/alltargets-Dwarnings,release/
fmt PASS. Eight private TZ process cases: leap,23/25h, Santiago midnight gap/
25h yesterday, Havana fold, Apia skipped date,7dates/DST. UI
search-calendar-ui1793460920785323202 PASS26framebuffers incl440² and1000×560,
actual AuxIo+files-helper: inclusive start/exclusive end+subsecond/raw name,
metadata_reads7 for seven candidates, no automatic helper on preset, yesterday/
seven, dates+content/raw filename/editor, equal mixed bounds reject and existing
UTC controls regression. Root0 after callback/ACK,16loggedPID/start helpers gone/
reused strictZfalse, not lifetimes/helpervivoatclose/kernelD/OSinput/clipboard/
otherOS/resources/startup proof. Earlier name-only scene1793460239093706643 retained;
syntax FAIL wiring log preserved. Full goal/ROADMAP, freeze and previewsPAUSA open.

Installed source36b8a18 through `evidence/0.0.4/installed-checkpoint-1793461261514927889.json`;5installed+5backups independently SHA256 exact. App7654450b01fadcfbf004461820ecdcb12f678a07085a05d3a7834dd4f2e9134a/helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Source publication/tag/global timezone/config changes not performed.

### 2026-10-31 — Size manager captured report and subtotal

SIZE-MANAGER now documents a completed-only checked apparent subtotal and a
bounded64KiB report: exact bytes/observation timestamps/raw debug escaping,
unknown/error/cancel/progress distinct from zero, explicit F5 refresh, keyboard
and mouse scroll/copy-request/return. Cached UTF8 byte-range layout (4096lines)
reuses text; no new scanner/worker/dependency/disk cache. Stale contexts block
Copy/Refresh and expose explicit Recapture. OS clipboard delivery remains
UNVALIDATED; local caps are not a resource or speed proof. Sizes across different
observation times are not a filesystem snapshot, deduplicated usage, freeable
space or operations authority. Full ROADMAP remains active; native input,
resource/startup gates, SFTP profiles/diagnostics, broader operations/shaders/i18n
and historical failures remain open. New previews remain paused.

### 2026-10-31 — Search locations in background tabs

SEARCH-BACKGROUND-TABS: Nerd result button/CtrlEnter creates a metadata-only tab
for directory or file parent+raw focus without closing search or changing active
pane/tab/root/generation. Existing8tabs/3MiB joint stored+closed+other-pane budget,
empty-history charge, no new listing/helper/prefetch until ordinary activation.
945Rust/Clippyworkspace-Dwarnings/release/fmt PASS; actual search_batches scene
1793463564614078345 scopedPASS10FB/provisional-retained/raw440/emptydir/8tablimit/
sourcehashes/root0ACK/3loggedPIDstartgone strictZfalse. Source/test accounting
reserve issue fixed, originalFAIL preserved. Native input/resources/lifetimes and
wholeROADMAP remain open, freeze/newpreviewPAUSA intact.

### 2026-10-31 — Search raw-path multi-selection and atomic location batches

SEARCH-MARKS: Nerd checkbox/Ctrlclick/Shift range/Space/Insert/CtrlA-D and mouse
Todo/Ninguno; focus independent of marks, visual-order ranges,128paths/64KiB
borrowed preflight. Raw paths reconciled provisional→final with pruned count;
explicit empty never falls back. Monotonic result epochs reject old row hits.
Marked locations open as atomic metadata-only tab batches in visual order under
existing8tab/jointbudget, no preloads.951Rust/Clippy/release/fmtPASS and actual
helper scene1793464566350502738 scopedPASS15FB/root0ACK/3loggedhelpersgone strictZno.
Copy/Move/Trash from multi-origin marks still need Source/target/queue integration;
no clipboard/native-input/resource/globalPASS. Freeze/newpreviewPAUSA intact.

## Search-result Copy/Move — 2026-10-31

F5/Copiar and Shift+F6/Mover from marked search results now capture up to128
roots/32parents through AuxIo, validate effective FD proofs and open the existing
review/queue. Cancel retains marks; stale intent/changed root cannot redirect the
job. Global queue/FD/metadata accounting remains shared. Native multi-parent Move
reports effective observed locations and confirmed/pending/uncertain roots;
EXDEV remains one regular nlink1 only, not batch/tree/hardlink transfer.
964Rust327/85/552 PASS+3existingignored, Clippyalltargets-Dwarnings/release/fmtPASS.
Final scene1793470188336000116 scopedPASS27realFB,5loggedhelpersgone strictZno,
root0ACK; cancellation/paused/replacement/partial conflict/raw/copy5/move5 and
single EXDEV actual bytes/inodes verified. See SEARCH-TRANSFERS.md and
evidence/search-order/transfer-validation/checks.json. Original FAILs retained.
No inputSO/clipboard/lifetime/performance proof. Resource/startup NOT_MEASURED
freeze and newpreviewPAUSA intact; full ROADMAP/global gates remain open.

## Explicit pending Move retry — 2026-10-31

Incomplete multi-parent Move now offers Nerd **Reintentar pendientes / Ctrl+R**
in its own report. Same original token/ledger/destination, fresh ID, no recapture
or replay of Confirmed; Uncertain/unresolved cleanup blocks. Atomic queue
reservation transfer restores full outcome/proof on rejection. Repeated failures
retain one diagnostic, cancellation before execution keeps proof until ACK.
970Rust327/85/558+3ignored PASS, finalClippyalltargets-Dwarnings/release/fmt; full
suite before only title text/assertion, targeted report-title test also PASS.
Scene1793471063548051155 scopedPASS29FB/root0ACK/6loggedhelpersgone strictZno,
occupied retry then fixture occupant removal→CtrlR pending-only/sameinode/1root.
Previous command-prefix FAIL and wrong success-title capture preserved/sidecars.
See SEARCH-TRANSFERS.md and evidence/search-order/retry-validation/checks.json.
No nativeinput/clipboard/lifetimes/metrics. PSS/Swap/CPU/startup NOT_MEASUREDfreeze;
new previews paused, full ROADMAP and global acceptance remain open.

## Keyboard and mouse operation reports — 2026-10-31

Up/Down/PgUp/PgDn/Home/End and wheel share bounded, viewport-derived navigation.
Nerd first/previous/next/last buttons have no active target at unavailable edges;
line range/total and key hints stay visible. Paint measures/draws full visible
rows only, resize clamps a full last page, pan clamps actual visible overflow.
No report clones/newworker/decoder/dependency; no PSS/speed claim.975Rust327/85/563
(+3ignored)PASS, final2focusedtests/Clippyalltargets-Dwarnings/release/fmtPASS;
fullsuite before onlyteststyle/tooltiplabels. Scene1793471804962465697 scopedPASS
34FB440/1000, report10/7visible→full-last/widefitsall0, operationunchanged,
root0ACK/6loggedhelpersgone strictZno. DisabledEnd fixtureFAIL+sidecar and2Clippy
test-styleFAILs retained. See REPORT-NAVIGATION.md and
evidence/search-order/report-navigation-validation/checks.json.
Freeze resources/startup/newpreviewsPAUSA, nativeinput/clipboard/AT-SPI/lifetimes
and global/fullROADMAP acceptance remain open.

## Saved/reusable search criteria — 2026-10-31

F2/Nerd catalogue and CtrlS named capture; readonly review, raw root, name/literal,
4size/UTCfields, localhidden/editor/order. Load stored/current search root fills
draft only; Enter executes. Metadata ACK never applies search fields and selects
a newly saved entry only for exact unsuperseded request/panel.32entries/128KiB,
2passcodec, regular-onlybounded reads, checkedatomic/CAS entry/creatorPIDflock;
existingAuxIo/sentACK/explicit retry-discard, published-unsynced distinct.
993Rust334/85/574+3ignored/Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
Scene1793475882305119370 scopedPASS4exit0ACK/14FB/6loggedhelpersgone strictZno,
load-nohelper then Enter/raw reopen/rename/remove/futurepreserve/compact440 fields
andcontentrows. Root fields32B and raw4096B/global128KiB are local bounds only.
Inheritedflock risk tested with2privateforks; original WouldBlock exactcause not
claimed. OriginalFAILs+sidecars andfirstPASS before new-row selection fix retained.
See SAVED-SEARCHES.md and evidence/search-order/saved-search-validation/checks.json.
No inputSO/OSclipboard/AT-SPI/lifetimes/powerloss/kernelD/resource/startup/global
acceptance; freeze/newpreviewPAUSA/fullROADMAP intact.

2026-10-31 Folder-size manager F5/Nerd Recalculate requeues only the selected completed observation with fresh ID, preserves other totals/pause/admission order, removes old subtotal/sort value immediately.997Rust+focused sort/Clippy/release/fmtPASS;24FB/rawhelper scopedPASS, SIZE-MANAGER.md. Resource/startup freeze and global/native gates remain open.

2026-10-31 Search elapsed windows: F8/key4–6 or Nerd1/24/168h choices materialize fixed UTC seconds on existing AuxIo; calendar1–3 remains local dates.999Rust/Clippy/release/fmt and23FB/helper scopedPASS; SEARCH-CALENDAR. Natural-language age syntax/custom rolling periods/index/pagination/native/resources/full ROADMAP remain open.

2026-10-31 Search pagination FOUNDATION_ONLY: retained FD walk/search_pages helper with exact bounded credit,500/256KiB windows and cumulative limits, core request-bound ledger.1007Rust/Clippy/fmt/dev-helper and7actual pipe cases PASS; SEARCH-PAGES.md. UI reader/history/navigation/progress/close and installation pending; installed SearchBatches still capped500. No resource/startup/global/native acceptance.

2026-10-31 Search pagination UI: F9 retained walk, CtrlPgUp/Dn/Nerd arrows, up to8pages/2MiB accounted history, explicit eviction/page-local marks, empty terminal/Stop preserve accepted data; invalid protocol cannot resurrect rows.1023Rust/Clippy/release/fmt, six Apps18FB/eightloggedhelpersgone scopedPASS; SEARCH-PAGES. Exact raw-path capacity bug red→green; seven same-SHA release pipe cases reused. No resource/startup/native/global PASS; index/global order/arbitrary history remain open.

### 2026-10-31 — Local SFTP connection profiles

SFTP-PROFILES: F2/Nerd catalogue, keyboard/wheel/mouse, named save/CtrlS, explicit
load-only draft/invalidated Review, remove exact entry, retry/discard/sync-only ACK.
32rows/128KiB, borrowed preflight/Target validation,0600/regular-only/atomic writer
with interprocess lock; same AuxIo, no new worker/credentials/automatic connection.
Modal+catalogue revision+stable ID protect stale load/remove even after ID reuse.
1040Rust349/91/600+3ignored PASS; final Clippy/release/fmt PASS.
Scene1793484790015975923 scopedPASS3apps/7realFB,440caption fits, no realhost/inputSO
or metrics/lifetimes/activefsync-close proof. Original clipped subtitle positive
and failures preserved; old spawn-marker test cause OPEN, strengthened assertion
without runtime supervisor change or serialization. Mount registry still one slot,
profiles are not host trust/keyring/reconnect/multi-mount. N01/N02/fullROADMAP open;
freeze resources/startup and preview pause/historical FAILs unchanged.

### 2026-10-31 — Readable SFTP diagnostic capture

SFTP-REPORT: F4/Nerd, captured draft≠observed connection, retained diagnostic,
24KiB/explicit truncation/control escapes/raw mountpoint representation. Glyph
width/whitespace wrap with scalar long-token fallback, borrowed line ranges/cache,
height clamp/source anchor on resize, arrows/wheel/pages/HomeEnd and Nerd controls.
F5 only recaptures held state; no probe or network dispatch. Copy requests exact
current snapshot; stamp/reportID protect stale refresh/back/copy/nav. Report and
profiles reject hidden form transport controls; readonly report rejects paste.
1050Rust349/91/610+3ignored/Clippyworkspace-Dwarnings/release/fmt PASS. Final report
1793486210147321426 scopedPASS2apps7PNG/root0ACK; profiles regression1793486235803344634
PASS3apps7PNG sameSHA. Copy handler observed, OS delivery UNVALIDATED; no actual
host/network/inputSO/lifetimes/activehelperclose/resources/startup experiment.
Original word-splitting positive+sidecar and fixture/compile/count failures retained.
No new worker/dependency/previews. Full ROADMAP/N01/N02 and historical gates open.

### 2026-10-31 — Archive operations shared foundation (not UI acceptance)

ARCHIVE-OPERATIONS-CONTRACT: common ZIP32 member streaming stored/DEFLATE,
realStreamEnd/exactconsumption/CRC, localneeded-version/header/extra/descriptor
checks,8KiBinput/32KiBoutput, quota-before-sink and cancellable reads inclEINTR.
Office adapter preserves1MiBVec/8MiBcompressed caps without duplicate decoder.
Read-only complete plan checks raw paths/types/prefix/canonical collisions,
64depth/255Bcomponent/20000nodes/1GiBoutput, local-range overlaps; no destination IO.
1068Rust367/91/610+3ignored, Clippyworkspace-Dwarnings/release/fmt PASS;30release
helperwire Python-generated stored/DEFLATE/descriptors PASS. No new dependency
package/version/checksum, only core edges to existing permissive codec packages.
No extraction/creation execution, queue/UI or new installation; current appCFAA
checkpoint5hashes independently unchanged. Initial false premise about legacy
emptyDEFLATE acceptance disproven and red retained, no vulnerability claim.
Next FD-bound staging/publication/rescue+sharedadmission+reviewUI; ZIP64/TAR/links/
otherformats and fullZ01 remain open. Freeze/newpreviewpause/globalFAILs intact.


### 2026-10-31 — ZIP extraction review and shared queue

Actions/context/catalogue extraer/zip → existing modal; source filename first,
new child destination editable/raw-preserving, review readonly then explicit Apply.
AuxIo+global operation worker only; no new thread/decoder/dependency.4MiB controller
unchanged, outcome must fit fixed2MiB report slot,3prepared+12transientFD reserve;
large graphs reject before mutation. Result drops before next dispatch; failed ZIP
cannot strand input charge.1093Rust387/91/615+3existingignored/Clippy/release/fmtPASS;
scene1793491842976117966 scopedPASS7PNGs, bytes/modes/raw/unchanged ZIP/other-tab
selection, occupied/traversal/CRC, callback paused→ACK→root0. Original positive
scene missing source basename at440 preserved+visualFAIL sidecar; final corrected.
Dedicated cleanup/sync retry buttons/durable catalogue, larger plans, create/ZIP64/
TAR/links/metadata/otherOS and fullgoal remain. No resources/startup/nativeinput/
clipboard-receiver/activekernelD/powerloss proof. Freeze/previewsPAUSA/oldFAILs stay.


### 2026-10-31 — Direct count is not a recursive size

Groups cards no longer label first-level stat sums as folder sizes. Typed direct
counts have no byte field,1024entry cap,complete/limited/partial/unavailable;
ReadDir only, no child name/type/stat requests. Existing root Stats supplies date.
Explicit calculator/helper unchanged, `Total:` only after measured recursive
completion; zero differs from unknown. Shared compare_sizes for core/active/
inactive browser, plain directory stat length never a total, unknowns last within their group both
ways. Same-root measured data used in inactive sorting; late count replies kept
across focus switch, avoiding permanent contando. Rebuild on Size invalidation
only, no extra Name/Modified sort on every switch. No new worker/cache/budget.
1100Rust388/91/621+3ignored/Clippy/release/fmtPASS; scene1793493877395472428 scopedPASS
4PNG1000/440,3directvs8B,0Bvsunknown,1024limit,bothorders,2loggedhelpersgone/root0ACK.
Original ETXTBSY fixtureFAIL+poison and wrong ascending-first GUI-harness FAIL+2PNG/
sidecar preserved. Inspector no-follow policy/shortcut now fit separately.
No resource/startup/native-input/lifetime/allocateddisk/snapshot/globalPASS claim;
freeze/newpreviewsPAUSA/fullROADMAP remain. Installation recorded separately.

Folder direct-summary/sort consistency INSTALLED7983c0f/app43a09343ee99,
record1793494152752973446;5installed+5backup hashes PASS.

### 2026-10-31 — Retire size observations without refreshing browser

SIZE-MANAGER installed8347b65/app18cb22c21e8c. O/Info button removes terminal row
only after active ACK; report/ShiftO uses a frozen completed-ID set, no newer values,
errors/pending lost, no filesystem deletion or refresh. Fresh IDs/control stamps,
128slot recovery unit proof, both-pane size-sort invalidation and browser selection
preserved.1128Rust+Clippy/release/fmtPASS;1793501340564075091 scopedPASS4PNG1000/440,
actual4DUhelpers then no helper on forget/pausedreadmit; whole own fixture tree
unchanged. Records forgotten are unknown, never measured0B. Freeze/native/global
acceptance unchanged; propagation of ordinary live size updates outside these
explicit actions remains a separate audit.

- 2026-11-01: live dual-pane folder-size sorting: accepted measurement updates
  inactive Size order while active Name order stays stable; browser and manager
  recalculation remove stale keys in both panes.1131 workspace tests/Clippy PASS.
  Real-helper scene and installation tracked in VALIDATION, no global gate claim.

- 2026-11-01: consistent Name/Size/Modified sort header with visible criterion/
  direction in single and dual lists; compact Nerd controls and keyboard palette
  date command. Inactive sorting preserves owner/measurements, rejects stale
  revisions.1133Rust +3existingignored/Clippy PASS; functional scene/install below.

- 2026-11-01: explicit zero selection survives local filter removal/Escape, with
  no default copy source; per-pane edit/clear controls and visible/loaded counters
  for mouse/keyboard workflows.1138Rust +3ignored/Clippy PASS. Full goal open.

- 2026-11-01: Terminal aquí (U02 parcial) en Actions/contexto/paleta: ruta actual
  capturada, broker uwsm servicio + helper ASCIIhex→cwd raw→xdg-terminal-exec;
  mismo slot de aperturas, no shelltext/defaultchange/daemonpropio. Otrasplataformas
  pendientes; aceptación/instalación se registra en TERMINAL-HERE y VALIDATION.

## Capture audit follow-up (NODE)

Fixture captures publish PPM atomically and optionally use request IDs, ACKs and
explicit observed/repainted comparisons. 1210Rust/Clippy/release/fmt and paired
X11×2/Wayland observations passed; INSTALADOe9281b5/app9e01f8493b57,5backups+5hashes,installed21captures/2rootexit0/6helpersgone,ROIequal. See `CAPTURE-AUDIT-CONTRACT.md`.
The original X11 missing-name frame remains an unresolved rendering/capture
observation. Matching ROI hashes in these samples do not establish its cause or
absence, and no resource/startup experiment or native-input claim is included.

## Search and bookmark path labels (NODE)

Result basenames/parents and bookmark path/history captions use visible raw-byte
and control escapes while retaining original destinations. History's caption
filter matches the displayed escapes; filesystem search syntax is unchanged.
1212Rust/Clippy/release/fmt and X11+Wayland50captures/6drainedroots/14tracked
reader identities gone/reused passed. INSTALADOcb74513/appff8b0c38b940,
5backups+5hashes, installed25captures/3roots/7trackedreadersgone. Original X11 capture
anomaly and remaining path surfaces/full-name clipping remain open.


- NODE 2026-11-01: propiedades muestra rutas solicitada/verificada y destino raw
  completos en líneas desplazables; nueve permisos y tres acciones caben a440×440.
  X11/Wayland funcional PASS31 capturas cada uno y regresión de rechazo/ACK PASS.
  Ver FILE-PROPERTIES.md y evidence/properties-layout. Copiar informe, metadatos
  restantes, entrada SO/AT-SPI/2× y el resto del objetivo completo siguen pendientes.


- NODE 2026-11-01: etiquetas escapadas comunes en copia/movimiento/ZIP y revisión
  de renombrado; original de copia distingue byte raw y escape literal sin tocar
  editores/targets. Copy X11/Wayland PASS13 capturas cadauno, rename raw-parent
  PASS. Clipping de cabeceras/columnas y selección/copia de rutas siguen pendientes.


- NODE 2026-11-01: S02 regex foundation SOURCE ONLY, core::search_regex bounded
  first-span/line including zero-width/EOF/multiline,429core+3ignored/focused4PASS.
  Existing std-only engine unchanged. CONTENT-REGEX-CONTRACT.md tracks required
  wire/snippet/page/helper/UI work; regex is NOT exposed/installed/complete yet.


- NODE 2026-11-01: regex wire foundation SOURCE ONLY: mode-bound optional
  regex_end spans, empty/EOF positions and no fabricated zero-width snippet;
  report/batch/page/stream validators preserve literal strictness/default mode.
 1224workspace433/95/696 PASS+3ignored/Clippy/fmt;500-hit wire limits unchanged.
  Query/helper/UI still pending in CONTENT-REGEX-CONTRACT; no new installation.


- NODE 2026-11-01: S02 advanced `regex:` content query integrated through core,
  checked-FD helper, batch/page streams and UI ∅ marker. Full allowedUTF8 file,
  multiline/empty/EOF; literalKMP preserved, bounded compiled state/read budget.
 1228Rust434/98/696+3ignored/Clippy/fmt/release and10CLI PASS; X11/Wayland28FB
  each/23loggedhelpersgone/root0. See CONTENT-REGEX-CONTRACT.md for exact receipts,
  initial diagnostic-build FAIL and corrected F9 fixture scope. Guidedregexmode,
  Unicodecasefold/otherencodings/index/remainingfullgoal stay pending.


- NODE 2026-11-01: guided Literal/Regex content selector and CtrlF6 preserve raw
  text/name/facets, no auto search, stale paste/result controls revoked. Presets
  v1 literal retained; mixedregexv2 mode explicit, real Save/Reload/Load tested.
 1230Rust435/98/697+3ignored/Clippy/fmt/releasePASS; X11/Wayland37captures each,
 27loggedhelpersgone/root0 and440px nonoverlap. GUIDED-REGEX-CONTRACT.md records
  limits and the preserved pre-paint fixture FAIL. Whole goal/global gates remain.


- NODE 2026-11-01: marked-folder size batch via command catalog and managerM,
  only visible marked dirs/no focus fallback/files/links; preflight/busy preserve
  old queue+pause, exact raw names, existing helper/ACK.1232Rust435/98/699+3ignored,
  Clippy/fmt/releasePASS. X11/Wayland10captures each/19loggedDUhelpersgone/root0,
 17B exact selected folders,440positive nonoverlap controls. See
  MARKED-FOLDER-SIZES.md; apparent per-name semantics/physical metrics remain distinct.


- NODE 2026-11-01: marked-only captured size report/scope+ShiftI, queue-wide default
  retained, missing observations explicit/not0, measured-only checked subtotal;
  Forget binds only captured IDs and preserves unselected observations. No new
  scan/helper.1235Rust435/98/702+3ignored/Clippy/fmt/releasePASS; X11/Wayland15FB
  each/19loggedDUhelpersgone/root0,60Bqueue/17Bmarked/43Bretained,440controlsfit.
  MARKED-SIZE-REPORT.md records limits; fullgoal/global gates stay active.

- NODE 2026-11-01: optional allocation foundation SOURCE ONLY in shared DU walker,
  regular-file blocks by name/device-inode with65,536identity bound and explicit
  overflow/limit/changed unavailability.1242Rust435/105/702+3ignored and strengthened
  helper105/Clippy/fmtPASS. No wire/UI/install change; ALLOCATION-CONTRACT.md
  defines semantics and integration pending; fullgoal/resource freeze remain.

- NODE 2026-11-01: allocation wire/core reader SOURCE ACCEPTED, opt-in v1
  measure_directory_allocated + typed final-only metrics; legacy usage unchanged.
  4096B reader/ID/mode/monotonic/exact final/sticky failure validation.1249Rust
  (442/105/702)+3ignored/Clippy/rustfmt/releasehelperPASS;7CLI incl65,536identity
  boundary, alias-at-cap/raw/sparse/legacy. UI job/queue/report integration pending;
  no install,5installedhashes unchanged; fullgoal/resourcefreeze remain active.

- NODE 2026-11-01: typed allocation worker+per-attempt queue SOURCE ACCEPTED;
  Job defaulttuple preview retained, tracked Measurement/mode-bound finals,
  retry/recalc clear oldallocation+retain mode/newIDs, staleACK/batchatomicity
  unchanged.1256Rust443/105/708+3ignored/Clippy/rustfmt/releasePASS;7CLI plus
  legacy marked-report regression X11+Wayland15FB/19helpersgone/root0 each.
  Historical folder-view WouldBlock initialFAIL preserved/nofixclaim. Noinstall;
  explicit allocation actions and report/UI presentation next; fullgoal/freeze.

- NODE 2026-11-01: allocation product controls/catalog focused+marked, manager
  B/ShiftB+9footer controls in3rows, per-row B mode/details and immutable reports
  expose regular-file blocks pername/peridentity, explicit unavailable/absence
  and no cross-root union/freeable claim.1258Rust/Clippy/fmt/release+7CLIPASS;
  actual allocated UI Wayland/X11 each22FB/2root0/25helpersgone. Local install
  acceptance follows; fullgoal/resourcefreeze/historicalfolderview+X11 remain.

- NODE allocation UI source8606261 INSTALLED checkpoint1793539706031130409;
  installed1793539716211647444 PASS22FB/2root0drain/25helpersgone,5+5hashesverified.
  Appb9941ab1acfb/helpera8fc8abaf26c; ALLOCATION-UI.md covers scope/limits.
  Full original objective and native/resource/global gates remain open.

- NODE 2026-11-01: general listing audit confirms existing notify usage is theme/media
  only. SOURCE listing_watch foundation:2targets,1controlworker,coalesced epoch/serial
  ACKslots,async attach/retry/stop;5local testsPASS/Clippy/fmtPASS. Initial workspace
  1263PASS beforeattachment refinement;latestfull1262PASS+1historical folder-view
  WouldBlockFAIL preserved,no globalPASS. Model bridge+root-alias coverage pending;
  no install/resourceexperiments; LISTING-WATCH-CONTRACT.md states limits.

- NODE 2026-11-01: listing watcher root/ancestor alias repair SOURCE ACCEPTED.
  Reproduced old root-symlink rearmFAIL; background resolved dev/inode checks
  before/after attach and every2s detect persistent alias resolution changes;
  no content scan. Per-arm stale native callbacks rejected.1267Rust+3ignored,
  final9watcher tests/Clippy/fmtPASS. Modelbridge/auto-refresh UI stillpending;
  no install/resourceexperiments; historicalWouldBlock/X11 notfixed/closed.

- NODE 2026-11-01: listing_sync Model bridge SOURCE ACCEPTED:stablebrowser/tab/path
  watches,coalesced deferredrefresh+serialACK,rangeanchor/explicit-empty/rawmarks
  and painted namedviewport restored aftermetadata,dual/tabs/closeACK/resume.
  1272Rust443/105/724+3ignored/Clippy/fmt/releasePASS;watch UI X11/Wayland6FB2roots
  each;allocation regression22FB2roots25helpers each. Columns/SpaceMap defer and
  native/offline/globalgates remainpending; fullgoal/resourcefreeze unchanged.

- NODE listing auto-refresh source81837e5 INSTALLED checkpoint1793544731266983878;
  installedwatch1793544742660384859 PASS6FB/2root0drain + allocation regression
  1793544743043408636 PASS22FB/2root0drain/25helpersgone.5+5hashes app8f7733dfce15.
  Columns/SpaceMap own updates/native input/offline/global/resource gates remain
  open; no globalPASS, previews/resourcefreeze unchanged.

- NODE 2026-11-01 SpaceMap marks de6c3a4/cd12a06 installed record1793555602802600273:
  projected All/Invert/Clear/MCInsert, ignored retained filter, stale context guards,
  partial/unknown/raw/hidden rows.1294Rust+3ignored;finalClippy/fmt/releasePASS;
  sourceWayland/XvfbX11+installed16captures3roots6helpers each. Five backup+install
  hashes; SPACE-MAP-SELECTION.md. Inspector CopyF5 caption still needs context audit;
  fullgoal/native/2x/offline/resource gates and historical X11 anomaly stay open.

- NODE 2026-11-01 command context f9e5388 INSTALLED1793556391638203701:
  inspector/menu copy captions no falseF5, generic copy tooltip, palette contextual
  recalculation label/alias/F5CtrlR; keyboard+MC share f5_action; actualMap !dual
  consistency guarded synthetically (normaldual setsList; no user-visible repro).
  1296Rust447/105/744+3ignored/Clippy/fmt/releasePASS;Wayland/XvfbX11+installed
  each19FB3normal roots7knownhelpers. Five backup+installhashes app5a9660a4ed5c.
  Initial watcher-testFAIL kept: Create+Modify maystraddle refresh; ownedkernel
  audit, one-event rename stimulus and laterwrite assertion; production unchanged.
  Next real command-availability parity Map keyboard/inspector/menu/palette/MC;
  fullgoal and original anomalies/native/2x/offline/resource gates stay pending.

- NODE 2026-11-01 acciones de Espacio 6d9c5dd/fixture924f8d4 INSTALADAS
  record1793558033383070484: disponibilidad compartida mapa para MC/paleta/menú/
  inspector/dispatch; validación de marcas antes de sanear sin fallback; revisión
  concurrente con copia en cola, F5 reservado y CtrlR ActionRefresh; previa existente
  en paleta, propiedades de una marca.1301Rust447/105/749+3ignored/Clippy/fmt/release
  PASS. SourceWayland/XvfbX11+installed cada29FB4normalexits15knownhelpers.
  Fixture XDG-data padre faltante causó dos FAIL28FB; mismo binario tras mkdirPASS;
  producción Trash intacta.5+5hashes app2cae3b73db4e. SPACE-MAP-ACTIONS.md.
  Next paridad de revisión con cola activa fuera deMap; CopyPath noUTF8 textual
  aún to_string_lossy (no losslessClipboardclaim). Fullgoal/freeze/gates pendientes.

- NODE 2026-11-01 QueueReview9ddd253 INSTALADO1793560331250393795: revisión Copy/
  Move/ZIP disponible con copia activa desde MC/paleta/menú/inspector/Action;
  validate antes de sanear impide fallback de marcas filtradas/ocultas/obsoletas;
  cola llena retiene formulario; cargos/IDs/ACK/admisión intactos. Progreso sobre
  MC y controles en2filas estrechos; último resultado sobre aviso compacto.
  1306Rust447/105/754+3ignored/Clippy/fmt/releasePASS. SourceWayland/XvfbX11+installed
  cada35FB5normalroots; Mapregression29FB4roots15helpers en cada backend. Wrapper
  Xvfb inicial cleanupFAIL con reciboPASS conservado; solo mismo binario exit0.
  Cinco backups+hashes appa715c17069d6. QUEUE-REVIEW.md.
  Nuevo pendiente observado: Dual440+lateral expandida tiene NameWidth0, control
  wide1000normal; priorizar nombre/metadatos adaptables y revisar List estrecho.
  No cierre de anomalía X11 ancha original. CopyPath noUTF8/native/2x/fullgoal/
  freeze y el resto de ROADMAP siguen pendientes.

- NODE 2026-11-01 NarrowNames f6a735f/fixture031aa00 INSTALADO1793562789004828933:
  celdas nombre primero, tamaño/fecha opcionales según ancho, sin formatear datos
  omitidos; List/Dual min440+sidebar con nombres legibles, tres sorttargets y raw
  focus/marks/owner intactos.1309Rust447/105/757+3ignored/Clippy/fmt/releasePASS;
  sourceWayland/XvfbX11+installed cada27FB2normalroots; queue regression35FB5roots.
  InicialFAIL expectativa1000×650 vs helper/PNG1000×560, corregida mismo binario;
  linthex fixture corregido sin suppress/serialización.5+5hashes app8f88cc804da3.
  NARROW-NAMES.md. Nombres arbitrariamente largos aún pueden recortarse; no cierre
  de anomalía X11 ancha original. Next CopyPath textual noUTF8 lossless policy;
  fullgoal/native/2x/otrosSO/offline/resources y freeze continúan.


## Reconciliación NODE — 2026-11-01

Las filas y entradas antiguas son historial, no prueba de que un pendiente siga
abierto. Se contrastaron fuentes antes de escoger C02: regex guiada ya existe
(`search_ui/state`, GUIDED-REGEX-CONTRACT); TerminalHere ya existe (`open_job`,
TERMINAL-HERE); informes SFTP y de operaciones ya tienen navegación/copia
(`sftp_report_ui`, `mutations`, SFTP-REPORT). No se reimplementan por una nota vieja.
No se han auditado ni cerrado todas las capacidades de esta matriz.

| Avance reciente | Evidencia vigente | Límite de la afirmación |
|---|---|---|
| Acciones de Espacio/selección MC y revisión durante copia activa | SPACE-MAP-ACTIONS.md, QUEUE-REVIEW.md | Disponibilidad y handlers/operaciones propios; gates nativos/globales abiertos |
| Nombres en List/Dual estrecho | NARROW-NAMES.md | Nombre priorizado sobre metadatos; no arreglo causal de la anomalía X11 ancha histórica |
| Copiar ruta y URI local sin pérdida raw | COPY-PATH.md, evidence/copy-path/assessment.json, aceptación84469ac | Instalado5c258f3a8d21,1311pruebas; sink aislado no entrega nativa de clipboard |
| Comparar árboles C02 | DIRECTORY-COMPARE-CONTRACT.md, core/directory_compare.rs | Foundation solamente; sin helper/UI ni instalación de esta capacidad |

FULL objetivo, sincronización y todos los demás pendientes se conservan.
Freeze absoluto PSS/Swap/CPU/startup y previews nuevas pausadas; ningún nuevo
resultado de esta reconciliación acredita métricas, otras plataformas, inputSO,
clipboard, AT-SPI,2x, lifetimes completos ni ausencia de bloqueo offline.


- C02 NODE: transporte/helper de comparación y supervisor verificados en
  evidence/directory-compare-wire/assessment.json; aún sin panel visible ni
  instalación. No confundir el job público de transporte con admisión/cierre de
  Model, ni sus observaciones con permiso de sincronización.


- C02 NODE: comparación visible integrada por directory_compare_ui; Acciones /
  paleta / menú de fondo, raíces reversibles y filtro de observaciones retenidas.
  Los diagnósticos sin informe no inventan veredicto ni borran el error. La
  admisión/cierre espera ACK; Pan conserva los demás nombres legibles.
  DIRECTORY-COMPARE-UI.md y su assessment contienen recibos finales/instalación.
  El fallo de tamaño de una regresión QueueReview con dos fixtures Wayland
  simultáneas está retenido; misma versión aisladaPASS, causa exacta sin confirmar.
  No arreglo causal de anomalías nativas ni PASSglobal; sincronización sigue abierta.

- C02 NODE: `directory_sync` source-only fresh review preparation adds Update and
  Mirror in either direction plus bidirectional Merge without inferred deletions
  or timestamp winner. Explicit content/link/type/basic-metadata/destination-only
  conflicts, checked one-row choices, subtree coverage and complete-review block
  on partial/changed observations. It consumes a new comparison, never an old
  report, checks canonical root containment and final root identities. This is
  not an execution token or installed synchronization. DIRECTORY-SYNC-CONTRACT.md
  records the audited copy/queue/rename/trash/EXDEV reuse boundaries and required
  supervised visible review, fresh pinned manifest, recoverable replacement and
  removal, journal/barriers, queue/cancel/close ACK and crash/reopen/undo gates.
  Missing-name copies alone do not meet C02. Full goal and resource/preview freeze
  remain active; comparison UI is still the installed checkpoint.

- C02 NODE: preparación de sincronización ahora tiene helper
  `prepare_directory_sync` + Intentversion1 y Job supervisado SOURCE ONLY.
  Modoversión/ID/roots/budget, ledger comparación estricto reutilizado,
  propuestas/choicesPending derivadas localmente, stream23400576B sólo para este
  comando; ordinarios4MiB intactos.1388Rust489/112/787+3ignored/Clippy/fmt/helperdebug
  PASS,17tests nuevos y18CLIhelper reales incluyendo ambos inventarios completos
  de8192nombres-control porlado (>4MiB) sin truncar ni cambios enfixtures.
  EOF/exit0/reap antes de resultado, nozeroexit/EOFsinexit/cancel postterminal
  rechazan y recogen hijos propios. Aún sin Model/admisión/rootcloseACK/UI visible
  ni Apply/manifestFD/journal/recovery/instalación sync. Próximo dryrun visible;
  DIRECTORY-SYNC-CONTRACT y evidence/directory-sync-wire documentan alcance.
  FULL objetivo/freezes y comparación instalada permanecen; no PASSglobal.

- C02 NODE: `directory_sync_manifest` SOURCE ONLY fresh pinned evidence reuses
  EXDEV inventory without changing FLMOVE/Entry wire format or legacy bounds.
  Parent/root pins, original-path reopen/mount/ancestry checks, per-entry mount
  proof, full revision/content/xattr-metadata digests, namespace and hardlink
  internal/external groups; both8192child inventories plusroots fit. Shared regular
  payload budget includes final rehash, no oldreport/choice-to-Apply conversion.
  1409Rust502/112/795+3ignored/13newtests+Clippy/fmtPASS normal, including real
  independent filesystem and ancestor-alias retarget. Initial1408PASS followed by
  collapsible-if lint retained/fixed. DIRECTORY-SYNC-MANIFEST.md and assessment
  record scope; canonical seal/new approval, helper/UI, journal, recoverable
  effects/queue/crashreopenundo pending. Installed dry-run remains289db327/helper170f.
  Absolute resource/preview freezes, full goal and global gates remain active.

- C02 NODE: `directory_sync_record` SOURCE ONLY canonical sealed evidence and
  byte-exact/rehash live binding; decoded history and56B journal comparison keys
  do not grant operator approval, fresh barriers or writes. MAXwire4657718B,
  complete16386entries beyond4MiB, raw fields/roots/metadata/mounts/ancestors,
  directory child hashes and derived hardlinks validated.1425Rust518/112/795+3ignored
  and16newtests/Clippy/fmt PASS normal. Initial map inference/lint retained; real
  deep-root fixture RED→GREEN separates absolute-root depth from content depth.
  DIRECTORY-SYNC-RECORD.md/assessment record scope. New action-plan/choices seal,
  explicit approval, helper/UI/session, durable journal/recoverable effects/queue/
  crashundo remain pending; installed dry-run unchanged, fullgoal/freezes active.

- C02 NODE: `directory_sync_plan` SOURCE ONLY derives all five modes from canonical
  fresh evidence; shared directories/coverage/type-content-link-metadata conflicts,
  explicit choices and sealed metadata/root/hardlink policy. Canonical16385rows /
  16464B plans, stale row receipts, resolved live binding and88B job/evidence/plan
  comparison keys; decoded history/keys grant no approval or durable barrier.
  Eleven new tests,1436Rust529/112/795+3ignored, all-target Clippy/fmt PASS normal;
  DIRECTORY-SYNC-PLAN.md and assessment detail acceptance. Physical hardlink/
  metadata planner, explicit UI approval/helper ownership, journal/staging/backup/
  publication/queue/crashundo remain pending; installed dry-run unchanged.
  Full goal, resource/startup freeze, preview pause and global gates remain active.

- C02 NODE: `directory_sync_physical` SOURCE ONLY now derives complete selected
  transactions/stage trees/old-preservation scopes and parent/mount references;
  staged hardlink cohorts across transactions, explicit omitted/external counts,
  metadata recipes/unknown-value constraints, dirty parents and old alias ctime
  impacts. All sources must stage before any destination effect.16384T/nodes/old
  entries, MAXwire3145856B, sealed separate staging payload≤1TiB charges independent
  hardlink multiplication and symlink targets.1450Rust543/112/795+3ignored/14new,
  all-target Clippy/fmt PASS normal; DIRECTORY-SYNC-PHYSICAL.md/assessment scope.
  No actual metadata acquisition, physical execution/backup/journal or UI/install.
  Full topology of untouched equal/retained names needs explicit review; selected
  cohorts never rewrite skips. Resolve/seal actual metadata then live ownership/
  approval/journal/ownrevision ledger/staging/publication/queue/crashundo next.
  Installed dry-run and fullgoal/freezes/preview pause/global open gates unchanged.

- C02 NODE: `directory_sync_metadata` SOURCE ONLY reads actual bounded raw values
  through pinned no-follow paths with full revision/mount/ancestor checks; Full/
  Basic recipes and chmod-equivalent Linux ACL adjustment, actual cohort equality
  (including convergent old hashes), separate directory undo/AfterOwnChildren.
  FLSMETv1 streams one≤128KiB value payload at a time,64attrs/131584B codec,
  max32770records/default64MiB/max8GiB artifact budget; no helper stdout increase.
  Final sink mutation/cancel rejects live Acquired after full fresh revalidation;
  decoded/forged history/StoredValues comparison never approval or durability.
  1465Rust558/112/795+3ignored/15new + all-target Clippy/fmt PASS normal. Four initial
  Clippy style lints fixed without suppression; full16384values/rawxattrs/ACL real/
  cohorts/undo/deep substitution/limits/history scope documented in
  DIRECTORY-SYNC-METADATA.md/assessment. Next owned metadata/journal file identity,
  readback/fsync ACK tied job/evidence/logical/physical/values, then explicit UI
  approval/helper pins/ACK, own-effect ledger, global staging/preservation/
  publication/queue/crashundo. Installed dry-run unchanged; FULLgoal/freezes open.

- C02 NODE: `directory_sync_store` SOURCE ONLY creates exclusive owned0600
  FLSSAVv1 bundles in private disjoint0700 directories, with complete canonical
  evidence/choices/stage budget and actual values. SavedIntent rebuilds review
  without original paths or pins; full16384record artifact>8MiB verified.
  Type-first O_PATH readback, file/name/mount/revision/ancestor checks, per-file
  PID-guarded flock, file and child-first ancestor namespace fsync,212B identity/
  intent/value comparison proof; reopen History, failed renewal Uncertain until
  explicit reopen, borrowed reverified artifact-only barrier.1482Rust575/112/795
  +3ignored/17new/Clippy/fmt PASS normal; initial range lint fixed, owned abrupt
  exit three phases and live inherited-forkFD checks. DIRECTORY-SYNC-STORE.md /
  assessment scope. No effects, journal, approval/helper/UI/release/install yet;
  next indexed value access + owned effect journal/approval/ownmutation ledger/
  global staging/recoverable no-clobber barriers/queue/crashundo. Complete goal,
  resource/startup freeze, preview pause and native/global open gates unchanged.

- C02 NODE: SOURCE ONLY indexed Store values now derive offset/span/original-byte
  digest only from full validated intent/metadata readback. Exact expected proof,
  positional one-record read≤131594B, header/type/policy/checksum and post-binding
  verification; no shared cursor or repeated full artifact replay. History and
  Uncertain values remain data, not ACK/Sourcefreshness/approval.32770slot bound,
  no payload retention/persisted offsets/wire-format or stdout-cap changes.
  Physical stage metadata visits each cohort leader once, directories postorder;
  IndependentNames retains every inode.1493Rust586/112/795+3ignored/11new,
  Clippy/fmt PASS normal; DIRECTORY-SYNC-INDEX.md/assessment, full16384records>8MiB,
  reverse/concurrent/offline/empty/corrupt/bounded-span/flags tests. Installed
  dry-run unchanged, no release/helper/UI/new resources. Effect journal/new
  approval/helperpinsACK/ownledger/staging/backups/publication/queue/crashundo
  next; complete goal/freezes/preview pause/native/global open gates unchanged.

- C02 NODE source foundation: sequenced effect declarations and Linux owned
  journal persistence; preparation-bound header, strict digest-chain replay,
  all-stage-before-effects, preserve-before-publish, directory metadata and undo
  ordering. Failed write/ACK latches Uncertain; reopen History and explicit
  renewal. Tickets/observations are declarations, not approval/verification.
  See DIRECTORY-SYNC-JOURNAL.md. Actual executor, own-mutation ledger, native
  approval/helper pins/ACK, global staging/backups/publication and recovery UI
  remain pending. Installed dry-run, complete goal and freezes unchanged.

- C02 NODE source-only own-root-rename reconciliation now rereads opt-in sync
  inventories under new raw prefixes, matches every content/metadata/mount and
  child revision, permits only root ctime change, commits recorded paths only
  after complete success. Trees/hardlinks/symlinks supported without weakening
  legacy EXDEV.1507Rust600/112/795+3ignored/5new, Clippy/fmt PASS; full8193-entry
  case and unrelated hardlink snapshot still strict. DIRECTORY-SYNC-REBASE.md.
  Full alias/parent namespace ledger and positive own-effect provenance remain
  pending, as do actual sync execution, approval/helper pins and recovery UI.

- C02 NODE source-only exact rename transition now reconciles known alias scopes
  as one recorded-state batch: exact old/new revision and values, full rewalks,
  duplicate physical-name detection, shared read allowance, final root checks
  then all-owner commit. Same-inode distinct names remain legal; overlapping
  names and arbitrary ctime exceptions reject. Cooperative worker Control kept.
  1515Rust608/112/795+3ignored/8new, Clippy/fmt PASS; full32772 entries/one-over,
  raw/symlink aliases, stale transition and later-scope failure/cancel coverage.
  DIRECTORY-SYNC-ALIASES.md. Parent namespace/nlink ledger, positive own-effect
  provenance, approval/helper pins, actual effects/recovery and journal scaling
  remain pending; installed dry-run and full goal/freezes/gates unchanged.

- C02 NODE source-only namespace rename/transfer reconciliation now checks one
  or two complete scopes, planned moved subtree and exact parent child sets,
  unchanged attributes and predicted nlink, then commits both recorded owners.
  Candidate shape budgets precede path clones; compact metadata templates keep
  canonical digest unchanged. Separate execution scope16,385/2MiB retained names
  supports full input union/reopen while input8193/1MiB remains unchanged.
  1524Rust617/112/795+3ignored/9new/Clippy/fmt PASS, raw/subtree/alias/attribute/
  atomic-failure/control and full-union/one-over/preflight-bound coverage.
  DIRECTORY-SYNC-NAMESPACE.md. Actual no-clobber own syscalls/provenance, durable
  journal barriers, staged metadata reproduction, visible approval/helper pins
  and recovery UI still pending. Full goal/freezes/native/global gates unchanged.

- C02 NODE source-only owned namespace rename now joins a durable intent with
  exactly one FD-relative RENAME_NOREPLACE, strict post-reconciliation, moved
  data/parent fsync and journal confirmation before a private receipt. Failures
  poison the owner and preserve objects; no implicit retry or rollback. Prepared
  snapshots retain original symlink metadata. 1533Rust626/112/795+3ignored/9new,
  Clippy/fmt PASS; collision/substitution/ten faults/three actual exit83 coverage.
  DIRECTORY-SYNC-OWN-RENAME.md. Trusted coordinator still must establish actual
  global staging, native approval/helper and absolute bindings. Full engine,
  metadata reproduction, recovery/undo UI and quadratic bulk scaling remain open;
  installed read-only sync, full goal/freezes/native/global gates unchanged.

- C02 NODE source-only actual staging barrier verifies ALL transaction workspaces
  against disk and sealed desired node observations, fsyncs pinned regular/dir
  data and containing parents, then fully reverifies under one shared read budget
  before durable WorkspaceSynced/StagingComplete ACKs. Private historical receipt;
  partial ACK poisons owner, reopen never resumes implicitly. Control pause/resume/
  cancel retained.1540Rust633/112/795+3ignored/7new, Clippy/fmt PASS; two actual
  workspaces/partial first ACK/late mutation/fresh unwanted values/read bounds.
  DIRECTORY-SYNC-STAGING-BARRIER.md. Object creation/metadata reproduction, native
  approval/helper/source/absolute-root freshness, full recovery/undo and UI Apply
  remain pending; quadratic scaling, native/global gates and freezes unchanged.

- C02 NODE source-only stage metadata reproduction now applies and exactly
  rereads raw values on pinned owned objects: group, temporary private mode,
  extra-attribute removal, raw/ACL values, final mode and exact times. Held FDs
  protect replacement names; errors retain partial metadata and original
  inventories, with explicit mutation-attempt flags. Control retained.1548Rust
  641/112/795+3ignored/8new/Clippy/fmt PASS; Basic retained attrs+ACL and Full
  hardlinked-tree fixtures precede actual staging barrier/own rename. Identical
  symlink attrs preserved; changes refused without safe FD mutation (NODE O_PATH
  xattrat EBADF); nonempty link profiles have no positive acceptance.
  DIRECTORY-SYNC-STAGE-METADATA.md. Real staged tree creation/durable create-meta
  intents, source/native approval/helper/absolute bindings, recovery/undo/UIApply
  and bulk scaling remain pending; installed dry-run/freezes/global gates intact.

- C02 NODE source-only actual staged-node creation now writes durable tag28
  intent before one exclusive create/mkdir/symlink/held-FD hardlink, ACKs identity
  before file bytes, verifies bounded copy/readback/literals, syncs data/parent
  and ACKs tag29 materialization. Pending guards prevent implicit replay/retry;
  errors retain partials/explicit syscall flags. O_EXCL and held-FD evidence are
  distinguished from directory/symlink named postimages.1556Rust649/112/795
  +3ignored/8new/Clippy/fmt PASS; cross-workspace regular+symlink cohorts, raw
  tree/multichunk/EOF/faults/Control/three real exit89, full sync fixtures now use
  creator→metadata→global barrier→own rename. DIRECTORY-SYNC-STAGE-CREATION.md.
  Workspace root creation/placement and shape/FD admission, durable metadata
  intents, source/native approval/helper/absolute bindings/recovery/undo/UIApply
  and bulk scaling remain pending; installed dry-run/freezes/global gates intact.

- C02 NODE source-only durable staged metadata now uses tag30 before178B and
  tag31 after146B around real FD writes, with per-syscall live journal guards,
  exact Physical metadata order/prior confirmations, complete all-scope pre/post
  verification, inode/parent sync and final guards before atomic alias-owner
  commits. Budget for both passes checked before intent; failure preserves all
  original snapshots and poisons owner after intent writing.1564Rust657/112/795
  +3ignored/8new/Clippy/fmt PASS; two-workspace file/symlink aliases, sixteen faults,
  post-ACK rejection, Control/tamper and three real exit97. Full owned pipeline
  now creator→durable metadata→global barrier→own rename.
  DIRECTORY-SYNC-METADATA-JOURNAL.md. Workspace-root factory/placement/shape-FD
  admission, native approval/helper/absolute/source lifetimes, SharedDir
  AfterOwnChildren, explicit recovery/undo/UIApply and quadratic bulk scaling
  remain pending; installed dry-run/fullgoal/freezes/native-global gates intact.

Checkpoint 2026-11-02: [whole-batch staging admission](DIRECTORY-SYNC-ADMISSION.md)
now checks every `new`+`old` workspace and the destination-name envelope before
new journal creation and owned effects. Retained FD slots are a policy forecast;
workspace placement/factory and actual host/transient FD admission remain open.

Checkpoint 2026-11-02: [owned workspace factory](DIRECTORY-SYNC-WORKSPACES.md)
now creates complete batches under pre-existing private disjoint same-mount
containers, with durable intent/identity/materialization and a point-in-time FD
headroom check. Container selection/creation, integrated coordinator/FD lifetimes
and native/source/helper/absolute approval bindings remain open.

Checkpoint 2026-11-02: [durable placement registry](DIRECTORY-SYNC-PLACEMENTS.md)
now preserves complete raw parent paths/chains and canonical workspace names
before mkdir, anchors the registry seal with tag34 and requires a matching live
renewed registry for registered journals. History inspection remains independent
of saved path availability; artifact attachment supplies no native approval.

Checkpoint 2026-11-02: [shared-container batch preparation](DIRECTORY-SYNC-BATCH-PREPARATION.md)
now groups exact raw parent paths, bounds requested placement metadata before
pinning, shares parent handles and uses the grouped FD forecast. Registry rows
and per-effect absolute/name checks remain intact; no process-resource or native
execution acceptance is inferred.

Checkpoint 2026-11-02: [private staging coordinator](DIRECTORY-SYNC-STAGING-COORDINATOR.md)
now connects complete owned creation, metadata and global barrier with cumulative
payload admission, bounded leader receipt lifetimes, shared parents and fresh
source checks. Destination execution, native approval and recovery are separate
unfinished phases; no installed Apply or bulk/resource acceptance follows.

- C02 NODE source-only batch namespace observer prerequisite now includes all
  private workspaces and original roots in one atomic reconciliation, preserving
  cross-workspace hardlink ctime coherence. Explicit aggregate quotas leave
  endpoint-only and original capture profiles unchanged. See
  DIRECTORY-SYNC-NAMESPACE-BATCH.md. Destination batch orchestration, effect
  budget/absolute/native bindings, shared-directory metadata, recovery/undo and
  UI Apply remain pending; FULL goal and all existing freezes remain active.

- User-authorized100k follow-up: sourceff7af56 wakes both the bounded listing
  producer and FLTK event loop, keeping capacity8/batch256/pump64/generation/
  limiter4.1612Rust/Clippy/fmt pass. NODE100k median206ms vsFlea0.4 408ms,
 32.6vs104.6MiB tree RAM,4runs/group; original603ms retained. M2 separate
  ARM trial:100k213vs195ms(near polling resolution, no Files win), Flea uses
  llvmpipe. M2 first-draw10/10<100ms(method-limited), corrected1080p10kGroups
  memoryFAIL58.46MB peak vs50MB; native input/default file chooser/global
  acceptance remain open. LISTING-EVENT-WAKE.md+assessment document all limits.

- 2026-11-04 Intel comparison, same optimizedff7af56 native binaries,16validruns:
  Corei5-5257U/Iris6100 hardwareVulkan,1256x750client@2x.10kFiles159vsFlea279ms,
 39.0vs56.4MiB treeRAM;100kFiles1420vsFlea271ms,51.8vs58.8MiB. Flea DRMresident
 79.8MiB observed separately, no blind addition toPSS. INTEL-COMPARISON.md.
  Userclarifies50MBobjective is NOT project veto; memorywin remainsvaluable.
  Different host/display configs preclude cross-host ranking;100k Intel weakness
  needs profiling, not a fabricated universal victory or unmeasured cause.

- 2026-11-04 input checkpoint: overlay repaint fix and reusable bounded wheel
  motion live in lib-fltk-omarchy fb581c7 (unpublished), exported exactly under
  vendor with provenance. Files uses the shared explicit flush and scroll
  controller across main/dual/column views. Papelera is pinned in the sidebar
  and opens a bounded read-only home-trash catalog. 1620 workspace tests and
  Clippy pass; mapped X11 functional scene passes release/menu pixels, exact
  wheel travel/return, catalog reload and data preservation. Final native/app
  installation validation is recorded separately. Physical touchpad inertia,
  persistent restore/empty UI, other volumes, Intel100k profiling and the full
  original goal remain open. TOOLKIT-INPUT.md describes the scope and limits.
- Follow-up: this input checkpoint is installed on NODE, 5 backups/hashes
  verified; source X11/Wayland and installed Wayland scenes pass. Receipt
  installed-checkpoint-1793772472114089109.json and TOOLKIT-INPUT.md. No M2/Intel
  replacement, no toolkit publication, no physical touchpad acceptance yet.
