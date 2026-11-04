# EXDEV y recuperación — contrato M25

2026-10-27. Implementación en curso; M24 instalado sigue rechazando EXDEV.
O01/O03 permanecen parciales. Freeze recursos/startup vigente. No autorizar
borrado mediante CopyReport.completed ni equiparar fsync con atomicidad entre FS.

## Unidad y límites

Reusar revisión FD, cola, pausa/cancel/ACK y resultados de M24. La primera
integración ejecutable puede limitar tipos/lote explícitamente; no degradará
árboles, hardlinks, ACL o xattrs a una copia incompleta seguida de borrado.
Cada limitación conserva origen y tiene diagnóstico. El contrato final exige
manifiesto por entrada, nombres raw, identidad FS y revisión/contenido/metadatos.
Máximo inicial8192 entradas, rutas relativas4096B/64componentes, manifiesto4MiB
wire, journal total8MiB. Rechazar ANTES de reservar/copiar datos que excedan.
Contabilizar capacidad retenida, descriptores y coexistencia con revisión/cola;
son límites locales, NO prueba de PSS ni autorización para levantar freeze.

## Raíces de lote y publicación parcial

Un journal corresponde EXACTAMENTE a una raíz (archivo, symlink o árbol). El
codec de inventario puede representar128 raíces para preflight del lote, pero
create, State y replay/open rechazan un journal multiraíz aunque su CRC sea válido.
El controlador de lote tendrá IDs estables de raíz y journals independientes;
no dividir clonando128 manifiestos fuera de presupuesto. El máximo8192entradas/
4MiB de manifiesto es agregado del lote admitido, no128 veces ese límite.
Antes de tocar cada raíz debe existir su Prepared durable. Una caída entre raíces
muestra cuáles completaron y cuáles quedaron interrumpidas; no se infiere éxito
ni restore global del estado de una raíz. Reanudar/restaurar es por raíz con sus
identidades, sin deshacer las ya completadas ni reclamar nombres ajenos. El
registro de lote agrupa resultados, nunca sustituye barreras de cada journal.
Para un árbol, staging/publicación de su raíz es un rename único, y el borrado
interior sí es por entrada; esa distinción elimina el falso rename atómico del
lote. Recuperación UI y controlador multiraíz aún son parte pendiente de M25.

## Publicación y eliminación

El ejecutor mantiene padres/objetos por FD. Usa staging privado0700, nombres
exclusivos y no-clobber; nunca remove_dir_all sobre ruta reconstruida. El origen
se puede aislar por rename en staging del MISMO FS, con intención persistida
antes y comprobación posterior de inode; no es aún eliminación de datos. Una
entrada reemplazada o publicación no confirmada conserva todo y exige revisión.

Estados durables, en orden (cada flecha requiere ACK de fsync del journal):

1. **Prepared**: manifiesto completo acotado, identificadores/parents/staging
   previstos. Journal creado exclusivo, archivo y directorio sincronizados.
2. **StageIntent**: se registran identidades de staging privado ya sincronizado,
   antes de renombrar origen. Recuperar inspecciona ambos nombres por identidad.
3. **SourceStaged**: identidad origen confirmada en staging; sincronizar staging
   y padre origen. Ninguna copia autoriza eliminar esta fuente preservada.
4. **DestinationVerified**: cada entrada destino tiene identidad registrada,
   contenido y metadatos comparados, file fsync y directorios staging fsync.
   Root inode/mtime NO acredita descendientes. Verificar permisos/uid/gid,
   timestamps, ACL/xattrs y topología de hardlinks; fallo conserva origen.
5. **PublishIntent**: write-ahead antes de publicar con rename NOREPLACE.
6. **DestinationPublished**: publicación confirmada por identidad. Puede estar
   visible pero NO durable. Error/caída aquí nunca habilita borrar origen.
7. **DestinationSynced**: destino y padre sincronizados después de publicación,
   identidades confirmadas y este estado sincronizado en journal. Un error de
   sync posterior a rename se informa como publicado no confirmado/durable;
   no volver a asumir el estado anterior ni sobrescribir entrada ajena.
