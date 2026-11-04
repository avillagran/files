# PDF multipágina (0.0.4 parcial)

Space renderiza páginas mediante pdfinfo/pdftoppm delSO opcionales, sin
enlace/distribución de bibliotecas del lector ni crate nuevo. Controles
Anterior/Siguiente, ←/→ yPageUp/PageDown, Home/End; Ctrl+rueda o +/- zoom,
arrastre y0ajustar. Sólo una páginaRGB o texto extraído y un viewport visible, sin caché histórica.

Linux abre enworker FDfinal O_NOFOLLOW|NONBLOCK regular≤256MiB y comparteese
inode con ambosprocesos directos secuenciales; sólo después del reap deinfo
se lanza render. Sin shell/childhelper/nietos. Cambiarpágina cancela y drena
antesdelúltimopedido; cerrar mantieneelprocesoprincipal hastaACK. Cambios
in-place noestán aislados; padres/mounts siguen navegaciónSO, no sandbox.

Metadatos64KiB,1..100000páginas, localeC. pdftoppm rasteriza unasolapágina con
-scale-to960. OutputPPMlimitado960²×3+4096Bantes deguardar; headerP6/medidas/
maxval255 ypayload/EOFexactos antesdeexponerbitmap. El mismoVec se mueve tras
drain(header), sinsegundoRGB. El supervisor común sólo entiendeheaderJSONde
unalínea:PPMse valida alfinaldentrodelwireduro, noantesderetenerelsnapshot.
No se asigna según dimensiones sinvalidar. Selección sobre geometría originalPDF pendiente; el modo de texto extraído se describe en M14.

CPU3s/proceso,wall5s,AS96MiBvirtual,FDacotados,core0,OMP/OPENBLAS1. EstoNOes
PSS ni permiteexcedertecho150MB. PDFcomplejos/protegidos puedenfallar; parser
Poppler noes sandboxfilesystem/red. El host noinvocaJavaScript/URLs/acciones
ni abre recursos externos pedidos porUI; vulnerabilidades/referenciasPDF
requieren contención adicional para tratarcontenidoshostilesarbitrarios.

`python3 scripts/pdf-ui-check.py` creaPDFsintético2páginas rojo/azul con
texto, verificaRGByconteo/página/revisión, zoom/arrastre, rápidoúltimodestino,
malformado/symlink,cierre0ACK. Handlers+framebufferreal, noeventosSO. Recursos
yarranqueNOT_MEASURED porfaseusuario; selección/texto/caché, doccomplejos/
password/otrasplataformas ypresupuestoárbol pendientes.

Verificación actual:266Rust(73/33/160),Clippy/releasePASS. Escenapdf-ui-
1793038247910320945PASSfuncionalconRGB/revisión ypanY60 traszoom2×. Primer
intento1793038113834599772FAILporhandlerinstrumentalassert-pan ausente;
seconserva. CierreACK antesdelnuevo spawnnoacreditakillhijocorriendo.


## Revisión de fuente y publicación — 2026-10-27 (M12)

PDF seleccionado entra en el mismo watcher/token/epoch/reconcile1s de medios;
no se agrega watcher por página ni I/O en paint/UI. Probe estable autoriza una
Revision obligatoria antes del open del renderworker. El FD real debe coincidir
con ella, ser regular no vacío≤256MiB; pdfinfo y pdftoppm heredan ese mismo FD.
Se revalidan FD+nombre después de metadata y después del decodePPM. Bitmap lleva
la firma efectiva; State sólo publica con revisión/generación esperadas. Cambiar
fuente reinicia página1 y conteo, cancela y libera el RGB anterior. Modificar,
reemplazar o borrar oculta los píxeles viejos y exige nueva verificación.

Fallos de fuente tienen flag tipado: invalidación antes del ACK y token nuevo,
no decisión basada en strings de Poppler. O_NOFOLLOW sólo componente final;
padres/mounts siguen semántica del SO, no confinamiento/snapshot. Cambios después
del último stat se detectan por evento o reconciliación, no instantáneos garantizados.

