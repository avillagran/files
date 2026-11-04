# Nautilus como piso mínimo — matriz de paridad

2026-11-04. Pedido del usuario: Files debe tener como mínimo las capacidades
de Nautilus (GNOME Files 47–49) y tomar lo mejor de otros gestores.

Fuentes de Nautilus: GNOME release notes 47/48, `NEWS` de Nautilus, ayuda
de GNOME Files y, de la versión 49.0, `help-overlay.ui` (atajos),
`nautilus-preferences-dialog.ui` y `freedesktop-dbus-interfaces.xml`.
El estado de Files se verificó en el código (no en docs) a fecha 6348af9.

Estados: **sí** · **parcial** · **no**. Prioridad: P1 = se nota en el uso diario.

| # | Capacidad (Nautilus) | Files | Brecha | Prioridad |
|---|---|---|---|---|
| 1 | Lista/grilla, zoom Ctrl±/0 | parcial | zoom sólo en grilla | P2 |
| 2 | Columnas configurables y orden por tipo/dueño/permisos/fechas | parcial | columnas fijas Nombre/Tamaño/Fecha | P1 |
| 3 | Carpetas expandibles en lista | no | — (Columnas/Miller como alternativa) | P3 |
| 4 | Pestañas, historial por pestaña | sí | — | — |
| 5 | Barra de ruta + Ctrl+L, `/` y `~` | sí | — | — |
| 6 | Escribir para filtrar | sí | — | — |
| 7 | Ocultos Ctrl+H | sí | — | — |
| 8 | Miniaturas imagen/vídeo/PDF/carátulas | parcial | PDF, carátula audio, lista | P2 |
| 9 | Selección por rectángulo (marquee) | **sí** | bloque 3: Grupos/Lista/Grilla, Ctrl/Shift suma, clic en vacío limpia | — |
| 10 | Seleccionar por patrón Ctrl+S | parcial | dos pasos (filtro + marcar) | P2 |
| 11 | Portapapeles con otras apps | parcial | bloque 3: Ctrl+V pega lo último copiado (Files o sistema); sin prueba de punta a punta (no se toca el portapapeles personal) | P2 |
| 12 | Arrastrar y soltar con otras apps | parcial | soltar siempre pasa por revisión | P2 |
| 13 | Papelera: vaciar, restaurar elegidos, Shift+Supr | **sí** | bloque 1: Shift+Supr con confirmación, Restaurar/Eliminar/Vaciar en el catálogo | — |
| 14 | Deshacer/rehacer operaciones | parcial | sólo renombrar y último lote a papelera | P2 |
| 15 | Renombrar y renombrado masivo | sí | (Files supera: regex, secuencias) | — |
| 16 | Nueva carpeta, con selección, plantillas `~/Templates` | parcial | carpeta con selección, plantillas | P2 |
| 17 | Comprimir/extraer zip, tar.*, 7z | parcial | sólo ZIP | P2 |
| 18 | Crear enlace Ctrl+M / Shift+Ctrl+M | parcial | bloque 1: Shift+Ctrl+M; falta Ctrl+M (enlace a lo copiado) | P2 |
| 19 | Propiedades: contenido, espacio libre, dueño/grupo, recursivo | parcial | sin conteo/espacio/dueño/recursivo | P2 |
| 20 | Abrir con otra app y fijar predeterminada | **sí** | bloque 2: Enter abre con la predeterminada; Abrir con… (Ctrl+Shift+O) | — |
| 21 | Terminal aquí (Ctrl+.) | **sí** | bloque 1 | — |
| 22 | Scripts de usuario / acciones | no | — | P2 |
| 23 | Conflictos: reemplazar, fusionar, por archivo | parcial | sin reemplazar ni fusionar | P2 |
| 24 | Progreso de operaciones | sí | — | — |
| 25 | Destacados (estrellas) | no | — | P2 |
| 26 | Recientes | no | — | P2 |
| 27 | Lateral: marcadores, dispositivos vivos, red | parcial | bloque 4: marcadores y dispositivos vivos; falta red | P3 |
| 28 | Montar/desmontar/expulsar, espacio por dispositivo | **sí** | bloque 4: udisksctl, espacio libre, expulsar con apagado | — |
| 29 | Red: sftp/smb/ftp/dav/nfs | parcial | sólo SFTP | P3 |
| 30 | Búsqueda con filtros y texto completo | sí | — | — |
| 31 | Ejecutables y `.desktop` | no | — | P2 |
| 32 | Selector de archivos (portal) | no | — | P3 |
| 33 | `org.freedesktop.FileManager1` y `.desktop` | no | «Mostrar en carpeta» de otras apps | P1 |
| 34 | Ayuda de atajos | sí | — | — |
| 35 | Marcadores Ctrl+D | sí | — | — |
| 36 | Copiar ruta | **sí** | bloque 1: Ctrl+Alt+C y menú contextual | — |
| 37 | Espacio libre | sí | — | — |
| 38 | Fondo de pantalla | no | — | P3 |
| 39 | i18n | no | — | P3 |
| 40 | Accesibilidad AT-SPI | no | FLTK sin puente AT-SPI | bloqueado |
| 41 | Vista previa Space | sí | Files supera (vídeo, PDF, HTML, DOCX, ZIP) | — |
| 42 | Suma de verificación | parcial | no en propiedades | P3 |