8. Por cada entrada, hijos antes que padres: **DeleteIntent(i)** sólo después de
   revalidar el origen capturado y el destino durable por entrada (contenido,
   revisión, metadatos e identidad). Persistir intención, comprobar otra vez y
   unlink relativo al FD del staging propio. **Removed(i)** sólo después de
   sincronizar su directorio. Fallo intermedio detiene siguientes eliminaciones.
9. **Completed** sólo tras todas las entradas eliminadas, limpieza de staging
   vacío cuya identidad coincida y sincronizaciones finales confirmadas.

Un ACK fallido de journal puede haber escrito bytes visibles. La instancia se
marca incierta y deja de mutar; no actualiza estado confirmado ni reintenta una
acción suponiendo que nada ocurrió. Al reabrir, parsear registros íntegros bajo
lock; cola cortada/corrupta, versión futura o identidad cambiada => conservar
bytes y modo revisión, sin truncar/saltar el problema. CRC detecta corrupción
accidental, no autenticidad frente al mismo usuario hostil.

## Recuperación e identidades

Sin reanudar tareas al arrancar. Enumerar journals acotadamente fuera de UI y
mostrar interrumpidos; recuperación sólo mediante acción explícita. Lock por
journal mantenido durante carga/actualización; no tomar control de job vivo.
Validar nombre→FD, propietario, tipo regular, límites, parents/staging/entradas.
Si padre fue renombrado y no puede resolverse con identidad, informar ubicación
no confirmada; no operar sobre un reemplazo en el path antiguo ni buscar todo FS.

La recuperación es idempotente: intención no equivale a syscall ejecutado.
Comparar origen/staging/destino contra identidades registradas, distinguir ausente
de inaccesible; si coincide un objeto ajeno, no mover/borrar/reclamarlo. Un destino
ya publicado verificado puede conservarse mientras se restaura origen con
NOREPLACE: resultado parcial explícito, nunca Move completo ni doble borrado.
Después de DeleteIntent, una entrada ausente permite confirmar su eliminación
sólo con destino durable intacto y sync del padre; una entrada nueva NO se toca.
Toda verificación de borrado se repite al recuperar, aunque el journal tuviera
una verificación anterior. Incluso DestinationSynced legible puede provenir de
un ACK fsync fallido: renovar verificación y fsync de destino, padres y journal
en esta ejecución antes de cualquier nueva eliminación, sin asumir reboot. Limpieza sólo de archivos/staging propios confirmados.

Pausa/cancel antes de publicar permite restaurar origen sin clobber; después de
publicar detiene eliminación y conserva/restaura origen cuando sea verificable.
Raíces previas completadas no tienen rollback global. Cierre cancela TODOS los
subsistemas, drena worker/ACK y muestra rescate; syscall bloqueada sigue siendo
limitación explícita. Rename/unlink Linux no son inode-conditionales: queda
intervalo frente a escritor hostil; staging privado reduce exposición pero no
prueba exclusión de FDs abiertos ni confinamiento de todos los ancestros.

## Revisiones alteradas por nuestras operaciones

La revisión inicial no se compara ciegamente después de staging/eliminación:
rename puede cambiar ctime; eliminar hardlinks cambia nlink/ctime del mismo inode;
quitar hijos modifica mtime/ctime/nlink de directorios. El ejecutor mantiene un
ledger por inode de enlaces internos aún existentes y los nombres hijos propios.
Sólo actualiza revisión observada inmediatamente tras una syscall propia cuya
identidad/publicación está confirmada; no predice timestamps ni acepta una
revisión nueva arbitraria. Volver a comprobar contenido/hash, metadata no mutada,
conjunto de nombres y enlaces ajenos esperados; discrepancia => conservar y parar.
Para directorios, comparar hijos restantes por manifiesto (no sólo mtime); para
hardlinks, validar topología y bytes una vez por inode y todas las entradas por
nombre. Casos obligatorios: dos hardlinks de un inode y carpeta con varios hijos,
con y sin escritor externo. Si aún no se implementa ese ledger, esos tipos se
rechazan ANTES de staging; no ampliar el comparador a ignorar ctime/nlink global.

