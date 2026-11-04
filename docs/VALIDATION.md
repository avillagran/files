# Validación del checkpoint files0.0.2

2026-10-26 · Linux aarch64/Apple M2 Pro, Hyprland/Wayland nativo.
Checkpoint funcional; **no PASS global**: arranque estricto y varios escenarios
ampliados siguen pendientes. Fuente UI:6fc0fe7, readiness1785d41, NaturalKeyRef7a62cdc,
du22cef80/1cb4880. Shadersab3e3ba son fixtures independientes, no efectos en app.

## Memoria con árbol y escena realmente cargada

10k/100k bajo `~/.cache/files-test`, ventana960×540 lógica/output2=1920×1080 físico,
FLTK scale1, efectos OFF. Intervalo solicitado25ms, cinco segundos tras geometría
**y** señal de loaded/view/count/stats o layout; estable mediana del segundo siguiente.
TreeSampler identifica PID+starttime y descendientes/reparentados ya observados;
PSS+SwapPss total, nunca RSS. `complete` verifica lecturas de identidades supervivientes
legibles: **no garantiza lifetimes completos ni snapshot simultáneo**. Salidas
confirmadas durante lectura generan eventos con identidad/fase/motivo y memoria
no disponible; PermissionError/parse corrupto/vivo no legible invalidan.

| Escenario | Pico muestreado MiB | Pico MB decimal | Estable PSS+Swap KiB | Estado |
|---|---:|---:|---:|---|
| Base10k checkpoint final | 40.807 | 42.789 | 29320 | FAIL umbral de análisis40MiB; sin PASS global |
| Base10k tras du visible (anterior) | 33.980 | 35.631 | 30309 | PASS gate base; objetivo35MiB cumplido en esta corrida |
| Espacial10k | 34.267 | 35.931 | 29305 | PASS muestreado con advertencia: salida durante lectura con memoria no disponible |
| Grupos100k, metadatos completos | 53.015 | 55.590 | 54241 | PASS techo; objetivo50MB NO cumplido |
| Espacial100k, layout cargado | 47.152 | 49.443 | 42076 | PASS gate muestreado/objetivo50MB |
| du100k, principal+helper | 33.481 | 35.108 | 33543 | PASS escenario propio; geometría no valida baseline1080p |

Datos: `evidence/0.0.2/memory-*-ready-*.json`, `memory-baseline-after-du.json` y
`evidence/du/memory-100000.json`. Casos100k previos al botón du, cuya regresión10k
sí usa el binario integrado. El primer baseline listo dio39,730MiB de pico y28,388MiB
estable; se conserva junto al último, sin sustituir el peor por la mediana.

Base tras du: máximo13FD/5hilos/1proceso observado; estable11FD/5hilos, CPU idle
mediana/max0%, Swap0 en esa corrida. du100k: máximo20FD/7hilos/2procesos observados,
helper identificado por PID/start/exe y recogido, estable un proceso. Máximo SwapPss
independiente3168KiB; **no equivale a Swap0 ni se suma a otro pico temporal de PSS**.
CPU al instante de máximo de memoria77,93% de un núcleo; no es máximo global de carga.
Memoria GPU excluida de /proc; no hay contexto de efectos en estas corridas.

Se conservan `memory-before-save-ack.json` FAIL53,36MiB y `memory-spatial-10k.json`
UNVALIDATED. `geometry()` antes forzaba resize cada observación, causando trabajo
transitorio; ahora sólo pide cambio si falta. El baseline tras esa corrección dio
37,657MiB. Las primeras corridas no atestiguaban loaded/count: se conservan con
esa limitación. El objetivo35MiB difiere del stop40MiB y hard60MiB base; en escenarios
modernos50MB es ideal,150MB techo absoluto, o presupuesto específico explícito.

## Arranque

- Sin sampler: **81.419ms p50, 111.423ms p95/max,
  9/10<100ms, FAIL** (`evidence/0.0.2/startup-timing-only.json`).
- Con sampler:87,508ms p50,198,730ms máximo,7/10<100ms, FAIL
  (`startup-tree.json`). Incluye intervalos incompletos de salida que no se ocultan;
  fixes de ESRCH/fin esperado posteriores se probaron con fixtures, todavía no
  tienen nueva serie del binario. El coste /proc puede retrasar recepción del marcador.
- Se solicita `POSIX_FADV_DONTNEED` del ejecutable, sin vaciar cachés globales de
  fuentes/bibliotecas/SO. No es latencia de presentación física del compositor.
- Selectors no bloqueantes/deadline monotónico incluso stderr abierto tras el marcador;
  cualquier ejecución>=100ms falla, no sólo la mediana. Falta estabilizar el máximo.

## Aceptación funcional y límites

- Cargo workspace integrado:95 tests (71core,16UI,8helper), incluidos23copier y20rename.
  Los motores exportados son preparación de las próximas interfaces, no features UI
  completas ni throughput/PSS validados. Logs en evidence/0.0.2/cargo-test.log.
- du UI16tests/Clippy/release; helpertexto8fixtures ydu6fixtures. TextoCLI aún sinSpace,
  selección ni resaltado;256KiB/2000LF,UTF8/UTF16BOM; ver TEXT-PREVIEW yDU-HELPER.
- Python32 tests del sampler/gates/timeout/readiness/exits. No equivalen a benchmark.
- `spatial-check.json`: handlers de puntero sobre hit targets pintados; posición72/48
  y zoom1,15 recuperados en nueva instancia. Cierre normal tras ACK; selección/rango,
  cola llena/fallo/retry/cierre, alias, drag multícono atómico, TOML incompatible preservado.
- `ui-check.json`: temas Hackerman/Tokyo, TOML parcial/symlink/replacement, bordes y
  conservación de effects/future en config; Grupos/Lista/riel/Espacial.
- Nerd FiraCode confirmado; revisión sin tofu evidente1x/2x/resize. FLTK captura
  coordenadas lógicas1000×650 aun cuando el compositor observa4000×2600 físicos:
  prueba de layout/escala, **no nitidez nativa de todos los píxeles2x**.
- du visible recursivo aparente, sin seguir symlinks, cancel/reap al cambiar selección,
  navegar, ocultar/cerrar. du10k UNVALIDATED: helper demasiado rápido para ser observado.
  Treemap, adaptadores y cancelación de syscall del SO todavía pendientes.
- CUA no enumera la ventana; **eventos SO siguen sin comprobar**, no se improvisan
  clicks alternativos. Navegación offline usa como máximo4workers y primer lote antes
  de statvfs; syscalls bloqueadas aún requieren aislamiento/recuperación posterior.
- Falta navegación repetida/multitab/multipanel, latencia de eventos SO, CPU carga
  global, futuros previews/operaciones/red/shaders y plataformas de la matriz.

Binario `files` tras du UI (SHA256): `7bd0fe159f2c724beb9fd01620d1aaf9866d908f223725bd52cfe0ee1ec5ca7b`.
La tabla100k se refiere al candidato anterior; conservar hashes/escenarios, no
reetiquetar sus datos como medidas del nuevo binario.

---

# Validación de files 0.0.1

2026-10-26 · Linux aarch64 (Apple M2/Asahi), Hyprland 0.56.2, Wayland nativo.

## Memoria

Carpeta sintética de **10.000 archivos**, sin widgets por entrada. Se comprueba
la geometría y se espera cinco segundos antes de leer `/proc/<pid>/smaps_rollup`.
Ventana: **960×540 lógicos × output scale 2 = 1920×1080 físicos**. La escala de
interfaz de FLTK se fija en 1 para hacer reproducible el contenido.

| Preferencia | PSS real | SwapPss |
|---|---:|---:|
| Seguir Omarchy (rounding 0) | 29.03 MiB | 0.00 MiB |
| Redondeados interiores | 34.13 MiB | 0.00 MiB |

Ambas mediciones quedan bajo el objetivo de 35 MiB. El gate de análisis falla
sobre 40 MiB y el límite solicitado permanece en 60 MiB. Se usa PSS, nunca RSS.
Mediciones previas oscilaron entre 26,40 y 33,39 MiB; una tuvo 4 MiB de SwapPss.
Las lecturas dependen de la residencia/compartición de páginas y del tamaño físico.
La prueba principal no captura imágenes ni activa recorte nativo de ventana.

Datos: [memory.json](evidence/memory.json), [memory-rounded.json](evidence/memory-rounded.json).
Reproducir: `make memcheck`. Los temporales se eliminan al salir y viven bajo
`~/.cache/files-test`; no se usa `/tmp` para compilar o generar fixtures.

## Arranque

Diez procesos nuevos, solicitando `POSIX_FADV_DONTNEED` sólo para las páginas del
binario antes de cada lanzamiento. **No se vacían globalmente las cachés de fuentes,
bibliotecas ni del SO**. El reloj externo empieza antes de crear el proceso y termina
cuando llega la señal emitida al finalizar el primer callback de dibujo completo.
No mide el instante físico de presentación del compositor ni la finalización del listado.

- Serie final: **84.59 ms de mediana**, **111.85 ms máximo**;
  **8/10** ejecuciones bajo 100 ms.
- Una serie anterior con la misma arquitectura sin decoración de cliente obtuvo
  89,83 ms de mediana y 95,02 ms máximo (10/10 bajo 100 ms).
- **El objetivo estricto de todos los arranques bajo 100 ms sigue pendiente**:
  hay variación en esta sesión compartida. No se presenta la mediana como un límite.

Datos completos: [startup.json](evidence/startup.json).
Reproducir: `python3 scripts/startup-check.py`.

## Pruebas y límites

- `cargo test --workspace`: seis pruebas aprobadas, incluido streaming de 10k
  entradas sin stat inicial, formatos, MIME y reemplazos atómicos del watcher.
- `cargo clippy --workspace --all-targets -- -D warnings`: aprobado.
- `cargo fmt --all --check`: aprobado.
- `cargo build --release`: aprobado, sin warnings.
- Protocolo del helper: respuesta JSONL `unsupported` correlacionada por ID,
  límite de petición de 64 KiB; `du` continúa sin implementación.
- `python3 scripts/ui-check.py`: escenarios Grupos, Lista, riel expandido,
  bordes persistidos; tema Hackerman → TOML inválido conservado → Tokyo Night por
  cambio de symlink → reemplazo atómico en el nuevo destino. Todo aprobado.
- Capturas del framebuffer de la **ventana Wayland real**: no mockups ni HTML.
  La captura se habilita sólo con `--ui-smoke` + `FILES_TEST_CAPTURE`; no forma parte
  de la medición de memoria. Se usa para excluir ventanas ajenas superpuestas.
- CUA no descubre estas ventanas; los escenarios aplican acciones internas al modelo.
  **La entrega de eventos físicos de teclado/ratón queda sin verificar por controlador**.
- **Contorno exterior gestionado por Hyprland**. La preferencia de bordes modifica
  la UI interior; el recorte propio se dejó fuera por el límite de anti-aliasing
  de FLTK y validación incompleta de su coste. Detalle en Hallazgos del diseño.
- Windows/macOS y X11 no se han compilado ni ejecutado en esta sesión.

Binario validado (SHA-256):
`78c296c932c65fea11c831b6c49a917dc8615eaa03115c625ab284010179ebbd`.

## Diagnóstico posterior y suspensión de medidas

`memory-checkpoint-final.json` conserva el FAIL de pico41786KiB=40,8066MiB,
PSS39434KiB+SwapPss2352KiB al mismo instante; estable29320KiB, con Swap incluido.
Primera pintura interna138,447ms. No se sustituye este pico por la mediana ni por
el PASS anterior. Su binario incluye helpertexto y motores exportados, anterior a
la integración UI del renombrador. Esta última todavía no tiene gate de memoria.

`memory-mappings-diagnostic.json`, `memory-mappings-geometry.json` y
`memory-mappings-static-mmap.json` son **diagnósticos acceptance=false**: lecturas
no simultáneas, instrumentación distinta, no gates ni optimización demostrada.
En el primero, heap32144KiB permanece constante, mientras memfd:libdecor desciende
25384→7112KiB; delta18272KiB próximo a la caída de árbol18173KiB, sin probar causalidad.
La corrida corregida registra geometría inicial854×1065 lógica y final960×540;
Size de un mmap no equivale a PSS. Fijar el umbral mmap de malloc en otra corrida
no demostró mejora concluyente, por lo que no se adoptó como arreglo.

Revisión estática de FLTK1.5.23: `create_shm_buffer` reserva por defecto10000000B,
o2×framebytes si éste supera ese tamaño; pueden coexistir pools. `buffer_release`
libera draw_buffer, difiere wl_buffer en uso; `do_buffer_release` sólo desmonta
pool vacío distinto de current_pool. Esto permite retención del último pool grande,
pero no atribuye por sí solo el pico observado. No se parcheó registro Cargo ni
backend global. La optimización permanece pendiente, manteniendo el FAIL.

El usuario suspendió nuevas mediciones/capturas0.0.2 y pidió continuar0.0.3.
El checkpoint es funcional con límites conocidos, **no una aprobación global de
rendimiento**. Pruebas Rust posteriores de integración se registran aparte.

## 0.0.3: renombrador y creación visibles

`c462fa1` integra formulario propio F2, patrones/secuencia/literal/caso, preview
antes/después, Apply/undo/redo y crear archivo/carpeta exclusivo. Aviso de éxito
compacto con Undo; errores/rescate conservan informe grande copiable. Generaciones
únicas descartan previews tardíos; cierre/exit-after cancela y drena ACK aun con
preferencias descartadas.106tests Cargo (73core/25UI/8helper), Clippy y release limpios.

- `evidence/0.0.3/rename-ui.json`: PASS funcional; bytes/nombres comprobados en
  fixture lote2+untouched, preview no modifica, apply→undo→redo, creación y ausencia
  de staging; salida0 con ACK exacto de Apply-close. Framebuffer/handlers internos,
  **no eventos SO**. Capturas `*-compact.png` muestran resultado ya recargado.
- `initial-modal/` conserva la primera UI, luego corregida por bloquear tras éxito.
  `rename-preview.png` publicado conserva su ruta histórica. Captura sin sampler
  acredita salida de raíz; su cleanup no demuestra árbol de descendientes.
- `rename-resources.json`: **UNVALIDATED**, pico observado42,402816MB; observación
  root-dead final invalidó el instrumento. `goal_met=true` en este archivo histórico
  sólo expresa estar bajo50MB observado, no validez: debe leerse desconocido para
  aceptación. Se conserva sin modificar.
- `rename-resources-normal-exit.json`: PASS gate de escenario, pico árbol34239KiB
  =35,060736MB (33,4365MiB). Ciclo pico34159KiB, estable30531KiB; cierre pendiente
  Apply pico34239KiB, estable30415KiB. Swap máximo independiente64KiB, incluido en
  cada suma temporal. Máximo observado14FD/6hilos/1proceso, cleanup PID/starttime
  completo tras salida0. Ventana estable [since,until) postcreación: CPU mediana0%,
  máximo27,15%; **no acredita idle asentado**, puede incluir recarga del listado.
  Cierre usa ventana de preview previa a Apply; CPU mediana/máximo0% allí.
- Lectura terminal vacía/sin errores se difiere explícitamente hasta confirmar
  exit0+cleanup, sin convertir memoria de un proceso ilegible vivo en cero. Eventos
  se guardan en terminal_exit_observations. Picos/errores conservan toda la corrida;
  stable_until limita sólo medianas/CPU estable. No garantiza lifetimes completos.
- `memory-10k.json`: nueva base0.0.3 loaded10000/1080p, PASS muestreado con advertencia
  de hijo salido durante lectura. Pico36715KiB=35,8545MiB=37,59616MB; estable28335KiB
  =27,6709MiB, PSS26607+Swap1728. Objetivo35MiB **no cumplido**; umbral40MiB no
  superado en esta corrida. CPU estable0%; máximo13FD/5hilos/1proceso concurrente
  observado, dos identidades durante vida; hueco del hijo documentado. No elimina
  FAIL anterior0.0.2 ni prueba ausencia de picos entre muestras.
- `startup-timing-only.json`: nueva serie0.0.3 **FAIL**,8/10<100ms, externo p50
  87,449927ms/p95 y máximo150,310623ms; interno máximo137,918ms. No se atribuye
  causalmente la variación a rename ni se oculta con mediana. Sigue pendiente.

Build/capturas/recursos/base/arranque fueron serializados. SHA del binario después
de cleanup: `5c54617793ef7dc20a32bb9632b59fe82a0b6ca4707f2aa616f1289ca70cadec`.
Escena de lote2 no cubre100k/8MiB de UI+Arc+planner+PreparedRename simultáneos;
latencia y cancelación de ese lote grande quedan pendientes. Regex/fechas,
undo de creación, journal crash durable y migración de layouts de carpetas siguen
pendientes. El checkpoint funcional **no es PASS global**.

## 0.0.4 parcial: Space texto y contrato FLTK wait

Implementación992fe04: Space/Vista previa con helper único cancel/drain, texto sólo
visible, selección por width real+mapa byte→display para TAB/control, CRLF/UTF8
originales, código plano; Markdown/HTML siguen siendo fuente.126tests Rust, Clippy
workspace/release aprobados. Error/truncado/cambioA→B→C/cierre normal drenado tienen
framebuffer/handlers internos, sin teclas/clics SO. Capturas en evidence/0.0.4.
**Hito no cerrado ni etiquetado**: clipboard externo sigue FAIL por fixture sin foco
Wayland, serial0; user conserva restricción CUA/jev. No usar wl-copy para simularlo.

- wait-contract.json: dos pipes legibles→raw2/callbacks2/errno0. Rustwrapper1.5.23
  rechazaría≥2; reemplazo local del contrato C acepta≥0, retry sóloEINTR con pausa,
  otros errores conservados y drain de operaciones/helpers antes de salida1.
- Capturas nuevas registran raw4/3/2 exitosos, por tanto confirman ocurrencia del caso
  válido multFD en la escena corregida. Original carecía de raw; no se atribuye
  causalidad exacta retrospectiva. FAIL originales se conservan.
- text-ui.json: functional_gate FAIL/clipboard FAIL, contenido seleccionado exacto
  Unicode+TAB+CRLF y app::copy invocado; wl-paste no obtiene oferta. Ventana activa
  de compositor distinta de fixture, traza muestra set_selection serial0 sin enter.
  Foco nativo sigue pendiente, sin improvisar control alternativo. Resto de fixtures
  y salida0 trasPREVIEW_DRAINED comprobados; no equivalen a clipboard aprobado.
- text-resources.json: **resource UNVALIDATED y functional FAIL**. Pico observado
 34858KiB=35,694592MB (34,0410MiB), estable34858KiB; Swap observado0. Máximo17FD,
 6hilos/2procesos observados; CPU de ventana truncada mediana/máximo0%. Árbol
 superviviente legible y cleanup observado completo; helper_identities_verified
 vacío, helpers demasiado breves para identificarlos/medirlos en esta corrida.
 No es presupuesto completo de preview ni helper=0. Salida de hijo durante lectura
 conserva memoria no disponible. Cadencia solicitada5ms, intervalo máximo91,20ms
 por instrumentación/proceso/acciones; campo heredado25ms corregido con nota de
 metadata, sin cambiar muestras. No se usa este resultado como PASS.
- Secuencia build/captura/recurso separada. Hashes principal/helper en JSON después
 de cleanup; benchmark no ejecuta wl-paste ni captura. No nuevos benchmarks0.0.2.

Continúa copia UI mientras foco/clipboard queda pendiente, según instrucción del
usuario. Falta ventana horizontal completa de líneas>4096B, syntax highlighting,
render Markdown/HTML y resto previews; no se infla el alcance de Space texto actual.

# Copia visible y recarga (0.0.4 parcial, 2026-10-26)

171 Rust tests (73core/8helper/90UI),36 Python, Clippy all-targets -Dwarnings y
release PASS. Worker reutilizable/generaciones/capacidad4FS, snapshot de selección
acotado8MiB, recargas consecutivas, bloqueo visible de selección mientras recarga,
rechazo sin pérdida y reintento al reducir el lote. Sin serializar la suite para
ocultar la contención original; fallos previos conservados en copy-initial/.

Copia F5/inspector, destino, ocho pendientes/uno activo, pausa/reanudar/cancelar,
conflicto sin overwrite, diagnóstico/rescate completo acotado y cierre con ACK.
Captura corregida copy-ui-1793025091272455448.json: handlers internos/framebuffer
real, no input SO. Origen/bytes/SHA256/rwx/mtime/symlink y ausencia de staging,
cancelación activa tras3120archivos y cierre pausado antes de trabajar. La primera
captura tenía toast sobre controles y conflicto de copia sobre sí misma; se conserva.

Recurso separado copy-resources.json: pico34671KiB (33.8584MiB/35.503104MB),
estable32542KiB (33.323008MB), Swap observado0, máximo19FD/6hilos/1proceso legible
simultáneo. PASS muestreado/objetivo50MB para16MiB+directorio10karchivos+symlink,
ventana1000×650lógica, no baseline10kfilas/1080p. Una identidad hija conocida salió
mientras se leía smaps: memoria no disponible; complete sólo verifica supervivientes,
no lifetimes.125muestras, máximo intervalo104.188ms frente a10ms solicitado.
CPU ventana postcopia mediana0/máximo118.82%; no idle asentado. Cancelación tras
3280archivos, raíz no publicada limpiada, cierre0 tras ACK5 y árbol observado recogido.

**Baseline nuevo FAIL**, no reemplazado por copiaPASS: copy-baseline-10k.json,
loaded10000/metadatos completos,1080p verificado; pico55414KiB (56.743936MB),
estable40484KiB (41.455616MB). Se supera análisis40MiB, por debajo del hard60MiB.
Al pico sólo raíz55382PSS+32Swap,5hilos/12FD, ~407ms desde PIDstart; no atribuido a
helper. Diagnóstico posterior acceptance=false copy-baseline-mappings.json tiene
otra distribución: máximo42001KiB a90ms (anon16016/memfdlibdecor14224), estable
30705KiB (heap12272/memfd7112/anon1952); geometría inicial854×1065→960×540. No
reproduce el gate ni prueba causa/optimización/PASS. Lecturas no simultáneas y
coste de instrumentación distinto; retención/transición FLTK sigue hipótesis de
fuente y candidato de investigación, no un diagnóstico causal concluido.

