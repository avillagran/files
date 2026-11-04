# Markdown nativo, 0.0.4 en desarrollo

Space reconoce `.md` y `.markdown`. El texto se lee con el helper acotado,
se recoge el hijo y se parsea/proyecta en el mismo hilo supervisor. La UI recibe
texto visible con estilos, sin copiar delimitadores ni metadatos de lenguaje del
bloque de código a la selección. No intervienen WebView, servidor, scripts ni red.

El renderer propio dibuja títulos, negrita, énfasis, tachado, listas ordenadas y
anidadas, tareas, citas, código, reglas y tablas con alineación por columna.
Los estilos usan las caras regular/bold/italic de la misma Nerd Font resuelta
para Omarchy. El backend Cairo/FLTK interpreta sus sufijos mediante Pango;
fuera de este host la evidencia visual de caras queda pendiente.

El layout refluye al cambiar el ancho y conserva selección por offsets UTF8 del
texto visible. Cada paso cede después de aproximadamente4ms entre fragmentos
acotados. Una llamada de shaping no se puede interrumpir; `step_max_us` registra
el mayor paso observado, sin prometer4ms como peor caso. El tick deja de pedir
redibujos al completarse. Sólo los fragmentos que intersectan viewport se pintan.
Hay anclas de filas vacías/finales para caret y navegación.

Los tabs se expanden a columnas8 y controles a notación visible, con el mismo
mapa para dibujar y medir; el texto seleccionado conserva sus bytes. Movimiento
por escalares y anchos de prefijos **no acredita navegación por grafemas ni
shaping ZWJ correcto**. El fixture conserva combining/ZWJ completos en la selección
comprobada, sin convertirlo en una prueba general de shaping.

## Enlaces y recursos

Ctrl+clic sobre un enlace permitido navega dentro de Space. Los fragmentos
buscan slugs de títulos (alfanuméricos Unicode en minúscula, espacios a guiones).
Las rutas Markdown relativas se abren por `preview_relative_text`: raíz abierta
en helper, componentes por `openat` sin symlinks, archivo regular, prefijo acotado
y decoder UTF8/UTF16 compartido. Cada petición fija su FD de raíz durante la
lectura; reabrir la raíz en otra petición no equivale a un snapshot permanente.

No hay apertura automática de otra app ni de URLs. Destinos externos, absolutos,
autoridades, controles, queries y escapes léxicos quedan bloqueados. La política ahora conserva padres pendientes al parsear y los resuelve contra
el documento actual relativo a su raíz ANTES del helper. `../` puede regresar
desde una subcarpeta, pero cruzar la raíz se rechaza sin lanzar una carga.
No hay segunda decodificación URL; el helper sigue rechazando ParentDir crudo. Mountpoints/hardlinks e in-place writes no están
confinados; esta política de recursos no es un sandbox del proceso.

El primer bloque7dcc037 mostraba alt/estado sin bitmap. El bloque posterior
integra bitmaps locales según la sección de incrustadas inferior; P04 sigue sin
presupuesto validado ni LRU entre documentos. HTML incrustado queda literal y no implementa
la vista HTML P05. Están pendientes recursos de imagen, LRU, fuentes/rendering de
otros SO y accesibilidad del documento enriquecido.

## Límites

- Entrada Markdown256KiB; helper2000LF y UTF8/UTF16 con truncado explícito.
- IR8192eventos, profundidad64, columnas64, destinos agregados64KiB.
- Texto visible384KiB/2001líneas,16384spans,32768fragmentos de layout.
- Fragmentos de medición hasta259bytes UTF8 y tabs/control expandidos localmente.
- Tabla demasiado ancha produce error visible, no columnas inaccesibles ocultas.
- Un helper de preview a la vez; cambios conservan sólo último destino y drenan
  el anterior. Cierre sigue pump hasta ACK; Drop sólo cancela.

Estos límites acotan representaciones retenidas; no son un límite de PSS total
ni del parser. Medir principal+helpers+Swap por separado del build/capturas.

## Reproducción y evidencia

`python3 scripts/markdown-ui-check.py --capture` usa fixtures propios y handlers
internos con framebuffer real. No prueba input SO ni clipboard Wayland. Cada
READY exige revisión fresca y layout terminado al ancho comprobado; resize espera
el nuevo ancho antes de capturar. Capturas nuevas tienen nombres únicos.

`python3 scripts/markdown-ui-check.py` mide otro proceso sin capturas/compilación.
El gate exige identidad PID/start+exe y memoria de todos los helpers registrados,
samples completos y cleanup normal del árbol observado. Si faltan lecturas,
UNVALIDATED; un pico observado sobre50MB sigue demostrando incumplimiento ideal.
150MB es techo del escenario, no reemplaza el gate baseline10k ni el objetivo50MB.

Estado de capturas/recursos se añade al completar la corrida. Baseline10k y
arranque previos FAIL, clipboard de Space FAIL sin foco y P02 progresivo parcial
permanecen abiertos; este documento no constituye PASS global.

## Resultado verificado del primer bloque visible

230Rust tests (73core/33helper/124UI), Clippyworkspace/all-targets con-Dwarnings y
release sinwarnings: logs en `docs/evidence/0.0.4/markdown-logs/`.

