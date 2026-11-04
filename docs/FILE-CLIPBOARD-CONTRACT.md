# Cortar/copiar/pegar — M28 (2026-10-27)

Prioridad funciones/UI tras M27 instalado. Previews nuevas pausadas y freeze de
recursos/startup intacto. Esta unidad reutiliza cola, revisiones, ACK y motores;
no otro motor de copia/movimiento. Contrato previo y progreso verificable al final.

## Dos capacidades distintas

Primera entrega: **portapapeles de archivos de files, interno a esta ventana**,
con nombres/rutas raw e identidad capturados. Ctrl+C/Ctrl+X, Acciones/contextual
Copiar/Cortar y Pegar abren revisión visible con fuente/destino; F5/F6 conservan
su acceso directo al opuesto. UI identifica el buffer «Portapapeles de files».
No publica una lista de rutas como supuesto clipboard interoperable del SO, ni
modifica el clipboard de texto al capturar archivos. O06 continúa parcial hasta
backend MIME externo y pruebas reales. Cerrar ventana libera este buffer; no
se persiste ni reanuda un Cut al restaurar sesión.

FLTK1.5.23 Wayland copia texto plano aunque reciba type; el serial0 de foco sigue
sin resolver. Para input externo, get_clipboard_or_dragged_text transforma todo
text/uri-list, decodifica URI, quita file:// y entrega texto plano; %0A/%00/hostnames
no son un transporte lossless y la lectura upstream no está acotada por un cap
Rust posterior. No tratar app::event_text como MIME/bytes crudos de archivos,
no volver a decodificar, no split-lines como autorización de mover, no editar
registry/vendor incidentalmente. Interoperabilidad SO/DnD externo requiere un
adaptador nativo MIME/bytes acotado ANTES de esa transformación; no se marca
completa con el buffer privado ni con URI escrita en text/plain. DnD interno
posterior puede reutilizar el mismo token raw sin inferir archivos de texto.

## Captura y autoridad

Copiar/Cortar capturan sólo selección visible vigente (incluye foco sólo si no
hay lote). Nombres OsString, ruta PathBuf y owner(panelID,tabID), sin conversión
lossy operativa; cambios de foco/navegación/ocultos después no cambian el lote.
Cut NO elimina ni renombra al capturar. Preflight prestado antes de clonar:128
raíces, componentes únicos<=4096B, padres/rutas<=4096B,<=1MiB agregado de nombres
más paths; rechazo visible conserva buffer previo. El límite128 común permite
la misma revisión Move existente, sin fingir soporte ilimitado de clipboard.

Un trabajo de captura/revalidación en AuxIo existente obtiene identidad del
padre por FD y firmas de los nombres (device/inode/kind/size/mtime/ctime/nlink,
sin atime). Sin stat/open/revalidate en draw/click/hilo UI. Publicación requiere
ID/generación vigentes; respuesta vieja nunca sustituye buffer nuevo ni inicia
operación al cerrar. No un hilo por gesto: máximo una captura en vuelo, latest
intención deseada acotada, cancelación cooperativa/drenaje del ACK. Kernel D
sigue pudiendo bloquear la syscall, no crear un worker sustituto.

Revisión/ejecución revalida origen contra captura antes de aceptar movimiento;
PreparedMove ya fija objetos por FD y revalida antes de rename. Reemplazo del
mismo nombre o padre vinculado distinto requiere recaptura, no mover el sustituto.
Las firmas detectan cambios, no snapshot de contenido ni defensa atómica frente
escritor hostil; se conservan límites inode-conditional Linux de Move. Copy no
borra origen y conserva el contrato de estabilidad del motor actual. Fuente y
resultado se muestran sin reducir identidad a ruta textual o CopyReport.completed.

## Pegar, destinos y texto

CtrlV se enruta ANTES del interceptor canvas que hoy siempre llama paste_text.
- Omnibar/rename/destino editable mantienen pegado TEXTO del SO, con los límites
  y semántica existentes; no convertir texto en una operación de archivos.
- Navegador sin inputs/modales abre Pegar del buffer privado y captura AHORA
  destino raw, owner y token de intención. El diálogo no resuelve destino contra
  panel/tab que sea activo cuando llegue respuesta asíncrona. No iniciar nada
  automáticamente al recibir FL_PASTE.