Arranque sin sampler nuevo: p50externo90.644308ms, p95/max151.978267ms,7/10<100,
**FAIL**. Interno máximo120.429ms. No es presentación física del compositor;
cachés globales no se evictan. Evidencia copy-startup-timing-only.json.

Throughput separado64MiB/1000archivos/tres repeticiones y smoke16MiB pasan integridad;
ver COPY-BENCH. No se generaliza superioridad ni se convierte reflink en throughput
físico. files SHA d8f2934638a56d069428e55a2209948db1df1a562c1eef8a012dd010e4059c40.
No etiqueta0.0.4 ni PASSglobal: Space clipboardFAIL/recursosUNVALIDATED siguen
preservados, y faltan movimiento/journal/resume, plataformas y resto ROADMAP.

## Imágenes acotadas, checkpoint0335e21

188Rusttests,36Python, Clippy/release y regresiones release helpertexto8/du6PASS.
PNG/JPEG/WebP,alpha/EXIF8transformaciones, zoom/pan, mismo ciclo single-flight que
texto. Framebufferimage-ui-1793027538323979457.jsonPASS; noinputSO ni clipboard.

Recursoimage-resources-1793027605038038146.json **UNVALIDATED**: tres helpers con
PSS observado y cinco de ocho sinmuestra,122muestras/maxinterval92.070ms. Lowerbound
52440KiB=53.69856MB **YA supera ideal50MB** (`observed_goal_exceeded=true`); la
cobertura incompleta no vuelve incierto ese incumplimiento. Al pico42152PSS+
10288SwapKiB agregados; establetotal27800KiB=28.4672MB, CPU0mediana/maxventana1s.
Máximos independientesPSS43478/Swap24096KiB no se suman; fuenteSHA/identidades enJSON.
Cleanup de árbol observado completo, pero no presupuesto total ni lifetimes garantizados.

No se repite para buscarPASS ni se afirma límite32MiBdecoder como PSS: entrada
JPEGimage puede duplicarse y coexistir condecoded/source/output/wire/bitmap/viewport
más copiaFLTK y buffersWayland. P02 sigue parcial4Mipíxeles, fotos12/24MP yLRUmulti
pendientes. Se preparaIDCTrealconjpeg-decoderMIT/Apache, sinadoptarIJG. Esa preparación
sinwiring no cuenta como soporte de fotos; verJPEG-SCALED-NOTES yIMAGE-PREVIEW.

## Markdown renderizado parcial, después de16e6a7c

230Rust(73core/33helper/124UI),Clippyalltargets-Dwarnings,releasePASS. Escena
markdown-ui-1793032850488569491.json funcionalPASS: estilos/tablas/listas/selección
Unicode,resize,freshrevision/layout,linksrelativos,sylinkrechazado,truncado visible
ycierreACK. NoinputSO/clipboard. Capturas originales ysidecars preservan falso
positivo de primer assert deerror; too-many fueREADYtruncated, no rechazo.

Recursosmarkdown-resources-1793032456170456020.json UNVALIDATED:43.177984MBpico/
37.692416MBestable observado,PSS+Swap,Swap0;2/8helpers muestreados,368samples/max74.676ms,
19FD7threads2proc,CPU0/162.718%postReady,noidle. No rerun paraPASS. P04parcial:
incrustadas yLRU pendientes. Reflow4mscooperativo tuvo18.306msmáximo recurso/
60.543ms captura inicial; doc201033bytes~635ms hastaREADY medido. VerMARKDOWN-PREVIEW.
Base10k56.743936MB yarranque151.978msFAIL,SpaceclipboardFAIL mantienen vigencia.

JPEG anterior16e6a7c: funcional12/24MPbaseline+EXIFPASS,recursos42.36288MBmuestreado
con2/2helpersy1discoverygap,no lifetimescompletos. Progresivosgrandes rechazados
por32MiBcoef aúnP02parcial; no convertir límite decoder enPSS. VerIMAGE-PREVIEW.

## Markdown con incrustadas/contextual, recursos suspendidos por usuario

242Rust(73/33/136),Clippyalltargets-Dwarnings yreleasePASS.
markdown-embedded-ui-1793034469432638103.json funcionalPASS: bitmaps framebuffer
real,5apariciones/2recursos,capacityledger3164572B; grandebitmap320x320102400píxeles
exactos. Exceso4MiB,remote/escape/symlink nodisponibles condiagnóstico; contextual
root/subdir/nested yretornos/escape/cierre0ACK. Método handlers, noinputSO/clipboard.

RecursosincrustadasNOT_MEASURED: nueva instrucción sólointegración/captura,
no nuevasmedidas/startup hasta liberación. Intento previo bloqueado ANTESlaunch
porbuildcompetidor:markdown-resource-blocked-1793034332264805450NOT_RUN/UNVALIDATED.
No sesuspendió ni modificó buildajeno. No extrapolar43.178MBantiguos al binarioactual,
ni confundirledgerauxiliar8MiBretained+scratch conPSSdelárbol. VerMARKDOWN-PREVIEW.
No0.0.4tag/globalPASS; P09LRUentreDocs,HTML/video,base/arranque yclipboardpendientes.


## HTML/CSS visible experimental, recursos suspendidos

252Rust(73core/33helper/146UI), Clippyworkspaceall-targets-Dwarnings y release
limpios. html-ui-1793037143255536981.json funcionalPASS: motor litehtml nativo
efímero realmente renderiza estilos/CSS, tabla y listas en Painter; selección
visibleUTF8, mediaquery1000→800 con revisión fresca, scrollfinal, bloqueoCSS/
imágenes/script, rechazo32KiB, cambios rápidos y salida0 tras previewACK.
Método handlers internos+framebuffer real; sin inputSO ni clipboard. Captura
1000x650/800x500 lógica no demuestra nitidez de cada píxel físico2x.

Métricas del adaptador aproximadas y recorteCJK/celdas observado; enlaces y
recursosHTML bloqueados, sin preservaciónselección tras reflow ni allocator
previo deGumbo acotado. Nativehelper opcional preparado junto a release,
packaging portable pendiente. P05 parcial; verHTML-ENGINE-PROBE/DEPENDENCIES.

Recursos/startup NOT_MEASURED por fase explícita usuario. No se leyó PSS ni
se lanzó benchmark. Hashes app/adaptador enJSON; no extrapolar presupuesto
de otro binario. SiguenFAIL baseline56.743936MB,arranque151.978ms y clipboard
sinfoco, yUNVALIDATED imágenes/Markdown históricos. No0.0.4tag/PASSglobal.


## Vídeo: snapshots y seek visibles, consumo pendiente

260Rust(73core/33helper/154UI), Clippyworkspaceall-targets-Dwarnings y release
limpios. video-ui-1793037769557008693.json funcionalPASS: fixtureMatroskaFFV1
640×360/8s, primerframerojo,seek5sazul, clictimeline25%rojo, EOFnormalizado7.8s
azul,cambio rápido al último seek,errorbad/symlink y cierre0trasACK. BytesRGBdel
helper y revisiones frescas comprobados; imágenes framebuffer real. Posición
es requestnormalizada, no framePTS. Método handlers,noinputSO/clipboard.

P07parcial: no reproducción/audio/clip; rotación/SAR tienen tests puros pero
no evidencia visual. Faltan escenarios grandes y presupuesto árbol. Recursos
ystartup NOT_MEASURED porfaseexplícita. RLIMIT_AS768MiB virtual y max_alloc32MiB
NOequivalen aPSS ni autorización de exceder150MB. No aprobaciónglobal; FAILbase/
arranque/clipboard conservados. Toolpaths/versiones/hashes yappSHA enJSON.


## PDF multipágina visible (parcial)

266Rust(73core/33helper/160UI),Clippyworkspaceall-targets-Dwarnings yrelease
limpios. pdf-ui-1793038247910320945.json funcionalPASS: PDF2páginaspropio,
RGBrojo/página1→azul/página2, zoom2×/panY60, últimoscambioscoalescidos,
malformado/symlink y cierre0trasACK. HerramientasPDFinfo/PPMreales, hashes/version
yappSHA enJSON. Handlers+framebufferreal,noinputSO ni seleccióntextoPDF.

Se conserva FAILpdf-ui-failure-1793038113834599772: handler assert-pan ausente
enverificador; loimplementa la revisión posterior, no se borró ni cambió elFAIL.
PPMboundedwire se valida conEOFexacto antes deexponer bitmap, sin segundaRGB;
no presupuestopreviodimensionheader en supervisorJSON. AS96MiBnoPSS. PDFcomplejos/
password, extracción/selección, contenciónparser yotrosSO pendientes.

Recursos/startupNOT_MEASUREDporfaseexplicita; no se midió árbol,nuevoCPU/GPU
niarranque. Cierre canceló job antes delnuevo spawnregistrado; noclaim killactivo.
SiguenFAILbase/startup/clipboard yUNVALIDATED históricos, no0.0.4tag/globalPASS.


## Reproducción de vídeo: funcional, consumo sin atribución

272Rust(73/33/166),Clippyall-targets-Dwarnings yreleasePASS. video-play-ui-
1793038888536010466funcionalPASS:framesrealesrojo→azul,pausaACK, resume/EOF
ycierrecondecoderobservadovivo(PID/starttime)confirmadoausentedespués.
Fallo1793038705143512044conservado: verificadoraceptósequenceviejoenresume;
resetsequence yfreshspawn/frameimpideneseerror. Slotlatestacotado,nohistorial.

Clip24framebuffersreales a~4fpsensambladosa4fpsdespuésdesalir; no grabaciónOS
continuani prueba12fpsUI. SininputSO/audio/shapingmediaexhaustivo ni benchmarks.
Footerprincipal76.6MiB bajo24readbacksin-process: **UNVALIDATEDpara atribuirlo
alreproductor**; noSwap/hijos/peakestable. capture_window añadebuffers/readbacks
yconfundecomponente. Contextsidecar conservacifrayexperimento discriminante
pendiente,NOejecutado: playbacksinclipyluegocapturassolas cuandousuarioabra fase.
Noafirmarcumplimiento niincumplimiento playback-only. Resource/startupNOT_MEASURED,
FAILhistóricosbase/arranque/clipboardintactos,sin0.0.4tag/globalPASS.


## Audio: metadatos visibles

276 tests Rust (73/33/170), Clippy workspace all-targets -D warnings y release
limpios. audio-ui-1793039367932469197.json funcional PASS: FLAC/WAV sintéticos,
título/autor/álbum/duración/códec/tasa/canales reales, selección Unicode,
etiquetas recortadas, cambios rápidos, inválido/symlink y cierre normal con ACK.
Handlers y framebuffer real; no reproducción ni clipboard/eventos SO probados.
Campos externos saneados, no código fuente presentado como metadatos.

Recursos/startup NOT_MEASURED por fase, sin nuevas corridas de presupuesto.
No extrapolar PSS de otro binario o de capturas. P08 sigue parcial; pendientes
waveform/reproducción/carátulas/archivos/Office y presupuesto del árbol.


## Space para carpetas

279 tests Rust (73/35/171), Clippy workspace all-targets -D warnings y release
limpios. folder-ui-1793040015107090730.json funcional PASS: 8 B/5 entradas,
symlinks sin recorrer (ciclo y archivo externo propio de 1MiB), carpeta vacía,
rechazo de archivo que sustituye una carpeta listada, cambios rápidos y cierre
normal tras ACK. Protocolo preview_directory exige directorio; du conserva API.
Sin I/O filesystem en UI; métodos internos/framebuffer real, no input SO.

Recursos/startup NOT_MEASURED por fase. No extrapolar la medición du anterior
a este binario. Montajes offline, treemap y snapshot concurrente pendientes.


## ZIP: catálogo real de sólo lectura

288 tests Rust (79/37/172), Clippy workspace all-targets -D warnings y release
limpios. zip-ui-1793040518449068984.json funcional PASS: Unicode, symlink,
traversal/drive y duplicados; raw no UTF-8 escapado; vacío; 2500 entradas con
130846 B/1620 líneas y aviso TRUNCADO; malformado y 10001 entradas rechazados;
cambios rápidos y salida normal tras ACK. No extracción ni CRC/datos comprobados.
No entrada del archivo ZIP creó un archivo en el fixture de destino.

Método: handlers y framebuffer real, no input SO/clipboard. Cierre antes del
nuevo spawn registrado no equivale a kill activo. Recursos/startup NOT_MEASURED
por fase; no extrapolar presupuesto. Z01/P08/P09 siguen parciales.

## Ventana estándar y menús — 0.0.4, 2026-10-26

window-ui-1793042474907145360.json funcional PASS acotado. Tres instancias reales:
normal por defecto (Hyprlandfloating=false/fullscreen0), elección fullscreen2
observada y guardada, reapertura fullscreen y retorno/reapertura normal0. Campos
TOML effects=false/future.value42 conservados, tres salidas0 después de ACK/drain.
Cambios desde centros de targets reales de hit testing por handlers internos.
Menús excluyentes Preferencias/Acciones con Nerd, exclusión de todos los controles
del header, cuatro esquinas y banda libre; layouts845×1061 y1728×1117. Capturas
del framebuffer FLTK, no incluyen decoración exterior del compositor ni prueban
inputSO. Bridge nativo no se llama desde escena; gesto/cursores bajoinputSO PENDING.

292tests Rust (80core/37helper/175UI), Clippyworkspacealltargets-Dwarnings y release
limpios; logs en evidence/window. Bridge C++ compila contra headers reales pin
fltk-sys1.5.23, FL_API10405/ABI10400. No cambio registry/SO ni fallback activo.
Recursos/startup NOT_MEASURED por freeze, nada inferido del pie PSS de capturas.

Fallos conservados: window-ui-failure-1793041926965059030 esperaba maximización
que la regla real Omarchy suppress_event=maximize ignora (FLTKflag no prueba WM).
Modo extra retirado antes de entrega; sólo normal/fullscreen soportados expuestos.
window-ui-failure-1793042207114132046: fixture validó targets del menú durante
cierre, cuando UI ya limpia todos sus hits; condición de readiness ahora excluye
closing. Tres cierres normales nuevos prueban corrección; no se reutiliza SIGTERM
como PASS. Fallos iniciales de compilevariant/configpaths/Clippy también preservados.
Detalle funcional/build/accesibilidad pendiente en WINDOW-MODES.md.

## Papelera y restauración visibles — 0.0.4, 2026-10-26

`trash-ui-1793045192155795906.json` funcional PASS acotado en release SHA256
`eca3344bec10a7ae3aa0081fff9b14c8582fbc6467840f2616e6145497d7e82c`.
Acciones/Supr y Deshacer ejecutan el motor Linux del mismo filesystem; selección
prestada con preflight de 64 elementos/256 KiB y worker único. No borrado permanente
ni fallback de copia. Se enviaron cuatro objetos propios (dos archivos, directorio
no vacío y symlink); destino externo del symlink intacto. Metadata 0600 y formato
Trash Info verificados. Undo restauró tres, conservó el nuevo ocupante del cuarto,
mostró rutas verificadas; al retirar ese ocupante del fixture, el reintento restauró
bytes exactos. Nombre raw no UTF-8 `%FF` round-trip y lote de 65 rechazado antes de
mutación, sin truncamiento ni pérdida. Cierre normal 0 tras ACK y marcador drenado.

Método: centros de targets reales y handlers internos, filesystem real de fixture
aislado en `~/.cache/files-test`, framebuffer FLTK 845×1061. Incluye panel de error
con pan horizontal para leer `.trashinfo` completo. No input del SO, clipboard ni
cierre durante una syscall activa probado en esta escena; cancelación y drenaje
se cubren también por tests de worker. No lectura de papelera personal.

314 tests Rust (98 core/37 helper/179 UI), Clippy workspace/all-targets con
`-D warnings` y release limpios. Regresiones de sustituciones externas en ambos
sentidos no mueven un inode ajeno como rollback; FD originales conservan ubicación
actual para diagnóstico. Restore confirmado con fallo sólo de resolución de ruta
retira el token ya consumido y emite advertencia; recibos no reintentables pasan a
recuperación manual acotada sin Undo obsoleto. Logs en `evidence/trash/`.

Recursos y arranque **NOT_MEASURED** por freeze; no usar el pie PSS principal de la
captura como presupuesto del árbol. Pendientes: papelera por volumen/EXDEV funcional,
catálogo de recibos persistidos, redo, journal durable/crash, otros SO y presupuesto
64-elementos. O04 sigue parcial. La primera corrida de tests falló porque un fixture
de límite repetía 64 veces el mismo nombre tras añadirse rechazo de duplicados;
se preserva `initial-test-duplicate-fixture.log`, fixture corregido a nombres únicos.
Las dos escenas anteriores conservadas probaron el flujo normal con builds previos;
no acreditan las correcciones finales de identidad/recibos.

## Regex visible en renombrador — 0.0.4, 2026-10-26

`rename-regex-ui-1793046339654903640.json` funcional PASS acotado: selección2,
preview antes/después sin tocar bytes, capturas con nombre, error de regex y
expansión mayor a4096B rechazados sin plan; Aplicar/Undo/Redo/Undo preservan bytes,
archivo no seleccionado intacto, staging vacío y salida normal0 tras ACK/drain.
Método: handlers internos y framebuffer real (normal gestionada por compositor),
no eventos SO/clipboard ni benchmark de renombrado masivo. SHA256 release
`5a9b4e38cf6f469efd92951b248376fba15d660b6741a43490725da97aa89d8d`.

324 Rust (106core/37helper/181UI; última corrida UI181), Clippy workspace all-targets
-Dwarnings y release PASS. Regex compilado una vez por lote, NFA1MiB/nest64/
capturas64/segmentos256, salida4096B contada en slices antes de reservar. Sólo
std, sin activar tablas Unicode de licencia no autorizada; literales/rangos/dot y
capturas UTF-8 disponibles, clases/propiedades/casefold Unicode muestran error.
Árbol seleccionado y auditoría en evidence/rename-regex, sin nueva excepción.

La captura anterior `1793046051262606727` tenía PASS de disco pero FAIL visual:
FLTK multiline en footer28px ocultaba el motivo de error. Conservada con sidecar;
resumen de dibujo único/bounded corregido y regresión probada. Recursos/startup
NOT_MEASURED por fase; footer PSS no es gate. Fechas, Unicode avanzado y budget
100k siguen pendientes. Adaptación Cargo del benchmark puro sólo py_compile,
sin ejecución/mediciones ni sustitución de su evidencia histórica.

## Propiedades y permisos — 0.0.4, 2026-10-26

341 Rust (115core/37helper/189UI), Clippy workspace all-targets -Dwarnings y
release PASS. properties-ui-1793049068223979012 funcional PASS: dos procesos,
modos/bytes reales de fixtures, rechazo de observación obsoleta, symlink readonly,
Recargar y cierre0 tras ACK. Cierre inmediatamente tras submit canceló antes de
chmod; no afirmar syscall activo. Capturas/handlers internos, no input SO.
Detalles y fallos iniciales preservados en FILE-PROPERTIES.md y evidence/properties.
READY ahora exige repintado posterior al comando; los controles recortados no
reciben hits. Recursos/startup NOT_MEASURED por freeze; no gate global ni tag.

## open directo y trazabilidad de cierre — 0.0.4, 2026-10-26

345Rust115/37/193,Clippyworkspace/releasePASS,open_job4regresionesPASS.
open-close-ui1793056978667673268PASS funcional acotado en3instancias: hostopen
ausente visible, fakeopen conargv exactossinshell yexit7, symlinknoasumecarpeta,
directorionavega, callbackFLTK→request→drain→exit0. No visorrealniSuperWvalidado.
CUA0windows/hyprctlownPIDpresente; rutaSOpendiente. Binaryinstaladoprevioantiguo
borderless ycallbackFLTKunframedvacío son hallazgos estáticos, no causa reproducida.
Resumenerroranteriorrecortado preservado con sidecarFAIL; nuevacapturamuestracausa.
OPEN-AND-CLOSE detalla contrato/límites; freezeNOT_MEASUREDsinPASSglobal.

## Espacio treemap — 0.0.4, 2026-10-26

370Rust121/44/205,Clippyworkspacealltargets-Dwarnings/releasePASS. Helper CLI9casos
PASS; space-map-ui1793057823344999138funcionalPASS: proporción8:4:2, selección
inspector, entrar/volver, vacío/cero/error/parcial, resize real por modos, cancel
recorrido20k, Lista libera y callbackcierre con helpervivo,11PID/startgone/root0.
Sólofixturespropios, handlers/framebuffer; inputnativoaisladofixtureflag, noOSinput.
Fallos previos de registroCLI/validación/Clippy/escenaCancelterminado preservados;
primeraescenaconinteracciónambientalnoaisladafalló, no tomarlecturascomoaceptación.
SPACE-MAP detalla aparentebytes, límites2048/256KiB, desconocidos sinárea yno
snapshot/hardlinkdedup. Recursos/startupNOT_MEASURED porfreeze; footer no gate.
V09 UI real entregada, adaptadores/rawroot/offlinemounts/budget pendientes. No tag.

## Ocultos globales, Espacio v3 y contextual (2026-10-26, 0.0.4)

388 Rust (126core/44helper/218UI), Clippy workspace/all-targets -Dwarnings y
release PASS. HIDDEN-CONTEXT-AND-STATUS.md detalla contratos y fallos preservados.
`evidence/hidden-context/hidden-context-ui-1793059768952742165.json`: funcional
PASS, dos procesos leen/persisten el MISMO show_hidden global (defaultfalse,
true→nueva instancia→false); cuatro vistas, selección/raw hits/operaciones podadas,
áreas normalizadas visibles y total aparente con ocultos distinto. Contextual
lote/fondo/teclado invoca formularios existentes sin abrir por B3. Frame label vacío;
marquee con reloj inyectado altera sólo bottom32px. Capturas reales1000×560,
tema actual, handlers internos; no inputSO ni prueba minimizar/gestos nativos.
Cierre callback→request→drain→exit0, dos helpers PID/start observados y recogidos.
Primer reporteUI sin logging hijos tiene sidecar: no acredita cleanup del árbol.
FAIL smoke antiguo order.len==entries.len, type mismatch y ETXTBSY fixture se
conservan. Testfixture open usa intérprete instalado; producción sin cambios.

