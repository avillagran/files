//! Bounded discovery/dispatch of existing Actions. Never parses shell commands or does I/O.
use crate::{
    model::{Action, Hit, Model, View},
    painter::{Painter, Rect},
};
use files_core::listing::Sort;
use std::collections::HashSet;
fn next_epoch() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut current = NEXT.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .expect("command input epoch exhausted");
        match NEXT.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(value) => return value,
            Err(value) => current = value,
        }
    }
}
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
const ROW: i32 = 44;
pub const QUERY_LIMIT: usize = 256;
pub const MAX_COMMANDS: usize = 96;
const PROOF_LIMIT: usize = 64 * 1024;
#[derive(Clone, Copy)]
enum Gate {
    Root,
    Listing,
    Focus,
    Selection,
}
enum Dispatch {
    Existing(Action),
    Parent,
    CloseTab,
}
pub struct Spec {
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
    aliases: &'static str,
    gate: Gate,
    dispatch: Dispatch,
}
impl Spec {
    fn label_for(&self, m: &Model) -> &'static str {
        if crate::selection_tools::is_map(m) {
            match self.dispatch {
                Dispatch::Existing(Action::Refresh) => "Recalcular Espacio y actualizar carpeta",
                Dispatch::Existing(Action::Selection(crate::selection_tools::Command::All)) => {
                    "Marcar objetos del mapa"
                }
                Dispatch::Existing(Action::Selection(crate::selection_tools::Command::Invert)) => {
                    "Invertir marcas del mapa"
                }
                Dispatch::Existing(Action::Selection(
                    crate::selection_tools::Command::Filtered,
                )) => "Filtro no aplicado en Espacio",
                _ => self.label,
            }
        } else {
            self.label
        }
    }
    fn shortcut_for(&self, m: &Model) -> &'static str {
        if self.id == "copy" && crate::selection_tools::is_map(m) {
            ""
        } else if self.id == "refresh" && crate::selection_tools::is_map(m) {
            "F5 / Ctrl+R"
        } else {
            self.shortcut
        }
    }
}
macro_rules! spec {
    ($id:literal,$label:literal,$key:literal,$aliases:literal,$gate:ident,$action:expr) => {
        Spec {
            id: $id,
            label: $label,
            shortcut: $key,
            aliases: $aliases,
            gate: Gate::$gate,
            dispatch: Dispatch::Existing($action),
        }
    };
}
pub const ITEMS: &[Spec] = &[
    spec!(
        "search",
        "Buscar archivos en subcarpetas",
        "Ctrl+Shift+F",
        "find search recursive buscar grep contenido texto",
        Root,
        Action::Search
    ),
    spec!(
        "filter",
        "Filtrar en esta carpeta",
        "Ctrl+F",
        "filter fuzzy buscar local",
        Root,
        Action::Filter
    ),
    spec!(
        "sizes-manager",
        "Gestor de tamaños por carpeta",
        "Ctrl+Alt+Espacio",
        "tamano size du cola queue manager errores",
        Root,
        Action::Sizes
    ),
    spec!(
        "sftp",
        "SFTP · conectar y controlar montaje",
        "",
        "ssh sshfs remoto remote conexion red network",
        Root,
        Action::Sftp
    ),
    spec!(
        "sizes-one",
        "Calcular tamaño de carpeta enfocada",
        "Ctrl+Espacio",
        "tamano size du folder calcular",
        Focus,
        Action::CalculateSize
    ),
    spec!(
        "sizes-marked",
        "Calcular tamaños de carpetas marcadas visibles",
        "",
        "tamanos seleccion marcadas size selected marked du calcular",
        Selection,
        Action::CalculateMarkedSizes
    ),
    spec!(
        "sizes-blocks",
        "Medir bloques de la carpeta enfocada",
        "",
        "bloques asignados allocation sparse hardlinks disco tamano",
        Focus,
        Action::CalculateAllocatedSize
    ),
    spec!(
        "sizes-marked-blocks",
        "Medir bloques de carpetas marcadas visibles",
        "",
        "bloques asignados marcadas allocation sparse hardlinks seleccion",
        Selection,
        Action::CalculateMarkedAllocatedSizes
    ),
    spec!(
        "sizes-list",
        "Calcular carpetas del listado",
        "Ctrl+Shift+Espacio",
        "tamanos size du batch calcular",
        Listing,
        Action::CalculateListedSizes
    ),
    spec!(
        "sizes-stop",
        "Parar cálculo de tamaños",
        "",
        "tamano cancelar size stop du",
        Root,
        Action::CancelListedSizes
    ),
    spec!(
        "sizes-pause",
        "Pausar siguientes tamaños",
        "",
        "tamano size du pausa pause esperar",
        Root,
        Action::PauseSizes(true)
    ),
    spec!(
        "sizes-resume",
        "Reanudar tamaños pendientes",
        "",
        "tamano size du continuar resume reanudar",
        Root,
        Action::PauseSizes(false)
    ),
    spec!(
        "sizes-retry",
        "Reintentar tamaños fallidos",
        "",
        "tamano size du reintentar retry error fallidos",
        Root,
        Action::RetrySizeErrors
    ),
    spec!(
        "copy",
        "Copiar selección: revisar destino",
        "F5",
        "copy copiar duplicate",
        Selection,
        Action::CopyFiles
    ),
    spec!(
        "create-zip",
        "Crear ZIP: revisar selección y destino",
        "",
        "zip crear comprimir empaquetar compress archive",
        Selection,
        Action::CreateArchive
    ),
    spec!(
        "extract",
        "Extraer ZIP: revisar y aplicar",
        "",
        "zip extraer extract descomprimir archivo",
        Selection,
        Action::ExtractArchive
    ),
    spec!(
        "move",
        "Mover selección: revisar destino",
        "F6",
        "move mover transferir",
        Selection,
        Action::MoveFiles
    ),
    spec!(
        "rename",
        "Renombrar selección: vista previa",
        "F2",
        "rename batch patrones regex renombrar",
        Selection,
        Action::Rename
    ),
    spec!(
        "trash",
        "Enviar selección a Papelera",
        "F8 / Supr",
        "trash delete borrar papelera",
        Selection,
        Action::Trash
    ),
    spec!(
        "open-trash",
        "Abrir Papelera de Inicio",
        "",
        "trash papelera abrir consultar inicio",
        Root,
        Action::OpenTrash
    ),
    spec!(
        "properties",
        "Propiedades y permisos",
        "Alt+Enter",
        "properties chmod permisos",
        Selection,
        Action::Properties
    ),
    spec!(
        "new-dir",
        "Crear carpeta",
        "F7",
        "mkdir new folder carpeta crear",
        Listing,
        Action::NewEntry(true)
    ),
    spec!(
        "new-file",
        "Crear archivo",
        "Ctrl+N",
        "touch new file archivo crear",
        Listing,
        Action::NewEntry(false)
    ),
    spec!(
        "clipboard-copy",
        "Copiar al buffer de files",
        "Ctrl+C",
        "clipboard copy portapapeles copiar",
        Selection,
        Action::ClipboardCopy
    ),
    spec!(
        "clipboard-cut",
        "Cortar al buffer de files",
        "Ctrl+X",
        "clipboard cut portapapeles cortar",
        Selection,
        Action::ClipboardCut
    ),
    spec!(
        "clipboard-paste",
        "Pegar desde el buffer de files",
        "Ctrl+V",
        "clipboard paste portapapeles pegar",
        Listing,
        Action::ClipboardPaste
    ),
    spec!(
        "open",
        "Abrir elemento enfocado",
        "Enter",
        "open abrir",
        Focus,
        Action::Open
    ),
    spec!(
        "preview",
        "Vista previa del elemento enfocado",
        "Espacio / F3",
        "preview previa previsualizar ver leer",
        Focus,
        Action::Preview
    ),
    spec!(
        "copy-path",
        "Copiar ruta del elemento",
        "",
        "path ruta copiar",
        Focus,
        Action::CopyPath
    ),
    spec!(
        "undo",
        "Deshacer operación disponible",
        "Ctrl+Z",
        "undo deshacer",
        Listing,
        Action::Undo(false)
    ),
    spec!(
        "redo",
        "Rehacer renombrado disponible",
        "Ctrl+Shift+Z",
        "redo rehacer",
        Listing,
        Action::Undo(true)
    ),
    spec!(
        "history-back",
        "Atrás en el historial",
        "Alt+Izquierda",
        "back history atras",
        Root,
        Action::History(false)
    ),
    spec!(
        "history-forward",
        "Adelante en el historial",
        "Alt+Derecha",
        "forward history adelante",
        Root,
        Action::History(true)
    ),
    Spec {
        id: "parent",
        label: "Subir a la carpeta superior",
        shortcut: "Backspace",
        aliases: "parent up subir",
        gate: Gate::Root,
        dispatch: Dispatch::Parent,
    },
    spec!(
        "tab-new",
        "Nueva pestaña",
        "Ctrl+T",
        "tab nueva pestana",
        Root,
        Action::Tab(crate::tabs::Command::New)
    ),
    Spec {
        id: "tab-close",
        label: "Cerrar pestaña activa",
        shortcut: "Ctrl+W",
        aliases: "close tab cerrar pestana",
        gate: Gate::Root,
        dispatch: Dispatch::CloseTab,
    },
    spec!(
        "tab-reopen",
        "Reabrir pestaña cerrada",
        "Ctrl+Shift+T",
        "reopen tab pestana",
        Root,
        Action::Tab(crate::tabs::Command::Reopen)
    ),
    spec!(
        "tab-next",
        "Pestaña siguiente",
        "Ctrl+Tab",
        "next tab pestana",
        Root,
        Action::Tab(crate::tabs::Command::Cycle(1))
    ),
    spec!(
        "tab-prev",
        "Pestaña anterior",
        "Ctrl+Shift+Tab",
        "previous tab pestana",
        Root,
        Action::Tab(crate::tabs::Command::Cycle(-1))
    ),
    spec!(
        "folder-view-save",
        "Guardar vista de esta carpeta",
        "",
        "carpeta preferencias folder view save guardar",
        Listing,
        Action::SaveFolderView
    ),
    spec!(
        "folder-view-forget",
        "Olvidar vista de esta carpeta",
        "",
        "carpeta preferencias folder view forget olvidar",
        Listing,
        Action::ForgetFolderView
    ),
    spec!(
        "folder-view-retry",
        "Recargar o reintentar vista de carpeta",
        "",
        "vista retry reintentar recargar",
        Root,
        Action::RetryFolderView
    ),
    spec!(
        "folder-view-discard",
        "Omitir intento de vista de carpeta",
        "",
        "vista discard omitir",
        Root,
        Action::DiscardFolderView
    ),
    spec!(
        "view-list",
        "Vista Lista",
        "Ctrl+2",
        "list lista",
        Root,
        Action::View(View::List)
    ),
    spec!(
        "view-groups",
        "Vista Grupos",
        "Ctrl+1",
        "groups grupos",
        Root,
        Action::View(View::Groups)
    ),
    spec!(
        "view-grid",
        "Vista Grilla",
        "Ctrl+5",
        "grid icons iconos grilla",
        Root,
        Action::View(View::Grid)
    ),
    spec!(
        "view-columns",
        "Vista Columnas",
        "Ctrl+3",
        "finder columns columnas",
        Root,
        Action::View(View::Columns)
    ),
    spec!(
        "view-space",
        "Vista Espacio (treemap)",
        "Ctrl+4",
        "treemap size tamano espacio",
        Root,
        Action::View(View::SpaceMap)
    ),
    spec!(
        "dual",
        "Alternar doble panel",
        "F9",
        "mc commander dual panel",
        Root,
        Action::DualPane
    ),
    spec!(
        "hidden",
        "Alternar archivos ocultos",
        "Ctrl+H",
        "hidden dotfiles ocultos",
        Root,
        Action::ToggleHidden
    ),
    spec!(
        "refresh",
        "Actualizar carpeta",
        "Ctrl+R",
        "reload refresh actualizar recalcular espacio mapa",
        Root,
        Action::Refresh
    ),
    spec!(
        "select-all",
        "Seleccionar todo",
        "Ctrl+A",
        "select all seleccionar",
        Listing,
        Action::Selection(crate::selection_tools::Command::All)
    ),
    spec!(
        "invert",
        "Invertir selección",
        "Ctrl+I",
        "invert selection marcas",
        Listing,
        Action::Selection(crate::selection_tools::Command::Invert)
    ),
    spec!(
        "filtered",
        "Marcar filtrados y mostrar todo",
        "Ctrl+Shift+A",
        "filtered select filtro marcar",
        Listing,
        Action::Selection(crate::selection_tools::Command::Filtered)
    ),
    spec!(
        "clear",
        "Limpiar marcas y foco",
        "Ctrl+Shift+D",
        "clear selection limpiar",
        Listing,
        Action::Selection(crate::selection_tools::Command::Clear)
    ),
    spec!(
        "sort-size",
        "Ordenar por tamaño",
        "",
        "sort size tamano ordenar",
        Listing,
        Action::Sort(Sort::Size)
    ),
    spec!(
        "sort-date",
        "Ordenar por fecha de modificación",
        "",
        "sort date modified fecha modificado ordenar",
        Listing,
        Action::Sort(Sort::Modified)
    ),
    spec!(
        "sort-name",
        "Ordenar por nombre",
        "",
        "sort name nombre ordenar",
        Listing,
        Action::Sort(Sort::Name)
    ),
    spec!(
        "terminal",
        "Terminal en esta carpeta",
        "",
        "terminal shell consola here aqui carpeta",
        Root,
        Action::Terminal
    ),
    spec!(
        "inspector",
        "Alternar inspector (un panel)",
        "",
        "inspector information informacion",
        Root,
        Action::Inspector
    ),
    spec!(
        "operation-bar",
        "Barra de operaciones MC",
        "Ctrl+Alt+B",
        "mc teclado keyboard botones herramientas barra",
        Root,
        Action::ToggleOperationBar
    ),
    spec!(
        "bookmarks",
        "Favoritos, lugares e historial",
        "Ctrl+Alt+D",
        "bookmark favorites favoritos lugares jump salto",
        Root,
        Action::Bookmarks
    ),
    spec!(
        "bookmark-add",
        "Añadir carpeta actual a favoritos",
        "Ctrl+D",
        "bookmark favorite guardar lugar",
        Root,
        Action::BookmarkAdd
    ),
    spec!(
        "settings",
        "Preferencias",
        "Ctrl+,",
        "settings preferences configuracion",
        Root,
        Action::Settings
    ),
    spec!(
        "help",
        "Ayuda y atajos",
        "F1",
        "help keyboard ayuda atajos",
        Root,
        Action::Help
    ),
    spec!(
        "save-session",
        "Guardar sesión",
        "",
        "save session guardar sesion",
        Root,
        Action::SaveSession
    ),
    spec!(
        "compare-folders",
        "Comparar dos carpetas",
        "",
        "compare comparar diferencias carpetas directorios diff",
        Root,
        Action::CompareFolders
    ),
    spec!(
        "review-sync-folders",
        "Preparar sincronización de carpetas",
        "",
        "actualizar espejo combinar conflictos dry-run revisar sin cambios",
        Root,
        Action::ReviewSyncFolders
    ),
    spec!(
        "checksums",
        "Calcular y comparar SHA-256 de archivos",
        "",
        "hash checksum sha256 duplicados comparar integridad",
        Selection,
        Action::Checksums
    ),
    spec!(
        "duplicates",
        "Buscar duplicados en esta carpeta",
        "",
        "duplicate duplicados carpeta sha256 comparar",
        Root,
        Action::Duplicates
    ),
    spec!(
        "recovery",
        "Movimientos interrumpidos",
        "",
        "recovery journal recuperar rescate",
        Root,
        Action::RecoveryCatalog
    ),
];
const _: () = assert!(ITEMS.len() <= MAX_COMMANDS);

