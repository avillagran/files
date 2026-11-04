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
| 9 | Selección por rectángulo (marquee) | no | gesto de ratón básico | P1 |
| 10 | Seleccionar por patrón Ctrl+S | parcial | dos pasos (filtro + marcar) | P2 |
| 11 | Portapapeles con otras apps | parcial | Ctrl+V no pega lo copiado fuera | P1 |
| 12 | Arrastrar y soltar con otras apps | parcial | soltar siempre pasa por revisión | P2 |
| 13 | Papelera: vaciar, restaurar elegidos, Shift+Supr | **sí** | bloque 1: Shift+Supr con confirmación, Restaurar/Eliminar/Vaciar en el catálogo | — |
| 14 | Deshacer/rehacer operaciones | parcial | sólo renombrar y último lote a papelera | P2 |
| 15 | Renombrar y renombrado masivo | sí | (Files supera: regex, secuencias) | — |
| 16 | Nueva carpeta, con selección, plantillas `~/Templates` | parcial | carpeta con selección, plantillas | P2 |
| 17 | Comprimir/extraer zip, tar.*, 7z | parcial | sólo ZIP | P2 |
| 18 | Crear enlace Ctrl+M / Shift+Ctrl+M | parcial | bloque 1: Shift+Ctrl+M; falta Ctrl+M (enlace a lo copiado) | P2 |
| 19 | Propiedades: contenido, espacio libre, dueño/grupo, recursivo | parcial | sin conteo/espacio/dueño/recursivo | P2 |
| 20 | Abrir con otra app y fijar predeterminada | parcial | sin selector ni predeterminada | P1 |
| 21 | Terminal aquí (Ctrl+.) | **sí** | bloque 1 | — |
| 22 | Scripts de usuario / acciones | no | — | P2 |
| 23 | Conflictos: reemplazar, fusionar, por archivo | parcial | sin reemplazar ni fusionar | P2 |
| 24 | Progreso de operaciones | sí | — | — |
| 25 | Destacados (estrellas) | no | — | P2 |
| 26 | Recientes | no | — | P2 |
| 27 | Lateral: marcadores, dispositivos vivos, red | parcial | marcadores no listados, montajes leídos al inicio | P1 |
| 28 | Montar/desmontar/expulsar, espacio por dispositivo | no | — | P1 |
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