`markdown-ui-1793032850488569491.json`: escenario funcional PASS, revisión y ancho
frescos, títulos/estilos/tablas/listas,54bytes Unicode seleccionados, resize800×500
conserva selección, guía local/retorno, error real de symlink, READY truncado de
2000LF, documento201033bytes y cierre normal drenado. Framebuffer1000×650/800×500,
escala1. InputOS, shapingZWJ general yclipboard NOvalidados. HTMLliteral e imágenes
alt no equivalen a sus respectivos renderers completos.

La primera captura `markdown-ui-1793032403245429423.json` y el reporte de recursos
contienen PASS funcional original cuyo assert de error era demasiado amplio:
`too-many.md` fue READYtruncated, no rechazo. Ambos originales quedan byteporbyte;
sus sidecars `*-correction.json` invalidan esa afirmación. La captura corregida
comprueba el comportamiento real; no se repitió memoria para buscarPASS.

`markdown-resources-1793032456170456020.json`: **UNVALIDATED**, sólo2/8helpers
registrados tienen muestra de memoria+exe. Pico observado42166KiB=41.177734MiB=
43.177984MB; mediana postReady36809KiB=35.946289MiB=37.692416MB, Swapobservado0.
368samples, intervalo máximo74.675593ms,19FD/7threads/2procesos; CPU postReady
mediana0/max162.718%, noidle asegurado.3070eventos de salida corresponden10identidades,
no3070hijos. Cleanup de identidades observadas completo. No presupuesto de lifetimes
completos ni deGPU. Ideal50MB no acreditado por cobertura incompleta.

Parse/layout ready observado del documento201033bytes:634.649ms en la corrida de
recursos,908.578ms en captura corregida. Reflow incremental mantiene callbacks;
un paso individual alcanzó18.306ms (recursos),27.564ms (captura corregida),60.543ms
en primera captura preservada. No garantía4ms. Primera pintura interna92.329ms en
recursos/193.599ms en captura; NOserie cold-start externa ni sustituto del FAIL
previo p95/max151.978ms. AppSHA240ce31875b282dfc25321c4982b11eabf66433d28637da0494f284b11327417,
helperSHA9f225bf85dbb3a264f961dbcde7db07751565a3993112978f3dfc5b530c5ae86.

## Incrustadas y navegación contextual, segundo bloque

Se integran482393f/7504a93(helperrelative_text/image) y3676855(resolvercontextual).
PNG/JPEG/WebP locales usan los mismos límites de codec ya documentados. Los
helpers se ejecutan secuencialmente en el supervisor del documento: reaped uno
antes del siguiente, cancelación y tope10s del conjunto (cada hijo hasta5s).

Hasta8referencias locales se procesan; destinos externos/fragmentos/escape,
symlinks, errores y límites se muestran junto al alt. El footer cuenta imágenes
**no disponibles**, no cargas que siguen esperando. La selección copia el alt
estable, no un texto de 'pendiente' que se vuelva falso al completar el bitmap.

RGB retenido:4MiB agregado, cobrando `Vec::capacity`, `String::capacity`,
Bitmap yArc. Rutas normalizadas dentro de un documento compartenArc/RGB; no
se clona un bitmap por aparición. Antes de un decode se reserva por ledger
otro máximo4MiB para wire/RGB transiente, y se omiten nuevas cargas si ya está
lleno. Este ledger NO incluye todo el proceso/helper y NO acredita suPSS.
Un solo buffer software640×320 máximo se reutiliza entre imágenes visibles,
invalida key al cambiar bitmap y registra capacity real. No Nbuffers por imagen.

No LRU entre documentos todavía: se vuelve a leer al navegar para evitar una
caché sin invalidación segura. P09 sigue pendiente. Current-root path se abre
porFD en cada petición; no se afirma snapshot ni confinamiento ante hardlinks/
mountpoints. Los escapes contextuales se rechazan antes del syscallhelper.

242Rust(73core/33helper/136UI),Clippyalltargets-Dwarnings y release sinwarnings.
`markdown-embedded-ui-1793034469432638103.json` escenario funcionalPASS:
5apariciones/2recursosretenidos,3164572bytes ledger; tres franjas reales en
framebuffer (5792/5611/5611píxeles), bitmapadmitido320×320 con102400píxeles
exactos; otro bitmap rechazado por4MiB. Navegaciónroot→subdir→nested→subdir→root,
fragments, tres escapes bloqueados sin nuevohelper/revisión y cierre0trasACK.
Capturas anteriores1793034242503041636 intactas; labelsfinalesnodisponibles.

**Recursos NOT_MEASURED por instrucción explícita del usuario: sólo integración
ycaptura en esta fase; no más recursos/startup hasta liberar la fase.**
El intento1793034332264805450 se bloqueó enpreflight antes de lanzar la app al
ver compiladores; `markdown-resource-blocked-1793034332264805450.json` tiene
UNVALIDATED/NOT_RUN, sin lecturas de memoria nuevas. No se usa elPSSdel footer
como presupuesto. La corridaMDanterior43.177984MBUNVALIDATED no mide estebinario.

AppSHA16cd4565e50b40f20ed21eed3665cfaf085726f75f65d38f704b2a42f8dc1d80,
helperSHA9f225bf85dbb3a264f961dbcde7db07751565a3993112978f3dfc5b530c5ae86.
NoinputSO/clipboard/graphemeshapinggeneral, no0.0.4tag niPASSglobal.
SiguienteP05HTMLreal, luego vídeo/restoROADMAP; no contar probeHTML como feature.