## Aceptación exigida antes de habilitar EXDEV

- SameFS sigue intacto. Transferencia entre FS real separada de fault injection;
  fixtures propios, nunca datos personales ni /tmp, sin montar/configurar global.
- Bytes, permisos/ACL/xattrs/timestamps/hardlinks/symlinks por tipo soportado;
  unsupported o error => fuente conservada y resultado parcial claro.
- Caída/fallo en cada transición antes/después write, fsync, rename, unlink;
  reabrir dos veces, sin borrar objetos reemplazantes ni journals incompatibles.
- Source/dest/padres sustituidos; descendiente editado durante copia; colisión,
  destinos dentro de origen, raw paths, cancel previo y tras progreso real.
- UI real revisar/confirmar/progreso/resultado/interrumpidos, ambas carpetas y
  pestañas; cierre normal tras ACK, helpers/FD propios liberados. Escenas internas
  no equivalen a input SO. Instalar sólo checkpoint visible validado coherente.
- Recursos/startup NOT_MEASURED hasta que el usuario levante freeze; no declarar
  objetivo memoria ni cierre global con sólo tests/caps de estos módulos.

## Inventario efectivo en curso

Recorrido FD Linux: padres por componentes sin symlinks, O_PATH primero para no
abrir FIFO/dispositivo, datos O_NOFOLLOW|NONBLOCK|NOATIME (fuentes propias por
ahora), fdopendir sobre dupCLOEXEC de directoryFD NOATIME, cierre RAII. Scratch
único64KiB,8192entradas/64niveles/4096Bpaths/1MiB capacidad+nombres pendientes,
xattrs64/128KiB agregado (incluye ACL/security). SHA256 una vez por grupo inode,
atributos y nombres de hijos ordenados, revisión antes/después. Orden final raw,
verify por lookup prestado sin segundo inventario owned. No snapshot atómico ni
interrupción de syscall bloqueada; cancelar entre syscalls/chunks.

AccessTime separado conserva primera observación para destino. Directorios y
files NOATIME no cambian atime por estas lecturas. Symlink readlink puede cambiar
atime del origen: no se restaura mediante escritura y no sirve como detector de
actor externo; verify reutiliza acceso inicial, también entre alias hardlink del
symlink. Contenido, mtime/ctime, ownership/modo/xattrs y topología sí se comprueban.
Hardlinks entre raíces requieren ledger compartido de lote; mientras no exista,
se rechazan antes de staging. Hardlinks dentro de un árbol no acreditan soporte
global entre raíces ni ledger de borrado. Ninguno de estos módulos habilita EXDEV
en la UI todavía; ejecutar copy/publish/recovery es el siguiente trabajo de M25.

Copia a staging: el primer backend mantiene FD de cada nodo destino creado para
cleanup runtime sin reutilización de inode; antes de crear limita128nodos por
raíz. Es límite temporal explícito del backend, no reducción del alcance final
de árboles8192/lotes/hardlinks/journal. Fuente no se borra ni publica en esta
fase. Debe contabilizarse junto a revisión/cola antes del wiring; no multiplicar
128FD por8jobs ni asumir que los límites aislados acreditan memoria/FDglobal.
El soporte mayor necesita estrategia de propiedad/recuperación por manifiesto
sin retener un FD por nodo, aún pendiente, sin reinterpretar identidades dudosas.

## M25: executor y recuperación de archivo único (WIP, 2026-10-27)

`exdev_move` ejecuta un archivo regular nlink=1; rechaza árboles, symlinks y
hardlinks ANTES de staging. NativeMove sigue separado. La copia efectiva de
árboles tiene tests de staging, pero no autoriza su publicación/borrado por este
executor. No hay soporte EXDEV UI ni recuperación automática habilitados aún.

Cada rename propio tiene fase retornó/identidad confirmada separada. Se conserva
la revisión nueva sólo tras comparar bytes/metadatos/revisión previa (excepto
ctime de ese rename); los recibos SourceStaged/DestinationPublished persisten la
revisión efectiva. Tras unlink se retiene sync pendiente; retry no borra un nuevo
ocupante. Stage planificado no creado nunca autoriza cleanup; creado sin FD o
binding confirmado queda candidato de rescate, no propiedad inferida por nombre.