- Un callback de TEXTO pendiente lleva intención/contexto editable; al cambiar
  panel/tab, salir de edición, cerrar o abrir otro diálogo se descarta en lugar
  de pegar en el nuevo. Máximo una solicitud de texto pendiente; segunda no
  reemplaza silenciosamente el token. El callback sin token no autoriza archivos.
  FL_PASTE carece de requestID: invalidar deja tombstone y mantiene ocupado el
  slot hasta drenar ese callback; no reasignarlo por timeout (evitar ABA A→B).
  paste_text se llama después de liberar RefMut<Model>, pues FLTK puede entregar
  el pegado sincrónicamente para selección propia. A invalidado→B rechazado→A
  drenado→B nuevo legítimo tiene regresión independiente del producto de SO.
  Preflight MIME compatible antes de reservar slot. FLTK1.5.23 Wayland real
  entrega tanto own como external sincrónicamente y puede retornar SIN callback
  (vacío/tipo no compatible); sólo ese backend comprobado permite liberar al
  retorno definitivo NoCallback. X11/macOS/Windows/desconocido mantienen tombstone
  hasta callback. Un receipt identifica nuestra invocación/return, NO FL_PASTE:
  el retorno viejo tampoco puede limpiar una solicitud reentrante más nueva.

Diálogo usa el destino capturado aunque cambie foco. Cambiar explícitamente su
texto sustituye PathBuf raw igual que M27, sin romper destino0xFE. Confirmar
admite en la cola global existente, conserva jobID/rutas/proofs y se puede pausar,
cancelar o cerrar con ACK. Pending/reject conserva buffer y propiedad. No duplicar
trabajos por CtrlV repetido mientras diálogo/revisión ya exista.

## ACK, memoria y estado visible

Cada operación admitida se vincula a generación de clipboard y subconjunto exacto
capturado; reemplazar el buffer NO modifica job ni permite a su ACK vaciar el nuevo.
Copy conserva el buffer. Cut consume únicamente raíces confirmadas por el resultado
Move. Éxito total confirmado consume ese lote; error/cancel conserva no completados.
En parcial, sólo destinos exactos correlacionables con las solicitudes y publicación
confirmada se consumen; ambigüedad/ruta renombrada/recovery pendiente se conserva y
se informa para revalidación. Nunca inferir éxito de ausencia del origen. ACK de
recovery conserva la misma intención/identidad, sin reinterpretar buffer nuevo.

Un buffer actual más capturas/dialógos/cola/resultados sin ACK comparten un límite
agregado2MiB de metadata de clipboard, comprobado antes asignar/retener (Arc sin
clonar nombres); incluir capacidades y scratch de captura por separado. Un FD de
padre por captura efectiva cuenta además de FDs de revisión/activo/pendientes y
resultado sin ACK en los gates locales existentes256, no presupuestar cada job
como si estuviera solo. No PSSclaim por estos caps: recursos NOT_MEASURED freeze.
Descartar versiones sólo al liberar su último consumidor, no al mero enqueue.

## Aceptación prevista (extensión de escena de tabs/dual existente)

A/B mismo nombre distintos bytes; capturar A→cambiar panel/tab→Pegar en B muestra
fuente A/destino B explícitos. Nombres0xFF y destino0xFE exactos. Cut no mutante,
review+paused no mutante, copiar conserva, mover consume confirmados; cancelar/
colisión parcial no elimina pendientes. Reemplazo mismo path antes revalidación
rechaza; nuevo clipboard durante job sobrevive ACK viejo. Texto en omnibar,
renombrador/destino no dispara operación; respuesta tardía tras cambio/cierre,
doble Paste pendiente, cierre ACK. Capturas reales/handlers, sin afirmar inputSO/
clipboard externo ni nuevas métricas. No nuevo instrumento salvo extensión del
existente y regresiones unitarias que ejerciten estos contratos.


## Fundación histórica M28 — 1fa3d00, anterior al wiring visible

`selection_source.rs` captura por padre FD y nombres raw, firmas con ctime/mtime
sin atime, campos privados y acceso readonly.1FD retenido; verify abre como máximo
1FD padre temporal adicional (current se libera antes last). O_NOFOLLOW sólo
componente final del padre y statat sin seguir objeto; no confinamiento de todos
los ancestros ni snapshot de directorio recursivo. Los descriptores del token
PreparedMove/PreparedFileMove se comparan con la firma capturada (padre y raíz
efectivos), no basta verify(path)→capture(path). Regresiones deterministas
verifyA→replaceB→captureB rechazan B en ambos motores, sin mutación. EXDEV de esta
regresión usa capture_inner(false) sólo para probar el token, no acreditación
nueva de st_dev distintos ni transferencia EXDEV.

