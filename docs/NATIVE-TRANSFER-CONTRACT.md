# MIME nativo y drag/drop — M29 (2026-10-27)

Contrato ANTES de wiring. M28 instalado1a8e48a/1c95bf1 permanece estable. O06
sigue parcial: el buffer privado no equivale a MIME del SO. Prioridad funciones
/UI, previews nuevas pausadas y freeze recursos/startup intacto. No nuevas
mediciones, daemons, configuración global, dependencias/licencias incompatibles
ni input/foco fabricado. Contrato no es feature ni aceptación.

## Capacidades y gesto

Producto Linux/Wayland: puente de plataforma con wl_data_device_manager,
wl_data_device y wl_data_source; se usan display/seat reales del backend FLTK.
Publicar clipboard o iniciar drag exige un permiso capturado durante gesto
explícito de esta ventana, con serial real del evento y foco/surface asociados.
El ACK asíncrono nunca consulta get_serial() para fabricar esa asociación. Un
callback fixture no genera ese permiso. Sin gesto/foco/capacidad: error visible y buffer privado
M28 conservado; no apropiación global silenciosa ni publicación al restaurar
sesión. Recepción sólo tras Pegar explícito/drop aceptado sobre nuestra ventana.

NO usar ext/wlr-data-control para sortear foco/serial de fixtures, leer clipboard
personal en background o reemplazar una selección sin acción. Inventario local
ext-data-control no es adopción. No wl-copy como falso arreglo del bridge.
Codecs/transporte en fixtures privados se distinguen de handshake y eventos SO.
CUA0/nativeinput sigue pendiente; no convierte una prueba interna en input SO.

La publicación nativa tendrá estado propio (no publicado/owner/ownership perdido/
error); la UI distingue Pegar de files y Pegar del sistema, sin fallback ambiguo
cuando ambos difieren. CtrlC/X puede mantener buffer privado y publicar sólo si
el backend recibe el gesto válido; nunca describir éxito global si sólo privado.

## Permiso de gesto frente a captura asíncrona

En el handler real se fija GesturePermit {id,intent,kind,serial,seat,seat_epoch,
surface,window_lifetime,event_epoch,deadline}. El puente obtiene serial del
**evento actual comprobado** y valida su foco en esa misma ventana; el serial no
se serializa en clipboard ni viaja a otro display. El token C++ opaco se consume
una vez: Rust conserva ID, no pointers prestados reutilizables como autoridad.
Cualquier nuevo gesto relevante, intención Copy/Cut/drag distinta, cancelación,
Closing, cambio/pérdida seat/surface o vencimiento2s invalida permiso pendiente.
Un ACK Source lleva intent+permitID; sólo el puente que capturó el permiso puede
revalidar lifetime/epoch/foco y publicar usando EL serial guardado. Nunca obtener
el serial más reciente al recibir ACK ni hacer stat/open síncrono en handler.
Si proof tarda/permiso caduca, buffer privado puede quedar listo pero la UI dice
«No publicado al sistema; vuelve a copiar», manteniendo clipboard global previo.
Servidor/compositor decide aceptación; request enviado no equivale a ownership.

Clipboard permite publicación diferida bajo ese gesto explícito, aunque la tecla
ya no esté presionada, sólo si permiso/foco siguen válidos. DragPermit es distinto:
serial de button press real + botón/grab implícito todavía mantenido sobre surface
propia; ni keyboard_enter_serial ni un gesto de teclado autorizan start_drag.
Si Source aún no está listo al soltar, cancelar drag antes de iniciarlo, sin I/O UI
ni inventar press. Motion/threshold no reemplaza el serial del press original.
No publicar metadatos URI antes de Source proof en esta entrega; propiedad MIME
no constituye por sí sola autoridad de mover/borrar de todos modos.

## Coexistencia con FLTK, propiedad y ofertas

No reemplazar listeners ni destruir wl_data_device/wl_data_source/offer proxies
ajenos a FLTK. Crear **nuestro** data_device desde manager/seat prestados; listener
sólo en ese proxy y en offers/sources creados/entregados a ese listener. Display,
seat y manager siguen propiedad FLTK. Si la coexistencia de dos data_devices no
funciona en host, declarar capacidad no disponible; no secuestrar listener ajeno.
No conexión Wayland nueva para reutilizar un serial de otra conexión.