Freeze recursos/startup NOT_MEASURED. El PSS principal visible en pie no es gate
de árbol ni incluye Swap. Baseline10k56.743936MB/startup151.978ms/clipboardFAIL y
otros UNVALIDATED anteriores siguen vigentes. Sin tag0.0.4/PASSglobal.

## Grilla de iconos (2026-10-26, 0.0.4)

391 Rust126/44/221, Clippy/release PASS. GRID-VIEW.md: iconos Nerd virtualizados,
botón/Ctrl5, zoom, selección/rango/reveal, ocultos globales y contextual. Pruebas
puras100k/recorder1000 no sustituyen gate app100k. grid-ui1793060477438092576
funcionalPASS162visibles+2ocultos, ocho capturas1000×560/800×500 por fixture own
window constraints, carpeta/volver, raw/toggle, callbackdrainexit0. Handlers internos,
no eventosSO. Ningún helper de previews/du/treemap solicitado: lifecycle raíz,
no prueba de árbol por conjunto vacío. Glifos antes recortados y fallos de repaint
inicial/fixturefullscreenconstraints/ETXTBSYdu conservados. Recursos/startup
NOT_MEASUREDfreeze; miniaturas/LRU/persistencia específica pendientes, sinPASSglobal.

## Historial y omnibar acotada (2026-10-26, 0.0.4)

399Rust129/44/226+Clippy/releasePASS. NAVIGATION-HISTORY: dos botonesNerd yAlt+flechas,
64rutaslossless4096B/pila total y sin sondas UI, nueva rama limpiaAdelante, rechazar
guardado no muevehistorial. Preflight paste4096B prestado conserva texto/replace.
history-ui1793061306527375025funcionalPASS: navegación real por carpetas owned,
error missing recuperado por botónAtrás, rechazo1MiB visible/Unicode posterior,
seisframebuffers1000×560 ycallbackdrainexit0. NoeventosSO/clipboard externo,
nohelpersprev/du/treemap solicitados (lifecycle raíz sólo). Pestañas/sesiones aún
pendientes. Buildcompartióhost conCargoOmabridgeajeno sin tocarlo. Congelación
recursos/startupNOT_MEASURED; ningún PASSglobal ni cambio aFAILhistóricos.

## Ayuda F1 (2026-10-26, 0.0.4)

401Rust129/44/228, Clippy/releasePASS. SHORTCUT-HELP: F1/Preferencias con38filas
estáticas, paginación/Esc/cierre/resize, sólofunciones implementadas. Escena
help-ui1793061761953895411funcionalPASS4capturas1000×560/800×500, nohitarchivo
bajooverlay, selección/ruta preservadas ycallbackabierto/drain/exit0. Handlers
internos/framebuffer, NOinputSO; lifecycle principal sin helpers solicitados.
AT-SPI/i18n/terminal/otrosatajos yrecursos/startupNOT_MEASUREDsiguenpendientes;
fallosglobalespreviosintactos. No0.0.4tag/PASSglobal.


## Medios seleccionados: identidad e invalidación (2026-10-26, 0.0.4)

MEDIA-LIFECYCLE M1:410Rust132/45/233, Clippy workspace/all-targets -Dwarnings y
release PASS. Imagen checked transporta revisión esperada hasta FD real y vuelve
con source; Ready/stream exigen generación+revisión. Watcher no recursivo, época
y token serializados, attach antesprobe, reconciliación1s/reattach del padre,
worker único reutilizable sin I/O ni join de syscall en UI. Own ACK/refresh invalida.

media-lifecycle-ui1793064138443049803 funcionalPASS: cuatro framebuffers1000×650,
patrones rojo/azul/verde, samepath same-size+mtime reemplazado, modify/delete/rename/
parent replacement, rápido A→B→C, vídeo borrado en reproducción, hidden sin nuevo
helper, Papelera ACK y cierre0/13helpers PIDstartgone/probereadonlyACKtrue. Sólo
handlers internos+FSfixtures+framebuffer, NO inputSO, no cobertura de lifetimes.
FAIL1793064084657403710 preservado+sidecar por ACK deshabilitado en instrumento y
nombre Trash incorrecto. Sin caché de producto en fixture source/config/data;
Trash conserva original legítimo. Recursos/startup NOT_MEASURED freeze; footer
selfPSS no es gate. LRU/miniaturas/embedded/nonUTF8IPC/otrosSO/offline/minimizar
siguen pendientes. FAIL/UNVALIDATED históricos intactos, sin0.0.4tag/PASSglobal.


## Miniaturas bajo demanda y pérdida de widget (2026-10-26,0.0.4)

MEDIA-THUMBNAILS:423Rust132/46/245, Clippy workspace/all-targets -Dwarnings y
release limpios. Grilla/inspector comparten RGB prestado LRU4MiB+ticket antesdecode,
32entradas/64KiBclaves y64KiBdemanda; scratch76800B aparte. JPEG12/24baseline IDCT
+160px, PNG/WebP y frame vídeo desde motores existentes; rawUnix byteIPC. Source
FD proof/epoch, invalida event/timer; no I/O draw, no decode reiterado si revisión
no cambia, helper único coordinado Space/du/Espacio. No presupuesto PSS acreditado.

thumbnails-ui1793069315038209135 escenariofuncionalPASS3instancias serializadas:
7recursos/JPEG12MP160×120/filename0xFF/inspector sinclonar/Space/cache/replace/delete/
modify/TrashACK/carpeta/callbackhelpervivo→exit0; notify suprimido FDreconcile→exit0;
C++widgetpropio destruido conhelpervivo→guard/failuredrain/exit1esperado sinpanic.
45/2/10IDs PIDstartgone; liveobservado1/0/1 no cobertura de lifetimes.5framebuffers
reales1000×650, handlers internos NOinputSO. Recursos/startupNOT_MEASUREDfreeze.
FAILs6839306230828(marcador),7329848136770(ESRCH),7614908477249(firstpaint+panic)
intactos+sidecars. PanicWidgetTracker original sinbacktrace causaabierta; idle20s
no repro noresuelve. Guardas nuevas nunca usan widgets eliminados y cancelan/
drenan; NO prueba compositor/SuperW ni journalcrashdurable. M2parcial, P09global/
embedded/SpaceLRU/otrosSO/HiDPI/minimización yFAILsglobales quedanpendientes.


## Rutas opacas CLI/Request/imagen completa (2026-10-27,0.0.4)

RAW-PATH-WIRE:427Rust136/46/245+Clippyworkspacealltargets-Dwarnings+release limpios.
Request PathBuf stringlegacy/arrayrawUnix4096B/NULreject, relativo sinURLdecode ni
normalización; CLI args_os/--. Primer SpaceMaptest esperaba rechazo anterior y
falló, luego exige roundtripid1/rutabytes. No límite quitado ni nueva dependencia
runtime(coredevserde_json yaenlock). Core permanece sinUI.

raw-path-ui1793071898429792393 funcionalPASS: argvroot0xFE+filename0xFF, Grilla/
inspector→Space morado, RGBcompleto verificado y source dev/ino/bytes real; dos
framebuffers1000×650/callbackdrainexit0/3helpersPIDstartgone. Sólohandlersinternos,
NOinputSO. Informe anterior1677948673706 preservado, segundaescena añadecapturaGrid,
no recursos/startup. Otros comandos rawGUI/otrosSO/omnibaredit y firmasembedded
siguenpendientes; Tracker original causaabierta. NOT_MEASUREDfreeze; noPASSglobal.

## 2026-10-27 — Firma FD de recursos Markdown

RELATIVE-SOURCE:434Rust141/47/246, Clippyworkspacealltargets-Dwarnings y release
limpios. Wire1793074260681793818PASS con reemplazo determinista entre probe/open,
root/intermediate reemplazados, symlinks/traversal/raw. WireFAIL1793074121342824158
con sidecar se conserva (allowlist inicial del helper incompleta).
UI1793074268202431350PASS:5ocurrencias/2recursos comprobados,5framebuffers1000×650,
root→subdir→nested→subdir→root, callback/drain/exit0. Captura sin sampler: cleanup
sólo acredita raíz, no árbol de hijos ni presupuesto. NoinputSO/clipboard.
Recursos/startupNOT_MEASUREDfreeze, históricosFAILactivos, no0.0.4tag/PASSglobal.
Watcher/reconcile/postpublicaciónembedded siguen pendientes; ver RELATIVE-SOURCE.

## 2026-10-27 — Publicación e invalidación de medios Markdown

DOCUMENT-MEDIA:442Rust141/47/254, Clippyworkspace-Dwarnings y releasePASS.
UI1793076116463397564PASS2instancias normal/sin callbacks; mismo path/size/mtime
replace, in-place, delete/recreate, rename,rootreplace/recover,5capturas1000×650.
CandidateLoading no visible hasta watches+manifestproof+epoch; RGB viejo ausente
en framebuffer y firmasFDcoinciden confixture. Callback→helperACK→exit0,26identidades
helper porinstancia ausentes; sin sampling/lifetimecoverage. Readonlystopfalse/true
registrado, no afirmar primer probe drenado/nojoinUI. RegresiónMDcontextual
1793075764251781328PASS. FuenteNOT_MEASUREDrecursos/startupfreeze, noinputSO,
P09global y fallos históricos siguen abiertos; ver límites en DOCUMENT-MEDIA.

## 2026-10-27 — JPEG progresivo reducido

JPEG-DC-PREVIEW445Rust141/50/254+10vendorlib, Clippyworkspace/release limpios.
UI1793078251643976150PASS:12MP500×375,24MP750×500,EXIF500×750,33colores yfirmaFD,
6framebuffers1000×650 (compositor2x no nitidez2x). Cierrependiente/ACK/exit0,
8PID/starthelpers ausentes, Znoaceptado; helpervivo alcloseNO_VALIDATED.
Sin nuevoPSS/startup/cpu porfreeze. Capcoef32MiB no aumentó:1i16/bloque,
ACentropyomitida/nointegridadcompleta. Color coincidefullcodec1/8; comparación
Turbo4:2:0FAIL explicadoDCTchroma2vs1,4:4:4equivalentePASS≤3/255.
Archive/17src/patchfuzz0/licenciasauditados; driftformato8srcanterior
reproducido conrustfmt. Doctestsvendorfallanporimágenesupstreamnoincluidas,
no falsePASSdelcomando. VerJSON/logsevidenciajpeg-dc y límitesP02aúnparcial.

## 2026-10-27 — PNG reducido por filas

PNG-ROWS-PREVIEW:453Rust141/58/254, Clippyworkspace-Dwarnings/releasePASS.
UIimage-ui-1793081848018085943PASS funcional9framebuffers1000×650; PNG9/12/24MP,
alfa/EXIFtardío/Adam7/zoom/corrupto, JPEG/WebPregresión yfirmaFD efectiva.
Compositor2x/físico2000×1300 no prueba nitidez de todos los píxeles2x.
Closepending→ACK/exit0,12PID/starthelpers ausentes o reutilizados, Znoaceptado;
helpervivo al closeNOT_VALIDATED, no lifetimecoverage niinputSO.
Sin next_frame/fullsourceRGB, decoder32MiB best-effort noPSS; RGB3MiB yfila64KiB.
Recursos/startupNOT_MEASURED porfreeze, fallosbase históricos intactos.
P02parcial(WebPgrande/APNG/ICC/calidad) yP09global pendientes.

## 2026-10-27 — Buffers vídeo y miniaturas grandes

454Rust141/58/255+Clippyworkspace-Dwarnings/releasePASS. VIDEO-PREVIEW
1793082933186610779 funcionalPASS4FB1000×650, play/pausa/resumeEOF/cierre
streamactivo,9loggedPID/startgoneZnoaceptado. SlotúnicoReady/Recycled recibe
RGB desplazadoUI; prueba100handoffs+State, no ahorroPSS/CPUafirmado.
FAIL1793082885016655179 preservado+sidecar: fixture perdióentrada, trigger
noestablecido; repetición aísla input ambiente por flagexistente, NO inputSO.
Thumbnails1793082861888227321PASS PNG24MP/JPEGprogressive24MP160px/FDfirmas,
Grilla-inspector/Space/raw/delete/replace/Trash/navigate/closeactivo;4FB y44logged
helpersgone/exit0, no lifetimecoverage. Los presupuestos locales no sonPSS.
Freezevigente: recursos/startupNOT_MEASURED; oldfooter76.6MiB y fallosbase intactos.

## 2026-10-27 — Audio, FD efectivo e invalidación

458Rust141/58/259, Clippyworkspace-Dwarnings y releasePASS. AudioJob exige
expectedRevision antes de ffprobe y devuelve actualFDproof tras revalidar;
State conserva firma/generación. Determinísticos probeA/openB/ReadyA tardío
y gate readonly sin falsoACK, no son pruebas de kernelD real.
audio-ui1793083931390323451PASS funcional8FB1000×650 con notify suprimido:
same path/size/mtime reemplazo, in-place mtime restaurado, delete/recreate,
rename/padre recuperado; título nuevo y FDstamp reales, borrar oculta metadatos.
Rootexit0/ACK y11loggedPID/startgone (Zmismoidentity noaceptado), no lifetime
coverage ni ffprobevivoalclose probado. Audio sigue metadatos/SIN reproducción.
FreezeNOT_MEASURED recursos/startup; sin inputSO ni nitidez nativa2x, P09parcial.

## 2026-10-27 — Audio reproducible con Pulse

463Rust141/58/264, Clippyworkspace-Dwarnings y releasePASS. AUDIO-PLAYBACK
UI1793086873981565834PASS9FB1000×650, fuente silenciosa6s/salidaPulse delSO:
play/pause, seek por puntero preserva selección, reanudar/EOF, ocultar no reinicia,
borrar cancela/oculta y restaurar revalida. Callback FLTK conplayerPID/startvivo
→ACK→exit0,14loggedidentidadesgone/reused Znoaceptado; no lifetimecoverage.
No inputSO/SuperW, audibilidad, posiciónPCM/sample-exactseek ni nitidez nativa2x.
PrototipoFFplayNativeEOFtimeout/dummyPASS, staleHyprsignature yargv-infbuf0FAIL
preservados. PulseAS512 mmapFAIL ycontrato768PASS son contención virtual, noPSS;
AS768como vídeo/max_alloc32M, techo150MB/ideal50MB sin cambio. Sin recursos/CPU/
startup/GPU nuevos porfreeze, no0.0.4tag/PASSglobal; ver AUDIO-PLAYBACK.

2026-10-27 tras reinicio de Hyprland: native-inventory1793087442753417655
confirma ventana propia mapped antes/después, pero cua-driver MCP retorna array
windows vacío. Autoexitnormal conACK; ningún foco/tecla/clic enviado, ningún
contenido de ventanas ajenas registrado. Skill jev-computer-use impide fallback
de control; inputSO/clipboard con foco/SuperW permanecen pendientes.

## 2026-10-27 — Helpers propios ante muerte del padre

464Rust141/58/265+Clippyworkspace/releasePASS. OWNED-HELPERS cubre spawn común,
stream vídeo, du yEspacio; open de usuario independiente. helper-crash
1793088449742352360PASS4rolesPID/startvivos→rootSIGKILL→adopción/reap9por test
subreaperprivado. Puedeocurrir antesdeprogreso; no grupo/killmanualhelperenPASS,
noMutationsActivas/recoverydurable/crashduringSave/kernelD/nietos/PID1latencia.
Relaunchconfigprivada/FDverificado/exit0ACK,3FB845×1071. AudioPulse normal
1793088666411517841PASS9FB yvídeo1793089019986522162PASS4FB/cierreactivo, strict
loggedPID/startcleanup. No PSS/Swap/CPU/startup porfreeze niinputSO.


## 2026-10-27 — Vídeo con audio opt-in (M11)

470Rust141/58/271, Clippyworkspace/all-targets-Dwarnings y releasePASS.
VIDEO-PREVIEW: stream único ffmpeg RGB+Pulse, mute por defecto/M, índices reales,
source failure tipado frente a error de salida recuperable. NativePulsezeroPCM
video-play-ui1793091780741012397 PASS5FB1000×650/9loggedhelpersgone strictZfalse/
hot-toggleACK/pause/resumeEOF/callbackclose0ACK. Recovery socketausente
1793091967178711219 PASS6FB/9gone/errorvisible→mute→framesEOF/cierre0ACK.
Standaloneaudio1793092217283190449 PASS9FB/14gone/activeclose0ACK. Misma appSHA
52cde7a05957ba702f5c50946f2eaa5130e01d76663d1e0117929bcc7029e820,
helper187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.

Defaultarenas+AS768 virtual Pulse mmapFAIL1793091361377293413 preservado;
child-only MALLOC_ARENA_MAX2 permite contexto con mismo cap, contrato funcional
video-audio-arena-contract. No PSS/CPU/throughput claim. FAIL fixture PulseServer
postPopen1793091839762673007 conservado+sidecar, corregido antes del spawn.
Handlers/framebuffer, sin inputSO/audibilidad/PCM/A-Vsync/native2xsharpness.
Sin recursos/startup nuevos porfreeze; oldFAILs yP07/P09parciales, noPASSglobal.


## 2026-10-27 — PDF con revisión efectiva (M12)

473Rust141/58/274, Clippyworkspace/all-targets-Dwarnings y releasePASS.
PDF-PREVIEW: expectedFD obligatorio/metadata y render mismoFD/sourceproof enReady,
watch/token/epoch/reconcile1s existentes, SourceUnavailable tipado/nuevo token.
pdf-ui1793092913810767063 funcionalPASS8FB1000×650 con notify suprimido: RGB y
firma reales, replace igualsize+mtime, inplace mtime restaurado, delete/recreate,
renamefile/parentrecover, oculto nohelper, páginas/zoom/rapid/errors/cierre0ACK.
23loggedPID/startgone/reused strictZfalse, no lifetimecoverage ni helpervivoalclose.
readonly_gate compartidoaudio/PDF conserva salidaReadOnlypermitida≠workerACK,
no joinFSUI ni prueba kernelD. No inputSO/native2xsharpness/textoselecciónPDF.
AppSHA3dacd1b81f77cf85da634484cddc1640623dfcb3836433df160fcd456a74ab2b,
helper187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.
Recursos/startupNOT_MEASUREDfreeze, fallosbase intactos, sin0.0.4tag/PASSglobal.


## 2026-10-27 — HTML renderizado ligado al FD (M13)

476Rust141/58/277, Clippyworkspace/all-targets-Dwarnings/releasePASS.
HTML-ENGINE-PROBE: expectedFD obligatorio, adaptador hereda ese FD y usa pread,
Revision efectiva tras render/decode/revalidate, gate watcher/token/epoch/reconcile
existentes; readonlyclose permiso≠ACK, no joinFSUI ni nuevos workers acumulados.
Nativebuild1793093259627217246 yfd-contract1793093345381869451 PASS funcional;
legacyCLI conserva compatibilidad, FD inválido/FIFO/>32KiB sin salida parcial.
html-ui1793093483630467339 PASS10FB/resize800×500/notify suprimido: texto/firma
FD trasreplace mismo size+mtime/inplace mtime restaurado/delete/recreate/file+
parentrename recovery, hidden sinhelper; selección/renderCSS/scroll/block/error/
rapid/cierre0ACK.14loggedPID/startgone/reused strictZfalse, no lifetimecoverage
ni helpervivoalclose/inputSO/clipboard/native2xsharpness/kernelDreal.
AppSHA bb32f9725d40d6327c66897aa0a1595c7a2c113e28fd81206f41f0a83a83575c;
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4;
helper187c939eb981500c35fa6c292b066717dd21027aa518cdd3943840378701dfb9.
Métricas tipográficas aproximadas/recortes visibles siguen pendientes; NO shaping
correcto. Recursos/startupNOT_MEASUREDfreeze, valores delpie no son gateárbol.
Sin0.0.4tag/PASSglobal, fallosbase históricos conservados.


## 2026-10-27 — PDF texto extraído seleccionable (M14)

480Rust141/58/281, Clippyworkspace/all-targets-Dwarnings y releasePASS.
PDF-PREVIEW: T/botónNerd cambiaPágina/Texto de la misma página, anterior/siguiente,
CtrlPgUpDown yselecciónTextDocument; pdftotext usa mismoFDexpected/efectivo y Job,
outputs excluyentes/RGBliberado, stdout256KiB/2000LF/UTF8strict sinNUL.
String tomaVec sinclone, caps anteshelper/retenciónwire; sin cache/archivo de producto.

pdf-ui1793094189643854652 funcionalPASS12FB1000×650/notify suprimido: textoASCII
páginas2/1, firmaFD/selección porpuntero/Copyhandler yvolverRaster, más lifecycleM12.
SourcecapPDF256MiB+1/HTML32KiB+1 conserva revisión ysin nuevos hijos en intervalo
reconcile: error de capacidad estable yaNO fuerza token de fuente repetido.
29loggedPID/startgone/reused strictZfalse/cierre0ACK, no lifetimecoverage nihelper
vivoalclose. TestsUTF8/TAB/CRLF/caps ytextlatest/sourcegate, noPDFUnicode real.
App786c52d7c9afef200ad7c258514af96a531b4cf805be4e2b66f67614aad6be92;
helper187c939eb981/HTML85ea1b8009ef sin cambio dehelper. No inputSO/clipboard/
selecciónsobregeometríaPDF/nitideznativa2x. Recursos/startupNOT_MEASUREDfreeze;
fallosbasehistóricos intactos, sin0.0.4tag/PASSglobal.


## 2026-10-27 — PCM de audio y vídeo en sink privado (M15)

481Rust141/58/282+Clippyworkspace/all-targets-Dwarnings/releasePASS,4PythonPurePASS.
AUDIO-PCM: actualmonitorPulse localUnix en nullsinkúnico/propio, dispositivo
explícito sólofixtures pares/ASCII64B, sin fallback ni cambio default. Fuentes
WAV96000zero+480000tono→FLAC440/videoFFV1+FLAC880, metadata12s crosschecked.
Audio-pcm-ui1793096793305882328 funcionalPASS2FB1000×650, tono440/880 confirmados
y silencio pausa/defaultmuteRMS0, ruta sink porPID, callbackstreamvivo/root0ACK/
8loggedhelpersgone/reused strictZfalse, recorderreaped/moduleabsent/defaultigual.
No altavoces físicos/A-Vsync/PCMclock/exactseek/inputSO/clipboard/native2x/lifetime.
Appb76ecfdb6c4f3d76d93f4ca6e02bbb8e14b044212cc570123bf70502e899749d.