Recovery abre FDs nuevos y exige los parentIDs y stageIDs persistidos. Antes de
cada acción comprueba salud/binding del journal: ACK incierto exige reabrir, no
retry mutante sobre la misma instancia. Publicación con PublishIntent sin ACK
requiere identidad destino registrada + hash/metadatos completos. Renueva sync
del archivo publicado, destination_parent, destination_stage y journal+su padre
antes de borrar origen. DestinationPublished no registra DestinationSynced si
falla la barrera del stage. Ausencia no se convierte en identidad: sólo se
reconoce origen ya desvinculado dentro de stage conocido con DeleteIntent, tras
verificación/sync renovados del destino; nuevo ocupante se conserva con error.
RestoreIntent reabierto verifica origen por inode/firma, restaura no-clobber y
sincroniza ambos directorios. Destino publicado nunca es cleanup de restore.
Copias parciales sin identidad registrada y stages previos a StageIntent se
conservan como candidatos; recuperación informa error en vez de reclamarlos.

Pausa/cancel cooperan en fases, traversal y cada chunk de hash/copia64KiB. La
restauración runtime drena a pesar de Cancel para conservar origen; no promete
interrumpir syscall en mount offline. Tras DeleteIntent una eliminación ya
iniciada puede finalizar pese Cancel tardío. Padres iniciales se abren con
O_NOFOLLOW final, pero sus ancestros pueden ser symlinks: no es confinamiento
absoluto. El recorrido descendiente sí es FD-relative/no-follow.

Verificación de núcleo:191tests PASS,3ignored; Clippy core all-targets-Dwarnings
PASS. Dos tests opt-in ejecutados aparte: transferencia real st_dev63→75; tres
pares de procesos en esos FS (salida73 sin Drop tras publish-rename, source-unlink
o restore-rename, luego proceso nuevo recovery exit0). No prueba powerloss,
SIGKILL ni fsync que falló realmente: fallos de sync/ACK son hooks declarados.
Primer FAIL atime del harness y Clippy inicial se conservan en target; lectura
fixture pasó a O_NOATIME sin relajar comprobación del motor.

Falta wiring de cola/AuxIo/UI y sus presupuestos conjuntos de FD/metadata,
recuperación visible, captura, instalación; luego ledger de árboles/hardlinks y
lotes por raíz. Sin nuevo release ni instalación por esta fundación. M24 queda
instalado. Recursos/startup NOT_MEASURED: freeze intacto y FAIL previos activos.


## M25 — integración visible validada, 2026-10-27

Esta entrega sustituye el estado sin UI de la fundación anterior. F6 conserva
revisión no mutante y cola global; archivo regular único nlink1 en distinto FS
usa el executor verificado. Acciones → Movimientos interrumpidos enumera nombres
y origen→destino, con estado legible y detalles técnicos copiables. Finalizar o
Restaurar son explícitos y dependen de la fase; Completed/Restored no ofrecen
recuperación y RestoreIntent sólo permite Restaurar. No reanudación automática.

El catálogo abre el mismo directorio enumerado por FD. Una acción exige prueba
de identidad del directorio/journal y SHA256 del manifiesto revisado; sustituir
un job por otro no cambia silenciosamente sus rutas. El owner/lock retenido se
transfiere al request y sólo se libera dentro del worker antes de reopen. Rechazo
de admisión conserva owner; cancel pendiente deja journal redescubrible.

586 tests Rust PASS (193core/67helper/326UI), tres entradas opt-in ignored en
suite ordinaria documentadas y ejecutadas aparte cuando corresponde. Clippy
workspace/all-targets-Dwarnings y release limpios. Escena
exdev-ui-1793122272433112203.json PASS: tres casos reales st_dev63→75, review y
pausa sin mutación; transferencia, proceso nuevo tras gap publish y restore,
normal exit0 posterior a ACK y stages ausentes. Diez framebuffer con dimensiones
reales comprobadas. Ventana INICIAL440×440 muestra controles completos; títulos
y diagnósticos largos recortan, no se acredita lectura completa ni clipboard SO.
Resize en vivo sigue UNVALIDATED: oscilación440↔1000 preservada en FAIL anterior,
no sustituida por aceptación de cualquier captura. Método handlers/Actions +
framebuffer, sin input SO, SIGKILL, powerloss ni fallo fsync real.

