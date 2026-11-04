# Resumen de carpetas en Space

0.0.4, parcial. Space sobre una carpeta muestra tamaño aparente recursivo y
entradas descendientes. Reutiliza el helper du existente: no hace recorrido ni
sondas filesystem en el hilo UI. La intención de carpeta viene del listado.
El nuevo comando preview_directory exige una carpeta real al abrirla; si el
nombre fue sustituido por un archivo o symlink, devuelve error. El comando du
original conserva su API para cálculo de tamaño de archivos.

El recorrido Linux ancla cada directorio con FD y O_DIRECTORY/O_NOFOLLOW.
No sigue symlinks, cuenta hardlinks por nombre y mide bytes aparentes, no
bloques físicos. No es un snapshot ante cambios concurrentes. Límites de du:
1M entradas, 128 niveles y 15s cooperativos; supervisor 20s, cancelación/reap
fuera de UI. Un syscall de un montaje offline puede no ser interrumpible por
la señal; ese escenario y adaptadores de otros SO siguen pendientes.

279 tests Rust (73 core, 35 helper, 171 UI), Clippy workspace all-targets
-D warnings y release limpios. folder-ui-1793040015107090730.json funcional
PASS: árbol propio de 8 bytes/5 entradas con symlink cíclico y enlace a otro
archivo de 1MiB excluido del total, carpeta vacía, sustitución de carpeta por
archivo tras listar, cambios rápidos y cierre normal tras ACK. Método: handlers
y framebuffer real; sin input SO ni prueba de kill de un hijo activo al cerrar.

Recursos y arranque NOT_MEASURED por la fase explícita del usuario. No se
extrapola el gate du anterior al binario actual. Treemap, caché, snapshot del
árbol y montajes offline no están completos.