AUDIO-PCM detalla4FAILs harness preservados, incluido primer modulecleanup fallido
recuperado pornonce y mismatch inicialsink sinID cuya causa NOse establece.
Zero-route6246104965614 sólo ceros; PCM6434340845151 iniciallavfiduración10.042667
noacredita12s/preamble, correcciónsidecar/finalWAV explícito. No ocultarhistoria.
ENOSPC.pyc ySIGINT130 sólobuildpropio, logpreservado, limpiar repo incremental y
retry sinincremental481PASS; no configglobal/buildsajenos ni afirmacióncausal.
Recursos/startupNOT_MEASUREDfreeze, sin0.0.4tag/PASSglobal ni ahorroPSS/CPUafirmado.


## 2026-10-27 — GIF bajo demanda y miniatura inspector (M16)

485Rust141/58/286+Clippyworkspace/all-targets-Dwarnings/releasePASS. GIF-PREVIEW
mismoenginevídeo, perfil8MiB/4Mipix/300s, codec/demuxgif exclusivo eignore_loop1
explicit en tres rutas; sin decoder/GPU/dependencia nueva ni cache producto.
video-play-ui1793098465429205562 funcionalPASS6FB1000×650: snapshotpausedred,
playred→blue/pause/resumeEOFnoticeNone peseNETSCAPEloopcount0; CloseSpace no
stream nuevo, inspector160×90primerred/firmaFD/pixel/captura. Activeclose0ACK,
10loggedPID/startgone/reused strictZfalse, no lifetimecoverage/inputSO/native2x.
AppSHAe0a9e7f6c4a0e21aef2b7872518b4e6e275d76bf7c3a186b00df8c61e928f87a.
FAIL1793098062479122223preservado+harnesscorrection: contabaTODOShelpers después
ClosePreviewaunqueinspector visible pedía thumbnail; ahora verificastream+thumbnail
porseparado. EOFnoreconocebackenderror comoéxito. Alpha/disposal/PTS/zoom yGIFgrilla
no verificados; recursos/startupNOT_MEASUREDfreeze, sin0.0.4tag/PASSglobal.


## 2026-10-27 — Vídeo/GIF readonly close sin ACK ficticio (M17)

487Rust141/58/288+Clippyworkspace/all-targets-Dwarnings/releasePASS.
VIDEO-SOURCE-CLOSE: snapshot/stream gateReadOnly/Helper/Stopped compartido,
mutexclosing contraadmisión antes cada spawn, readonlypermiso≠FinishedACK;
Helperexigereap/try_waitSome enEOF. Seleccionados, miniaturalifecyclenocambiado.
video-readonly-close1793099666110053045PASS2apps/2FB1000×650 conholdSIMULADO30s:
snapshot ystream→callback→exit0 conreadonlypendingtrue/drainedfalse/nofuturechild,
ninguna admisióntardíadespuéshold. No esperar30s ni afirmar kernelDinterrumpido.

RegresiónGFvideo-play-ui1793099692852727198PASS6FB/redblue/pause/resumeEOF/
inspector160/sourceFD/activecloseACK0,10loggedPID/startgone/reused strictZfalse.
AppSHAa7ab09cab0c7f89414fb6b650d586e1d46ee9d62155848373556bde5944ff2b8.
BloqueoFS durantefaseHelper siguependiente; no lifecyclecompletooffline/SuperW/
inputSO/native2x/lifetimecoverage ni nuevos recursos/startup porfreeze.
Sin0.0.4tag/PASSglobal. Intermedia E0061 aridad conservada, finalsinwarnings.

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


## M21 DOCX — recuperación y aceptación funcional (2026-10-27)

500Rust(141core/67helper/292UI), Clippyworkspace/alltargets-Dwarnings, release y
fmtPASS.5fixturesENOSPC anteriores ejecutados porcoordinador conbinarioexistente
PASS; suite nueva completa PASS, sin omitir/reubicarfixtures. LogsFAILintactos,
causa de almacenamiento no inferida. Regresión XML nueva reprodujo RED ypasó
confix;10casoswirehelper realesPASS. office-ui-1793105654289278359.jsonPASS6FB,
expected/effectiveFDrevision ygeneración, rawUnix, replaceigualsize/mtime,
delete/recreate, erroresCRC/DTD/cap/symlink, Unicode102Bselección, truncado256KiB,
callback0ACK/9loggedhelpersgoneZfalse. No helpervivoalclose/inputSO/clipboardreal/
Wordlayoutfiel/otrosOffice. Recursos/startupNOT_MEASUREDfreeze; piePSSincidental
nocuenta. DOCX-PREVIEW detalla límites, revisión y evidencias; sinPASSglobal.


## M22 — pestañas y restauración de estado en la instancia (2026-10-27)

513Rust (141core/67helper/305UI), Clippy workspace/all-targets -Dwarnings, release y
fmt PASS. Logs finales tabs-checkpoint-test, tabs-empty-clippy,
tabs-checkpoint-release y tabs-checkpoint-fmt; copia en tabs-checkpoint-validation.json.
Escena tabs-ui-1793109776417107918 PASS: nueve framebuffer lógicos reales, A/B
con mismos nombres y contenido distinto, estado independiente y cambios sin pump,
scroll120 conservado, hidden global/poda vacía sostenida tras rebuild, raw0xFF,
ocho tabs y reabrir con ID nuevo. Resize archivo/carpeta con metadata lista
800×500/440; CalculateSize omitido cuando no cabe y presente a650 en recorder.
Última pestaña exit0 posterior a ACK; un helper registrado por PID/start desaparece,
no zombies. No cobertura de lifetimes/helper vivo exacto al cerrar.

Preservados tres FAIL: tabs-ui-1793108574031992206 (socket harness),
1793108595977296407 (scroll por clamp prematuro) y1793108996440627071 (rebuild
seleccionaba carpeta tras poda). Las regresiones de paint durante carga/layout
y selección vacía/recovery tienen tests; asserts originales mantenidos.

Método: handlers de puntero para targets visibles; Actions para ciclos y tabs
fuera de strip. No eventos SO ni clipboard externo. No prueba de nitidez nativa2x.
Recursos y arranque NOT_MEASURED por freeze; PSS incidental del footer no cuenta.
V10 parcial/sesión durable pendiente; sin PASS global ni tag0.0.4.


## M23 — sesión durable (2026-10-27)

527Rust141core/67helper/319UI, Clippyworkspace/alltargets-Dwarnings, release/fmt
PASS; session-checkpoint-validation.json conserva logs incluyendo FAIL de fixture
Receiverdescartado y dos FAIL Clippy de estilo. tabs-session-ui1793111266679001110
PASS6FB/8procesos exit0 posteriores a ACK. Save→restore real de metadata A/B,
closedraw0xFF, argv explícito no aplica/autoguarda, --no-sessiondisabled; dos
ventanas sin takeover/stomp y bytes inválido/futuro preservados. Regresiones
Model reales contra SessionOpened tardío después de selección/filtro/nav/cierre;
capencode/decode prestado, FIFO/symlink no bloquean, lockreplaced rechazado,
syncdirfailure posterior a rename devuelve Unsynced y retry valida archivo real.
SESSION-CONTRACT detalla reservas/limitaciones. Handlers/Actions, no inputSO ni
clipboard; sin recursos/startup por freeze, sin recuperación tareas/crashglobal.
Tres FAIL visuales M22 intactos. No PASS global/tag0.0.4.


## M24 — mover nativo (2026-10-27)

537Rust148/67/322, Clippyworkspacealltargets-Dwarnings, release/fmtPASS.
move-checkpoint-validation conserva logs finales y FAIL de sintaxis/type/lifetime,
assert menú por índice y Clippyinitializer; corregidos sin relajar seguridad.
move-ui1793113502303715506PASS6FB, filesystem comprobado antesConfirm y enPaused
intacto; luego4roots inode/dev/mode/uid/gid/mtime/ns/bytes/xattrs, symlink literal
y raw0xFF. Target conserva selección, source relista sin roots movidos;
no-clobber y EXDEVpreflight/proc real sin publicar ni copy/delete. Cancel/close
pausado sourceconservado, exit0 trasACK. Core cubre canceltras1root, IDs cambiados,
parent rename/replacement, destination reubicado dentro source, publicación
incierta con sourcehardlink y actors preservados. Queue cuenta FDactivo/unread
ACK+pending; close cancela todos antes de esperar revisión.

Scope MOVE-CONTRACT: native sameFS sólo,128roots, otrosSO Unsupported,
EXDEVtransfer/journal/undo pendientes. No inputSO/clipboard/recursos/startup;
no extrapolar PSS incidental de capturas ni prometer protección inodecondicional
Linux contra actor hostil. Sin PASS global/tag0.0.4.


## M25 — EXDEV single-file y recuperación visible (2026-10-27)

586Rust (193core/67helper/326UI), tres coreignored privados/opt-in delimitados,
Clippy workspace/all-targets-Dwarnings y release limpios. Logs finales
exdev-workspace-final-test, exdev-workspace-final-clippy, exdev-resize-release;
se conservan primeros FAIL compilación/FD assertion/Clippy. Core realdev63→75 y
tres pares de procesos gap publish/delete/restore ejecutados aparte (exit73 sin
Drop → proceso nuevo0); no SIGKILL/powerloss/fallo syscall fsync real.

exdev-ui-1793122272433112203 PASS: tres casos dev63→75, review/pausa no mutantes,
bytes/metadata exactos, tabs refrescadas, recuperación catálogo explícito, normal
exit0 tras ACK y stages ausentes. Diez PNG IHDR coinciden capture_dimensions.
Ventana INICIAL440×440 acredita controles separados/completos; diagnósticos y
título largos recortados, no lectura completa/clipboard SO. Handlers/Actions y
framebuffer FLTK; no eventos SO. Scope archivo regular nlink1, no árboles/lotes.

Preservados: 1793121684715864566 FAIL socket Hyprland obsoleto;
1793121723013735005 PASS funcional con sidecar que corrige supuesto resize440
(captura1000×650); 1793122169103469951 FAIL guard dimensiones detectó oscilación
440↔1000. Escena final usa instancia inicial440; LiveResize UNVALIDATED, no se
rebajó la aserción. No nuevas medidas recursos/startup: NOT_MEASURED por freeze,
baseline/arranque FAIL históricos siguen activos; pie PSS incidental no es gate.
Sin PASS global ni etiqueta0.0.4. Nuevas previews pausadas; sigue funciones/UI.


## M26 — Columnas/Miller (2026-10-27)

596Rust194core/67helper/335UI y3coreignored delimitados, Clippyworkspacealltargets
-Dwarnings, release/fmt PASS. columns-checkpoint-validation.json conserva logs:
compilación inicial (match/borrow), fixture de presupuesto errónea, primer
workspace WouldBlock recovery, regresión FDduplicado RED y GREEN; FAIL enfocado
intermitente entrada A ausente sin contexto suficiente no reproducido en exacto,
fixture luego con creación exclusiva/ID atómico/diagnóstico y suite final PASS.
No atribución causal exacta inventada. JournalDrop libera explícitamente flock
sólo en PIDcreador; no descarga owner al rechazar Request ni salta IntentProof.

columns-ui-1793124416464738573 PASS6PNG IHDR y captura_dimensions coinciden,
cinco1000×650+initial800×440. Cadena y podaA/B, copiar desde columna enfocada
(A/nested frente a mismo nombre B), bytes exactos/datos originales intactos,
filtro/raw y renombrador payload; dos normal0 tras ACK. Flechas y pasos rápidos
por métodos Model; filas/selector/confirmación por handlers de puntero visibles.
No eventos SO/clipboard externo/liveResize, ni sesiones de cadena completa.
Regresiones adicionales unitarias:12niveles/máximo8retenidas, foco antesDone/
Statsstale, Ctrl/Shift al activar, menú derecho diferido+cancelable, migra tag3
sin geometría y guarda tag5 conscroll. Nombres largos/ancestros parcialmente
recortados por scroll/clipping; no alegar lectura completa. Recursos/startup
NOT_MEASURED freeze y FAILs históricos activos; PSS incidental footer no cuenta.


## M27 — doble panel MC, 2026-10-27

DUAL-PANE-CONTRACT y `evidence/0.0.4/dual-checkpoint-validation.json`:603Rust
(194core/67helper/342UI),3ignored preexistentes, Clippy/all-targets-Dwarnings,
release/fmt PASS. Últimos dos cambios sólo labels de renderer; framebuffer final
confirma filtros por panel y basename antes del contexto de ruta.
`dual-ui-1793126826929914048.json` funcional PASS, ocho PNG con dimensiones IHDR
exactas,2procesos exit0trasACK. Copy/move raw0xFF→ruta0xFE con bytes/nombres reales,
review/paused no mutante, job termina tras foco/panel cerrado, selección/query
B preservados. Unitarios contextual/budgetconjunto/rename+Undo amboslistados y
worker único con backend bloqueado. No input SO/clipboard/liveResize ni metrics.
V07 parcial: persistencia ambos paneles/otros SO/otras vistas/gates pendientes.
Métricas NOT_MEASURED por freeze y FAILs previos conservados; no PASSglobal.


Corrección de reporte M27: `dual-title-fmt.log` era FAIL de una única línea por
wrapping, aunque se había descrito PASS. Original preservado. Formato aplicado
sin cambio funcional y `dual-format-correction.log` registra fmt--check exit0.
Clippy/release/escena/hashes instalados previos siguen válidos; no se reconstruye
ni instala M28 WIP por esta corrección sólo de whitespace.


## M28 — clipboard interno, 2026-10-27

624Rust203/67/354 PASS (3coreignored), Clippy all-targets-Dwarnings, release sin
warnings y fmt exit0 verificados en clipboard-checkpoint-validation.json.
clipboard-ui1793130549985863820 funcional scoped PASS8FB IHDRexactos/2root0ACK.
Copy raw, Cut2 parcial/retry/cancel/rechazo reemplazo/ACKviejo sobrebuffer nuevo,
menús800×440 sin toast y callbacktexto propio inmediato+eventloop exactos.
FLTKconst canónica true frente RustCString false demuestra causa del preflight
anterior, no pérdida de foco/selección. FAILs1793129984528710931/1793130010623243148/
1793130118639376044/1793130164558745696 y PASSfuncional1793130287088020335 con
sidecar visual Acciones preservados. Menú contextual sólo dejaba franja de toast
fuera de rect sin tapar controles; su supresión es limpieza opcional.
Handlers/framebuffer, no eventosSO, compositorownership/clipboard MIME externo,
liveResize ni procesoárbol independiente. Recursos/PSS+Swap/CPU/arranque
NOT_MEASURED por freeze; footer principal incidental NO presupuesto. O06 parcial,
no tag0.0.4/PASSglobal. Instalar sólo app/helper/HTML coincidentes con esta escena.


## M29a — clipboard nativo parcial, 2026-10-27

637Rust(208core/67helper/362UI),3ignored, Clippy all-targets-Dwarnings, fmt y
release exit0: native-transfer-checkpoint-validation.json. Primer FAIL C++
reading_epoch preservado. Escena clipboard-ui1793133638367792338 scopedPASS:
9PNG con dimensiones exactas,2rootexit0ACK. Regresión M28raw/cutparcial/retry/
cancel/reemplazo/ACKviejo/texto propio; menú800×440 muestra Pegar del sistema y
handler SIN gesto real rechaza sin operación/fallback ni cambio de generación.

isolate-input mantiene bridge deshabilitado: NO prueba registry/data_device
activo, serial/foco real, ownership, envío/recepción MIME entre aplicaciones ni
DnD. Unit State inyecta callbacks; pipe es privado. M29/O06 siguen abiertos,
KDE/multi-origen/DnD pendientes. OtrosSO/inputSO/liveResize sin validar. Recursos
y arranque NOT_MEASURED por freeze, footer principal incidental NO presupuesto.


## M29b — receptor DnD parcial, 2026-10-27

642Rust (208core/67helper/367UI),3ignored, Clippy workspace/all-targets
-Dwarnings, fmt--check y release exit0. `native-dnd-receiver-final-test.log`
ejercita la TU C++ real con proxies simulados: busy→source_actions, drop_epoch
obsoleto antes pipe, ACKfalse sin finish y COPYfinish. El test no es protocolo SO.
Rust cubre Lease completo antes ACK, target cambiado/filtrado, cancel/cierre y
pipe privado→AuxIo→Source→revisión→cola/copia bytes sin tocar origen.

`clipboard-ui-1793135944759735086.json` scoped PASS:12PNG dimensiones reales,
2rootexit0ACK; hover, revisión, cancelar y copia real de fixture raw, selección
conservada. Escena isolate-input con handlers/pipe/ACK privados: bridge del
compositor, serial, negociación, recepción externa e inputSO UNVALIDATED.
Hook pinned FLTK tiene fuente/parche/licencia propios y guard fuerte por hash;
FAILs de integración anteriores preservados. No se infiere disponibilidad real
de DnD del fixture. Sender, KDE, multipadre, otros SO y O06 siguen pendientes.
Recursos/startup NOT_MEASURED por freeze; footer PSS incidental no gate.


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

## Tema lib-fltk-omarchy v0.0.1 — 2026-10-28

Primera build debug PASS con guard de `config.h` y `FL/fl_config.h`; evidencia
`docs/evidence/0.0.4/fltk-omarchy-first-build-headers.json` registra hashes y
comparación normalizada (se excluyen sólo rutas de instalación). Cargo.lock
contiene una copia de fltk y fltk-sys 1.5.23 y toolkit git `50a42927`.

`toolkit-theme-ui-1793181381907414143.json`: PASS acotado, dos capturas FLTK
1000×650 y conteo de píxeles del fondo antes/después de sustituir el symlink,
lectura de font.base del shell y Corners square retenido; cierre normal. No
comprueba input SO, ni aplicación de escala tipográfica a Painter. 668 tests
workspace PASS (222 core/67 helper/379 UI; 3 core ignorados), Clippy/fmt y
release PASS. Recursos y arranque NOT_MEASURED. Componentes toolkit restantes,
paridad global y multipadre no están terminados; se mantiene FAIL anterior de
clipboard y limitación de Wayland nativo.