Pendientes: EXDEV de árboles/lotes/hardlinks, ledger compartido por lote,
conflictos generales y recuperación de otras tareas. O01/O03/X02 permanecen
parciales. Previews/medios nuevos pausados por instrucción del usuario: siguientes
unidades visibles doble panel, cortar/pegar, DnD, operaciones y SFTP montado.


## Presupuesto conjunto de la integración Linux

Contabilidad local; NO es medición de proceso/PSS ni gate global aprobado.

| Propiedad | Límite/admisión | Coexistencia considerada |
|---|---|---|
| Cola |4MiB,8 pendientes,1activo/ACK | Request.metadata_bytes reserva1MiB para cada EXDEV/Recovery; rutas/capacidades por separado; no ocho motores |
| FD tokens |256 incluyendo reserva24 para último resultado externo | Native roots+2; EXDEV/Recovery reserva24 (retorno+temporales), activos/ACK+pendientes se suman; revisión AuxIo adicional hasta130 =>386 tokens; catálogo hasta4 FD adicionales =>390 tokens conservadores (no total del proceso) |
| Resultado | un Outcome completo, reserva2MiB | CopyError nativo O ExdevFailure; no ambos. Model libera copy_last antes de siguiente dispatch. Recovery mueve owner previo dentro de Request; worker lo suelta justo antes reopen; rechazo devuelve owner intacto |
| EXDEV retenido | reserva1MiB/input; un archivo sin hardlinks | Inventario capacity32 Entries (~7KiB)+nombre255; manifest dos padres4096+raíz255 (<12KiB); journal/state/mapas de una entrada; 2stage names;1Created pin; contextos/proof/copias de rutas y reporte de1raíz/candidatos. Xattrs/hash scratch NO se conservan en resultado |
| Wire recovery/catálogo |64KiB ANTES readVec; genérico WAL8MiB no se usa en UI | decode/scan/materialización limitados por ese wire, rechazo de árboles antes retener Recovery. intent_proof encode de manifiesto single-file <12KiB; fstat/encode sólo worker/AuxIo |
| Trabajo EXDEV | scratch64KiB +attrs64/128KiB agregado por fase | trabajo transiente de hash/copia/verificación/metadata adicional al retenido; reservado conservadoramente2MiB en planificación, no adjudicado como consumo app. Syscalls/kernel/allocator/FLTK aparte |
| Catálogo |32 filas,1024 entradas examinadas, cargo conservador512KiB | preflight incluye2×padres/nombres (growth de join), raíz+2048B por fila para structs/proof/diagnóstico; old UI+new response+wire/transientes requieren reserva separada3MiB, no sólo512KiB |

El resultado de error conserva fase/intent proof/ID, rutas capturadas y ubicaciones
resueltas por FD, staging candidato y owner Transfer/Recovery. No se convierte en
sólo CopyReport ni se opera leyendo rutas visuales. Cancelar Recovery pendiente
libera FDs pero deja journal redescubrible en el catálogo (regresión). Root de
catálogo se enumera y abre por FD; la acción exige identidad directorio+archivo y
SHA256 del manifiesto revisado antes de actuar. No autentica contra un escritor
hostil del mismo usuario ni sustituye barreras actuales. Cambios de bytes/identidad
/intención dan error para revisar; no reinterpretar mismo job con otras rutas.

Estos caps/reservas incluyen los nuevos objetos locales; **el presupuesto global
real sigue NOT_MEASURED por freeze**. UI/FLTK/listado/previews instalados/OS threads
se agregan al proceso y helpers; no deducir objetivo50MB de esta tabla. Baseline y
arranque FAIL anteriores siguen abiertos.
