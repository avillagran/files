# Recorrer elementos desde la vista previa — 0.0.4

M19 añade botones Nerd Font anterior/siguiente en el header, con nombre accesible
“Elemento anterior/siguiente de la carpeta, Alt+Arriba/Abajo”. Ambos y los atajos
llaman la misma Action. Son elementos del orden visible actual, incluidos carpetas
y archivos sin preview soportada; no es una lista de reproducción automática.

El recorrido usa índices ya presentes en el modelo, sin stat/readdir en UI ni
precarga. Ocultos/filtro/orden conservan el criterio de la vista de origen. En un
extremo no se vuelve a abrir el elemento ni se inicia otro helper; mientras el
listado no esté completo no se cambia la selección desde estos controles.

Cada cambio real pasa por selección/open_preview existentes: cierra el estado
anterior, cambia generación/firma y cancela; sólo el último destino queda pendiente
hasta ACK/reap. El error de un archivo conserva los botones para volver a otro.
Nombres Path/OsString raw no se convierten con pérdida para abrir o seleccionar;
el label visible puede usar la representación existente del path.

Los controles son por carpeta visible, no back/forward de enlaces Markdown,
pestañas, playlist ni navegación recursiva. No persisten un índice/caché nuevos.
488Rust (141core/58helper/289UI), Clippy workspace/all-targets-Dwarnings,
release yfmt PASS. `preview-neighbor-ui-1793101665648168454.json` PASS funcional:
5framebuffers1000×650, PNG→WebP→PNG de nombre raw0xFF, sin visitar.hidden.png;
se comprueban selección/path raw/revisión efectiva del FD y píxeles rojo/azul/verde.
Hit targets dibujados≥32×28 y nombres accesibles comprobados. Borde inicial no
cambia revisión ni lanza decode; error real de PNG corrupto conserva controles,
volver recupera imagen y back/next/back sólo publica último destino azul.
Root0trascallback/ACK,7identidades de helpers registradas ya no vivas; Z no
aceptado. No helper observado vivo al cerrar en esta escena, ni prueba de
lifetimes/inputSO/clipboard/AT-SPI. Se inspeccionaron imágenes de raw y error,
sin tofu evidente en botones o colisiones de header a1000×650.
Freeze recursos/startup vigente: NOT_MEASURED, sin afirmar presupuesto.

Checkpoint7ef3852 instalado con backup`/home/avillagran/.cache/files-test/installed-backups/1793101735048288545`, hashes release/instalación
coherentes en`docs/evidence/0.0.4/installed-checkpoint-1793101735171805904.json`. files`3a59002aa2935a715e09a24c29fddd0cf0b267c9ba70e9f07c49eac0d9097340`.