201core PASS/3ignored, Clippy core/all-targets-Dwarnings PASS. Primer FAIL compile
de nlink u32 aarch64 conservado. Budget por ruta4096 y agregado son independientes;
128 nombres válidos con suma>4096 y frontera por ruta exacta probados.
`text_paste_gate.rs` aún sin wiring/lib.rs:7tests standalone PASS de tombstone,
retorno síncrono/NoCallback, recibo de invocación y borrow liberado; no prueba de
clipboard SO. Scope actual no clipboard UI completo, sin nueva instalación.
Sigue AuxIo/Controller/diálogo, cuotas/FD conjuntos, payloads de cola/ACK parciales
y enrutamiento canvas. M27 instalado intacto, no previews ni métricas nuevas.


## Wiring M28 y consulta MIME de texto

AuxIo captura Source y publica por ID; el buffer, el diálogo y los trabajos
comparten Lease por Arc. El ACK consume únicamente el subconjunto confirmado.
Los reintentos de un Cut parcial verifican las firmas originales sólo de las
raíces restantes: no buscan ni recapturan la ya movida. Weak muertos se podan
antes del límite de leases. Source/Prepared usan Control vivo entre raíces y
conservan io::ErrorKind::Interrupted, sin snapshot AtomicBool ni string que
convierta cancelación en fallo genérico. Recuperación sólo hereda el tag de Cut
si coinciden job y IntentProof revisados; otro journal no consume ese buffer.

Canvas intercepta CtrlV antes de tomar RefMut. FL_PASTE sin token no modifica
texto ni autoriza archivos; los campos conservan preflight acotado. Al cerrar
se cancela captura y se espera ACK junto al drenaje existente; no join bloqueante
ni worker sustituto. Seguir trabajando después de cancelar cierre conserva buffer.

Hallazgo local reproducible de fuente: fltk-rs1.5.23 clipboard_contains(Text)
crea CString; el driver Wayland compara el puntero con fl_selection_type[1].
El adaptador nativo local files_clipboard_has_text pasa la constante pública
Fl::clipboard_plain_text. No se edita registry/vendor, no se lee un clipboard
ajeno, no se fabrica serial ni se solicita foco. Otros backends usan la API
existente y su comportamiento sigue sin validación de host.

La prueba de texto propio en una misma llamada sólo acredita callback síncrono,
borrowing y bytes de ese caso. La escena anterior con copy→event loop→paste
falló por preflight falso negativo: se conserva FAIL. No atribuir ese resultado
a pérdida de selección/foco. Persistencia entre vueltas requiere su observación
con la consulta corregida; compositor e intercambio externo siguen pendientes. No usar data-control
para sortear este límite o apropiarse de la selección global sin gesto explícito.

M29: contrato previo a wiring para MIME nativo/URI bytes bounded y DnD real,
con gesto de la ventana propia y serial real vía wl_data_device/puente FLTK;
codecs/transporte privados se prueban por separado del input SO. No fallback
data-control de foco, ni promesa de interoperabilidad a partir del buffer M28.


## Entrega visible verificada (M28)

624Rust (203core/67helper/354UI,3coreignored), Clippy all-targets-Dwarnings,
release sin warnings y fmt--check exit0. Cinco regresiones de flujo real AuxIo→
buffer→diálogo→cola→ACK incluyen subset parcial+100leases, reemplazo, ACK viejo
frente a buffer nuevo y token texto ABA/cierre. Cancelación controlada conserva
Interrupted y se prueba entre raíces, sin claim de interrupción de syscall.

`clipboard-ui-1793130549985863820.json` funcional scoped PASS: ocho framebuffer
IHDR exactos (seis1000×650,dos800×440), dos root exit0 tras drain ACK; Copy0xFF
hacia destino0xFE conserva bytes/origen/ocupantes. Cut2 captura/review/paused sin
mutar; cancelar diálogo o trabajo pausado conserva fuentes/count/generación;
colisión posterior a review publica sólo cut-a y retiene cut-b, retry del subset
completa. Reemplazo rechazado sin mover actor. Nuevo Copy sobrevive ACK viejo.
Texto propio inmediato Y copy→eventloop→paste exactos; dos preflights muestran
rust_wrapper=false/canonical=true. Esto corrige falso negativo de puntero, no
prueba serial/foco externo ni MIME de archivos. No atribuir FAIL previo a pérdida
de selección. Menús estrechos completos sin toast, mensaje conservado al cerrar.

PASS funcional anterior1793130287088020335 y sidecar visual de Acciones se
conservan; contextual no tapaba controles (franja lateral solamente). Los FAIL
originales de key loaded, JSON parcial y consulta MIME quedan intactos. Ver
`clipboard-checkpoint-validation.json` para logs/exitcodes/hashes y límites.
No inputSO/liveResize/recursos/startup, no PASSglobal ni cierre O06. Siguiente
M29 requiere contrato MIME/DnD antes de wiring, sin data-control de foco.