readonly_gate extrae el contrato ya usado por audio. Helper requiere reap antes
ACK/salida; close durante ReadOnly prohíbe futuros helpers y permite salir sin
join de syscall imposible de cancelar. Esa autorización NO es workerDRAINED:
State conserva separación exit_allowed/is_drained. No prueba kernelD real ni
interrupción de FS offline. No hilo nuevo al navegar hasta drenar el anterior.

473Rust(141core/58helper/274UI), Clippyworkspace/all-targets-Dwarnings y release
PASS. pdf-ui-1793092913810767063 funcionalPASS con --source-lifecycle --drop-events:
8framebuffers1000×650, RGB y firma efectiva reales; reemplazo tamaño+mtime igual,
modificación in-place restaurando mtime, borrar/recrear, renombrar archivo/padre,
recuperación, oculto sin nuevo helper, rápidoúltimo destino, errores y cierre0ACK.
23PID/start registrados gone/reused, Znoaceptado; no lifetimecoverage ni helpervivo
alclose demostrado. Tests deterministas VerifiedA/openB y ReadyA tardío/Dirty/B.
Handlers internos/framebuffer real, no inputSO ni nitidez nativa2x. Helper sin
cambio de SHA. Recursos/startup NOT_MEASURED por freeze; sin extraer valores del
footer. Texto/selección, parserhostil/otrosSO/globalLRU continúan pendientes.


## Texto extraído bajo demanda — 2026-10-27 (M14)

Botón Nerd Texto/Página yT alternan la página renderizada con texto extraído de
ESA página. Anterior/Siguiente yCtrl+PgUp/PgDn cambian página en ambos modos;
en texto las flechas/Shift/Home/End/Ctrl+A/C reutilizan selección yscroll de
TextDocument. El texto se etiqueta explícitamente como extraído, no selección
sobre el layout original ni OCR. Páginas escaneadas pueden estar vacías: se
muestra aviso, sin insertar contenido ficticio en documento/clipboard.

Reutiliza el Job PDF y FDexpected/efectivo. En texto ejecuta pdfinfo ypdftotext
secuenciales, NO rasteriza ni retiene bitmap simultáneo. Entrada FD ysalida stdout,
`-f/-l` una página, UTF-8/layout/nopgbrk; no archivos temporales/cache junto a fuente.
Cambiar modo/página cancela y esperaACK antesúltimopedido. SourceDirty libera
texto, reinicia modoPágina y exige verificación. Misma puerta readonly/helper y
parent-death del supervisor existente, sin otro motor/daemon/dependencia Cargo.

Salida≤256KiB antes de retener wire,≤2000LF(2001líneas con último vacío), UTF-8
estricto ysin NUL; String toma el mismoVec, TextDocument lo posee sin clonarlo.
Exceso se RECHAZA visible, no texto parcial silencioso. Si falta pdftotext aparece
capacidad no disponible yPágina permite volver a raster. Límites locales/AS96MiB
NO son PSStotal ni calidad/compatibilidad para documentos arbitrarios.

También se corrige el gate de tamaños: un PDF estable vacío/>256MiB oHTML>32KiB
es error terminal para esa revisión, no SourceUnavailable que haga rearmar token
yreintentar en bucle. Un reemplazo real sigue invalidando. Tests sin I/O de fuente
comprueban rechazo antes open/child yflag de reprobe intacto.

480Rust(141core/58helper/281UI), Clippyworkspace/all-targets-Dwarnings/releasePASS.
pdf-ui1793094189643854652 funcionalPASS12FB1000×650/notify suprimido: modosTexto
páginas2/1, firmaFD, selección porpuntero igual a textoASCIIextraído, handlerCopy
invocado yretorno aPágina; sourcecapsPDF/HTML mantienenrevisión ysin child durante
un intervalo de reconciliación. Repite scoped lifecycle/rasterM12, errores,
rapid/oculto/cierre0ACK y29loggedPID/startgone/reused strictZfalse. No lifetime
coverage/helpervivoalclose/inputSO/clipboard/UnicodePDFfontcoverage/nitideznativa2x.
UTF8+TAB+CRLF ylímites cubiertos en tests puros, no equivale a PDFUnicode real.
Firma app786c52d7c9afef200ad7c258514af96a531b4cf805be4e2b66f67614aad6be92.
Recursos/startupNOT_MEASURED porfreeze; P06parcial, no0.0.4tag/PASSglobal.

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