Lo que Files ya tiene y Nautilus no: doble panel (F9), Columnas/Miller,
mapa de espacio, tamaños de carpeta, comparar/sincronizar carpetas, duplicados,
renombrado con regex, búsqueda guardada, paleta de comandos, vistas previas
de vídeo/PDF/HTML/DOCX/ZIP.

De otros gestores (pendiente, después del piso): acciones personalizadas con
`%f/%F` (Thunar/Dolphin/Nemo), panel de terminal (Dolphin), etiquetas de color
(Finder), salto difuso/frecuencia tipo zoxide (yazi), administrador de tareas
en segundo plano unificado (yazi).

## Orden de trabajo

1. Papelera completa, crear enlace, copiar ruta y terminal con atajos.
2. Abrir con / predeterminada, `.desktop` + FileManager1.
3. Ctrl+V desde el sistema, selección por rectángulo.
4. Dispositivos vivos y montar/expulsar, marcadores en el lateral.
5. Columnas/orden, plantillas, recientes, destacados, tar, conflictos, scripts.

El avance de cada fila se registra aquí con commit y evidencia.

## Bloque 1 — 2026-11-04

- Borrado permanente (`remove_tree`): descriptores fijados, sin seguir
  enlaces, sin cruzar montajes, iterativo y cancelable, en hilo propio.
- Papelera: restaurar por nombre con renameat2 NOREPLACE, eliminar un
  elemento con sus metadatos, vaciar (incluidos metadatos huérfanos).
- Crear enlace (Shift+Ctrl+M), Ctrl+. terminal, Ctrl+Alt+C copiar ruta.
- Pedidos del usuario en la misma entrega: menú contextual agrupado por
  tipo (ABRIR, EDITAR, ORGANIZAR, COMPRIMIR, ELIMINAR, SELECCIÓN / CREAR,
  PEGAR, VER, SELECCIÓN, HERRAMIENTAS), «Agregar nueva» junto a CARPETAS en
  Grupos, y refresco de subcarpetas visibles (`child_watch`): inotify sin
  recursión sobre ≤64 carpetas visibles, en hilo propio; espera 1,5 s de
  calma, relee sólo esa fila (conteo y fecha) y recalcula un tamaño medido
  como mucho una vez por minuto por carpeta, en la cola secuencial de
  tamaños. Sin sondeo: coste cero en reposo. Límite: cambios a más de un
  nivel de profundidad no se detectan hasta recalcular a mano.
- Validación: `scripts/nautilus-floor-ui-check.py` en ventana real con
  papelera y fixtures privados; 712/112/807 pruebas Rust, Clippy, fmt.

Correcciones de la misma entrega (2026-11-04):
- Botón de Grupos: icono Nerd «carpeta +» (nf-md-folder_plus), sin texto,
  por pedido del usuario.
- Refresco de subcarpetas intermitente (2/3 fallos al instalar): una recarga
  de la carpeta abierta retiraba las vigilancias (`loaded=false`) y luego los
  índices visibles reordenados las sacaban un instante. Ahora una vigilancia
  dura 5 s fuera de la vista y una carpeta vigilada de nuevo tras un hueco
  relee su fila. 6/6 fuente y 3/3 instalado en NODE y en M2; FAILs previos
  conservados en `docs/evidence/nautilus-floor-20261104`.
