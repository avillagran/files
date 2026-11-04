# Transferencia multi-origen — contrato de implementación, 2026-10-27

Estado al 2026-10-28: preparación y ejecución provisional same-FS y reserva
del resultado parcial en la cola están probadas en fixtures. No hay entrada
visible para lotes multipadre, ni integración con MIME/Cut o reintento desde
UI; Copy multipadre, EXDEV de lote y aceptación SO siguen pendientes. No es
un checkpoint funcional ni modifica los binarios instalados. Suite workspace:
667 pruebas aprobadas, 3 ignoradas; no se midieron memoria ni arranque.

Alcance: hasta 128 raíces de hasta 32 padres raw en una oferta MIME local.
Primero Copy y Cut existentes; no cambiar el significado de EXDEV ni declarar
el lote completo hasta que revisión, ejecución, reintento y UI estén conectados.
No truncar ni convertir una ruta no UTF-8. Una oferta equivocada o cambiada se
rechaza en conjunto antes de abrir revisión.

- Agrupar rutas prestadas tras preflight completo (límites por ruta y agregados),
  contar padres y raíces antes de reservar/clonar y comprobar nombres de destino
  duplicados por bytes raw entre padres. Si hay colisión, mostrar ambos orígenes
  y pedir reducir/corregir el lote; ningún elemento se encola ni se mueve.
  También rechazar raíces solapadas (una seleccionada bajo otra). Preflight
  léxico detecta rutas aparentes; en AuxIo comprobar ancestros y aliases físicos
  mediante descriptores/dev+ino, incluyendo padres enlazados y bind mounts,
  antes de revisión y repetir las comprobaciones que dependen de ubicación al
  ejecutar. Si no se puede probar independencia, rechazar el lote con aviso.
  Recorrer `..` y comparar dev+ino NO demuestra independencia ante un bind mount
  cuyo origen sea un subárbol: ese recorrido puede subir por otro montaje.
  Condición conservadora en desarrollo: `statx` sobre FD padre y cada raíz;
  si dos padres comparten dispositivo pero tienen IDs de montaje diferentes,
  o una raíz directorio y cualquier padre tienen IDs diferentes (incluso en
  dispositivos distintos), o una raíz directorio y el destino efectivo tienen
  IDs diferentes, rechazar por alias ambiguo. Si faltan IDs,
  rechazar. Para montajes iguales se comparan identidades y ancestros anclados,
  y se guardan/revalidan ambos antes de preparar el motor. No cubre todas las
  semánticas de FS virtual ni una carrera hostil entre verificación y syscall;
  la captura sigue provisional hasta conectar validación de los FD efectivos.
- AuxIo captura un Source anclado por padre, con firmas individuales de cada
  raíz y cancelación; al menos un grupo fallido descarta toda la revisión. El
  total de capacidades, FDs y scratch debe admitirse antes de clonar o abrir;
  la revisión, cola, activo y resultado sin ACK comparten los límites existentes.
  Ninguna verificación por ruta seguida de recaptura autoriza un sustituto:
  comparar cada FD capturado por motor contra la firma del grupo.
- Un lote lógico ocupa **un** slot de la cola global (o admisión equivalente
  que admita de verdad 32 padres), no 32 `enqueue` parciales contra ocho slots.
  El mismo worker ejecuta grupos/raíces secuencialmente, con ID y rutas de
  origen/destino capturadas; el cambio de panel, pestaña o foco no redirige
  la operación. No reservar más de 256 FD entre entradas, review, activo,
  ACK y scratch, y rechazar el lote íntegro antes de cualquier mutación.
- Revisión muestra cada padre y cuenta, destino y conflictos. Orden de ejecución
  estable; si un grupo falla o cancela, informar resultados por raíz y conservar
  los pendientes para reintento con sus **firmas originales**, sin recaptura.
  No prometer rollback global tras publicar raíces anteriores. Cut sólo consume
  raíces confirmadas por el motor de mover; un ACK viejo no consume un clipboard
  nuevo. CopyReport.completed no autoriza borrar fuentes EXDEV.
  El ledger pertenece a una preparación concreta, no sólo al conjunto de firmas:
  dos revisiones del mismo origen hacia destinos distintos no pueden intercambiar
  estados. El resultado parcial debe conservar juntos la preparación y el ledger,
  con su cargo de memoria y FD, hasta que el receptor consuma el ACK o descarte
  expresamente el reintento; no liberar el cargo al terminar el worker.
- SameFS usa rename seguro existente; EXDEV sólo usa su motor/journal y sus
  límites actuales, sin degradar silenciosamente metadata ni borrar tras una
  copia no verificada. El caso no soportado conserva origen y muestra motivo.
  Hardlinks entre raíces/grupos se rechazan antes de mutar hasta contar con
  ledger de lote que mantenga topología y revisiones propias.
- Refrescar ambas carpetas/paneles afectadas según rutas/identidades capturadas,
  no sólo el panel activo; un panel cerrado se marca para recargar al volver.
  El cierre cancela, drena el único worker y conserva informes/rescate.

Aceptación: 32 padres/128 raíces, nombres raw, colisión entre padres sin mutación,
pruebas de límite y rechazo atómico con cola llena/presupuesto, cambio de origen
antes de captura efectiva, cancelación parcial/reintento y ACK obsoleto, cambios
de foco/cierre y Copy/Cut con fixtures. Una escena con UI/revisión y comprobación
de filesystem debe indicar que usa handlers y pipes privados; integración SO se
valida aparte. No inferir presupuesto PSS de caps locales; freeze vigente.