Ajuste posterior: el watcher existente de Files valida colores y shell antes
de invocar la recarga del toolkit v0.0.1; la validación y lectura del toolkit
pueden sufrir una carrera ante escritura simultánea. Escena
`toolkit-theme-ui-1793181731296656632.json` PASS: tres framebuffers, TOML
truncado rechazado con color anterior visible, symlink cambiado y salida0.
669 pruebas workspace PASS (223/67/379, 3 ignoradas), Clippy/fmt/release PASS.
[Issue #5](https://github.com/avillagran/lib-fltk-omarchy/issues/5) solicita al
toolkit validación y conservación de su propio último tema válido. NOT_MEASURED
para recursos/arranque; sin input SO.

## Elipsis de ayuda inferior — 2026-10-28

La pausa inicial de la marquesina indica truncamiento con `…`, calculando el
ancho con Painter y conservando límites UTF-8; durante el desplazamiento se
mantiene el texto completo. `toolkit-theme-ui-1793181920822762898.json` ofrece
framebuffer real 1000×650, sin input SO. 670 pruebas workspace y Clippy/fmt/
release PASS en la repetición; 3 ignoradas. La primera suite
`target/status-tip-ellipsis-test.log` falló en la prueba existente
`kde_active_cancel_after_uri_eof_closes_both_readers`: el worker devolvió
Interrupted pero un write al pipe aún tuvo éxito. La prueba aislada y la suite
posterior pasaron; causa no determinada, no se declara corregido ese fallo.
Recursos y arranque NOT_MEASURED.

## Pin lib-fltk-omarchy v0.0.2 — 2026-10-28

Primera build `target/fltk-omarchy-v002-first-build.log`: guard de headers PASS;
`fltk-omarchy-v002-headers.json` confirma una copia de fltk/fltk-sys y hashes
normalizados de headers. Workspace: 670 tests PASS (223/67/380), tres core
ignorados, Clippy/fmt/release PASS. `toolkit-theme-ui-1793212312149703731.json`
valida tema/rename/rechazo parcial en framebuffer de Files, sin input SO.
`toolkit-v002-focus-fixture.png` muestra un anillo de ButtonGroup en una
ventana aislada del toolkit mediante `focus::press`; no es un anillo en Files.
El router de Files, sus pruebas de Tab, foco y entradas reales quedan abiertos
según `FLTK-OMARCHY-KEYBOARD-MIGRATION.md`; no reemplaza el comportamiento actual.
Freeze recursos/arranque NOT_MEASURED. Fallos anteriores conservados.

## Pin toolkit v0.0.7 — 2026-10-28

Cambio de dependencia aislado: `3f5978a3836e42eca4940ae5007fe21ec66d9c3e`, GPU
apagada. Primera build/guard y `fltk-omarchy-v007-headers.json` PASS: headers
normalizados y una sola copia fltk/fltk-sys 1.5.23. 670 pruebas (223/67/380),
3 ignoradas; Clippy/fmt/release PASS. Escena
`toolkit-theme-ui-1793227096202584809.json` PASS para tema existente, framebuffer
real y handlers internos; no verifica PopupMenu, KeyboardPanel ni input SO.
Recursos y arranque NOT_MEASURED. Migración posterior y W02 descritos en
`FLTK-OMARCHY-KEYBOARD-MIGRATION.md`.

## Tema estricto del toolkit v0.0.7 — 2026-10-28

Files usa `Theme::try_load` en el arranque y `reload`/`last_error` en cambios;
se retiró su validación TOML duplicada. El watcher de Files conserva el override
`FILES_THEME`. Escena `toolkit-theme-ui-1793227227928691866.json` PASS: cinco
framebuffers, colores truncados, archivo de colores ausente y shell malformado
mantienen la última paleta; `last_error` comunica cada rechazo y un rename
válido recupera el tema. Cierre normal; handlers internos, no input SO.
669 tests (222 core/67 helper/380 UI; 3 ignorados), Clippy/fmt/release y guard
headers PASS. Una copia FLTK/fltk-sys 1.5.23, toolkit `3f5978a`, GPU off;
recursos y arranque NOT_MEASURED. Se eliminó una prueba de la validación local
retirada; los casos están cubiertos por esta escena real.

## 2026-10-30 — Local search / keyboard / directory-size slice

Scoped functional PASS: `evidence/usability/usability-ui-1793391860625293505.json`,
679 workspace tests (228/67/384, 3 existing ignored), Clippy all-targets -D warnings,
release workspace and fmt clean. Six framebuffer captures include 800×440 and
1000×650. Filters, Unicode/raw names, selection pruning for Copy, marking with
separate focus and directory size/count recalculation verified. The helper result
is apparent bytes by name, not allocated size or a filesystem snapshot. The
fixture changed its directory from 8 B/5 entries to 12 B/6; both results match.

Internal handlers/painted hits and real framebuffer only; no native input,
clipboard interoperability, startup or resource gate. Historical FAILs retained.
Installed checkpoint and exact five-artifact backup hashes are linked from
USABILITY.md. The new broad usability goal and remaining ROADMAP are not complete.

## 2026-10-30 — Recursive filename search

Scoped functional PASS: `evidence/search/recursive-search-ui-1793393137003734053.json`.
688 Rust (230/70/388), 3 existing ignored, Clippy all-targets -D warnings,
release/fmt clean. Eight captures include duplicate basenames/parents, raw result
location+selection, independent hidden scope, 500-hit explicit partial state,
invalid filter and a result directory. Both 1000×650 and 800×440 were checked.
Root callback/drain/exit0; eight logged helper PID/start identities absent/reused,
strict Z rejection. Cancellation before ACK observed, active OS helper exactly
at cancel/close not proven. Earlier scene PASS retained with finished-before-cancel.

Unit regression rejected delayed text paste after query revision change. Helper
symlink/cycle/invalid-root/raw/partial limits and bounded wire tests pass. No
runtime mount fixture, viewer/native input/clipboard or kernel-D acceptance;
resource/startup NOT_MEASURED. Full goal, historical FAILs and remaining S01
capabilities stay open. Detailed contract: RECURSIVE-SEARCH.md.

## 2026-10-30 — Queue and measured directory rows

697 Rust (230/72/395), three existing ignored, workspace/all-targets Clippy
-Dwarnings, release and fmt PASS. Final compact-inspector-only change follows the
suite, checked by Clippy/release plus final scene. `FOLDER-SIZES.md` records limits,
root/epoch invalidation, bounded stream grammar, cancelled/stale ACK rejection,
root replacement and progress sink errors. Final scene1793395687494116398 scoped
PASS six framebuffer, including 800×440; existing usability1793395692067448178
and recursive-search1793395693275772935 scenes PASS on the same final binary.
Earlier pre-compact scene observed positive progress; final scene did not catch
one, preserved without seeking a rerun. Counts are apparent observations, not a
snapshot; external deep changes require refresh. No native input/resource/startup
acceptance, global PASS or tag. Historical failures retained.

## 2026-10-30 — Pattern marking and operation focus

701 Rust (230/72/399), three existing core ignored; full workspace tests,
all-targets Clippy -Dwarnings pass. New semantic tests cover filter-to-mark/show-all
with exact raw copy names, empty selections that survive rebuild without F5
fallback, preserving focus under Select All, invalid pattern/partial-load guards,
and real background menu activation plus a bounded selection submenu at 440×440.
Tab refresh now finds a surviving marked focus after pruning the old focused name.

Visual inspection of earlier direct-action/copy scenes found an old painted
Actions fallback; subsequent wiring adds its compact selection submenu and fixes
post-background-action clearing. Original reports and menu-scope-sidecar.json
remain, as do initial compile/import and regression-fixture failures. No native
input/resource/startup acceptance or global PASS; preview expansion stays paused.

Final release/fmt and selection-tools-ui1793396623817639503 PASS10FB at1000×650/
800×440. Submenu painted-target and background All, exact raw marked copy to
opposite pane/bytes, empty no-F5 fallback and root0ACK verified. Same final binary
sizes1793396638238738889/search1793396640181618221 PASS. Native OS events, native
popup interaction, lifetime coverage and resource/startup NOT_MEASURED remain.

## 2026-10-30 — Keyboard navigation and viewport indexing

708Rust230/72/406 +3existing ignored; workspace all-targets Clippy-Dwarnings,
release/fmtPASS. KEYBOARD-NAVIGATION records shared key handling, Shift/Ctrl ranges,
Groups visual order, derived5endpoints and empty-frame invalidation, viewport-only
List/Groups row indexing, focus strips and partial-row reveal. Functional100k
Recorder bounds are not performance/resource evidence.

Final keyboard-nav-ui1793398675053039028PASS10FB1000×650/800×440, raw marked copy
review, age-section endpoints/ranges, Columns hierarchy, Grid/dual owner, editing
and empty guards, treemap single selection and root0ACK. Same app selection scene
1793398677117404890, sizes1793398679365350932 and search1793398681208364663PASS.
No native OS keys/clipboard/lifetime/active-helper-close/kernelD proof. Resource/
startup NOT_MEASURED freeze and all historical failures/global limits remain.

Original full-suite failures remain: space-map timeout PID-marker ENOENT and KDE
post-cancel writer accepted byte. Test-only proofs now use Rust-spawned PID plus
strict/proc absence, and parent reader FD identities respectively, with a new
deterministic pre-marker timeout case. No cause asserted, no deadlines increased,
no production transport changes or global test serialization. validation-notes.json
and target/keyboard-nav-* retain details.

## 2026-10-30 — Path text editing and compact header

719Rust231/72/416+3existingignored, Clippyworkspacealltargets-Dwarnings/release/fmt
PASS. PATH-EDITOR documents byte-preflighted scalar cursor/selection, text-only
undo/redo, byte-count IME replacement, bounded history/geometry, revision-targeted
late paste, exactly-once explicit local URI editing and literal ordinary paths.
Search transition drops editor geometry and invalidates prior callbacks.

Final path-edit-ui1793400883075229579PASS11FB1000×650/800×440/440×440, owned data
byte checks and callback0ACK; painted pointer/confirm/cancel/pencil and view-selector
controls. Native input/publication/IME/grapheme/bidi/filter-caret/context-menu and
resource/startup acceptance remain open. Same final app history1793400883822026213,
keyboard1793400885767519762, sizes1793400887737740092/search1793400889575792607PASS.
Initial width-type compile failure and pre-adaptive visual scopes preserved in
validation-notes.json. All historical FAILs remain; no global PASS/tag.

## 2026-10-30 — Recursive size/date filters

SEARCH-FACETS.md defines exact sizes (decimal/binary units, checked comparisons
and ranges) and inclusive-after/exclusive-before UTC modification boundaries.
Metadata reads are candidate-only and no-follow; size predicates include regular
files only, even under negation. Unknown metadata never matches a negative facet.
Ordinary name/ext/type searches report zero added metadata attempts. These are
functional/source properties, not performance or process-tree resource gates.
The UI requires the helper's metadata capability flag for metadata queries.

Workspace: 723 Rust tests (234 core / 73 helper / 416 UI), three existing core
ignored. Clippy workspace/all-targets -Dwarnings, release and fmt checks pass in
`target/search-facets-verified-*.log` and `target/search-facets-fmt.log`.
Resource/startup freeze and historical FAILs remain; no native input claim.

Functional scene `evidence/search/recursive-search-ui-1793401991577102975.json`:
PASS scoped, 11 real FLTK framebuffer images (1000×650 and 800×440), private
production actions, actual helper IPC. Known-size/mtime files, size exclusion,
invalid units (no running job), zero added metadata for names, raw locate,
local hidden scope, symlink cycle, 500 explicit partial, directory navigation,
source hashes and root exit0 after drain ACK. Ten logged helper PID/start
identities absent or reused (Z not accepted), not lifetime coverage. This run
finished before cancellation; active-helper cancellation/close is not established.
No native OS input/clipboard, filesystem snapshot, or consumption claim.

Same-binary folder-size calculator regression: `evidence/folder-sizes/folder-sizes-ui-1793402024183320126.json`, scoped PASS. Folder totals remain independent of apparent-file search predicates.


## 2026-10-30 — Visual search criteria

Four bounded minimum/maximum/since/before inputs combine with advanced queries.
F4/Tab/ShiftTab/Enter/Esc/CtrlF/CtrlA share policy between runtime and private
fixtures. Separate input epoch rejects a late text callback after focus changes
without invalidating an in-flight job merely on focus. Composed terms precede
advanced text to preserve an unfinished quoted name; bad ranges/dates/byte limits
retain drafts and never admit helpers. No new worker/dependency or filesystem I/O
in UI. SEARCH-FACETS.md documents UTC semantics and append-only input limitations.

727 Rust tests PASS (234 core / 73 helper / 420 UI), three pre-existing core
ignored; logs `target/search-controls-final-tests.log`. Last suite precedes only
the compact viewport drawing guard and fixture row-hit instrumentation. Final
Clippy workspace/all-targets -Dwarnings, release, fmt PASS:
`target/search-controls-viewport-{clippy,release,fmt}.log`. First Clippy misnamed
getter FAIL preserved in search-controls-clippy.log; renamed input_revision,
without suppressing warnings or changing helper generation semantics.

Final `evidence/search/search-controls-ui-1793402932050805344.json`: scoped PASS,
18 real FLTK framebuffers (1000×650/560,800×440,440×440), four painted field hits,
keyboard policy, invalid range without running helper, clear preserving query,
stale callback rejection, no clipped result targets while drawer hides retained
results. Includes previous recursive/raw/hidden/500-partial/no-match/error/locate
regression and byte hashes unchanged. Root exit0 after drain ACK;12logged helper
PID/start identities absent or reused, no Z accepted, not lifetime coverage.
Cancel-before-ACK observed, not proof of a live helper at cancel/close.

First functional PASS report1793402706606379401/17FB preserved with scoped visual
FAIL sidecar `controls-validation-notes.json`: help clipped to an unreadable8px
sliver at440. Final source omits help without54px and rows/targets without46px;
final scene verifies retained result invisibility and restoration. No native
input/OS clipboard or PSS/Swap/CPU/startup gate; freeze and prior FAILs intact.


## 2026-10-30 — Discoverable commands and existing tools

COMMANDS.md describes a 46-entry static catalog, Ctrl+Shift+P and Actions-menu
entry, independent-term fuzzy matching/aliases/shortcuts, explicit disabled reasons,
mouse/keyboard selection, context recapture and normal Action dispatch. Selection
proof is exact sparse indices/dense bits (64KiB vector budget);100k dense indices
use12504bytes of u64 backing in the structural unit fixture, NOT app memory proof.
Proof is checked on dispatch rather than per-paint; cached directory-presence
and virtual rows avoid new full-list painting scans. Query256B, source epochs and
owner/generation checks protect stale pointer/paste/context. No filesystem I/O,
new worker/timer/dependency/shell interpreter or persistent command history in this
layer. Existing operation preflight/review/queue/ACK remain authoritative.

Final735Rust PASS (234core/73helper/428UI),3pre-existing core ignored;
`target/command-palette-guard-tests.log`. Eight directed module cases include
same-count selection replacement, sparse high indices/dense100k/oversize preflight,
stale listing/context, disabled dispatch, modal file-route blocking, late paste
and reopened epochs, stale pointer activation, and notice arriving after opening.
Final Clippy workspace/all-targets-Dwarnings/release/fmt PASS in
`target/command-palette-guard-{clippy,release,fmt}.log`.

`evidence/commands/command-palette-ui-1793404858332671715.json` scoped functional
PASS:11real framebuffers1000×650/560,800×440,440×440; menu and shortcut policy,
46entries, disabled/no-match, page selection, stale context rejected until explicit
recapture, stale text callback, actual6B directory calculator, raw0xFF name and
raw0xFE destination review through the global dual-panel copy queue, empty target
before confirmation, bytes copied afterward and original byte hashes preserved,
command-to-search and root callback while catalog open→drainACK→exit0. One logged
measure_directory PID/start identity absent/reused, Znotaccepted; no lifetime
coverage or helper vivo exactly atclose. Native popup/inputSO/OSclipboard/AT-SPI
are unvalidated; this does not prove every command's end-to-end workflow.

First scene1793404366025571814/11FB preserved (functionalPASS), before final
presentation/notice-guard refinements; see validation-notes.json. Initial Clippy
FAIL and script patch exception remain in target/tool history; CAS checked epoch
and two style fixes suppress no warnings. No resource/startup measurements or
comparability claim; freeze, preview pause and historic failures remain intact.


## 2026-10-30 — Bounded live search counters

SEARCH-PROGRESS.md defines provisional counters, no early paths, final root proof
and compatibility.742Rust PASS (235core/74helper/433UI),3existing core ignored;
`target/search-progress-admission-tests.log`. Includes counter bounds/regressions,
fragmented framing/caps/EOF/summary consistency, actual helper callback/root replace,
UI latest-slot/cancel/ACK/new-intent and repeated-Enter policy. Final product suite
precedes only readonly pre-key phase logging in the private fixture; final Clippy
workspace/alltargets-Dwarnings/release/fmt pass in search-progress-keyphase-*.log.
Three release CLI contract cases PASS, hash bound in helper-search-cli-contract.json.
Legacy one-summary/raw path, streamed mode/metadata/final counts, request-command
mismatch and explicit query error are checked against the real executable.

`evidence/search/search-progress-ui-1793407162101821172.json` scopedPASS20real
FLTK framebuffers1000×650/560,800×440,440×440 with actual helper IPC.40k owned tiny
files force candidate metadata work; positive128visited observed while running,
reportNone/rowhits0 and live framebuffer. No percentage or performance timing gate.
Cancel-before-ACK→new query and raw locate, size/date/negation/hidden/500partial,
invalid/no-match, four-field controls and root0afterdrainACK all pass; byte hashes
unchanged.15loggedhelperPID/start identities absent/reused, Znotaccepted, not lifetime
coverage. Active child exactly atcancel/close is not proven. Enter phase in this
scene was completed0hits before the key, so it correctly refreshed; active Enter
not restarting is separately unit tested.

Preserved FAILs and causes in search-progress-validation-notes.json:
1.1793406487049744417: CLI whitelist omitted new command; standalone existing
release exit2/usage preserved in search-stream-admission-before-fix.json. Fixed
admission only; no raised AS96MiB/deadline/data budget. CLI contract then added.
2.1793406698673456631: positive progress/cancel worked; fixture appended ext:bin
to previous needlequery. Fixed explicit CtrlF/replace, not parser behavior.
3.1793406826248172232: positive progress then completed40622visited/0hits BEFORE
Enter, proven in preserved log. Fixture incorrectly required progress after that
key. Readonly before/after key phase logging now checks the correct branch.
Clippy manual-is-multiple-of FAIL logs preserved; fixed without allowances.

No native input/OSclipboard, snapshot of mutable tree, offline/kernel-D instant
termination, early result batches, CPU/PSS/Swap/startup acceptance or globalPASS.
Prior failures, preview pause and resource/startup freeze remain intact.

## 2026-10-30 — Batch pause and failed-only retry

748 Rust tests PASS (235 core / 74 helper / 439 UI; three pre-existing core ignored),
workspace/all-targets Clippy with warnings denied, release and fmt PASS. Logs:
`target/size-controls-footer-{tests,clippy,release}.log`. Tests include finishing
an active measurement while paused, old cancelled ACK admission barrier, retry
only errors preserving measured/cancelled rows, invalid preflight preserving pause,
navigation/Stop reset, normal Model dispatch and catalog availability, plus status
geometry at 440/800/1000. No new dependency, engine, queue or worker.

Final `evidence/folder-sizes/folder-sizes-ui-1793408838174586064.json` scoped PASS:
ten real framebuffers at 1000×650/560,800×440,440². The real batch is paused before
admission, resumes via catalog painted Execute; directory→file replacement remains
Error, restoring that directory then Retry starts exactly one new MeasureDirectory
helper. Other measured labels/selection remain; retry disables after success.
Raw filename, filtered hidden scope, 40k directory/positive partial sample, zero,
sort, Grid, refresh/recalculate, navigation and root0/drain ACK are checked. Nine
logged PID/start helper identities are absent/reused; zombie is not accepted.
This is not helper lifetime coverage or helper-alive-at-pause/close proof.

Initial `folder-sizes-ui-1793408604914973870.json` scoped functional PASS remains,
with visual FAIL because rotating help clipped paused status; original nine PNGs
and `size-controls-validation-notes.json` retained. Final suppresses help/secondary
free space while size status owns the bar, using a compact label below640px.
Earlier command regression `evidence/commands/command-palette-ui-1793408614103585218.json`
PASS11FB verifies 49 commands/disabled and modal context/raw dual copy/440 layout
on the binary before the footer-only correction; not claimed as final-binary run.
Final size scene rechecks catalog resume/retry plus their disabled states.

Handlers/painted targets and owned filesystem fixtures, not OS input/clipboard,
filesystem snapshot, kernel-D/offline, process-tree resources or startup proof.
Freeze/preview pause, historical FAILs and broader ROADMAP remain open.

## 2026-10-30 — Search input cursor, selection and bounded geometry

754Rust PASS (235core/74helper/445UI;3pre-existing core ignored), Clippy workspace
all-targets-Dwarnings, release and fmt PASS. Final logs target/query-editor-cached-
{tests,clippy}.log and query-editor-release.log. Tests cover Unicode middle edits,
selection/word deletion, per-field undo/redo, rejected1MiB preserving text/capacity/
selection/epoch/history, no-op deletion preserving results, live Fake worker not
cancelled by caret motion, delayed paste rejected on caret/result-focus change,
geometry pointer/scroll/stale stamp/cap/release and width-call reuse.

`evidence/search/search-editing-ui-1793409739193969692.json` scoped PASS24FB at
1000×650/560,800×440,440²: Unicode logical selection/Delete/undo/redo, query drag
from actual painted byte positions, private stale paste rejected, execute ext:rs
against actual helper, retained report while moving caret, Tab and Home/End input
vs result focus, criteria middle editing,4096B end/start viewport440, existing
filters/size/date/raw/hidden/500partial/locate/directory/cancel/reopen regressions.
Original fixture hashes unchanged, root0/drainACK;13logged helper PID/start absent
or reused, zombie not accepted. Not all-lifetimes/helper-alive-at-close proof.
No40k progress sampling this scene; live worker retention is a deterministic state
test, not a claim of observed native input during a real scan.

Exact draft cache and point table are bounded and release on close; fixed logical
font13 metrics reuse on unchanged draft/area and binary pointer lookup are unit
call-count/structural checks, not CPU/PSS savings. No engine/dependency/thread or
persistent text cache added. Search-only; browser filter still pending.

CJK sample has missing glyph coverage; UTF8 selected bytes are proven, native CJK
rendering is not. Auxiliary bottom shortcut text still clips in minimum window;
main fields/controls bounded. Notes in query-editing-validation-notes.json retain
these limits. No native input/clipboard/IME/grapheme/bidi/HiDPI/globalPASS/tag0.0.4.
Resource/startupNOT_MEASURED; freeze, preview pause and historical FAILs unchanged.

## 2026-10-30 — Browser filter focus and editing

758Rust PASS (235core/74helper/449UI;3pre-existing core ignored), workspace
all-targets Clippy-Dwarnings/release/fmt PASS. Final logs:
`target/filter-editor-focus-{tests,clippy,release}.log`. No serial test-thread flag.
New/updated cases: CtrlF retains query and selected text, path/treemap exit plus
type-to-filter→List, middle edit drives visible results, per-input undo/redo,
text-copy result not private-file copy, F5 suppression, huge paste preserves
capacity/epoch/selection, delayed cursor/blur paste reject, pane/tab/path focus
release, MC CtrlShiftA kept, Backspace text vs parent after Enter.

Final `evidence/usability/filter-editing-ui-1793411701021271474.json` scoped PASS:
12 real FLTK framebuffers1000×650/560,800×440,440². Owned fixture, painted byte-point
click/drag, Unicode ñ range/edit/undo/redo, private selected-text copy (no OS
publication), F2/F5/F6 no dialogs,1MiB rejection, stale caret/pane/tab paste rejected,
queries retained per pane/tab and by CtrlF, path pencil reachable with query,
4096B viewport start/end440, listing Backspace→owned parent then root return.
Existing name/raw/filter/view/MC selection and real helper sizes8B/5entries→12B/6
also pass. Original file hashes intact, root0 after drainACK;2logged helper
PID/start identities absent/reused, zombie not accepted. No helper lifetime or
helper-alive-at-close proof.

Same final binary recursive search regression
`evidence/search/search-editing-ui-1793411753809197070.json` PASS24FB,13logged
helper identities cleaned (same strict rule), actual name/size/date/raw/hidden/
500partial/locate/directory, input/criteria editing and delayed paste regressions.
No40k progress sampling or performance comparison.

Originals preserved with `filter-editing-validation-notes.json`:
-1793411028309635495 FAIL fixture expected oversize error cleared by SelectAll;
 explicit known-error state now checked and next valid edit must clear it.
-1793411079837629215 FAIL fixture omitted existing FILES_TEST_TABS opt-in;
 no unknown command accepted; flag enabled only for explicit editing fixture.
-1793411105881791432 FAIL old1000×650 dimension after explicit1000×560 resize;
 actual dimensions now asserted per invocation, no product resize changes.
-1793411146396526185 functionalPASS12FB with visual guide clipped at440; final
 short guide and contextual inspector hint inspected. Existing informational
 Panel cerrado toast still clips at minimum width; remains pending.

Initial workspace failure retained: CtrlF-erases-query assertion intentionally
replaced with preserve/select plus path/treemap assertions. Initial check warnings
(fetch_update deprecation, unused append) removed with existing CAS convention and
Cursor-backed retained preflight tests. No new dependency/engine/thread/timer or
session wire. Active editor bounds/history/owner/epochs in FILTER-EDITING.md.

Foreign Cargo observed during final unit run; own Clippy mistakenly launched after
printing a new competitor, finished clean. Mistake recorded, no foreign signal.
Final release uses an idle-snapshot launch guard. No resource/startup gates or
comparisons; timings incidental, freeze/new-preview pause and historical FAILs
remain. No native input/OS clipboard/IME/grapheme/bidi/HiDPI/kernelD/globalPASS/tag.

## 2026-10-30 — Visible folder-size queue manager

Actions, Ctrl+Alt+Space and the new `sizes-manager` command open a modal over the
same current-root queue. Mouse and keyboard select rows, pause/resume admission,
cancel/retry one attempt, stop pending work, retry only errors, or enqueue visible
directories. Wrapped diagnostic details expose available errors plus escaped raw
root/name. Closing this tool does not stop calculations. See SIZE-MANAGER.md.

Final logs `target/size-panel-wording-tests.log` (764 Rust:235 core/74 helper/455 UI,
3 core ignored), `size-panel-wording-clippy.log` (workspace/all-targets -D warnings),
`size-panel-wording-release.log` and fmt/check pass. Initial Clippy style failures
(collapsible_if, fixture after test module) preserved and fixed without suppressions.
Queue IDs are monotonic per model and change on each attempt. Unit tests cover
cancel→retry before old ACK, stale completion, old row/panel controls and context
invalidation; active filename clone removed. These are structural bounds, not a
measured resource saving. Error UTF8 retained512B with explicit ellipsis on truncation.

`evidence/folder-sizes/size-manager-ui-1793415512530551936.json` scoped PASS:
14 real FLTK framebuffers1000×650/560,800×440,440². Paused raw row cancelled/retried
with fresh ID, obsolete Cancel rejected, browser file focus retained; actual helper
error after directory replaced by a file, readable detail, restore/retry exactly
one error while good observations remain, successful retry disabled. Async refresh
invalidates row controls; explicit recapture plus a new paused batch rejects old
panel Stop. All enabled controls fit440. Original size/sort/Grid/refresh/navigation
regressions pass, root0 after drainACK.10 logged helper PID/start identities gone
or reused; original Z not accepted, not lifetime coverage. Positive partial progress
was not observed; active cancellation/ACK is unit proof, not this runtime scene.

Same final binary `evidence/commands/command-palette-ui-1793415519946934466.json`
scoped PASS11FB:50 commands, search/aliases/disabled/actions, raw marks, reviewed
copy, actual folder calculator, search transition and root0ACK. Prior manager scene
1793414496788583300 PASS14FB retained; final rerun follows error/count wording changes,
including generic helper directory error instead of misleading “vista previa”.
No original40k full-tree hash or helper-alive-exact-close claim. Private handlers,
owned filesystem fixtures and real framebuffer; no native input/OS clipboard,
AT-SPI/HiDPI/offline/kernelD/snapshot proof. No new helper/engine/dependency/thread,
cache/session wire/timer. Foreign Cargo launch refusal recorded; own final checks
admitted by idle-snapshot guard, not proof of no later overlap. Resource/startup
NOT_MEASURED freeze, new-preview pause and historical FAILs remain. No global PASS/tag.

Local source checkpoint `4eff560` installed with report
`evidence/0.0.4/installed-checkpoint-1793415566779715821.json`.
All five installed artifacts and five backups independently rehashed exact.
App `cd616a62c0f3bb829604d47284ded5f2b4f80aeb6a7bf95d4d909760f0c03826`,
helper `4d76b4843506ffb11b7baa6b503ea8d67b2029cd264862c18219997eed5dafed`,
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user authorized install only; no source publication, global configuration or
other windows controlled.

## 2026-10-31 — MC operation strip

`OPERATION-BAR.md`: Preferences/Ctrl+Alt+B/catalog toggle, persisted through the
existing atomic preference snapshot/ACK pipeline; compact browser default off.
Eight existing actions with F-key/Nerd labels, current owner/generation on click,
no captured filename payload. Disabled input clicks are handled before editor
blur; enabled Copy/Move still review/preflight through the existing queue.

`target/operation-bar-final-tests.log`:769 Rust (237core/74helper/458UI),3core
ignored, PASS. Follow-up `operation-bar-geometry-tests.log`3 PASS after stronger
end-focus assertions for List/Grid/dual and MC-notice clearing; no algorithm or
operation-engine change after that full suite. `operation-bar-clippy.log`
workspace/all-targets -Dwarnings PASS. No suppressed warnings. Preference tests
cover boolean/default, unrelated fields and in-flight false→true coalescing.
Stale pane/generation/invalid index and dim text-focus/no-selection controls cannot
launch; geometry test covers440/800/1000 and clipped entry targets above the strip.

`target/operation-bar-release.log` and fmt/check PASS. Final
`evidence/commands/operation-bar-ui-1793416379455845906.json` scoped PASS20 real FLTK
framebuffers1000×650/560,800×440,440². Eight painted buttons; no-selection Copy
explains without a form, text-focused Copy preserves query/caret/editor and cannot
launch, Rename/New-folder reviews cancel, Help opens/closes, minimum controls fit.
Dual Move reviews exact raw sources/destination and cancel leaves both intact;
bar Copy then commits expected bytes through the existing global queue. Existing
51-command catalog, disabled/stale/paste guards, calculator6B and search transition
regressions pass. Same owned config reopens in a second process with bar enabled,
then disabled persists; unrelated fixture field and hidden flag intact. Both roots
exit0 after ACK. One logged helper PID/start identity absent/reused, original Z
not accepted; no helper lifetime/helper-alive-exact-close proof.

Images inspected at1000/440 and text-focused state. No new preview backend or
feature expansion. Widget handler/pointer fixture, not native OS input/AT-SPI/
clipboard/HiDPI/interoperability. No additional thread/timer/dependency/session
wire or filename cache; fixed eight buttons per paint. Build idle-snapshot guard
admitted own serial Cargo; no global performance comparison. Resource/startup
NOT_MEASURED freeze and historical FAILs stay; no global PASS or release tag.

Source `c114f9a` installed via
`evidence/0.0.4/installed-checkpoint-1793416459957302416.json`; all five installed
artifacts and all five backups independently rehashed exact. App
`9c2d9a9b3b13af1d029098da64762f29988c382a2fd0e9d1670e16b482f7bdc0`, helper
`4d76b4843506ffb11b7baa6b503ea8d67b2029cd264862c18219997eed5dafed`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user install only, no source push/global config/unrelated window control.

## 2026-10-31 — Literal content search

CONTENT-SEARCH.md defines the UTF8/no-NUL regular-file scope, exact literal1024B,
1MiB/file and64MiB total reads,32KiB scratch/KMP prefix table, metadata from the
read FD and post-read FD/name revision. Binary prefix matches cannot publish.
Limits/time/read/change failures are partial; binaries are explicit scope
exclusions. Every hit carries first LF-line/byte, no snippets/index/cache. Same
SearchStream command/supervisor/progress/last-intent/reap admission, new optional
report counters/match fields checked against request mode. Name-only no-read path
unchanged. Guided content/name drafts and all six input editors are bounded.

`target/content-search-complete-tests.log`:780Rust (240/79/461;3core ignored) PASS.
Tests include quoted/Unicode queries, boundary-spanning UTF8/KMP overlap, late
binary/invalid UTF8 rejection, per-file/total/time bounds, real FD replacement/
in-place proof, metadata AND raw/hidden/link exclusions, forged report mode/
counters, independent guided draft/escaping/undo/preflight and latest-intent ACK.
`content-search-final-clippy.log` workspace/all-targets-Dwarnings PASS; final
release/fmt PASS. Last suite precedes only equivalent Clippy if-collapse and UI
labels/hint/scope presentation; no operation-engine changes after it.

Four real CLI/wire tests `evidence/search/content-helper-contract-1793418304103621516.json`
PASS on the exact final helper hash. Final
`evidence/search/content-search-ui-1793418723201526437.json` PASS24FB1000/800/440;
raw plain text with quotes/backslash, name+size AND, pointer field5, stale mode
paste rejection, oversize retention, binary2/limit1/line2-byte5, narrowed complete,
advanced raw hit and invalid empty criterion, all existing recursive/criteria
regressions. Original source hashes unchanged, root0 after drainACK;16 logged
PID/start helpers absent/reused, original Z not accepted. Runtime cancel finished
before request; no active-child cancellation/close or full lifetime proof.

`content-search-validation-notes.json` preserves initial E0283 test reader, missing
optional-field fixture, incorrect fake successor-ACK expectation and Clippy style
failures. First24FB functionalPASS report1793418314346868379 remains before legacy
minimum footer hint/Archivos count clarification. Final minimum footer inspected,
Nombres/UTF8 scope explicit; drawer with no visible rows has disabled result targets.
No native OS input/clipboard/AT-SPI/HiDPI/mounted-FS kernelD/other-platform/global
resource claim. No new dependency/service/thread per file or persistent content
cache. Cargo idle-snapshot admitted serial own checks; no performance comparison.
Resource/startup NOT_MEASURED freeze, new-preview pause and historical FAILs stay.

Source `fcfd7ce` installed through
`evidence/0.0.4/installed-checkpoint-1793418877381327408.json`. Five installed and
five backup hashes independently match. App
`eeeba274d8ef1c5e5c57368aeda6223ff9d40d7806e3d80a433300fa14b91f47`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install only; no source publication/global config/window control.


## 2026-10-31 — Favourites/places hub (scoped)

793Rust244core/79helper/470UI PASS,3 existing ignored. Workspace/all-targets
Clippy-Dwarnings/release/fmt PASS on final source. Final
`evidence/bookmarks/bookmarks-ui-1793422800552758666.json` PASS11FB/four normal
process exits0 after ACK: raw paths/addition merge/label expected tuple/remove
folder intact/new ID/old stamp/440 close resolution/nonblocking lock/captured
retry after navigation/current directory replaced by file/new-process restore/
future format preservation.11 actual PNG inspected, all IHDR dimensions match.
Same-binary command regression1793422810269958601 PASS11FB/53commands.
See BOOKMARKS.md and bookmarks-validation-notes.json for exact bounds/scope/logs.
Initial private scene marker expectation and subsequent actual stale-current-path
load defect remain FAIL with sidecars; fixed checker and product regression
respectively. CtrlAltD close and stale Edit controls regressions also PASS.
No real input/clipboard/native accessibility/kernelD/other-platform/lifetime or
powerloss acceptance; no measured PSS/Swap/CPU/startup. Freeze/new-preview pause,
historical failures and broad goal stay open. No new watcher/service/dependency
or claim of performance gains from source caps.

Source `1b448f0` installed through
`evidence/0.0.4/installed-checkpoint-1793422888499506241.json`. Five installed
artifacts and five backups independently rehashed exact. App
`12d5d44825a0ea647f96833595ec309bfc6cc3a1d098b4a7106557eca23a61ad`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Per-user install authorized, no source publication/global config/window control.


## 2026-10-31 — Copy/Move destination editor

798Rust244/79/475+3ignored, workspace/all-targets Clippy-Dwarnings, release/fmt PASS
(`target/destination-editor-captured-*`). Final scene
`evidence/destination/destination-ui-1793423889017087207.json` scopedPASS9FB1000/440:
Unicode path edits/undo, stale caret paste rejection, pointer end click,1MiB
preflight, actual copy18B/source hash intact, Move review remains on caret motion
but edit discards, cancel/raw protection/full replacement,4096B viewport at440,
root0 after ACK. NinePNG inspected; regular informational lines can clip, action
controls complete. Details and exact proof limits in DESTINATION-EDITOR.md and
`evidence/destination/destination-validation-notes.json`. Earlier9FB PASS retained
on preceding binary; final source adds control escaping/raw-display cap tests.
No native input/clipboard/AT-SPI/HiDPI/lifetimes/helper-active-close/resource proof,
no new thread/dependency/operation engine. Move runtime is review/cancel only.
General conflict policies remain ROADMAP. Freeze/previews/historical FAIL intact.

Source `9e850a5` installed through
`evidence/0.0.4/installed-checkpoint-1793423949236404441.json`. Five installed
artifacts and five backups independently rehashed exact. App
`7cadf781a7a08c5032c918bf58e8b80008e85ce367ced456375b9eeeb6e8ab1a`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install; no source push/global config/window control.


## 2026-10-31 — Copy conflict Skip policy

806Rust249core/79helper/478UI+3ignored PASS (`copy-conflicts-report-tests.log`),
final Clippyworkspacealltargets-Dwarnings/release/fmt PASS (`copy-conflicts-notice-*`).
Full suite before only final compact geometry/wording. Core initial/late collision,
uncertain cleanup actor data preserved, source errors, no directory merge,
entry/detail caps; queue capacity fixture shares completed+skipped payload cap,
keeps4MiB/2MiB headroom. Report totals survive diagnostic truncation.

`evidence/conflicts/copy-conflicts-ui-1793425513683762662.json` PASS8FB1000/440,
default Stop1publish thenerror, explicit Skip2publish/2skip, raw copied bytes,
symlink/directory/actor sentinel intact, fresh defaultfalse, mousecheckbox and
all4skip/0publish at440, Move policy blocked, source hashes intact/no stage,
root0drainACK. All8 PNG inspected. Existing pan/scroll needed for long detail
paths. Original1793425188533528112 functionalPASS/compactvisualFAIL preserved
with sidecar; fix actual inspector visibility for toast width/counts-first.

Core-only publication races are real owned-FS fixtures, not native UI/protocol or
powerloss proof. E0425 initial edit corrected/log retained. Foreign Cargo guarded,
not touched. No new benchmark: resource/startup NOT_MEASURED; historical FAIL,
new-preview pause/global goal remain. COPY-CONFLICTS.md records limits/pending.

Source `fdc8efa` installed through
`evidence/0.0.4/installed-checkpoint-1793425631915265384.json`. Five installed
artifacts and five backups independently rehashed exact. App
`ae7461f282a338ec73b6a0dc00c3436e83a8315535f82fa3fbbd894f09cd473f`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install, no source push/global config/window control.


## 2026-10-31 — Alternate Copy target names

818Rust252core/79helper/487UI+3existingignored PASS; final workspace all-target
Clippy-Dwarnings/release/fmt PASS (target/copy-names-counter-* logs).
Selected Copy validates actual opened root FD against captured Source before
staging; root-pinned symlink read and data FD checks, per-root name relation.
Core tests reject source replaced before open and later root replaced during
first-root progress. One extra temporary root FD, no new dependency/thread.
Final evidence/copy-names/copy-names-ui-1793429532169892092.json PASS9FB,
1000x650/440x440: five remapped roots, unchanged source bytes, directory/link,
raw/Unicode/literal-backslash, duplicate-plan rejection, occupied preservation,
stale paste, private clipboard and same-folder duplicate, root0drainACK.
All nine images inspected; source paths may clip, editor scroll remains usable.
First functionalPASS/visualFAIL report and sidecar preserved: pending fifth
name missing from title, fixed/tested. Initial E0282/warnings logs retained.
Incidental footer53.7MiB original/44.4MiB final is NOT tree+Swap/resource proof;
no causal/comparison claim. Resources/startup NOT_MEASURED freeze; native input,
MIME clipboard, full O03 and broad goal remain open. See COPY-NAMES.md.

Source32c77fa installed via evidence/0.0.4/installed-checkpoint-1793429595820606375.json. Five installed+five backups independently rehashed exact; app25c0ec3bcfa9/helper7e7dd8402607/HTML85ea1b8009ef. Per-user authorized, no source push/global configuration.


## 2026-10-31 — Folder-size manager display order

820Rust252core/79helper/489UI PASS,3 existing core ignored; workspace tests log
size-order-workspace-tests.log. Clippy all-targets-Dwarnings/release/fmt PASS,
final fixture-only Home action compiled by size-order-final-clippy/release.
Button/S cycles Queue/Largest/Name; bounded128 presentation indices only, queue
never reordered; selected record stays fixed while navigation follows display.
Pure tests keep partial/error unknowns after complete totals (zero is measured),
ties raw names; model test Delete targets original record after sorting.
Final evidence/folder-sizes/size-manager-ui-1793429969089468901.json PASS17FB,
all5 displayed IDs/order asserted; Home selects largest40kB, next9B/8B; NameEnd
selects E7B and opens corresponding detail. Existing raw cancel/retry, stale
controls, recapture, error/retry-only, hidden/filter, Grid and recalculation pass.
Three final PNG inspected (largest Home, Name detail,440 controls); all17 have
framebuffer dimensions checked by script. root0drainACK,9logged helperPID/start
absent/reused, Z not accepted; no lifetime/active-close/native-input proof.
First16FBPASS retained, final adds Home evidence (no production behavior change).
Incidental final440 footer50.8MiB and previous51.0MiB are main-only, exceed ideal
at face value; NOT tree+Swap proof/comparison. Resources/startup NOT_MEASURED under
freeze. No new engine/thread/dependency; global goal and historical FAIL remain.

Sourcecca8f3f installed via evidence/0.0.4/installed-checkpoint-1793430001081242152.json;5installed+5backup independently rehashed exact. App51c2cdd4feb7/helper7e7dd8402607/HTML85ea1b8009ef. Authorized per-user, no global config/sourcepush.


## 2026-10-31 — Omnibar command/place catalogs

823Rust252core/79helper/492UI+3existingignored PASS, workspaceClippyalltargets
-Dwarnings/release/fmt PASS (target/omni-catalog-*). CtrlL path Enter/commit
routes >query to commands and :query to places, query256B/control preflight before
clone/end-edit. No implicit execute/jump; same contexts/disabled gates/AuxIo.
Normal paths untouched; ./ prefix names remain literal. End-edit invalidates late
paste. Unit tests cover no execution/navigation/selection change, empty query,
UTF8oversize/control rejection retaining editor, literal paths and stale paste.
Final evidence/omnibar-catalogs/omnibar-catalogs-ui-1793430313608194102.json
PASS5FB1000/440; all5 inspected: >grid catalog/no action then explicitview-grid,
257Breject/draft/root preserved, :Destino filtered existing place then explicit
jump, actual ./ >/: directories navigated, catalog440, sourcehash/root0drainACK.
Private place list belongs only to fixture; no nativeinput/clipboard/OSprotocol,
new scanner/worker/dependency or full lifetime proof. Resources/startupNOT_MEASURED
freeze, previewsPAUSA/globalFAIL/goal remain. See OMNIBAR-CATALOGS.md.

Sourcee8302e4 installed via evidence/0.0.4/installed-checkpoint-1793430353104088658.json.5installed+5backup independently rehashed exact; appfb0eefd6fc41/helper7e7dd8402607/HTML85ea1b8009ef. Authorized per-user, no global config/sourcepush.


## 2026-10-31 — Incremental search scanner foundation WIP

New borrowed cumulative-prefix API shares scanner; optional first-hit checkpoint
without waiting128 visits, subsequent bounded cadence; no paths cloned by API.
3newtests exercise first1visit hit/raw prefix consistency, consumer BrokenPipe
stop, root replaced after provisional prefix => terminal error/original preserved.
10scanner tests and82helper tests PASS before last wrapper-only wiring. Initial
ClippyPASS before eager split; final ClippyFAIL unusedAPI preserved, corrected by
routing old counters wrapper through API with eager=false, no suppression.
Final formatter/tests/Clippy rerun NOT_STARTED: guard observed foreignCargoPID
2652552/start10686550 cwd/tmp/relb alive, untouched. Current WIP NOT accepted,
no release/capture/install; installed e8302e4/c063165 unchanged.
SEARCH-BATCHES.md describes wire/reader/UI/memory/generation contracts still
pending. Counters remain only installed incremental feature; visible rows NOT
implemented. FreezePSSSwapCPUstartup and previewsPAUSA unchanged, no globalPASS.


## 2026-10-31 — Incremental search bounded candidates foundation

ForeignCargo exited; own guard admitted fmt/workspace tests/Clippy. Full suite
830PASS256core/82helper/492UI+3existingignored, plus6codec targeted PASS including
2cases added after full suite. Workspace alltargetsClippy-Dwarnings/fmt PASS.
Actual scanner first-hit/prefix/rootreplace/BrokenPipe tests now compiled with
corrected counter wrapper; originalClippyunusedAPI101 and guard75 preserved.
Codec32hits/500total/256KiB pathcapacity, offsets/duplicates/malformedpaths/content
bounds reject atomically; bounded serde does not materialize33rdHit. RawUnix
roundtrip, equal-count path/kind/content-position mismatches reject; exact final
reconciliation accounts sorting, consumes candidates, no hash-only acceptance.
No new wire command/reader integration/UI/release/captures/install. This is a
foundation, NOT visible incremental results nor resource/globalPASS. Installed
checkpoint remains e8302e4/c063165; freeze and previewsPAUSA unchanged. See
SEARCH-BATCHES.md, target/search-batch-* logs.


## 2026-10-31 — Incremental search visible

837Rust258core/82helper/497UI+3existingignored PASS, workspaceClippyalltargets
-Dwarnings/release/fmt PASS. Fullsuite before fixture rawJSON/final wording only;
finalClippy/release compile them. Source legacy request/counters preserved, new
SearchBatches v1/32-entry frames borrowed helper/strict Candidates exactfinal.
One latestArc prefix slot in same reader; revision/atomicerror hides beforeACK,
finalselection byrawpath; no new engine/thread/dependency/persistent cache.
Final evidence/search-batches/search-batches-ui-1793432390766040716.json PASS7FB,
1000/440 all7 inspected. Realhelper held by isolatedfixture gate<=5s, livePID/start
whileprovisional/reportNone,40final exact/selection, cancel/rootreplace/newquery,
rawcontent440/provisionalLocate, sourcehash/root0ACK/6loggedhelpersgone-reusedZno.
This is artificialhold proof, NOTnatural latency/kernelD/lifetime/native input.
Samebinary content-search-ui-1793432406201870889.json PASS17FB/14loggedhelpers
strictcleanup/root0ACK;4PNG inspected (partialUTF8/complete/rawLocate/500limit).
Originals1793431914206752394/1793432088913319951PASS priorwording retained;
FAIL1793431925772843277 visiblepaintpredicate and1793432050269332012 legacy-only
cleanupmatcher preserved+sidecars; full originalassertion now waited/no product
navigation change. HelperSerdecompileFAIL fixed by coreFrame, no newpackage.
ForeignCargo guards75 intact, no processcontrolled. Resources/startupNOT_MEASURED,
footerNOtreeproof, previewsPAUSA and historical/globalFAIL remain. SEARCH-BATCHES
records localpath retention<=1MiB UI plus256KiBhelper, arrays/wire extra; NOTPSSgate.

Source90169c2 installed via evidence/0.0.4/installed-checkpoint-1793432548607567862.json.5installed+5backup independently rehashed exact. Appddf4b8f65a15/helper5d9239389b82/HTML85ea1b8009ef. Per-user authorized, no source push/global configuration/window control.


## 2026-10-31 — Content-search fragments

843Rust260core/85helper/498UI+3existingignored, Clippyworkspacealltargets-Dwarnings,
release/fmt PASS (target/search-snippet-*). Header plus optional bounded256B
snippet/UTF8marks/ellipsisflags, same single KMP/UTF8whole bounded read and FD/name
postread proof; no full-fileString/reopen/newengine/thread/dependency/cache.
Helper1088B previous tail+256B raw/normalized; arrays/wire separate. Per500 hits
adds128000B snippet capacity/population,4 UI populations<=512000B on top of path
budgets. NOT measured process tree or idealbudget proof. Worst JSON escaping test
keeps existing2MiBline/4MiBtotal wire caps. Bad positions/control/oversize reject.
Samebinary evidence/search/content-search-ui-1793434023389723179.json PASS20FB:
quote/slash literal, binary/invalid/limit no snippet, expected bytes/line/text marks,
Unicode context1000/440,1024Bliteral fullmatch/clipped256Bfragment, raw Locate,
old filters/500cap/errors, sourcehash/root0ACK/16loggedhelpersgone-reusedZno.
4content PNG inspected. evidence/search-batches/search-batches-ui-1793434087267418740
PASS7FB: provisional raw snippet+mark verified before terminal under realhelper
fixturehold, cancel/rootreplace/latest/Locate,root0ACK/6loggedidentitiesstrictgone;
compactSnippet PNG inspected. All27 dimensions checked; not all PNG visually read.
Original E0282 Optioninference fixed explicitMatch, E0063 old fixture field supplied;
edit-script ValueError left UI unchanged before corrected patch; logs preserved.
No OSinput/clipboard/lifetimes/native2x/latency/kernelD/resource/startup proof.
Resources/startup NOT_MEASURED freeze, previewsPAUSA/globalFAIL/goal open.

Source04e11bb installed via evidence/0.0.4/installed-checkpoint-1793434441549030735.json.5installed+5backup independently rehashed exact. Appadcdaac71a20/helper36e9302c3ba1/HTML85ea1b8009ef. Authorized per-user, no global config/source push or other window control.

## 2026-10-31 — Copy numeric Keep Both

853 Rust PASS269core/85helper/499UI,3 existing core ignored; workspace/all-targets
Clippy-Dwarnings/release/fmt PASS. Logs target/copy-keep-both-final-*.log.
Core publication-race/cancel/source-change/exhaustion and name-budget tests PASS;
UI evidence copy-keep-both-ui1793436602618559696 PASS7FB1000×650/440², all7
inspected; conflicts regression1793436615203947340 PASS8FB same binary. Two
independent roots normal0 after queue drainACK, no native input or helper-lifetime
proof. Original keep-both1793436577392527746 FAIL preserved+sidecar (harness
dismiss command on toast with no such hit); corrected report path only, no
product rerun patch. Paths/details can need pan/scroll, controls/counts visible.
No PSS/Swap/CPU/startup experiments; historical failures remain unchanged.
See COPY-KEEP-BOTH.md for publication/source-check limits. Source8623abd installed
with five artifacts and five backups independently rehashed exact; report
installed-checkpoint-1793436673368581944.json. No global acceptance.

## 2026-10-31 — Open selected folder from sizes manager

855Rust269core/85helper/501UI PASS,3ignored; Clippyworkspace/alltargets-Dwarnings/
release/fmt PASS (target/sizes-open-* logs). Original directed7PASS/1FAIL retained:
fixture forgot reselect after recapture, corrected without weakening guard.
sizes-open-ui1793437301816876363 PASS5FB1000×650/440²,all5inspected; real helper
32/9/8/0B, keyboardlargest/rawmouseopen, historyback/stalecontrolsdisabled, hashes
intact/root0drainACK. Original1793437284794884475 FAIL+sidecar: missing Actions
menu before painted Sizes hit, fixed scene route only. No nativeinput/AT-SPI/
clipboard/HiDPI/helperidentity-lifetimes/activechildclose/globalacceptance.
PSS/Swap/CPU/startupNOT_MEASUREDfreeze; incidentalfooter not treegate.
Sourcef58652b installed, report1793437358813676667; five installed+fivebackups
independently rehashed exact. Scope/limits in SIZE-MANAGER.md.

## 2026-10-31 — Search display order

858Rust269core/85helper/504UI PASS,3ignored; Clippyworkspace/alltargets-Dwarnings/
release/fmt PASS (target/search-order-*.log). search-order1793437888618938167
PASS5FB1000×650/440²(all5inspected), provisionalName→final42selectedpath retained,
natural2before10/pathties/Kindgrouping/visualkeys/rawFF→Locate/root0ACK/1logged
helperPIDstartabsent-reused,Znotaccepted. Regressionbatches1793437899122910460
PASS7FB/6helpersgone/root0ACK;2regression440PNG inspected. No nativeinput/
protocol/clipboard/wholelifetimes/exactactiveclose/global/resources proof.
Original1793437832543422732 FAIL+sidecar retained: unpaintedstate accepted by
harness; waitrowhits+indexcount now,no product/deadlinechange. Naturalkeys extra
transient and cachedindices no PSS/speedclaim. Freeze intact. Source64e18d3 installed; report1793437954301097754, five installed
and five backups independently SHA256 rehashed exact.

## 2026-10-31 — SFTP foundation WIP, no visible feature

SFTP-MOUNT-CONTRACT.md and evidence/sftp-foundation/foundation.json: core278PASS
(+3ignored), 6target/3mounttablefixtures; Clippycorealltargets-Dwarnings/fmtPASS.
Own Linux mountinfo read-only example parsed31records, no paths/sources printed
or mounts changed. Exactlog/sourcehashes in evidence. Empty/truncated/duplicate/
stacked/substituted mount records rejected; source/user/port/argv bounded before
owned fields. NO process argv dispatch/SSH/FUSE/authentication/UI/nativeinput/
resource/startup/install acceptance. sshfs still absent; no global package/config/
keys/knownhosts/service changes. Installed SearchOrder64e18d3/e236e1a intact.
Worker/lifecycle/reviewUI/realfixture pending; N01 not complete.

## 2026-10-31 — SFTP preparation only

873Rust278core/85helper/510UI PASS(+3ignored), Clippyworkspacealltargets-Dwarnings/
release/fmt PASS; final logs target/sftp-form-cas-workspace-tests/final-clippy/
cas-fmt/release.log. Original helpSFTPban failure and deprecatedfetch_update
ClippyFAIL preserved; revisedhelp admitsPreparationonly, CASusednosuppression.
sftp-preparation-ui1793440044492647516 PASS5FB1000×650/440²(all5inspected),
actualAuxIo read-onlytool/FUSEaccessprobe/missingSSHFS/hostportvalidation/literal
path/cursorundo/stalepaste-oldstamp/Actions+catalog/reopenempty/sourceunchanged/
root0mutationdrainACK. mount_gateNOT_IMPLEMENTED; no SSH/auth/FUSE/keys/config/
knownhosts/nativeinput/clipboardbytes/readonlysyscalldrain/lifetimes/resources/
startup/globalacceptance. Freeze/previewsPAUSA unchanged. Source86bf3ff preparation installed, report
1793440285730101786;five installed+five backups independently SHA256exact.

## 2026-10-31 — Native SFTP laboratory, Files compatibility FAIL

SFTP-NATIVE-MATRIX.md +evidence/sftp-native/*: actual loopback OpenSSHinternal-
sftp/privatefixturekeys-knownhost/SSHFS3.7.6privatebuild/kernelFUSE. Native
mount/read-raw-link/externalcat/write/busy-unmount/unmountabsent/SSHFS0 PASS.
Final1793442486810260624 overallFAIL/exit1: CorecopyFAILerrno22/0published/source
intact/stageclean; directrenameNOREPLACEerrno22; linkexclusiverejectsoccupiedbut
virtualinodealiasesdiffer.6loggedPIDstartgone-reused strictZfalse,0identityerrors,
notwholelifetimes. Earlierreports retained; genericcopyUNSUPPORTEDclassification
refinedtoFAIL andstrictscriptexitfixedwithsidecar/documentation. No production
worker/UI/newrelease/install/globalpackage/config change or proofweakening. Core
alltargetsClippy-Dwarnings/fmt/examplesbuildPASS; resource/startupNOT_MEASURED.
Installedpreparation86bf3ff/f0df843 remains. NativeGUI/input/daemon/offline/kernelD/
powerloss/hostilewriters/fullN01/globalPASS pending.

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

### 2026-10-31 — Held SFTP mountpoint provenance

sftp_mount_point creates exclusive 128bit-random private directories under pinned
0700/euid parent, no symlink components/no chmod/no Drop cleanup. Five new tests:
empty/drop-retention, permissions/symlinks, point substitution, parent substitution,
nonempty/synthetic mount coverage. 890 Rust PASS294/85/511+3existingignored:
target/sftp-point-workspace-tests.log; Clippyalltargets-Dwarnings and finalfmt PASS.
Core example holds FDs through real SSHFS: native1793444992430070322 pointgatePASS,
empty→mountedinspectionreject→unmountedidentity/empty,7loggedidentitiesgone,
0readerrors/survivors. DefaultcopyFAIL/overallFAIL retained; visibleadapterPASS.
No processworker/UI integration/newrelease/install/resources/startup acceptance.

### 2026-10-31 — Actual SFTP process engine, not product activation

895Rust299core/85helper/511UI+3ignored PASS (sftp-connection-accepted-tests2.log),
all-targets Clippy/release/fmt PASS. Core Connection spawns SSHFS/ordinary unmount,
setsid/no Drop kill/unmount, sealed8KiB anonymous stderr/bounded4KiB visible text
with truncation, exact mount FD initial serving proof and explicit disconnect.
Native current-source679630 engine/busy/retry PASS;679659 owner-exit/new remote
file read PASS;679678 strict wrong-hostkey reject/no Ready/cleanup PASS.12/7/7
logged identities gone/0errors/survivors, no whole-lifetime guarantee. Default
copyFAIL/overallFAIL preserved. Original wrongkey namespace-onlyReady588098 FAIL
and cleanup-harness611859 FAIL remain, with scope sidecars. Header mount presence
was insufficient; now initial statx/getdents + pre/post identity and process recheck.
No durable store/restart/GUI/AuxIo/new install/inputSO/resources/startup acceptance.

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

### 2026-10-31 — Sizes subtotal and captured report

942 Rust PASS (324core/85helper/533UI;3 existing core ignored), workspace
Clippy all-targets-Dwarnings/release/fmt PASS. Final workspace suite precedes only
a stale-context help label; final Clippy/release/fmt and framebuffer scene compile
and cover that label. Logs/checks in `evidence/folder-sizes/report-validation/`.
Tests cover unknown vs measured zero, running not a total, checked overflow,
UTF8/raw/control escaping, explicit64KiB truncation, layout reuse/resize/4096line
cap, frozen capture across late completion, explicit refresh and stale context.

`size-manager-ui-1793462910855146190.json` scoped PASS:21real FLTK framebuffers,
real files-helper IPC, five rows including raw0xFF, error excluded from subtotal
40017B and retried-only result40024B, measured empty0B, paused unknown not0,
copy-request handler only, F5/Home/End/I/Escape, no extra report helper, stale
Copy/Refresh disabled and mouse Recapture restored;440² controls. Final minimum
and stale PNGs inspected. Root0 after callback/drain ACK;9logged PID/start helpers
absent/reused, Z rejected; not complete lifetime coverage or helper-alive exact
close. Native input/OS shortcut/AT-SPI/clipboard delivery remain UNVALIDATED.

First compile E0716 and first UI1793462686342799927 FAIL retained. The latter
correctly found missing mouse Recapture on a stale report; production footer now
offers Current context, without weakening guards/deadline. Sidecar records fix.
Interim UI1793462811407106105 PASS retained; final rerun follows only accurate
stale Escape wording. No repeated search for resource PASS.

PSS/Swap/CPU/startup NOT_MEASURED under freeze. Incidental main-process footer
55.2MiB at440² is above the ideal at face value and is not tree+Swap proof or an
accepted budget. No resource/speed improvement claimed from layout caching/caps.
Historical resource/startup/clipboard failures and full ROADMAP remain open;
new previews paused. No global PASS, release tag, publication or OS config change.

Size-report source1885b60 installed through
`evidence/0.0.4/installed-checkpoint-1793462954825746258.json`.
Five installed files and five backups independently rehashed exact in
`evidence/folder-sizes/report-validation/install-verification.json`.
App13935bb18583ace4e3f1c2a230ccf8915e08e463a6ff9a168326e1e3d44891ee,
helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd,
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Authorized local install only; no source push/tag or running-window control.

### 2026-10-31 — Background tabs from search

945Rust324/85/536(+3existingcoreignored), workspace Clippyalltargets-Dwarnings/
release/fmt PASS. Logs/checks in `evidence/search-order/background-validation`.
New preflight includes empty-history reserve; joint closed-tab/other-pane and
8tab limits reject without partial insertion. Model coverage includes source
filter/selection/generation, raw focus restore, provisional directory/no probe,
empty results/invalid leaf/path/close and notice lifecycle. Initial test falsely
expected empty History charge0; assertion and previously missing source history
reserve corrected, originalFAIL preserved with sidecar.

`search-background-ui-1793463564614078345.json` scopedPASS10real FLTK framebuffer
PNGs: actual search_batches, provisional source/query/result/helpercount unchanged
on admission, raw0xFF parent selection after activation440, directory CtrlEnter
opens empty directory, 8tabs cap error preserves search/active/tabs, source hashes
unchanged and sorting regression. Three new PNGs inspected. Root0 after drain ACK;
3logged PID/start search helpers absent/reused, Z rejected; no lifetime coverage.
Private painted handlers are not native input/OS shortcut/AT-SPI or clipboard
proof. PSS/Swap/CPU/startup NOT_MEASUREDfreeze; main-process footer incidental only.
No new preview, memory/speed claim or globalPASS. Full goal/ROADMAP still active.

Background-tab source1a425b3 installed through
`evidence/0.0.4/installed-checkpoint-1793463655854916537.json`.
Five installed files/five backups independently rehashed exact in
`evidence/search-order/background-validation/install-verification.json`.
App224d138405a32bac5246fdbe224dafe99fa4d96820f1f710ca0bbd3eafc54d85;
helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd;
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Local authorized install only, no source push/tag/global config or window control.

### 2026-10-31 — Search marks and atomic background-location batches

951Rust324core/85helper/542UI(+3existingcoreignored)PASS; full workspace suite
precedes only final compact acknowledgment/tooltip/help wording. Final workspace
Clippyalltargets-Dwarnings/release/fmt PASS compile it. Logs/checks in
`evidence/search-order/marks-validation`. Pure marks/range/bytes/count/raw/pruning
and Model old-epoch/atomicbatch/invalidlast/visualorder/budget tests pass.

`search-marks-ui-1793464566350502738.json` scopedPASS15real FLTK framebuffers,
actual search_batches, provisional checkbox→final stable path, oldepoch ignored,
ShiftDown3→atomic3tabs, Insert retains other marks, CtrlA42 rejects entire tab
batch, Ninguno mouse→explicit empty no fallback, raw440activation/emptydir and
8tab regression, source hashes unchanged. Three new PNGs inspected. Root0 ACK;
3logged PID/start helpers absent/reused, Z not accepted; no lifetime coverage.
E0502 fixture closure and Clippy too_many_arguments FAIL logs preserved+sidecar;
fixed scalar epoch capture and grouped modifier flags, no lint/guard disabled.

No native input/shortcut/clipboard, multi-origin mutation Source proof or global
PASS. PSS/Swap/CPU/startup NOT_MEASUREDfreeze, footer incidental only; no memory/
speed claim. FullROADMAP active, new previews paused. Search marks do not grant
mutation authority; Copy/Move integration is the next separate contract.

Search-marks source04af97e installed through
`evidence/0.0.4/installed-checkpoint-1793464690558751222.json`.
Five installed files/five backups independently rehashed exact in
`evidence/search-order/marks-validation/install-verification.json`.
Appd484fc75ccdee27e08c8b1f4d1cd1d56db025514fe637c224dfbbec603fe7f9b,
helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd,
HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4.
Local authorized install only; no source push/tag/global config or window control.

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

Search transfers source2af8224 installed with five installed/backup hashes independently
verified in evidence/search-order/transfer-validation/install-verification.json;
installation report evidence/0.0.4/installed-checkpoint-1793470237646603707.json.
App3922c3dd373c/helper59187351e32c/HTML85ea1b8009ef. Local only.

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

Pending retry source58604de installed: report evidence/0.0.4/installed-checkpoint-1793471107693651572.json, independent5installed+5backup exact hashes in evidence/search-order/retry-validation/install-verification.json. Appda5c24312ca1/helper59187351e32c/HTML85ea1b8009ef; local authorized only.

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

Report navigation source17597f4 installed: evidence/0.0.4/installed-checkpoint-1793471905931935042.json. Independent5installed+5backup exact hashes: evidence/search-order/report-navigation-validation/install-verification.json. App91e92f71edcb/helper59187351e32c/HTML85ea1b8009ef; local only.

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

Saved searches source7ba2d57 installed: evidence/0.0.4/installed-checkpoint-1793476145647532183.json. Independent5installed+5backup exact hashes in evidence/search-order/saved-search-validation/install-verification.json; app0e7fed8a8883/helper59187351e32c/HTML85ea1b8009ef. Local authorized only.

## 2026-10-31 — Notice detail ownership

`notice-owner-validation/checks.json` under `docs/evidence/search-order` records 994 Rust PASS (334/85/575), three existing ignored, Clippy all-targets -Dwarnings, release and fmt PASS. Full-report painting and export share the open-notice operation-ID predicate; unrelated/closed/mismatched notices cannot export a retained old outcome. Retained outcome/proofs are preserved. Unit tests cover mismatched/closed/reopened ownership. Existing search scene extended with real transfer → unrelated saved-search validation diagnostic → explicit original-result reopening: scoped PASS, 39 framebuffer captures, four exit0 after ACK, ten logged search-helper identities gone/reused with Z rejected. No clipboard export invoked, native input/lifetime coverage unvalidated. Resources/startup NOT_MEASURED under existing freeze; no global PASS.

## 2026-10-31 — Selected folder-size recalculation

See SIZE-MANAGER and `docs/evidence/folder-sizes/recalculate-validation/checks.json`. F5/painted Recalculate refreshes only completed selected row through same queue/helper. Fresh attempt/context guard, other results and active ACK unchanged, old observation excluded from subtotal/browser size sort.997Rust workspacePASS+3existingignored; later sort/focus1PASS, final Clippy/release/fmtPASS. Real scene1793477083884390572 scopedPASS24FB/raw9→19→29/11loggedhelpersgone/root0ACK. Native input, lifetimes, OS clipboard and resources NOT validated; no global PASS. Original build/import and checker KeyError FAIL evidence retained.

## 2026-10-31 — Search elapsed-hour windows

SEARCH-CALENDAR and `docs/evidence/search/elapsed-validation/checks.json`:999Rust335/85/579+3existingignoredPASS; final Clippy/release/fmtPASS after label-only precision. Exact1/24/168h ends at captured floored reference; no auto execution. Real AuxIo/helper scene1793477561276508002 scopedPASS23FB/AND size+content/raw/inclusive-exclusive±1ns/future excluded, six metadata attempts per six candidates, root0ACK/loggedhelpersgone strictZno. Original three-button assert and script shadowing TypeError FAILs/sidecars retained. No native input/lifetimes/resource/startup/global PASS.

## 2026-10-31 — Retained search pages foundation, not UI

SEARCH-PAGES and `docs/evidence/search/pages-foundation-validation/checks.json`:1007Rust338/90/579+3existingignored, final Clippy/fmt/dev-helper-buildPASS. Seven CLI pipe tests PASS on dev helper:1207raw hits500/500/207, exact1000→empty terminal, exact credit/wrong ID-sequence/EOF, legacy regressions. Directory boundary descendants, root replace, session limits and wait-vs-active budget covered in Rust tests. UI/reader/history/progress/lifecycle/install pending; five installed artifact hashes unchanged. No PSS/Swap/CPU/startup experiment/globalPASS. Initial Clippy unnecessary_unwrap FAIL log preserved; if-let fix without lint suppression.

## 2026-10-31 — Search pages visible

SEARCH-PAGES and `docs/evidence/search-order/pages-validation/checks.json`:1023Rust341/91/591+3existingignored PASS; final Clippy/release/fmt PASS. Final scene1793482591192967537 scopedPASS6Apps/18FB/8loggedPID-startgone Zrejected, observed helper live before Stop and callback.1207,1000 empty-terminal,4507 eviction,500raw513B paths/Locate,602literal fragments440px, per-page marks/source review cancel/stale actions. Shared supervisor preserves child reap ACK; Stop keeps valid accepted data/cached navigation and ignores late finished Stop. Two real red tests preserve opaque-path false budget rejection and late-stop metadata erase; other fixtures/style errors/older scoped scenes retained. Same-SHA release helper seven pipe cases PASS. No native input/lifetimes/resource/startup/global acceptance.

### 2026-10-31 — SFTP profile catalogue

See SFTP-PROFILES.md and evidence/sftp/profiles-validation/checks.json.
1040Rust349core/91helper/600UI+3existingignored PASS, final Clippyworkspace
alltargets-Dwarnings/release/fmt PASS. Full suite precedes caption-only shortening.
`sftp-profiles-ui-1793484790015975923.json` scopedPASS:3apps/7PNG, save/reopen/load
explicit without review/network; Unicode/port spelling, duplicate,0600, real
interprocess lock failure+exact retry, future file preserved and3root0ACK.
Handlers/private pipe/real FLTK framebuffer; NOinputSO/clipboardinterop/network
or active storage-close/lifetimes/powerloss proof. Local caps are not tree PSS.
Preserved initial clipped caption positive+sidecar and unit/compile/layout FAILs;
dialogue no-spawn-marker original cause remains OPEN, now exact error assertion
and normal concurrency PASS. PSS/Swap/CPU/startup NOT_MEASURED underfreeze; no
new previews/tag/globalPASS. Installation report/hashes recorded separately.

Profiles source6f2eab1 INSTALLED: `docs/evidence/0.0.4/installed-checkpoint-1793484872289248556.json`; independent5+5 hashes PASS in
`evidence/sftp/profiles-validation/install-verification.json`, app `ec5ab99217fc`. Existing helper/HTML coherent.

### 2026-10-31 — SFTP readable capture/export request

See SFTP-REPORT.md/evidence/sftp/report-validation/checks.json.1050Rust349/91/610
+3existingignored PASS, final Clippyworkspace/alltargets-Dwarnings/release/fmtPASS.
Report1793486210147321426 scopedPASS2apps7PNG: long own diagnostic all pages,
Unicode/control escaping, fixed vs later status, refresh ID/stalecopy/readonlypaste,
form+Review intact,440 layouts/2root0ACK. Copy request handler only; OS bytes
UNVALIDATED. Profile regression1793486235803344634 PASS3apps7PNG sameSHA.
No real network/host/mount/inputSO/clipboard-receiver/lifetimes/activehelper-close
or PSS/Swap/CPU/startup experiment.24KiB text/range metadata are not PSS proof.
Original scalar-wordwrap positive+sidecar and compile/count/Python fixture FAILs
preserved; final whitespace wrapping byte-lossless. Freeze/new-preview pause and
historical global FAILs remain. Installation/hashes recorded separately.

SFTP report source d1345fb INSTALLED, app cfaa8f3ac1ac; `docs/evidence/0.0.4/installed-checkpoint-1793486330919943223.json` and independent5+5 SHA256 PASS in `evidence/sftp/report-validation/install-verification.json`.

### 2026-10-31 — Archive foundation source-only

See ARCHIVE-OPERATIONS-CONTRACT.md/evidence/archives/foundation-validation.
1068Rust367/91/610+3ignored PASS; Clippyworkspace/alltargets-Dwarnings/release/
fmt PASS.18new pure tests +30actual releasehelper Office wire cases generated
with Python ZIPstored/DEFLATE/descriptor cover preserved responses/source stamps.
No payload-tree extraction/destination FS/GUI/queue/publication/newinstall proof.
Installed5files independently unchanged cfaa8f3ac1ac checkpoint. Initial missing
test helper compileFAIL and wrong legacy-empty-stream comparison retained;
legacy also rejects it, no old-bug claim. Locks/packages unchanged except2core
depedges.8/32KiBchunks/quotas/nodecaps not PSS; freeze/new-preview pause/oldFAILs
unchanged. CRC/output success never permits publication absent separate proofs.


### 2026-10-31 — Archive execution source-only

See ARCHIVE-OPERATIONS-CONTRACT.md and evidence/archives/execution-validation.
1088Rust387/91/610 +3existingignored; final workspace Clippy/all-targets-Dwarnings,
release and fmt PASS.20 filesystem cases;12 independent Python zipfile/core-example
child contracts PASS (stored/DEFLATE/descriptors/raw/modes/umask0777/CRC/cancel/
unsafe paths/empty/occupied destination). This is not queue/UI/native-input proof.
Publication uncertainty and incomplete cleanup stay explicit; same-UID namespace
races and powerloss are not solved by metadata checks. Sync fault injection is
synthetic before syscall. Capacity/FD reservations are admission APIs, not PSS.
No new installation; installed SFTP-report checkpoint remains unchanged. Resource/
startup freeze, preview pause, full goal and prior FAILs remain open.


### 2026-10-31 — ZIP review/Apply and shared queue

1093Rust387core/91helper/615UI+3existingignored; finalClippyworkspacealltargets-Dwarnings,
release/fmtPASS; evidence/archives/ui-validation/checks.json. Five queue/UI tests
cover success, source replacement while paused, fixed outcome reservation and next
copy after ACK, over-budget reject without staging, source/destination intent and
stale buttons/replies. No report-layout assumption, no shared-budget increase.
Final archive-extract-ui1793491842976117966 PASS scoped:7actualPNG1000x650/440x440,
painted pointer handlers; review/cancel/paused no mutation, raw/bytes/modes/source
hashes, other-tab selection, occupied/traversal/CRC and stage absence. Own callback
with paused active job→worker ACK→root0; not live syscall/kernelD or lifetime proof.
Original1793491193816772458 passed filesystem but source filename absent in440
visual review: original+7PNGs+FAIL sidecar preserved. Final visual review passes.
No native input/clipboard delivery/powerloss/resource/startup measurements.1source
per review; ZIP32 regular/dir only; large plans and dedicated retry UI remain open.
Foreign Cargo ttfx observed; own admission returned75 without launching, revalidated
gone then final checks ran serially; competitor untouched. Installation separate.

Installed ZIP review sourcee0dd847; record installed-checkpoint1793492160790962056.
Independent five installed +five backup SHA256 PASS in archive ui-validation/
install-verification.json. App7e264dd0c3c9/helper63a05ffd3fcd/HTML85ea1b8009ef.
No new resource/startup gate and no overall0.0.4 tag/PASS.


### 2026-10-31 — Folder direct-count semantics and size ordering

1100Rust388core/91helper/621UI+3existingignored. Final full suite, Clippyalltargets
-Dwarnings/release/fmtPASS, evidence/folder-sizes/summary-validation/checks.json.
7newregressions: bounded iterator/error/cancel, no child metadata/descent, core and
inactive known-vs-unknown/metadata-vs-total ordering, late summary across panefocus.
Count iterator cap1024, fixedrecord<=32B and no automaticbyte field are source/API
contracts, not process-memory/kernel-I/O/latency experiments. No new worker/helper.
Final direct-summary-ui1793493877395472428 scopedPASS4PNG1000x650/440x440: direct3,
limited>=1024,emptycountnotTotal; explicit8B/0B recursive facts, unknownlast bothsort
orders, own callback→root0ACK,2loggedPID/start helper identities gone/reused, strictZ
not accepted. Two sourcefile hashes unchanged; no fulltree/lifetime/kernelD proof.
Initial fullsuite existing space_map ETXTBSY then mutexpoisonFAIL retained; tests
not altered or serialized. First GUI assumed ascending Size first; actual default
descending was correct, originalFAIL+2PNG+sidecar retained. Combined inspector hint
was clipped; final no-follow line and Calculate shortcut are legible. Native input/
clipboard receiver/resource/startup/allocateddisk/snapshot/globalgates unvalidated.

Installed folder summary source7983c0f/app43a09343ee99. Record1793494152752973446;
5installed+5backups independently rehashed exact, summary-validation/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. Resource/startup freeze remains.

### 2026-10-31 — Explicit ZIP retained-result recovery

1104Rust390core/91helper/623UI+3existingignored, finalworkspace/Clippyalltargets
-Dwarnings/release/fmtPASS. Same worker/controller budgets; atomic reviewed recovery
before pending jobs, no token in ordinary pending queue. Rejected admission returns
original capability/outcome; resultID+freshstamp guards controls. Cancelled cleanup
retains removal ledger; sync checks binding before/after actual parent fsync.
GUI1793496524673844308 scopedPASS5PNG1000/440: foreign retained/not deleted,
startpausedcancel/reviewBack/queue8reject/Clear+staleconfirm, cleanup0publishedroots,
sourceZIPremoved before sync, replacedpublishedname rejected then restoredinode
confirmed. Callback→root0ACK. No helper/lifetime/nativeinput/resource/startup proof.
Synthetic observer failure BEFORE fsync, NOT failed syscall/powerloss. FirstGUIFAIL
checker required modal for success; original/2PNG/sidecar intact. Initialtestcompile
andClippy failures retained in archives/recovery-validation. Durable restart recovery,
creation/ZIP64/TAR/links/otherOS/globalgates remain open; freeze/previews pause intact.

Installed ZIP recovery source61f93c6/app4143d6b8de54. Record1793496618085702365;
5installed+5backups independently rehashed exact in recovery-validation/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. No source push/tag/globalPASS.

### 2026-10-31 — Reviewed streaming ZIP32 creation

1114Rust398core/91helper/625UI+3existingignored; final workspace, Clippyalltargets
-Dwarnings, release andfmtPASS. Separate CreateZIP Action/context/command catalogue,
existing modal/AuxIo/shared queue, fresh review token/epochs, no source reinterpretation.
Read-only FD inventory; files/dirs/literal links, openat2 anchored/no-follow/no-XDEV,
parentdev/inode/statxMountID pre/postsync; no hostile mount topology proof.128roots/
4096entries/512KiBnames/depth32/3GiBregular bytes; controller scratch/outcome/FD charged,
no memory/CPUclaim. DEFLATE64KiB+CRC, exclusive stage/outputsync/no-clobber/parentsync;
ordinaryrwx, UTC DOS2s/clamp, raw Unix names. No ACL/xattrs/ownership/hardlink identity.

Scene1793498375771482261 scopedPASS7PNG1000/440: review/Back nooutput, pausedapply,
other-tab completion preserves selection, independent Python zipfile CRC/content/
raw/link/exec/empty/UTC checks and UIlinks1, occupiedrejection, activepositiveprogress
fixture→cancel cleans ownstage, changedsource preventsoutput, pausedclose→root0ACK.
Source file-byte/rwx/link-target fingerprints unchanged; not full metadata/snapshot.
Core also rejects a substituted stage without deleting occupant. No nativeinput/
clipboard/helperlifetimes/kernelD/powerloss or resource/startup gate. New previews paused.

Original six core Openat2 O_PATH|NONBLOCK failures preserved and fixed; initial UI
compile/style/deadcode failures preserved. FirstGUI1793497597931207464 checker stale
state TypeError, sidecar; second1793497610130038637 functionalPASS has visualFAIL
sidecar (folderlabel and clippedpolicy440), originals retained. Final labels/policy/
namepreview/cancel diagnostics legible. Creation durablejournal/recoveryUI/ZIP64/TAR/
otherOS and global0.0.4 acceptance remain open. Evidence archives/create-validation.

Installed creation sourcee5970ff/app20db1571eb91, record1793498441702024461.
Five installed+five backup hashes independently exact in create-validation/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. No source push/tag/globalPASS.

### 2026-10-31 — Search root navigation

1121Rust398core/91helper/632UI+3existingignored; fullworkspace/Clippyalltargets
-Dwarnings/release/fmtPASS. Root caption Nerd menu and Alt↑parent/Alt→resultfolder/
Alt←previous root. Same criteria, no automatic scan, eight paths/32KiB without old
result snapshots or new thread/helper. Rootepoch distinct from result epoch; menu
result captured/revoked on refresh, stale root controls rejected. Cancels active
search but retains it until ACK, prevents late rows. Pending date blocks scope
change instead of silently discarding criteria. Close/reopen clears root history.

Final1793499783655547501 scopedPASS8PNG1000/440/3apps: query/rawroot0xFE, key policy
and painted menu buttons, no spawn until explicit Enter, content-mode header,
actual old helper PID/start stateS before Root change; new report expected prefixed
paths and zero old unprefixed needle rows. Callbackroot0ACK;6loggedidentities gone/
reused, strictZnotaccepted. No wholelifetimes/overlap/kernelD/inputSO/AT-SPI or budgets.
Initial test compile Fake-scope error preserved; previousGUI1793499682385206785
PASS retained, final strengthens absence assertion over all old needle rows. Source
fixture hashes intact. PSS/Swap/CPU/startup NOT_MEASURED freeze; previews remain paused.

Installed search scope sourceb405554/app35c2c4003224, record1793499893189005044;
five installed+five backups independently exact in search-scope/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. Overall goal active; no source push/tag.

### 2026-10-31 — Forget directory-size observations without browser refresh

1128Rust398core/91helper/639UI+3existingignored, fullworkspace/Clippyalltargets
-Dwarnings/release/fmtPASS. O: terminal selected row only, with active-ACK guard;
Info Nerd button. ShiftO/report Nerd button: immutable completed IDs of that frozen
report, validate-all before removal, preserve later measurements/errors/queued and
pause. No FS deletion, worker, scan or queue-epoch reset; frees retained record/name/
error slots without ID reuse. Fresh panelstamp, nearest display neighbor, same
browser generation/selection and both-pane Size-order invalidation after forgetting.

Real scene1793501340564075091 scopedPASS4PNG1000/440:8B/zero/raw, removeRaw observation
and frozen3measurements, report unknown not0B, readmit4paused with fresh IDs and no
extra helper/refresh. Whole own fixture tree fingerprint unchanged, callbackroot0
postACK;4loggedmeasure_directory identities gone/reused, strictZnotaccepted. Native
input/clipboard/lifetimes/kernelD/process budgets not inferred. Core/unit128budget,
stale/atomic/newer results, cancelledactive ACK and inactive order separate proof.
Initial stamp-shadow, tuple and Arc marker compile errors retained. Two tests first
paused an already drained queue and expectedtrue; corrected with pending work,
originalFAILs retained, production pause unchanged. Freeze/newpreviewpause intact.

Installed forget-observations source8347b65/app18cb22c21e8c, record1793501593425632782;
five installed+five backups independently exact in size-forget/install-verification.json.
Helper63a05ffd3fcd/HTML85ea1b8009ef unchanged. No source push/tag/globalPASS.

## Live dual-pane size ordering — 2026-11-01

Preserved RED model-pump reproduction uses an injected completed Job: measured
20B accepted but quiet inactive Size order stayed stale. Three new regressions
cover valid completion and zero/error/stale ACKs plus browser/manager invalidation.
1131Rust (398core/91helper/642UI),3existingignored; workspace, Clippy all-targets
-Dwarnings, release and fmt PASS. No test serialization or new process/thread.
Evidence: `docs/evidence/size-sync/`. First native-helper scene FAIL is a checker
error (requested-set cardinality is not pending count), preserved with sidecar.
PSS/Swap/CPU/startup NOT_MEASURED; native OS input and global acceptance unchanged.

A later final suite preserved ETXTBSY before a fresh Python fixture script
entered its protocol (641UI PASS/1FAIL). Only registered test scripts now launch
through stable `/usr/bin/python3`, leaving production child spawning unchanged.
Full workspace1131 PASS again, same parallelism/no retries/ignores added. Second
scene FAIL preserves manager pause sent as a browser Action, blocked by modal
routing; fixture uses the manager P handler after F5 instead.

Real-helper scene `size-sync-ui-1793502610961034290.json` scoped PASS:
5 actual1000×650 FLTK framebuffers, completed20→1→16B updates and paused
browser/manager invalidation, active Name order and inactive selection/id/path
stable.4 logged helper PID/start identities absent/reused (Z not accepted),
root exit0 after ACK. Owned actor changes Z only; A source bytes unchanged.
Handlers/fixture keys, NOT native OS input/live resize/lifetime/resource proof.

Installed source3ff50b9 appd84418d722df/helper63a05ffd3fcd/HTML85ea1b8009ef.
Record1793502631105837393; independent5installed+5backup hashes exact.
No source push/tag/global PASS; resource/startup freeze unchanged.

## Sort discoverability — 2026-11-01

1133Rust398core/91helper/644UI+3existingignored, workspace and Clippy all-targets
-Dwarnings PASS. Two new regressions cover inactive Size/Modified/active Name
transitions, retained size observations/owner/generation, stale/refreshing guards,
and six disjoint targets at multiple widths. Original first imports/test API
compile failures preserved. Epoch assertion initially captured before enqueue
started the dataset; corrected capture after enqueue/finish, not weakened epoch
behavior. Resource/startup NOT_MEASURED; native input/AT-SPI remain unvalidated.

First sort scene1793502992179193770 scopedPASS6FB; extended scene
1793503025776201051 scopedPASS7FB adds single-list440. Visual review exposed
folder-size/date touching: folder-size cell now leaves6px gutter like file units.
Those originals and sidecar remain. Full suite precedes only this cell-width
adjustment; final Clippy/release/functional framebuffer scene follow. No logic
change, source mutation, test ignores or timing tolerance added.

Final sort-controls-ui1793503127353685479 scoped PASS7 actual PNG1000×650/440:
Name/Size/Modified header targets and direction toggles, inactive sort preserves
selection/owner/generation and20B measurement, date command keyboard dispatch,
compact dual/single lists,6px size/date gutter.1logged DU PID/start gone/reused
(strictZ not accepted); root0 after ACK. Source A bytes unchanged, date actor
only sets owned directory timestamps. No native input/AT-SPI/full-lifetime/budget
claim. Clippy workspace all-targets-Dwarnings/release/fmt final PASS.

Installed source39abf7b app6ac0be467e75/helper63a05ffd3fcd/HTML85ea1b8009ef;
record1793503143821263359, independent5installed+5backup hashes exact.
No sourcepush/tag/globalacceptance; resource/startup freeze unchanged.

## Pane filters and explicit zero selection — 2026-11-01

RED `red-exact.log` reproduces ClearFilter losing explicit zero-selection intent.
The first --exact command omitted the module prefix and ran0tests; it is preserved
and is not reproduction evidence. Direct exact qualified test then ran1FAIL.
Original added diagnostic used Debug on View (not implemented); compileFAIL log
preserved; changed only diagnostic formatting.
1138Rust398/91/649+3existingignored workspacePASS;5new tests cover Clear/Escape,
no-copy in four views, default focus for no-match, inactive owner/DU preservation
and stale/refresh guards, disjoint compact controls. Clippyalltargets-DwarningsPASS.
Resource/startup NOT_MEASURED; previewsPAUSA; no global/native input acceptance.

Visual review of prior scopedPASS7FB1793503735454209243 preserved with sidecar:
compact pattern recut and notice/counters overlapping. Compact header now omits
redundant Filtro label; counters use short form. Visible compact notice reserves
48px of browser area and moves above MC strip; shared visibility predicate and
geometry test validate footer/notice/row/strip nonoverlap440/800/1000, with/without
MC bar, full-modal/closing exclusion.

Workspace-layout-cancel-fail.log preserves646UI PASS/3FAIL: cancellation fixture
timed out waiting for PID marker, two existing PROCESS_TEST mutex poison failures.
Audit found script registry was thread-local although cancellation transfers
execution to a worker thread. Registry is now test-only shared set with brief
registration lookup locks released before spawn; production executable path and
process supervision unchanged. A deterministic thread-handoff/remove regression
checks registration. Original timeout's unique cause not proven; no deadline
increase/ignore/test-lifetime serialization added.

Final workspace1140Rust398/91/651+3existingignored PASS after compact labels,
notice reservation and test registry handoff fix; Clippyalltargets-DwarningsPASS.
Seven new tests relative to installed checkpoint, no ignored tests added.

Final pane-filter-ui1793504132191949858 scopedPASS8actualFB1000×650/440×440:
independent patterns/edit+clear targets, inactive clear keeps active owner/query/
generation/marks/20B DU, active clear retains marks; explicit zero survives clear
and Escape and F5 opens no copy form (real notice dismissed normally). Compact
pattern/counters visible, notice and MC strip separate. One logged real DU
PID/start gone/reused, strict Znotaccepted; root0 after ACK. Whole owned fixture
structure/content/symlink fingerprint unchanged, not a full metadata snapshot.
No nativeinput/clipboard/AT-SPI/lifetimes/PSS/Swap/CPU/startup proof. Release/fmt PASS.

Installed source4bf0b6a app41bec759247b/helper63a05ffd3fcd/HTML85ea1b8009ef;
record1793504185198676049, independent5installed+5backup hashes exact.
No sourcepush/tag/globalPASS; goal remains active and resource/startup freeze.

## Terminal request integration — 2026-11-01

Terminal raw-directory prototype was not sent to the real UWSM UTF8-only daemon.
Primary installed-source audit led to bounded ASCII hex + user-service helper
exec. Existing open unit contracts retained. First Clippy lint failure and menu
cast parse failure are preserved; no tolerance/test ignore/parallelism changes.
Functional/native report and final check totals follow after actual execution.
No resource/startup measurements; previews remain paused; no global acceptance.

Final unit/Clippy stage:1151Rust401/94/656+3existingignored PASS,11newtests.
Added absolute PATH binding before helper cwd switch, relative/non-executable/
oversize PATH cases; media caps unchanged. Compact labels retain private
clipboard (files) wording and full names/tooltips; text-width invariant added.
First functional/native scene1793506806484779429 PASS9FB confirmed actual mapped
Alacritty/PTY/exact raw cwd and independent lifetime. Its visual scope is fallback
font only: private PATH hid fc-match, not a host font failure. Sidecar preserved.
Final fixture adds only fc-match, keeping missing uwsm isolated; final Nerd scene
and install are pending below. No resource/startup/global gate claim.

Terminal final `terminal-ui-1793507918398541254.json` scoped PASS9FB, real Nerd
fonts, Actions/context/palette and all21 controls440. Actual compiled helper
executes raw cwd/argv after ASCII broker transport; native configured Alacritty
mapped with owned PID/start/class, writer tty0/tty1 true and exact raw cwd, stays
alive after Files exit0ACK then exits on release. Private fake launcher likewise
not killed by Files close; harness alone adopts/reaps it. Two root0ACK runs.
Independent cleanup:6registered identities absent/reused, same-start Znotaccepted,
owned unit inactive. Source9B file unchanged; not whole-tree/metadata/lifetime proof.
No native keyboard/clipboard/focus-control/otherOS/PSS/Swap/CPU/startup acceptance.

Installed source1a266e5 app8deae3545aac/helpera9a2c8dcde68/HTML85ea1b8009ef;
record1793508497934499916. Independent5installed+5backup hashes exact.
No sourcepush/tag/globalPASS; full goal active, metrics/startup freeze intact.


## 2026-11-01 · V12 folder-view storage foundation only

`folder_views` adds bounded exact-raw-path settings and cooperative atomic storage
under config, without writes into saved folders. 410 core tests PASS / 3 existing
ignored, final focused 9 PASS, core all-targets Clippy -Dwarnings and fmt PASS.
Initial two Clippy style errors are preserved in the evidence directory. Final
focused tests include corrected duplicate-key byte fixture and final source.
Evidence: `docs/evidence/folder-views/foundation.json` and copied logs.

No UI/controller integration, restoration, framebuffer scene or new installation
is claimed. Terminal2a4d018 remains installed. Whole-workspace/release validation
was not rerun for this isolated unused core module. See
`FOLDER-VIEWS-CONTRACT.md` for lifecycle/intent constraints still to implement.
PSS/Swap/CPU/startup NOT_MEASURED by freeze; previews remain paused and all historic
FAILs remain in force. No global PASS or 0.0.4 tag.