Callbacks en hilo UI sólo registran metadata/FDs bounded; no read bloqueante,
stat ni decode. Un device propio por proceso, con lifetime explícito y sin
recrear por cada repaint. MAX4 offers retenidas,32 MIME por oferta,256B por MIME;
overflow invalida capacidad/oferta y libera sólo proxies propios. Clipboard
selection y drag-enter se distinguen; IDs monotónicos no se reciclan ni son
punteros. Epoch de oferta/token(window,panel,tab,intención) acompaña cada lectura.
Oferta sustituida/cancelada, ventana Closing, generación distinta o destino
cerrado descartan respuesta aunque contenga bytes válidos. Última intención
vence, pero no reasignar un slot asíncrono sin ACK/correlación.

Fuente publicada mantiene payloads inmutables bounded hasta cancelled/loss y
cierre de sends pendientes. Se controla lifetime ante send/cancel reentrantes;
FD recibido en send es nuestro y se cierra exactamente una vez. Cancel no borra
archivos ni consume Cut. En cierre: invalidar intención, cancelar/drain lectura y
sends, destruir proxies propios sin tocar conexión/listeners FLTK. No join de
syscall bloqueada en UI ni worker sustituto. Estado/error de transporte visible.

## Transporte FD bounded antes de asignar

Una recepción activa y última deseada, usando AuxIo existente; no hilo por
oferta. Pipe CLOEXEC en ambos extremos, O_NONBLOCK sólo en lector propio; read/poll con deadline3s y cancelación por
chunks. MAX_WIRE1MiB, comprobar cap+1 ANTES de extender buffer, EOF exacto, ningún
read_to_end no acotado. Al exceder/truncar/error/deadline/cancel: cerrar FD,
responder error sin documento parcial ni operación. Identidad offer/epoch se
revalida al devolver resultado, no sólo al solicitar receive. Estado de lectura
sin FD/canal/ACK no se confunde con éxito vacío.

Enviar también no bloqueante: payload inmutable preflight antes serializar,
MAX4 FDs de send simultáneos, cuota por pump<=64KiB; EAGAIN conserva offset y
registro limitado, EPIPE/cancel cierra sin loop ocupado. No un proceso/daemon ni
buffer por lector sin límite. Reserva conjunta incluye payload MIME, buffer
recibido, Source proofs, revisión/cola/resultado sin ACK y FD scratch dentro gate
256; caps locales NO prueban PSS. Sin timer global60fps ni renders durante espera.

## Formatos y rutas

- text/uri-list: COPY por defecto. No inferir Cut de texto plano ni de URI aislada.
- x-special/gnome-copied-files: primera línea exacta copy/cut, luego URI; formato
  explícito de la misma oferta. KDE cutselection sólo se acepta como metadata
  exacta de esa misma offer/epoch, no combinando clipboard de instantes distintos.
- MIME propio versionado puede llevar identidad de operación/ventana para evitar
  doble despacho interno; nunca confiar un jobID de proceso extranjero como proof.

No usar FL_PASTE URI→texto: FLTK pin decodifica globalmente y quita file://,
perdiendo límites/%0A/%00/hostnames. Adapter recibe bytes MIME ANTES de eso.
Codec por URI, percent-decode EXACTAMENTE una vez; %0A dentro de filename no es
separador y %25 conserva %. Unix OsString raw, no UTF8 lossy operativo. file:///
y file://localhost/ sólo locales; host remoto/scheme distinto/NUL/query/fragment
sin escapar/path relativo/traversal/desborde rechazados antes I/O. '#' inicial
de línea sólo comentario de uri-list, no permiso para fragmentos. CRLF/LF separan
URIs ANTES de decode. No abrir ni navegar una URI al parsearla.

MAX128 raíces,4096B por ruta decodificada,1MiB wire y metadata conjunta explícita.
Preflight prestado count/longitudes/sumas checked antes Vec<PathBuf>. Padres raw
agrupados con Source proofs, máximo32 padres por entrega; si excede, rechazo
visible para reducir lote, sin truncado ni recaptura silenciosa. El límite no
reduce el objetivo futuro de operaciones multi-origen. Cada FD efectivo de
Copy/Move se valida contra su grupo de firmas, no verify(path)→capture sustituto.

Destino PathBuf/panelID/tabID/intent se fija al Pegar/drop; callback tardío no usa
panel ahora activo. Captura Source en AuxIo antes revisión; paths de una oferta
son entrada no confiable, no autorización de overwrite ni borrar. Diálogo y cola
M28 existentes, conflicto/cancel/ACK conservadores, refresh de ambas carpetas por
rutas capturadas, sin duplicar motores ni un Model por oferta.

