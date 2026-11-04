# Medios incrustados: publicación e invalidación — 0.0.4 (2026-10-27)

Continúa RELATIVE-SOURCE y los motores existentes. Esta unidad cubre Markdown
local con imágenes; no incorpora recursos HTML, LRU entre documentos ni un
presupuesto medido de todos los formatos. Recursos/startup NOT_MEASURED: freeze
explícito vigente. No nuevo tag0.0.4 ni PASS global.

## Orden de publicación

1. Helper de texto devuelve rootFD identity y revisión efectiva del archivo;
   helpers de imagen aplican ProbeRelativeFile→PreviewRelativeImageChecked.
   Esperado antes de leer, FD/nombre/raíz revalidados después de decode.
2. El supervisor construye un manifiesto de hasta8 destinos normalizados y las
   pruebas de los bitmaps admitidos. Los destinos ausentes/rechazados conservan
   intención, sin RGB, para detectar su recreación. Ocurrencias comparten Arc.
3. La UI retiene el candidato oculto en Loading. Cambia token/generación y entrega
   Arc del plan al MISMO worker de metadatos reutilizable de la aplicación.
4. Ese worker adjunta watches de directorios ANTES de la sonda final de
   publicación, y vuelve a abrir/verificar documento y cada recurso admitido
   mediante sus firmas esperadas. La lectura/decode previa todavía no era Ready.
   Token/época deben seguir iguales después de sondear; no se limpia dirty.
5. Sólo la respuesta de ese destino/época convierte candidato en Markdown visible.
   Guard.poll se ejecuta también antes de pintar: atomics/slots, nunca stat en UI.

Una modificación observada cancela la generación y retira documento/RGB antes de
cargar otra vez. Respuesta tardía de A no recupera el documento descartado. Navegar,
cerrar Space o cerrar ventana cancela y espera ACK/reap del helper antes del
siguiente trabajo. Ocultación reportada por FLTK suspende demanda; minimizar real
con eventos SO todavía no está validado.

No hay snapshot atómico de múltiples archivos ni hash de contenido. La época
protege cambios ya observados; un evento no entregado todavía puede requerir el
siguiente ciclo o reconciliación. Sin callback no se promete retirada instantánea.

## Watches, límites y recuperación

Plan máximo8 destinos,32 directorios/64KiB de nombres de watch; nombres de
manifiesto<=68KiB (documento, intenciones y pruebas admitidas), cada path<=4096B.
Arc comparte metadata con worker; el plan no conserva ningún bitmap. Límites
comprobados antes de joins/registro; no se recorre un árbol para precargarlo.
Vec/PathBuf capacity y metadata de watchers/backend no son PSS ni un presupuesto
medido: estos caps no acreditan50MB/150MB del árbol.

Se vigilan raíz, padre de raíz y ancestros de recursos/documento. Se filtran eventos
ajenos a la raíz para que logs/capturas/config del harness no recarguen el documento.
Actualmente un evento de otro archivo dentro de la misma raíz puede invalidar
conservadoramente el documento; afinar ese filtro sin perder renames es pendiente.
Directorio ausente de una imagen opcional conserva placeholder y reconciliación;
un fallo de watch de raíz/documento es visible y pausa decodes, sin retry loop.
Fallos de watches opcionales se cubren con reconciliación; falta diagnóstico UI
individual de esos fallos. No se presenta esa cobertura como inotify completo.

Cada1s visible se reenganchan directorios y observan hasta9 resultados de metadata
(root/fileFD o error) fuera de UI. Las observaciones de fallo NO autorizan Ready:
sólo detectan transición ausente→recreado/reemplazado y disparan lectura fresca.
Esto permite recuperar raíz renombrada/reemplazada aunque el watch anterior ya
no exista y se pierdan callbacks. El manifiesto obsoleto retenido durante error
contiene sólo metadata, nunca los RGB antiguos. Un destino nuevo reemplaza el
pedido pendiente; una syscall atascada no crea otro hilo de sonda por navegación.

Recursos relativos mantienen openat NOFOLLOW en todos sus componentes. Documento
abierto directamente conserva padres symlink anteriores; contexto relativo sigue
estricto. Watches de notify y lecturas no equivalen a sandbox: no hay confinamiento
de mountpoints/hardlinks ni protección global de otros accesos del proceso.

No caché de disco nueva, sólo RAM/pipes. No archivos junto al original, recolector
TTL ficticio ni borrado prometido con la app apagada. Operaciones propias invocan
invalidación desde refresh/ACK existente; escenario de operación propia sobre
incrustada aún pendiente, separado de la Papelera standalone ya validada.

## Evidencia

442Rust141core/47helper/254UI PASS, Clippyworkspacealltargets-Dwarnings y release
limpios. `target/document-watch-test.log`, `document-watch-clippy.log`,
`document-watch-release.log`. Regresiones deterministas: reemplazo entre decode y
publicación, candidato oculto/dirty/verificación tardía/cierre, error de watch sin
bucle, documento oculto sin spawn, recurso ausente/recreado, límites, alias directo
vs contexto relativo estricto y watcher real con callbacks suprimidos.

`evidence/document-media/document-media-ui-1793076116463397564.json`: funcionalPASS
acotado,2instancias (notify normal y callbacks suprimidos). En ambas: rojo→azul al
reemplazar mismo nombre/size/mtime con inode nuevo; modificación amarilla mismo
inode/revisión distinta; borrar retira RGB y muestra placeholder; recrear verde;
rename away/back; raíz ausente→raíz nueva recuperada con rootFD nuevo. Los píxeles
antiguos desaparecen en framebuffers; Proof efectivo coincide con stat del fixture.
Cerrar Space no inició helpers durante el intervalo observado; reabrir pide firma
nueva.5capturas1000×650. Handlers internos+filesystem propio+framebuffer, NOinputSO.

Ambas ventanas callback→close_requested→ACK→exit0;26 identidades PID/starttime de
helpers por instancia ausentes/reutilizadas al salir. Es cobertura de identidades
registradas, NO muestreo de todos los lifetimes ni recursos. Worker readonly:
ACKstop false en primera instancia, true en segunda. No se afirma drenaje completo
del primero: el cierre solicita stop sin join en UI. Un syscall kernel no
interrumpible puede retrasar incluso la terminación; offline/montaje atascado
sigue sin validar. El stop de sonda se distingue del reap de helpers.

Escenarios anteriores1793075662414294903/1793075748600705450 se conservan con
sidecars: checker original aceptaba zombie como no-vivo, sin registrar si ocurrió
ese estado; no prueba ausencia estricta en ese instante. No se observó ni se
atribuye un zombie real al producto. La escena final rechaza cualquier mismo
PID/starttime, incluso Z, y pasa ausencia/reutilización estricta. También añade
size fijo e in-place; no es una búsqueda de PASS de recursos. Regresión
`evidence/0.0.4/markdown-embedded-ui-1793075764251781328.json` PASS de bitmaps,
dedupe/presupuesto local/contextual/escape/close; sin sampler, cleanup sólo raíz.
Los tiempos diagnósticos preexistentes del harness no son gates de rendimiento.

c10bd74 instalado con backup/hash coherentes en
`evidence/0.0.4/installed-checkpoint-1793076271320347111.json`:
files 3c6f0c78ee808d759581e0718cdc2d88abbe0c217d4d2ade26aece9db9ddef1e;
helper fcf3a913c39b5173137a904d435a166bb7ae3c39052c17e382905786e3ca9fa7;
HTML a5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