// Exact selection membership, without cloning names or a per-entry heap set.
// Dense100k needs12504bytes; sparse high indices choose the shorter representation.

enum Proof {
    Sparse(Vec<usize>),
    Bits(Vec<u64>),
}
impl Proof {
    fn capture(selection: &HashSet<usize>, entries: usize) -> Option<Self> {
        let count = selection.len();
        let max = selection.iter().copied().max().unwrap_or(0);
        if count == 0 {
            return Some(Self::Sparse(Vec::new()));
        }
        if max >= entries {
            return None;
        }
        let sparse = count.checked_mul(std::mem::size_of::<usize>())?;
        let words = max.checked_div(64)?.checked_add(1)?;
        let dense = words.checked_mul(8)?;
        if sparse.min(dense) > PROOF_LIMIT {
            return None;
        }
        if sparse <= dense {
            let mut v = Vec::with_capacity(count);
            v.extend(selection.iter().copied());
            v.sort_unstable();
            Some(Self::Sparse(v))
        } else {
            let mut v = vec![0; words];
            for &i in selection {
                v[i / 64] |= 1u64 << (i % 64);
            }
            Some(Self::Bits(v))
        }
    }
    fn contains(&self, i: usize) -> bool {
        match self {
            Self::Sparse(v) => v.binary_search(&i).is_ok(),
            Self::Bits(v) => v.get(i / 64).is_some_and(|v| v & (1u64 << (i % 64)) != 0),
        }
    }
}
struct Context {
    owner: (u64, u64),
    generation: u64,
    focus: Option<usize>,
    count: usize,
    has_dirs: bool,
    ready: bool,
    proof: Option<Proof>,
    map: Option<(u64, bool)>,
}
impl Context {
    fn capture(m: &Model) -> Self {
        Self {
            map: crate::selection_tools::map_stamp(m),
            owner: m.pane_owner(),
            generation: m.generation,
            focus: m.selected,
            count: m.selection.len(),
            has_dirs: m
                .order
                .iter()
                .any(|&i| m.entries.get(i).is_some_and(|e| e.is_dir())),
            ready: m.loaded && !m.selection_refreshing(),
            proof: if m.loaded && !m.selection_refreshing() {
                Proof::capture(&m.selection, m.entries.len())
            } else {
                None
            },
        }
    }
    fn current(&self, m: &Model) -> bool {
        self.owner == m.pane_owner() && self.generation == m.generation
    }
    fn selection_current(&self, m: &Model) -> bool {
        self.focus == m.selected
            && self.count == m.selection.len()
            && self
                .proof
                .as_ref()
                .is_some_and(|p| m.selection.iter().all(|&i| p.contains(i)))
    }
}
pub struct State {
    pub query: String,
    pub matches: Vec<(usize, i32)>,
    pub selected: usize,
    pub top: usize,
    pub page: usize,
    pub error: String,
    pub replace: bool,
    pub epoch: u64,
    context: Context,
}
impl State {
    fn new(m: &Model) -> Self {
        let mut s = Self {
            query: String::new(),
            matches: Vec::with_capacity(ITEMS.len()),
            selected: 0,
            top: 0,
            page: 1,
            error: String::new(),
            replace: true,
            epoch: next_epoch(),
            context: Context::capture(m),
        };
        s.rebuild();
        s
    }
    fn bump(&mut self) {
        self.epoch = next_epoch();
    }
    pub fn select_input(&mut self) {
        self.replace = true;
        self.bump();
    }
    pub fn input(&mut self, text: &str) {
        let len = if self.replace { 0 } else { self.query.len() };
        if len.checked_add(text.len()).is_none_or(|n| n > QUERY_LIMIT)
            || text.chars().any(char::is_control)
        {
            self.error = "Comandos: máximo 256 bytes, sin saltos de línea".into();
            return;
        }
        if self.replace {
            self.query.clear();
        }
        self.query.push_str(text);
        self.replace = false;
        self.bump();
        self.rebuild();
    }
    pub fn backspace(&mut self) {
        if self.replace {
            self.query.clear();
        } else {
            self.query.pop();
        }
        self.replace = false;
        self.bump();
        self.rebuild();
    }
    fn rebuild(&mut self) {
        self.error.clear();
        self.matches.clear();
        for (i, s) in ITEMS.iter().enumerate() {
            let score = self.query.split_whitespace().try_fold(0i32, |sum, term| {
                let n = [s.id, s.label, s.aliases, s.shortcut]
                    .iter()
                    .filter_map(|s| files_core::listing::fuzzy_score(s, term))
                    .max()?;
                Some(sum.saturating_add(n))
            });
            if let Some(score) = score {
                self.matches.push((i, score));
            }
        }
        self.matches
            .sort_by_key(|&(i, n)| (std::cmp::Reverse(n), i));
        self.selected = 0;
        self.top = 0;
    }
    pub fn step(&mut self, n: i32) {
        self.error.clear();
        if !self.matches.is_empty() {
            self.selected = (self.selected as i64 + i64::from(n))
                .clamp(0, self.matches.len() as i64 - 1) as usize;
        }
    }
    pub fn reason(&self, m: &Model, i: usize) -> Option<&'static str> {
        let s = ITEMS.get(i)?;
        if m.notice_open
            || m.rename_dialog.is_some()
            || m.copy_form.is_some()
            || m.properties_dialog.is_some()
            || m.recovery_catalog.open
            || m.mutation_applied
        {
            return Some("Cierra el aviso o diálogo pendiente primero");
        }
        if !self.context.current(m) {
            return Some("Carpeta o panel cambió; usa contexto actual");
        }
        if !matches!(s.gate, Gate::Root) {
            if !m.loaded || m.selection_refreshing() {
                return Some("Espera a que termine la carga");
            }
            if self.context.map != crate::selection_tools::map_stamp(m) {
                return Some("El mapa cambió; usa contexto actual");
            }
            if let Dispatch::Existing(Action::Selection(command)) = s.dispatch {
                if let Some(reason) = crate::selection_tools::map_reason(m, command) {
                    return Some(reason);
                }
            } else if crate::selection_tools::is_map(m) {
                if let Dispatch::Existing(ref action) = s.dispatch
                    && crate::map_actions::supported(action)
                {
                    if let Some(why) = crate::map_actions::reason(m, action) {
                        return Some(why);
                    }
                } else {
                    return Some("Cambia a una vista de listado");
                }
            }
        }
        if matches!(s.gate, Gate::Focus | Gate::Selection) {
            if self.context.proof.is_none() {
                return Some(if self.context.ready {
                    "La selección es muy grande; reduce marcas"
                } else {
                    "Carga completada: usa contexto actual"
                });
            }
            if self.context.focus != m.selected || self.context.count != m.selection.len() {
                return Some("Selección cambió; usa contexto actual");
            }
            if matches!(s.gate, Gate::Focus) && m.selected.and_then(|i| m.entries.get(i)).is_none()
            {
                return Some("Enfoca un elemento primero");
            }
            if matches!(s.gate, Gate::Selection) && m.selection.is_empty() && m.selected.is_none() {
                return Some("Selecciona al menos un elemento");
            }
        }
        let review =
            matches!(&s.dispatch, Dispatch::Existing(a) if crate::copy_review::supported(a));
        if let Dispatch::Existing(ref action) = s.dispatch
            && let Some(why) = crate::copy_review::reason(m, action)
        {
            return Some(why);
        }
        let busy = m.mutations.busy || m.mutation_applied || m.copy_busy();
        match s.id {
            "terminal" if m.open_job.is_some() => Some("Ya hay una apertura o terminal pendiente"),
            "copy" | "move" | "rename" | "trash" | "properties" | "new-dir" | "new-file"
            | "undo" | "redo"
                if busy
                    && !review
                    && !(crate::selection_tools::is_map(m)
                        && matches!(&s.dispatch, Dispatch::Existing(a) if crate::map_actions::supported(a))) =>
            {
                Some("Espera la operación en curso")
            }
            "bookmark-add" if !m.loaded || m.selection_refreshing() => {
                Some("Espera la carga de la carpeta")
            }
            "bookmark-add" if !m.error.is_empty() => Some("Recarga la carpeta antes de guardarla"),
            "bookmark-add" if !m.bookmarks.can_request() => {
                Some("Espera, reintenta u omite favoritos pendientes")
            }
            "properties" if m.selection.len() > 1 => Some("Propiedades admite un elemento"),
            "sizes-one"
                if !m
                    .selected
                    .and_then(|i| m.entries.get(i))
                    .is_some_and(|e| e.is_dir()) =>
            {
                Some("Enfoca una carpeta")
            }
            "sizes-one" | "sizes-list" | "sizes-marked" if !m.space_map.is_drained() => {
                Some("Espera el cierre del cálculo Espacio")
            }
            "sizes-marked" if m.sizes.has_work() => {
                Some("Termina o cancela el lote de tamaños actual")
            }
            "sizes-marked" if m.marked_size_count() == 0 => {
                Some("Marca carpetas del listado; no se usa el foco")
            }
            "sizes-list" if !self.context.has_dirs => Some("No hay carpetas en el listado"),
            "sizes-pause" if m.sizes.paused() || !m.sizes.has_work() => {
                Some("No hay lote pendiente para pausar")
            }
            "sizes-resume" if !m.sizes.paused() => Some("La cola no está en pausa"),
            "sizes-retry" if m.sizes.errors() == 0 => Some("No hay tamaños fallidos"),
            "sizes-stop"
                if !m.sizes.has_work() && m.du_job.is_none() && m.du_requested.is_none() =>
            {
                Some("No hay cálculos activos")
            }
            "clipboard-copy" | "clipboard-cut" | "clipboard-paste"
                if m.mutations.busy || m.mutation_applied || m.clipboard_state.busy() =>
            {
                Some("Espera el buffer o la operación en curso")
            }
            "clipboard-paste" if m.clipboard_state.count() == 0 => {
                Some("El buffer de files está vacío")
            }
            "undo" if !m.can_undo => Some("No hay operación para deshacer"),
            "redo" if !m.can_redo => Some("No hay renombrado para rehacer"),
            "history-back" if !m.history.can_back() => Some("Sin historial anterior"),
            "history-forward" if !m.history.can_forward() => Some("Sin historial posterior"),
            "parent" if m.path.parent().is_none() => Some("Ya estás en la raíz"),
            "tab-new" if m.tabs.rows.len() == crate::tabs::MAX_TABS => {
                Some("Límite de ocho pestañas")
            }
            "tab-reopen" if m.tabs.closed.is_empty() => Some("No hay pestañas cerradas"),
            "tab-next" | "tab-prev" if m.tabs.rows.len() < 2 => Some("Sólo hay una pestaña"),
            "filtered" if m.query.trim().is_empty() => Some("Escribe primero un filtro local"),
            "save-session" if !matches!(m.session.mode, crate::session_state::Mode::Writable) => {
                Some("La sesión no está habilitada para escritura")
            }
            _ => None,
        }
    }
}
impl Model {
    pub fn open_commands(&mut self) {
        if self.bookmarks.panel.is_some()
            || self.closing
            || self.size_panel.is_some()
            || self.sftp.form.is_some()
            || self.editing
            || self.search.visible
            || self.preview.visible()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.properties_dialog.is_some()
            || self.notice_open
            || self.mutation_applied
            || self.recovery_catalog.open
        {
            return;
        }
        self.commands = None;
        self.commands = Some(State::new(self));
        self.help = None;
        self.settings = false;
        self.actions_menu = false;
        self.context_menu = None;
        self.drag = None;
        self.hits.clear();
        self.thumbnails.suspend();
    }
    pub fn run_command(&mut self) {
        let Some(state) = self.commands.as_ref() else {
            return;
        };
        let Some(&(i, _)) = state.matches.get(state.selected) else {
            return;
        };
        let source = matches!(ITEMS[i].gate, Gate::Focus | Gate::Selection);
        let error = state.reason(self, i).or_else(|| {
            (source && !state.context.selection_current(self))
                .then_some("Selección cambió; usa contexto actual")
        });
        if let Some(e) = error {
            self.commands.as_mut().unwrap().error = e.into();
            return;
        }
        let action = match &ITEMS[i].dispatch {
            Dispatch::Existing(a) => a.clone(),
            Dispatch::Parent => Action::Navigate(self.path.parent().unwrap().to_path_buf()),
            Dispatch::CloseTab => Action::Tab(crate::tabs::Command::Close(self.tabs.active)),
        };
        self.commands = None;
        self.hits.clear();
        self.action(action);
    }
    pub fn recapture_commands(&mut self) {
        let Some(mut s) = self.commands.take() else {
            return;
        };
        s.context.proof = None;
        s.context = Context::capture(self);
        s.error.clear();
        s.bump();
        self.commands = Some(s);
    }
}
fn panel(w: i32, h: i32) -> Rect {
    Rect::new(12, 66, (w - 24).max(1), (h - 106).max(1))
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 4 * m.radius, m.palette.lighter_background);
    p.text(
        Rect::new(r.x + 8, r.y, r.w - 16, r.h),
        label,
        12,
        m.palette.accent,
        true,
        false,
    );
    m.hits.push(Hit {
        rect: r,
        action,
        generation: m.generation,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(s) = m.commands.as_ref() else {
        return;
    };
    let r = panel(w, h);
    let c = m.palette.clone();
    m.hits.clear();
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.text(
        Rect::new(r.x + 14, r.y + 8, r.w - 138, 28),
        "  Comandos",
        16,
        c.foreground,
        true,
        false,
    );
    let query = if s.query.is_empty() {
        "│ Buscar comando o atajo…".to_owned()
    } else {
        format!("{}│", s.query)
    };
    p.rounded_rect(
        Rect::new(r.x + 14, r.y + 44, r.w - 28, 32),
        4 * m.radius,
        c.background,
    );
    p.text(
        Rect::new(r.x + 22, r.y + 44, r.w - 44, 32),
        &query,
        13,
        c.accent,
        true,
        false,
    );
    let context = format!(
        "{} comandos · {} marcas · foco: {}",
        s.matches.len(),
        m.selection.len(),
        m.selected
            .and_then(|i| m.entries.get(i))
            .map_or_else(|| "—".into(), |e| files_core::name_label::label(&e.name))
    );
    p.text(
        Rect::new(r.x + 14, r.y + 82, r.w - 28, 22),
        &context,
        11,
        c.dark_foreground,
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 110, r.y + 8, 96, 28),
        " Esc",
        Action::CommandClose,
    );
    m.hits.push(Hit {
        rect: Rect::new(r.x + 14, r.y + 44, r.w - 28, 32),
        action: Action::CommandFocus,
        generation: m.generation,
    });
    let body = Rect::new(r.x + 14, r.y + 112, r.w - 28, (r.h - 192).max(0));
    {
        let s = m.commands.as_mut().unwrap();
        s.page = (body.h / ROW).max(1) as usize;
        if s.selected < s.top {
            s.top = s.selected;
        }
        if s.selected >= s.top + s.page {
            s.top = s.selected + 1 - s.page;
        }
    }
    let s = m.commands.as_ref().unwrap();
    p.clip(body);
    for (row, &(i, _)) in s
        .matches
        .iter()
        .enumerate()
        .skip(s.top)
        .take(if body.h >= ROW { s.page } else { 0 })
    {
        let y = body.y + (row - s.top) as i32 * ROW;
        let reason = s.reason(m, i);
        if row == s.selected {
            p.rounded_rect(
                Rect::new(body.x, y, body.w, ROW - 2),
                4 * m.radius,
                c.selection,
            );
        }
        p.text(
            Rect::new(body.x + 6, y, body.w - 12, 22),
            ITEMS[i].label_for(m),
            12,
            if reason.is_some() {
                c.dark_foreground
            } else {
                c.foreground
            },
            true,
            false,
        );
        p.text(
            Rect::new(body.x + 6, y + 22, body.w - 12, 20),
            reason.unwrap_or(if ITEMS[i].shortcut_for(m).is_empty() {
                "Enter / doble clic"
            } else {
                ITEMS[i].shortcut_for(m)
            }),
            11,
            c.dark_foreground,
            true,
            false,
        );
        m.hits.push(Hit {
            rect: Rect::new(body.x, y, body.w, ROW - 2),
            action: Action::CommandSelect(s.epoch, row),
            generation: m.generation,
        });
    }
    if s.matches.is_empty() {
        p.text(
            body,
            "Sin comandos coincidentes",
            13,
            c.foreground,
            true,
            false,
        );
    }
    p.unclip();
    let epoch = s.epoch;
    let reason = s.matches.get(s.selected).and_then(|&(i, _)| s.reason(m, i));
    let message = if !s.error.is_empty() {
        s.error.as_str()
    } else {
        reason.unwrap_or("↑↓ o rueda · Enter ejecuta · Ctrl+A reemplaza")
    };
    p.text(
        Rect::new(r.x + 14, r.y + r.h - 72, r.w - 28, 22),
        message,
        11,
        if reason.is_some() || !s.error.is_empty() {
            c.accent
        } else {
            c.foreground
        },
        true,
        false,
    );
    if reason.is_none() && !s.matches.is_empty() {
        button(
            p,
            m,
            Rect::new(r.x + 14, r.y + r.h - 40, 132, 28),
            " Ejecutar",
            Action::CommandActivate(epoch),
        );
    } else {
        let disabled = Rect::new(r.x + 14, r.y + r.h - 40, 132, 28);
        p.rounded_rect(disabled, 4 * m.radius, c.background);
        p.text(
            Rect::new(disabled.x + 8, disabled.y, disabled.w - 16, disabled.h),
            " Ejecutar",
            12,
            c.dark_foreground,
            true,
            false,
        );
    }
    button(
        p,
        m,
        Rect::new(r.x + 158, r.y + r.h - 40, (r.w - 172).min(206), 28),
        " Usar contexto actual",
        Action::CommandRecapture,
    );
    p.unclip();
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut, text: &str) {
    if key == Key::Escape
        || (key == Key::from_char('p')
            && mods.intersects(Shortcut::Ctrl | Shortcut::Meta)
            && mods.intersects(Shortcut::Shift))
    {
        m.action(Action::CommandClose);
        return;
    }
    if key == Key::Enter || key == Key::KPEnter {
        m.action(Action::CommandRun);
        return;
    }
    let Some(s) = m.commands.as_mut() else {
        return;
    };
    if mods.intersects(Shortcut::Ctrl | Shortcut::Meta) {
        if key == Key::from_char('a') || key == Key::from_char('f') {
            s.select_input();
        }
        return;
    }
    match key {
        Key::Up => s.step(-1),
        Key::Down => s.step(1),
        Key::Home => s.step(i32::MIN),
        Key::End => s.step(i32::MAX),
        Key::PageUp => s.step(-(s.page as i32)),
        Key::PageDown => s.step(s.page as i32),
        Key::BackSpace => s.backspace(),
        _ => {
            if !text.is_empty() && text.chars().all(|c| !c.is_control()) {
                s.input(text);
            }
        }
    }
}
pub fn event(m: &mut Model, e: Event) -> bool {
    match e {
        Event::Push => {
            if app::event_button() == 1 {
                let a = m
                    .hits
                    .iter()
                    .rev()
                    .find(|h| {
                        h.generation == m.generation
                            && h.rect.contains(app::event_x(), app::event_y())
                    })
                    .map(|h| h.action.clone());
                if let Some(a) = a {
                    let run = matches!(a, Action::CommandSelect(epoch, _) if m.commands.as_ref().is_some_and(|s| s.epoch == epoch))
                        && app::event_clicks();
                    m.action(a);
                    if run {
                        m.action(Action::CommandRun);
                    }
                }
            }
        }
        Event::KeyDown | Event::Shortcut => {
            key(m, app::event_key(), app::event_state(), &app::event_text())
        }
        Event::MouseWheel => {
            if let Some(s) = &mut m.commands {
                s.step(app::event_dy_value() * 3);
            }
        }
        _ => {}
    }
    true
}