## DnD visible y acuerdo de movimiento

Drag sobre fila/lote captura selección visible raw; umbral de movimiento evita
robar click/rango/dobleclick/headergrab. Targets panel/folder/background por
ID+generación, highlight/reveal claros y Escape/leave/drop stale no mutante.
Drop abre revisión (destino al gesto) antes encolar. No mover al hover.

Enviar bytes o recibir dnd_finished NO permite unlink del origen. Origen de files
nunca ejecuta delete por ese evento ni por acción MOVE que mande un tercero.
Evitar doble delete si destino ya renameó; Cut sólo consume publicaciones propias
confirmadas por motor/IntentProof/ACK, no ausencia del nombre. Primera capacidad
externa se anuncia COPY únicamente; MOVE externo queda explícitamente pendiente
hasta acuerdo verificable que no duplique eliminación. No anunciar MOVE y luego
simularlo como Copy exitoso. DnD interno entre nuestros paneles puede ofrecer
Copy/Move mediante revisión y cola global/proofs, sin source-side auto-delete.
Si source externo exige sólo MOVE sin COPY, rechazar drop con explicación.
Esto NO convierte GNOME Cut importado por clipboard en Copy: cut explícito de
esa oferta conserva intención Move y abre revisión/proof/cola; si no se admite,
se rechaza visiblemente. DnD negotiation y Cut clipboard son canales distintos.
Publicar Cut tampoco autoriza a files-origen a borrar al servir bytes, cancelled
o dnd_finished: otro gestor puede haber hecho rename; no segundo delete. Sin ACK
propio verificable, el buffer privado no infiere consumo de ausencia de nombres.

## Aceptación por capas, sin métricas

1. Codec puro: LF/%0A/%25/noUTF8/localhost/host remoto/NUL/URI inválida, frontera
   count/path/wire, formato Cut explícito y cap antes alloc. No clipboard leído.
2. Transporte privado con pipes: chunks/EOF/EAGAIN/overflow/cancel/deadline, cambio
   offer/epoch/close/ACK/reentrancia; no afirmar compositor ni inputSO.
3. Bridge producto: propio data_device/listeners, permisos/serial/foco reales,
   MIME publicado/recibido exacto con otro consumidor privado autorizado, sin
   leer clipboard previo. Si driver nativo no disponible, mantener no validado.
4. UI extendiendo escena existente: origen/destino raw, foco/tab cambiado antes
   callback, DnD entre paneles y carpeta/leave/cancel, review/paused no mutante,
   conflicto/retry y root0ACK, menú800×440. Handlers inyectados≠evento SO.

Capturas reales y auto-instalación sólo checkpoint validado. O06 no completo por
codec/pipe o buffer privado; evidencia externa no se inventa desde fixtures.
Otros SO tendrán adaptadores explícitos; Linux-only no es compilación Windows/
macOS. Sin más previews ni recursos/startup mientras freeze siga vigente.


## M29a — implementación y validación parcial (2026-10-27)

Codec Rust integrado sin nuevas dependencias: dos pasadas antes de materializar,
Unix raw y percent-decode único por URI. Puente C++ sobre FLTK1.5.23 con manager,
data_device, sources/offers y listeners propios en display/seat prestados. Fuentes
one-shot y Fl_Widget_Tracker; no listener ajeno, conexión nueva ni data-control.
Status «solicitud enviada» no acredita ownership/consumo. El permiso guarda serial
y foco reales del gesto, nunca se reconstruye en ACK. Pérdida de ventana invalida
epoch nativa ANTES del borrow Model; reading_epoch impide aceptación reentrante.

AuxIo existente realiza recepción/captura/encode. Un slot conserva tombstone
hasta ACK; reader encolado caduca y cierra FD desde pump aunque AuxIo esté ocupado.
Reader worker usa poll20ms, deadline total3s/cancel/cap+1 antes extend. Emisor externo
recibe FD de escritura blocking; sends propios NONBLOCK, cuota64KiB/pump, deadline3s
/inactividad1s, cuatro slots. No lectura MIME mediante FL_PASTE ni read_to_end.
Destino PathBuf/panel-tab/generación capturado, oferta cambiada/cierre descartan.
Source importado entra a revisión existente con etiqueta Sistema y proof del FD;
no consume Cut privado de igual ID. Aviso de rechazo por presupuesto se conserva.