- Instalado: NODE 7f35c3bd4f47, M2 a7298e28e489 (42587aa), 5 respaldos y
  hashes verificados en ambas.

## Bloque 2 — 2026-11-04: Abrir y Abrir con

Hallazgo crítico: Enter/doble clic ejecutaban el programa `open`, que en
Omarchy es sólo una función de bash (no existe en PATH en NODE ni M2):
abrir archivos NO funcionaba. Ahora, como Nautilus, `files_core::apps`
resuelve el tipo MIME (nombre, y contenido si es desconocido), la
aplicación predeterminada (cadena `mimeapps.list`, entradas instaladas,
usuario sobre sistema) y lanza con `uwsm-app -t service -- <id> <ruta>`
(lanzador de Omarchy; vuelve al instante). Sin aplicación: error visible.

«Abrir con…» (Ctrl+Shift+O, menú ABRIR): recomendadas con la
predeterminada primero, Tab muestra todas, Enter abre, Ctrl+Enter «Usar
siempre» escribe `[Default Applications]` + `[Added Associations]` del
usuario de forma atómica conservando el resto. NoDisplay oculto como en
GNOME. Escaneo y escritura fuera del hilo UI.

Pendiente con consentimiento del usuario: registrar Files como gestor de
carpetas (`.desktop` + `org.freedesktop.FileManager1`), porque reemplazaría
el comportamiento actual de Omarchy (Nautilus) en «Mostrar en carpeta».

Validación: `scripts/open-with-ui-check.py` (lanzador grabador, XDG
privados; nunca toca las asociaciones del usuario) 3/3 + final;
716/112/807 pruebas, Clippy. `open-close-ui-check.py` queda SKIPPED en
Linux: ejecutarlo lanzaría la aplicación real del usuario.

## Bloque 3 — 2026-11-04: Ctrl+V del sistema y selección por rectángulo

- Ctrl+V pega lo último copiado, como Nautilus: el búfer de Files mientras
  siga siendo la selección del sistema (conserva «cortar») o si el sistema
  no ofrece archivos; si otra app copió archivos después, se importan esos.
  El puente nativo expone `selection_ours` (fuente publicada no cancelada)
  y `files_offered`. Decisión cubierta por pruebas; la importación real
  exige gesto real y portapapeles del usuario: NO validada de punta a punta.
- Selección por rectángulo en Grupos/Lista/Grilla con borde de acento;
  Ctrl/Shift suma; clic en vacío limpia (antes no hacía nada).
- `build.rs`: dos copias idénticas de fltk-sys en distintos índices del
  registro (otro proyecto) ya no rompen la compilación; distintas siguen
  fallando.
- Fixtures de menú buscan la entrada por acción (las secciones movieron los
  índices). `keyboard-nav-ui-check` falla en Espacio (Ctrl+Shift+Fin) igual
  con el binario previo a la paridad (fa50e53): fallo preexistente pendiente.
- Las ventanas de prueba deben lanzarse con un `exec` de Hyprland por script:
  encadenados, la regla de workspace caduca y aparecían en el escritorio.

## Bloque 4 — 2026-11-04: dispositivos y marcadores en el lateral

- `files_core::devices`: `lsblk -J` (sin montajes de sistema, sin
  particiones EFI/reservadas/recuperación/arranque Apple, sin sistemas de
  archivos que el kernel no soporta), espacio libre, y montar/desmontar/
  expulsar (desmontar + `power-off` del disco) con `udisksctl` (polkit, sin
  root). Vigilancia por eventos: `/proc/self/mountinfo` (POLLPRI) e inotify
  en `/dev/disk/by-uuid`, retardo 400 ms, y una relectura cada 2 min para el
  espacio libre. Antes los montajes se leían una sola vez al arrancar.
- Lateral: hasta 8 marcadores y los dispositivos (icono USB/disco, montado
  o atenuado, espacio libre, botón expulsar/desmontar con el lateral
  expandido). Clic en un dispositivo sin montar lo monta y lo abre; al
  expulsar el actual se vuelve a Inicio. Menú de fondo: sección DISPOSITIVO.
- NODE real listaría `/mnt/ubuntu`, Gbte, Peliculas y `/mnt/avd`.
- Validación: `scripts/devices-ui-check.py` (lsblk de fixture + udisksctl
  grabador; ningún disco real tocado), pruebas 719/112/808, Clippy.