/// The same invocation predicate is used by the canvas and owned fixture.
pub fn shortcut(m: &mut Model, key: Key, mods: Shortcut) -> bool {
    if key == Key::from_char('p')
        && mods.intersects(Shortcut::Ctrl | Shortcut::Meta)
        && mods.intersects(Shortcut::Shift)
    {
        m.action(Action::Commands);
        true
    } else {
        false
    }
}
pub fn fixture_state(m: &Model) -> serde_json::Value {
    m.commands.as_ref().map_or(serde_json::Value::Null,|s|serde_json::json!({
        "query":s.query,"selected":s.selected,"top":s.top,"page":s.page,"error":s.error,"epoch":s.epoch,"context_current":s.context.current(m),
        "matches":s.matches.iter().map(|&(i,_)|serde_json::json!({"id":ITEMS[i].id,"label":ITEMS[i].label_for(m),"shortcut":ITEMS[i].shortcut_for(m),"reason":s.reason(m,i)})).collect::<Vec<_>>(),
        "row_hits":m.hits.iter().filter(|h|matches!(h.action,Action::CommandSelect(_, _))).count(),
        "run_hit":m.hits.iter().any(|h|matches!(h.action,Action::CommandActivate(_)))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
    };
    struct Fixture {
        root: PathBuf,
        m: Model,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "commands-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            std::fs::create_dir_all(&root).unwrap();
            let mut m = Model::new(root.clone());
            let (tx, _) = mpsc::sync_channel(4);
            let (_, rx) = mpsc::channel();
            m.aux = crate::aux_io::AuxIo { tx, rx };
            m.entries = (0..3)
                .map(|i| files_core::listing::Entry {
                    name: format!("file{i}").into(),
                    kind: files_core::listing::Kind::File,
                    metadata: None,
                    stat_attempted: false,
                })
                .collect();
            m.loaded = true;
            m.order = vec![0, 1, 2];
            m.selected = Some(0);
            m.selection.extend([0, 1]);
            Self { root, m }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn choose(m: &mut Model, id: &str) {
        let s = m.commands.as_mut().unwrap();
        s.selected = s
            .matches
            .iter()
            .position(|&(i, _)| ITEMS[i].id == id)
            .unwrap();
    }
    #[test]
    fn map_file_commands_reject_stale_projection_and_equal_count_mark_substitution() {
        let mut f = Fixture::new();
        f.m.view = View::SpaceMap;
        let i = ITEMS.iter().position(|s| s.id == "copy").unwrap();
        f.m.open_commands();
        let state = f.m.commands.as_mut().unwrap();
        state.matches = vec![(i, 0)];
        state.selected = 0;
        f.m.show_hidden = true;
        f.m.run_command();
        assert!(f.m.copy_form.is_none());
        assert!(f.m.commands.as_ref().unwrap().error.contains("mapa cambió"));
        f.m.show_hidden = false;
        f.m.recapture_commands();
        f.m.selection.remove(&1);
        f.m.selection.insert(2);
        f.m.run_command();
        assert!(f.m.copy_form.is_none());
        assert!(
            f.m.commands
                .as_ref()
                .unwrap()
                .error
                .contains("Selección cambió")
        );
        f.m.recapture_commands();
        f.m.run_command();
        assert!(f.m.copy_form.is_some());
    }
    #[test]
    fn dual_listing_retaining_map_has_normal_palette_copy_gate() {
        let mut f = Fixture::new();
        f.m.view = View::SpaceMap;
        let i = ITEMS.iter().position(|s| s.id == "copy").unwrap();
        assert!(State::new(&f.m).reason(&f.m, i).is_none());
        f.m.dual.enabled = true;
        assert!(State::new(&f.m).reason(&f.m, i).is_none());
    }
    #[test]
    fn size_controls_advertise_availability_and_dispatch_only_failures() {
        use std::ffi::OsStr;
        let mut f = Fixture::new();
        let reason = |m: &Model, id: &str| {
            let s = State::new(m);
            s.reason(m, ITEMS.iter().position(|v| v.id == id).unwrap())
        };
        assert!(reason(&f.m, "sizes-pause").is_some());
        assert!(reason(&f.m, "sizes-resume").is_some());
        assert!(reason(&f.m, "sizes-retry").is_some());
        f.m.sizes
            .enqueue(
                &f.m.path,
                [OsStr::new("bad"), OsStr::new("next")].into_iter(),
            )
            .unwrap();
        assert!(reason(&f.m, "sizes-pause").is_none());
        f.m.sizes.dispatch().unwrap();
        f.m.sizes.finish(Err("denied".into()));
        f.m.action(Action::Commands);
        choose(&mut f.m, "sizes-pause");
        f.m.action(Action::CommandRun);
        assert!(f.m.sizes.paused());
        assert!(reason(&f.m, "sizes-resume").is_none());
        assert!(reason(&f.m, "sizes-pause").is_some());
        f.m.action(Action::Commands);
        choose(&mut f.m, "sizes-retry");
        f.m.action(Action::CommandRun);
        assert!(f.m.commands.is_none());
        assert!(f.m.sizes.paused());
        assert_eq!(f.m.sizes.errors(), 0);
        assert!(f.m.sizes.dispatch().is_none());
        f.m.action(Action::Commands);
        choose(&mut f.m, "sizes-resume");
        f.m.action(Action::CommandRun);
        assert_eq!(f.m.sizes.dispatch().unwrap(), f.m.path.join("bad"));
    }
    #[test]
    fn registry_is_unique_bounded_and_matches_aliases_shortcuts_and_unicode() {
        assert!(ITEMS.len() <= MAX_COMMANDS);
        let mut ids = HashSet::new();
        for item in ITEMS {
            assert!(ids.insert(item.id));
        }
        let f = Fixture::new();
        let mut s = State::new(&f.m);
        for (query, id) in [
            ("permisos", "properties"),
            ("F6", "move"),
            ("mc", "dual"),
            ("ReNoMb", "rename"),
            ("tamaño", "sizes-one"),
            ("tamano", "sizes-one"),
        ] {
            s.select_input();
            s.input(query);
            assert!(s.matches.iter().any(|&(i, _)| ITEMS[i].id == id), "{query}");
        }
        s.select_input();
        s.input("no such command zzz");
        assert!(s.matches.is_empty());
        s.select_input();
        s.input("copy");
        let old = s.query.clone();
        let cap = s.query.capacity();
        let epoch = s.epoch;
        s.input(&"x".repeat(1024 * 1024));
        assert_eq!(s.query, old);
        assert_eq!(s.query.capacity(), cap);
        assert_eq!(s.epoch, epoch);
    }
    #[test]
    fn dense_and_sparse_selection_proofs_are_exact_and_preflight_capacity() {
        let mut set: HashSet<usize> = (0..100_000).collect();
        let p = Proof::capture(&set, 200_000).unwrap();
        let Proof::Bits(v) = &p else {
            panic!("100k should use bits")
        };
        assert!(v.capacity() * 8 <= PROOF_LIMIT);
        assert!(p.contains(99_999));
        assert!(!p.contains(100_001));
        set.remove(&99_999);
        set.insert(100_001);
        assert!(!set.iter().all(|&i| p.contains(i)));
        let sparse = HashSet::from([usize::MAX - 1]);
        let p = Proof::capture(&sparse, usize::MAX).unwrap();
        assert!(matches!(p, Proof::Sparse(_)));
        assert!(p.contains(usize::MAX - 1));
        let over: HashSet<usize> = (0..8200).map(|i| i * 128).collect();
        assert!(Proof::capture(&over, 2_000_000).is_none());
        assert!(Proof::capture(&HashSet::from([3]), 3).is_none());
    }
    #[test]
    fn dispatch_revalidates_exact_selection_even_with_same_count_and_focus() {
        let mut f = Fixture::new();
        f.m.action(Action::Commands);
        choose(&mut f.m, "copy");
        f.m.selection.remove(&1);
        f.m.selection.insert(2);
        f.m.action(Action::CommandRun);
        assert!(f.m.copy_form.is_none());
        assert!(
            f.m.commands
                .as_ref()
                .unwrap()
                .error
                .contains("Selección cambió")
        );
        f.m.action(Action::CommandRecapture);
        f.m.action(Action::CommandRun);
        assert!(f.m.commands.is_none());
        let form = f.m.copy_form.as_ref().unwrap();
        assert_eq!(
            form.names.as_ref(),
            [
                std::ffi::OsString::from("file0"),
                std::ffi::OsString::from("file2")
            ]
        );
    }
    #[test]
    fn generation_change_rejects_dispatch_until_explicit_recapture() {
        let mut f = Fixture::new();
        f.m.action(Action::Commands);
        choose(&mut f.m, "view-list");
        let old = f.m.view;
        f.m.generation += 1;
        f.m.action(Action::CommandRun);
        assert!(f.m.view == old);
        assert!(f.m.commands.is_some());
        f.m.action(Action::CommandRecapture);
        f.m.action(Action::CommandRun);
        assert!(f.m.view == View::List);
        assert!(f.m.commands.is_none());
    }
    #[test]
    fn modal_blocks_file_routes_and_delayed_paste_does_not_revive_after_reopen() {
        let mut f = Fixture::new();
        f.m.action(Action::Commands);
        assert!(crate::native_drop::blocked(&f.m));
        assert!(crate::window_chrome::blocked(&f.m));
        let selected = f.m.selection.clone();
        f.m.selection_command(crate::selection_tools::Command::Clear);
        f.m.action(Action::Trash);
        f.m.capture_file_clipboard(crate::file_clipboard::Kind::Cut);
        assert_eq!(f.m.selection, selected);
        assert_eq!(f.m.clipboard_state.count(), 0);
        let context = f.m.text_context();
        assert!(f.m.text_paste.request(context).is_some());
        let old = f.m.commands.as_ref().unwrap().epoch;
        f.m.action(Action::CommandClose);
        f.m.action(Action::Commands);
        assert_ne!(old, f.m.commands.as_ref().unwrap().epoch);
        assert!(f.m.text_paste.request(f.m.text_context()).is_none());
        assert!(!f.m.receive_text_paste("stale"));
        assert!(f.m.commands.as_ref().unwrap().query.is_empty());
        assert!(f.m.text_paste.request(f.m.text_context()).is_some());
        assert!(f.m.receive_text_paste("tamano"));
        assert_eq!(f.m.commands.as_ref().unwrap().query, "tamano");
    }
    #[test]
    fn notice_arriving_after_open_blocks_dispatch_and_is_not_dismissed() {
        let mut f = Fixture::new();
        f.m.action(Action::Commands);
        choose(&mut f.m, "view-grid");
        f.m.notice_open = true;
        f.m.action(Action::CommandRun);
        assert!(f.m.commands.as_ref().unwrap().error.contains("aviso"));
        assert!(f.m.view == View::Groups);
        f.m.action(Action::CommandClose);
        assert!(f.m.commands.is_none());
        assert!(f.m.notice_open);
    }
    #[test]
    fn pointer_activation_epoch_cannot_execute_reordered_query_results() {
        let mut f = Fixture::new();
        f.m.action(Action::Commands);
        let old = f.m.commands.as_ref().unwrap().epoch;
        f.m.commands.as_mut().unwrap().input("view-grid");
        choose(&mut f.m, "view-grid");
        let fresh = f.m.commands.as_ref().unwrap().epoch;
        f.m.action(Action::CommandActivate(old));
        assert!(f.m.commands.is_some());
        assert!(f.m.view == View::Groups);
        f.m.action(Action::CommandActivate(fresh));
        assert!(f.m.commands.is_none());
        assert!(f.m.view == View::Grid);
    }
    #[test]
    fn disabled_command_retains_catalog_with_reason_and_preserves_context() {
        let mut f = Fixture::new();
        f.m.selected = None;
        f.m.selection.clear();
        f.m.action(Action::Commands);
        choose(&mut f.m, "rename");
        f.m.action(Action::CommandRun);
        assert!(f.m.rename_dialog.is_none());
        assert!(f.m.commands.as_ref().unwrap().error.contains("Selecciona"));
        let s = f.m.commands.as_ref().unwrap();
        let i = ITEMS.iter().position(|s| s.id == "sizes-one").unwrap();
        assert!(s.reason(&f.m, i).is_some());
    }
}