Reservas LOCALES, no gate PSS: dos payloads C++ de hasta1MiB (sends comparten
vistas, no duplican payload); un flight Rust con wire hasta1MiB, encode e import
mutuamente excluyentes. Antes de Source capture se libera wire; paths decodificados
<=128×4096B más contenedores, liberados antes de retener Source. Una fuente/revisión
nativa <=2MiB y anterior system-recovery <=2MiB se admiten conjuntamente <=4MiB;
fuera de Request son reserva separada, NO parte del cap privado2MiB ni supuesta
cola4MiB. Al encolar, Request ya cobra lease.retained_bytes() y sus FDs. Se reservan
8FD externos adicionales: hasta4send, reader, hasta2scratch Source y margen para
pipe writer transiente. Cap global existente256FD sigue contando review/activo/
cola/ACK; no multiplicar límites al mantener otro resultado. Estos caps no prueban
memoria del árbol ni latencia/CPU. Sin nuevos helpers/threads de transferencia.

Límites explícitos: primer import admite UN padre común, rechaza TODO lote
multi-origen antes capture (objetivo32 padres sigue pendiente); KDE sin GNOME
rechazado por falta de lectura Cut correlacionada; otros SO pendientes. DnD externo
NO implementado: backend FLTK pinned hace set_actions y roundtrip incluso con
handler false, además de su URI→texto global no acotado al aceptar drop. Retornar
false evita nuestra ruta FL_PASTE pero NO demuestra negociación coexistente.
Próxima adaptación: hook opt-in de ventanas propias ANTES enter/motion/drop del
backend, con cleanup/leave/offer ownership y default intacto para otras ventanas;
source/parche reproducible bajo repo, nunca editar registry global.

637 tests Rust (208core/67helper/362UI),3ignored; Clippy all-targets -Dwarnings y
fmt--check exit0. Incluyen codec5, reader3 y state5: oferta/destino/owner/cierre/
cancel, queued-expiry EPIPE antes take, rechazo de presupuesto visible, systemCut
sin consumo privado y pérdida de foco. State usa callbacks inyectados, no eventos
Wayland/C++; esa distinción se conserva. Primer FAIL C++ reading_epoch preservado.
Release exit0 y escena clipboard-ui1793133638367792338 scopedPASS9PNG/2root0ACK:
botón/rechazo sin gesto y regresión privada M28; isolate-input NO activa bridge.
Validación/hashes en native-transfer-checkpoint-validation.json. NO afirma MIME SO funcional,
DND/KDE/multi-origen completos ni PSS/startup. Previews/freeze intactos.


## M29b — receptor y hook DnD parcial (2026-10-27)

Hook opt-in de ventana propia en FLTK1.5.23 antes de enter/motion/drop: evita
negociación/roundtrip/lectura upstream en esa secuencia; libera sólo la oferta
propia de FLTK en leave/drop/unregister y suprime eventos tardíos hasta enter.
Las otras ventanas conservan upstream. `vendor/fltk-wayland-transfer/` contiene
source original/modificado/patch/hash/COPYING bajo licencia FLTK LGPL+excepción.
CMake sustituye una TU con hash verificado en build, sin escribir el registry.
Guard valida inputs y exige símbolo fuerte ligado al digest en enlace final;
si se actualizan inputs hay que versionar wrapper y toolchain según README.

Receptor propio anuncia COPY/URI, coalescea hover, fija pane/tab/generación y
destino raw antes de leer, usa un reader acotado por AuxIo y revisión/cola
existentes. Target prestado no cambia foco ni selección; al ACK revalida panel/
columna, revisión, nombre y filtro. Cancel/stale/budget no abren diálogo ni
finish. Drop marca drag inactivo y oferta queda fijada hasta ACK/cleanup;
source_actions tardío no renegocia. Epoch obsoleto rechaza antes pipe. ACK de
COPY de bytes aceptados nunca autoriza borrar origen; no se anuncia MOVE.

Receiver C++ wrapped-proxy PASS y14 tests Rust dirigidos +642 workspace PASS;
Clippy/fmt/release exit0. Escena clipboard-ui1793135944759735086 PASS scoped
con pipe privado/fake ACK: hover, cancel, review, copia raw y2cierres normales.
NO demuestra handshake, serial, oferta, DnD exterior, memoria ni inputSO.
M29/O06 pendientes: sender y DnD interno/externo demostrados, KDE correlacionado,
multi-padre agrupado y adaptadores SO. No activar data-control para sortear foco.


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
