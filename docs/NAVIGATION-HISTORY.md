# Historial de navegación y omnibar acotada — 0.0.4

Botones Nerd Atrás/Adelante a la izquierda de la omnibar, con tooltip y estado
inactivo; Alt+Izquierda/Alt+Derecha invocan las mismas Actions. Registro por
instancia, sólo rutas: máximo64 entradas entre ambas pilas y4096B por ruta
lossless, antes de copiar. Payload de nombres≤256KiB más contenedores fijos;
no equivale a presupuesto de proceso ni incluye el PathBuf ya poseído por caller.
No añade directorios, sondeos FS, caché de listado, helpers ni un worker por visita.

Se registran destinos solicitados, incluido un destino que después falla al
listar: Atrás puede recuperar la ubicación anterior sin sondeo síncrono en UI.
Visitar una ruta nueva descarta Adelante; misma ruta refresca sin duplicar historial.
El guardado Espacial debe admitir el snapshot antes de avanzar historial. Una
negativa de guardado/validación mantiene ruta y pilas. Load location invalida por
generación entradas/hits/hover/selección, cancela previews/du/treemap y reutiliza el
worker de listado. Navegación rápida reemplaza el último path pendiente; no crea
hilos para eludir un syscall bloqueado. No garantiza recuperación offline rápida.

Historial no restaura filtro/selección/scroll/vista por entrada. Pestañas/reabrir
pestaña/sesiones y persistencia de historial siguen pendientes V10/X02; no se
cuenta este bloque como esas features. No hay canonicalización/sandbox de rutas,
y la lectura FS mantiene sus errores reales.

Omnibar/filtro hacen preflight prestado antes de copiar paste/IME: máximo4096B
UTF8 después de excluir CR/LF/NUL. Rechazo muestra error y conserva texto y flag
de reemplazo, sin duplicar paste completo. Entrada válida posterior recupera.
Pruebas no equivalen a clipboard Wayland: el FAIL de foco/serial sigue vigente.
Source ranges/shaping/IME completo siguen pendientes U03.

Reproducir: cargo test --workspace; cargo clippy --workspace --all-targets --
-D warnings; cargo build --release; python3 scripts/history-ui-check.py. Fixtures
propias en ~/.cache/files-test. Pruebas puras cubren límite/branch/duplicados/raw,
paste Unicode/exceso; Model cubre varias navegaciones antes de draw, guardado
rechazado y conservación/reintento de input. La escena usa hits compartidos y
framebuffer/callback reales, no eventos SO ni clipboard externo.

Freeze recursos/startup sigue NOT_MEASURED. No se modifica presupuesto ni se
reemplazan fallos de baseline/arranque/clipboard; sin tag0.0.4 ni PASSglobal.

Validación final:399 Rust129core/44helper/226UI, Clippy workspace/all-targets
-Dwarnings y release PASS. history-ui1793061306527375025 funcionalPASS: seis
framebuffers1000×560, entrar/atrás/adelante/branch, missing real y recuperar,
paste1MiB rechazado conservando texto/replace y recuperaciónUnicode, callback
request/drain/exit0. No helpers de preview/du/treemap solicitados; lifecycle raíz,
no se afirma cleanup de hijos no observados. Alt+flechas tiene wiring compartido;
fixture usa botones/handlers internos, no eventoSO ni clipboard externo.
SHA files39db06227fa069b43d8bfe9c323b262d82ec46e420abea8dbcc8cdf10637426b,
helper4f7910cf27b1723b25d17a208b9c2287858decfb0e65abf23c534305aad620c0.
Otro Cargo de OmaBridge en target ajeno coincidió con compilación; no se tocó.
Se construyó con un job. Sin comparación de tiempos ni nueva medida de recursos.
