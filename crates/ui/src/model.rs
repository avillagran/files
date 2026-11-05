use crate::{
    painter::Rect,
    worker::{Job, Update, Worker},
};
use files_core::{
    listing::{Entry, Sort},
    places::{self, Mount, Place},
    theme::{self, Corners, Palette, Preferences, WindowMode},
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::SystemTime,
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Groups,
    List,
    Grid,
    Spatial, // Legacy internal layout compatibility; no public selector.
    Columns,
    SpaceMap,
}
#[derive(Clone)]
pub enum Action {
    /// Shift+Delete: confirm, then delete the selection permanently.
    DeletePermanently,
    /// Answer of the permanent-deletion confirmation.
    DeleteAnswer(bool),
    /// Shift+Ctrl+M: create a link to each selected entry.
    CreateLinks,
    /// Ctrl+Shift+O: choose the application for the focused file.
    OpenWith,
    /// "Nuevo: <plantilla>" from ~/Templates.
    NewFromTemplate(PathBuf),
    /// Nautilus "New Folder with Selection".
    FolderWithSelection,
    /// Sidebar device: open (mounting first) or an explicit operation.
    Device(PathBuf, Option<files_core::devices::Op>),
    OpenWithCmd(crate::open_with_ui::Command),
    OpenTrash,
    TrashBrowser(u64, crate::trash_browser::Command),
    Commands,
    Checksums,
    CompareFolders,
    ReviewSyncFolders,
    Comparison(u64, crate::directory_compare_ui::Command),
    Duplicates,
    Checksum(u64, crate::checksum_ui::Command),
    Sftp,
    SftpCommand(u64, crate::sftp_ui::Command),
    Bookmarks,
    BookmarkAdd,
    SaveFolderView,
    ForgetFolderView,
    RetryFolderView,
    DiscardFolderView,
    Bookmark(u64, crate::bookmarks_ui::Command),
    ToggleOperationBar,
    OperationBar((u64, u64), u64, usize),
    CommandSelect(u64, usize),
    CommandActivate(u64),
    CommandRun,
    CommandClose,
    CommandFocus,
    CommandRecapture,
    ClipboardCopy,
    ClipboardCut,
    ClipboardPaste,
    DualPane,
    PaneFocus(u64),
    PaneSort(u64, u64, Sort),
    PaneFilter(u64, u64, bool),
    PaneEntry(u64, u64, usize),
    PaneBackground(u64, u64),
    ColumnFocus(u64),
    ColumnEntry(u64, usize),
    ColumnScroll(i32),
    RecoveryCatalog,
    RecoveryClose,
    RecoverySelect(usize),
    RecoverMove(crate::move_operation::RecoveryAction),
    SaveSession,
    None,
    Tab(crate::tabs::Command),
    Entry(usize),
    Navigate(PathBuf),
    View(View),
    Inspector,
    Sizes,
    SizePanel(u64, crate::size_panel::Command),
    CalculateSize,
    CalculateListedSizes,
    CalculateMarkedSizes,
    CalculateAllocatedSize,
    CalculateMarkedAllocatedSizes,
    CancelListedSizes,
    PauseSizes(bool),
    RetrySizeErrors,
    CancelSize(std::ffi::OsString),
    Search,
    SearchStart,
    SearchPages,
    SearchPageStep(i8, u64),
    SearchCancel,
    SearchClose,
    SearchHidden,
    SearchFocus,
    SearchScopeMenu,
    SearchScope(crate::search_state::Scope, u64),
    SearchFilters,
    SearchOrder,
    SearchContent,
    SearchRegex,
    SearchField(usize),
    SearchClearFilters,
    SearchCalendar,
    SearchDate(files_core::search_dates::Preset),
    SearchSelect(usize, u64),
    SearchToggle(usize, u64),
    SearchMarkAll(bool),
    SearchOpen(bool),
    SearchBackgroundTab,
    SearchTransfer(bool),
    SavedSearches(bool),
    SavedSearchCommand(u64, crate::saved_search_ui::Command),
    Filter,
    ClearFilter,
    Selection(crate::selection_tools::Command),
    SelectionMenu(i32, i32),
    SpaceRefresh,
    SpaceCancel,
    SpaceListScroll(i32),
    ToggleHidden,
    Refresh,
    History(bool),
    Help,
    HelpClose,
    HelpScroll(i32),
    ContextItem(usize),
    CopyFiles,
    NativePaste,
    MoveFiles,
    ExtractArchive,
    CreateArchive,
    ApplyArchive(u64, Option<u64>),
    ArchiveReview(u64, crate::archive_operation::RecoveryAction),
    ArchiveBack(u64),
    ArchivePause(u64),
    ArchiveClearPending(u64),
    ArchiveCommit(u64),
    CopyDestination,
    CopyStartPaused,
    CopySkipExisting,
    CopyOptions,
    CopyVisiblePublication,
    CopySymlinkTimes,
    CopyNameFocus(u64),
    CopyNameStep(u64, i32),
    CopyNameReset(u64),
    CopyDialogCancel,
    CopyStart,
    CopyPause(u64, bool),
    CopyCancel(u64),
    CopyClearPending,
    CopyDetails,
    RetryPendingMove,
    NoticeNavigate(crate::notice_navigation::Command),
    CopyFullReport,
    Preview,
    ClosePreview,
    PreviewNeighbor(i32),
    PreviewImageZoom(f32),
    PreviewImageReset,
    PreviewRotate(i32),
    PreviewVideoSeek(f64),
    PreviewVideoToggle,
    PreviewVideoAudio,
    PreviewAudioToggle,
    PreviewAudioSeek(f64),
    PreviewPdfPage(i32),
    PreviewPdfText,
    CopyPreview,
    SelectPreview,
    Rename,
    Trash,
    Properties,
    PropertyBit(usize),
    PropertyApply,
    PropertyReload,
    PropertyClose,
    NewEntry(bool),
    RenameField(usize),
    RenameCase,
    RenameRegex,
    RenameApply,
    RenameCancel,
    Undo(bool),
    DismissNotice,
    CopyNotice,
    Settings,
    Actions,
    WindowMode(WindowMode),
    Sort(Sort),
    Corner(Corners),
    Theme(bool),
    Open,
    Terminal,
    CopyPath,
    Omni,
    PathCommit,
    PathCancel,
    ViewMenu(i32, i32),
    Zoom(f32),
    GridZoom(f32),
    RetrySaves,
    CancelClose,
    DiscardClose,
}
impl Action {
    pub fn accessible_name(&self) -> Option<&'static str> {
        match self {
            Self::OpenTrash => Some("Abrir Papelera de Inicio"),
            Self::CopyFiles => Some("Copiar selección: revisar destino"),
            Self::CopyNameFocus(_) => Some("Editar nombre destino del elemento, F2"),
            Self::CopyNameStep(_, _) => Some("Anterior/siguiente nombre destino, Alt+arriba/abajo"),
            Self::CopyNameReset(_) => Some("Restaurar nombre original; no renombra el origen"),
            Self::CopyOptions => Some("Opciones de copia y volver a revisión, F3"),
            Self::CopyVisiblePublication => {
                Some("Cambiar publicación: al terminar o visible durante copia, 1")
            }
            Self::CopySymlinkTimes => Some("Conservar u omitir fechas de enlaces simbólicos, 2"),
            Self::CopySkipExisting => {
                Some("Cambiar conflictos de copia: detener, omitir o conservar ambos; F4")
            }
            Self::ToggleOperationBar => Some("Barra de operaciones MC, Ctrl+Alt+B"),
            Self::OperationBar(_, _, index) => crate::operation_bar::label(*index),
            Self::Sftp => Some("Conectar y controlar montajes SFTP"),
            Self::SftpCommand(_, cmd) => Some(cmd.name()),
            Self::Bookmarks => Some("Favoritos y lugares, Ctrl+Alt+D"),
            Self::BookmarkAdd => Some("Añadir carpeta actual a favoritos, Ctrl+D"),
            Self::SaveFolderView => Some("Guardar vista de esta carpeta"),
            Self::ForgetFolderView => Some("Olvidar vista de esta carpeta"),
            Self::RetryFolderView => Some("Recargar o reintentar vista de carpeta"),
            Self::DiscardFolderView => Some("Omitir intento de vista; no revierte lo publicado"),
            Self::Bookmark(_, command) => Some(crate::bookmarks_ui::name(command)),
            Self::Duplicates => Some("Buscar duplicados en la carpeta actual"),
            Self::Checksums => Some("Calcular y comparar SHA-256 de archivos elegidos"),
            Self::Checksum(_, cmd) => Some(cmd.name()),
            Self::Commands => Some("Buscar comandos y herramientas, Ctrl+Shift+P"),
            Self::CommandSelect(_, _) => Some("Seleccionar comando"),
            Self::CommandActivate(_) => Some("Ejecutar comando visible"),
            Self::CommandRun => Some("Ejecutar comando seleccionado"),
            Self::CommandClose => Some("Cerrar comandos, Esc"),
            Self::CommandFocus => Some("Buscar comando, Ctrl+A reemplaza"),
            Self::CommandRecapture => Some("Usar selección, panel y carpeta actuales"),
            Self::DualPane => Some("Doble panel, F9; Tab cambia foco"),
            Self::View(View::Groups) => Some("Vista Grupos"),
            Self::View(View::List) => Some("Vista Lista"),
            Self::View(View::Grid) => Some("Vista Grilla, Ctrl+5"),
            Self::View(View::Spatial | View::Columns) => Some("Vista Columnas tipo Finder, Ctrl+3"),
            Self::View(View::SpaceMap) => Some("Vista Espacio, tamaños proporcionales"),
            Self::ToggleHidden => Some("Mostrar ocultos, Ctrl+H"),
            Self::Refresh => Some("Actualizar carpeta"),
            Self::Filter => Some("Filtrar aquí, Ctrl+F; subcarpetas, Ctrl+Shift+F"),
            Self::ClipboardCopy => Some("Copiar archivos al portapapeles privado de files, Ctrl+C"),
            Self::ClipboardCut => {
                Some("Cortar archivos en el portapapeles privado de files, Ctrl+X")
            }
            Self::ClipboardPaste => Some("Pegar desde el portapapeles privado de files, Ctrl+V"),
            Self::RecoveryCatalog => Some("Recuperar movimientos interrumpidos"),
            Self::NewEntry(false) => Some("Crear archivo, Ctrl+N"),
            Self::NewEntry(true) => Some("Crear carpeta, F7"),
            Self::Undo(false) => Some("Deshacer, Ctrl+Z"),
            Self::Undo(true) => Some("Rehacer, Ctrl+Shift+Z"),
            Self::Terminal => Some("Terminal en la carpeta actual; Ctrl+Shift+P → terminal"),
            Self::CompareFolders => Some("Comparar dos carpetas, sólo lectura"),
            Self::ReviewSyncFolders => Some("Preparar sincronización, sólo revisión sin cambios"),
            Self::Comparison(_, command) => Some(command.name()),
            Self::Search => Some("Buscar nombres y contenido, Ctrl+Shift+F"),
            Self::SearchStart => Some("Iniciar búsqueda de nombres, Enter"),
            Self::SearchPages => {
                Some("F9: reiniciar recorrido paginado desde la primera página; mismos criterios")
            }
            Self::SearchPageStep(-1, _) => Some("Página anterior conservada en RAM; Ctrl+PgUp"),
            Self::SearchPageStep(..) => {
                Some("Página siguiente o continuar mismo recorrido; Ctrl+PgDown")
            }
            Self::SearchCancel => Some("Cancelar búsqueda"),
            Self::SearchClose => Some("Cerrar buscador, Esc"),
            Self::SearchHidden => Some("Incluir ocultos sólo en esta búsqueda"),
            Self::SearchRegex => Some(
                "Literal/Regex · Ctrl+F6: conserva el texto, cancela resultados anteriores y espera Enter",
            ),
            Self::SearchContent => Some(
                "Texto UTF8 F6:1MiB/archivo,64MiB total; L=límite B=binario Δ=cambio E=error M=montaje",
            ),
            Self::NoticeNavigate(crate::notice_navigation::Command::Start) => {
                Some("Inicio del informe; Home")
            }
            Self::NoticeNavigate(crate::notice_navigation::Command::End) => {
                Some("Última página del informe; End")
            }
            Self::NoticeNavigate(crate::notice_navigation::Command::Pages(-1)) => {
                Some("Página anterior del informe; PgUp")
            }
            Self::NoticeNavigate(crate::notice_navigation::Command::Pages(1)) => {
                Some("Página siguiente del informe; PgDn")
            }
            Self::NoticeNavigate(_) => Some("Desplazar informe por línea; ↑/↓"),
            Self::SavedSearches(_) => Some("Búsquedas guardadas F2; guardar criterios Ctrl+S"),
            Self::SavedSearchCommand(_, command) => Some(crate::saved_search_ui::name(command)),
            Self::SearchOrder => Some("Ordenar resultados: ruta, nombre natural o tipo; F7"),
            Self::SearchFilters => Some("Mostrar filtros de tamaño y fecha, F4"),
            Self::SearchField(1) => {
                Some("Tamaño mínimo inclusivo; MB decimal o MiB binario; Tab cambia campo")
            }
            Self::SearchField(2) => Some("Tamaño máximo inclusivo; sólo archivos regulares"),
            Self::SearchCalendar => {
                Some("F8: fechas locales o últimas1/24/168horas; referencia fijada al elegir")
            }
            Self::SearchDate(preset) => Some(preset.label()),
            Self::SearchField(3) => Some("Modificado desde fecha o instante UTC, incluido"),
            Self::SearchField(4) => Some("Modificado antes de fecha o instante UTC, excluido"),
            Self::SearchField(_) => Some("Editar criterio, Tab cambia de campo"),
            Self::SearchClearFilters => {
                Some("Quitar los cuatro criterios visuales; conservar consulta avanzada")
            }
            Self::SearchScopeMenu => {
                Some("Cambiar raíz: Alt+↑ sube, Alt+→ carpeta del resultado, Alt+← vuelve")
            }
            Self::SearchScope(..) => Some("Cambiar raíz conservando criterios; Enter busca"),
            Self::SearchFocus => Some("Editar filtro del buscador"),
            Self::SearchTransfer(false) => {
                Some("Copiar resultados: capturar y revisar destino; F5")
            }
            Self::SearchTransfer(true) => {
                Some("Mover resultados: capturar y revisar destino; Shift+F6")
            }
            Self::SearchBackgroundTab => {
                Some("Ubicaciones marcadas en pestañas de segundo plano, Ctrl+Enter")
            }
            Self::SearchOpen(false) => Some("Abrir resultado, Enter"),
            Self::SearchOpen(true) => Some("Localizar resultado, Alt+Enter"),
            Self::SearchToggle(..) => Some("Marcar o desmarcar resultado; Espacio/Insert"),
            Self::SearchMarkAll(false) => {
                Some("Marcar todos los resultados de la página actual; Ctrl+A")
            }
            Self::SearchMarkAll(true) => Some("Vaciar marcas; Ctrl+D"),
            Self::SearchSelect(..) => Some("Seleccionar resultado de búsqueda"),
            Self::ViewMenu(_, _) => Some("Cambiar vista"),
            Self::Omni => Some("Editar ruta, Ctrl+L"),
            Self::PathCommit => Some("Confirmar ruta, Enter"),
            Self::PathCancel => Some("Cancelar edición de ruta, Esc"),
            Self::SelectionMenu(_, _) => Some("Herramientas de selección"),
            Self::Selection(_) => Some("Herramientas de selección de archivos visibles"),
            Self::Sort(Sort::Name) | Self::PaneSort(_, _, Sort::Name) => {
                Some("Ordenar por nombre; repetir invierte dirección; conserva foco")
            }
            Self::Sort(Sort::Size) | Self::PaneSort(_, _, Sort::Size) => {
                Some("Ordenar por tamaño; repetir invierte; carpetas sin medir al final")
            }
            Self::Sort(Sort::Modified) | Self::PaneSort(_, _, Sort::Modified) => {
                Some("Ordenar por fecha de modificación; repetir invierte; conserva foco")
            }
            Self::Sort(Sort::Type) | Self::PaneSort(_, _, Sort::Type) => {
                Some("Ordenar por tipo (extensión) y nombre; repetir invierte")
            }
            Self::PaneFilter(_, _, true) => {
                Some("Quitar sólo el filtro de este panel; conservar foco y marcas")
            }
            Self::PaneFilter(_, _, false) => {
                Some("Editar filtro de este panel; Enter vuelve al listado")
            }
            Self::ClearFilter => Some("Quitar filtro, Esc"),
            Self::PauseSizes(true) => Some("Pausar siguientes tamaños; el activo termina"),
            Self::PauseSizes(false) => Some("Reanudar tamaños pendientes"),
            Self::RetrySizeErrors => Some("Reintentar sólo tamaños fallidos; conservar resultados"),
            Self::CancelListedSizes => Some("Parar todos los tamaños pendientes"),
            Self::CancelSize(_) => Some("Cancelar este cálculo de tamaño"),
            Self::CalculateMarkedSizes => Some(
                "Calcular sólo carpetas marcadas visibles; sin foco implícito ni cancelar otro lote",
            ),
            Self::CalculateListedSizes => {
                Some("Calcular o cancelar carpetas listadas, Ctrl+Shift+Espacio")
            }
            Self::Sizes => Some("Gestor de tamaños, Ctrl+Alt+Espacio"),
            Self::SizePanel(_, command) => Some(command.name()),
            Self::CalculateSize => Some("Calcular o cancelar tamaño de carpeta, Ctrl+Espacio"),
            Self::CalculateAllocatedSize => {
                Some("Medir bloques de archivos regulares de la carpeta enfocada")
            }
            Self::CalculateMarkedAllocatedSizes => {
                Some("Medir bloques de carpetas marcadas visibles")
            }
            Self::Preview => Some("Vista previa, Espacio"),
            Self::ArchiveReview(_, action) => Some(action.title()),
            Self::ArchiveBack(_) => Some("Volver al resultado ZIP sin modificar archivos"),
            Self::ArchivePause(_) => Some("Iniciar recuperación en pausa, Ctrl+P"),
            Self::ArchiveClearPending(_) => Some("Cancelar trabajos pendientes antes de recuperar"),
            Self::ArchiveCommit(_) => Some("Confirmar recuperación ZIP revisada, Ctrl+Enter"),
            Self::ApplyArchive(..) => Some("Revisar ZIP o aplicar el plan mostrado"),
            Self::CreateArchive => Some("Crear ZIP: revisar selección y destino sin sobrescribir"),
            Self::ExtractArchive => Some("Extraer ZIP: revisar y aplicar en carpeta nueva"),
            Self::MoveFiles => Some("Mover selección, F6"),
            Self::PreviewNeighbor(-1) => Some("Elemento anterior de la carpeta, Alt+Arriba"),
            Self::PreviewNeighbor(1) => Some("Elemento siguiente de la carpeta, Alt+Abajo"),
            Self::Tab(crate::tabs::Command::New) => Some("Nueva pestaña, Ctrl+T"),
            Self::Tab(crate::tabs::Command::Close(_)) => Some("Cerrar pestaña, Ctrl+W"),
            Self::Tab(_) => Some("Cambiar pestaña, Ctrl+Tab; reabrir Ctrl+Shift+T"),
            Self::SpaceRefresh => Some("Actualizar cálculo de Espacio"),
            Self::SpaceCancel => Some("Cancelar cálculo de Espacio"),
            Self::SpaceListScroll(_) => Some("Desplazar lista sin área de Espacio"),
            Self::Settings => Some("Preferencias"),
            Self::Help => Some("Ayuda de atajos, F1"),
            Self::HelpClose => Some("Cerrar ayuda, Esc"),
            Self::History(false) => Some("Atrás, Alt+Izquierda"),
            Self::History(true) => Some("Adelante, Alt+Derecha"),
            Self::Actions => Some("Acciones"),
            Self::RenameRegex => Some("Alternar reemplazo literal o regex"),
            Self::PreviewRotate(-1) => {
                Some("Girar vista a la izquierda, Shift+R; no modifica el archivo")
            }
            Self::PreviewRotate(1) => Some("Girar vista a la derecha, R; no modifica el archivo"),
            Self::PreviewPdfText => Some("Alternar página o texto PDF, T"),
            Self::Properties => Some("Propiedades y permisos"),
            Self::PropertyApply => Some("Confirmar y aplicar permisos"),
            Self::PropertyReload => Some("Recargar propiedades"),
            Self::NativePaste => Some("Pegar archivos MIME del sistema"),
            Self::PropertyClose => Some("Cerrar propiedades"),
            Self::PropertyBit(_) => Some("Alternar permiso seleccionado"),
            _ => None,
        }
    }
}
/// Rubber-band selection: entries whose hit area touches the rectangle are
/// selected (added to `base` when Ctrl/Shift was held), like Nautilus.
#[derive(Clone, Debug)]
pub struct Marquee {
    pub generation: u64,
    pub start: (i32, i32),
    pub current: (i32, i32),
    pub base: std::collections::HashSet<usize>,
    pub moved: bool,
}
impl Marquee {
    pub fn rect(&self) -> Rect {
        let (x0, x1) = (
            self.start.0.min(self.current.0),
            self.start.0.max(self.current.0),
        );
        let (y0, y1) = (
            self.start.1.min(self.current.1),
            self.start.1.max(self.current.1),
        );
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
}
pub struct Hit {
    pub generation: u64,
    pub rect: Rect,
    pub action: Action,
}
pub struct Model {
    pub native_transfer: crate::native_transfer::State,
    pub text_paste: crate::text_paste_gate::Gate<Option<crate::clipboard_ui::TextKey>>,
    pub clipboard_state: crate::file_clipboard::State,
    pub clipboard_jobs: HashMap<u64, crate::file_clipboard::JobTag>,
    pub clipboard_recovery: Option<crate::file_clipboard::RecoveryTag>,
    pub pane_revision: u64,
    pub dual_width: i32,
    pub dual: crate::dual_pane::State,
    pub copy_owners: HashMap<u64, (u64, u64)>,
    pub columns: crate::columns::State,
    pub archive_confirmation: Option<crate::archive_recovery::Confirmation>,
    /// Permanent deletion and trash administration.
    pub delete: crate::delete_ui::State,
    pub open_with: Option<crate::open_with_ui::Panel>,
    /// Drives in the sidebar (event-driven watcher thread).
    pub devices: crate::devices_ui::State,
    /// Listing area of the last paint (rubber-band selection starts here).
    pub listing_rect: Rect,
    /// Rubber-band selection in progress.
    pub marquee: Option<Marquee>,
    pub archive_preparing: Option<(u64, files_core::copy_engine::Control)>,
    pub move_preparing: Option<(u64, Arc<std::sync::atomic::AtomicBool>)>,
    pub move_review_fds: usize,
    pub copy_sources: HashMap<u64, PathBuf>,
    pub session: crate::session_state::State,
    pub tabs: crate::tabs::Tabs,
    pub refresh_selection: Option<crate::refresh_selection::Snapshot>,
    pub refresh_deferred: bool,
    pub listing_sync: crate::listing_sync::State,
    pub refresh_viewport: Option<crate::listing_sync::Viewport>,
    pub copy: crate::copy_state::Controller,
    pub search_transfer: crate::search_transfer::State,
    pub copy_form: Option<crate::copy_dialog::Dialog>,
    pub copy_id: u64,
    pub copy_last: Option<Box<crate::copy_job::Outcome>>,
    pub recovery_catalog: crate::move_catalog::Ui,
    pub recovery_context: Option<crate::move_operation::Context>,
    pub copy_report_truncated: bool,
    pub copy_path: Option<PathBuf>,
    pub copy_destination: Option<PathBuf>,
    pub copy_locations: HashMap<u64, PathBuf>,
    pub preview: crate::text_preview_state::State,
    pub thumbnails: crate::thumbnails::State,
    pub media_visible: bool,
    pub search: crate::search_state::State,
    pub search_locate: Option<(u64, std::ffi::OsString)>,
    pub search_reveal: bool,
    pub sizes: crate::size_queue::State,
    pub du_requested: Option<PathBuf>,
    media_guard: crate::media_watch::Guard,
    pub space_map: crate::space_map_state::State,
    pub space_geometry: crate::space_map_view::Geometry,
    pub space_requested: bool,
    pub text_viewport: crate::text_preview_view::Viewport,
    pub mutations: crate::rename_job::Worker,
    pub rename_dialog: Option<crate::rename_dialog::Dialog>,
    pub properties_dialog: Option<crate::properties_dialog::Dialog>,
    pub mutation_id: u64,
    pub mutation_applied: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub operation_message: Option<String>,
    pub operation_details: Vec<String>,
    pub notice_open: bool,
    pub trash_browser: Option<crate::trash_browser::Panel>,
    pub copy_notice_id: Option<u64>,
    pub notice_scroll: usize,
    pub notice_pan: i32,
    pub notice_width: i32,
    pub notice_rows: usize,
    pub notice_pan_limit: i32,
    pub du_job: Option<crate::du_job::Job<files_core::folder_usage::Measurement>>,
    pub du_target: Option<PathBuf>,
    pub du_text: Option<String>,
    pub generation: u64,
    pub visible_generation: u64,
    pub selection: HashSet<usize>,
    pub anchor: Option<usize>,
    pub layout: Arc<files_core::spatial::SpatialLayout>,
    pub layout_revision: u64,
    pub layout_path: Option<PathBuf>,
    pub next_revision: u64,
    pub saves: crate::save_queue::SaveQueue,
    pub closing: bool,
    pub discard_close: bool,
    pub config_revision: u64,
    pub config_in_flight: Option<u64>,
    pub config_dirty: bool,
    pub config_error: Option<String>,
    pub layout_ready: bool,
    pub layout_requested: bool,
    pub layout_dirty: bool,
    pub pan_x: f32,
    pub drag: Option<crate::spatial_view::Drag>,
    pub aux: crate::aux_io::AuxIo,
    pub path: PathBuf,
    pub history: files_core::navigation_history::History,
    pub entries: Vec<Entry>,
    pub order: Vec<usize>,
    pub selected: Option<usize>,
    pub preserve_empty_selection: bool,
    pub view: View,
    pub sort: Sort,
    pub descending: bool,
    pub query: String,
    pub filter_edit: crate::filter_edit::State,
    pub editing: bool,
    pub edit: String,
    pub replace_edit: bool,
    pub path_edit: crate::path_edit::State,
    pub scroll: i32,
    pub wheel_scroll: crate::wheel_scroll::State,
    pub content_height: i32,
    pub group_ends: [usize; 5],
    pub rail: bool,
    pub inspector: bool,
    pub settings: bool,
    pub help: Option<crate::help_view::State>,
    pub commands: Option<crate::command_palette::State>,
    pub sftp: crate::sftp_state::State,
    pub sftp_profiles: crate::sftp_profiles_state::State,
    pub size_panel: Option<crate::size_panel::State>,
    pub actions_menu: bool,
    pub context_menu: Option<crate::context_menu::Menu>,
    pub toolkit_popup: bool,
    pub show_hidden: bool,
    pub operation_bar: bool,
    /// True when config.toml says whether previews start muted.
    pub preview_muted_configured: bool,
    pub grid_zoom: f32,
    pub status_tips: crate::status_tips::State,
    pub open_job: Option<crate::open_job::Job>,
    pub launch_notice: Option<&'static str>,
    pub window_mode: WindowMode,
    pub alternate_theme: bool,
    pub palette: Palette,
    pub system_palette: Palette,
    pub corners: Corners,
    pub radius: i32,
    pub badge_radius: i32,
    pub checksums: Option<crate::checksum_ui::Panel>,
    pub comparison: Option<crate::directory_compare_ui::Panel>,
    pub bookmarks: crate::bookmarks_state::State,
    pub folder_views: crate::folder_views_state::State,
    pub(crate) folder_restore: Option<crate::folder_views_ui::Restore>,
    pub saved_searches: crate::saved_search_state::State,
    pub places: Vec<Place>,
    pub mounts: Vec<Mount>,
    pub hits: Vec<Hit>,
    pub visible: Vec<usize>,
    pub hovered: Option<usize>,
    pub worker: Worker,
    pub loaded: bool,
    pub error: String,
    pub pss: Option<u64>,
    pub free: Option<u64>,
    pub summaries: HashMap<usize, crate::folder_summary::Summary>,
    pub requested: HashSet<usize>,
    pub stats_requested: HashSet<usize>,
    /// Watches visible subfolders so their rows follow changes inside them.
    pub child_watch: crate::child_watch::ChildWatch,
    pub child_pending: crate::child_watch::Pending,
    pub all_stat: bool,
}
impl Model {
    pub fn new(path: PathBuf) -> Self {
        let palette = std::fs::read_to_string(theme::theme_path())
            .ok()
            .and_then(|s| Palette::parse(&s).ok())
            .unwrap_or_default();
        let prefs: Preferences = std::fs::read_to_string(theme::config_dir().join("config.toml"))
            .ok()
            .and_then(|s| toml_parse(&s))
            .unwrap_or_default();
        let worker = Worker::start(path.clone());
        let mut model = Self {
            listing_sync: Default::default(),
            refresh_viewport: None,
            native_transfer: Default::default(),
            text_paste: Default::default(),
            clipboard_state: Default::default(),
            clipboard_jobs: HashMap::new(),
            clipboard_recovery: None,
            pane_revision: 1,
            dual_width: 0,
            dual: Default::default(),
            copy_owners: HashMap::new(),
            columns: Default::default(),
            archive_confirmation: None,
            delete: Default::default(),
            open_with: None,
            devices: Default::default(),
            listing_rect: Rect::default(),
            marquee: None,
            child_watch: Default::default(),
            child_pending: Default::default(),
            archive_preparing: None,
            move_preparing: None,
            move_review_fds: 0,
            copy_sources: HashMap::new(),
            session: Default::default(),
            tabs: crate::tabs::Tabs::new(path.clone()),
            refresh_selection: None,
            refresh_deferred: false,
            copy: crate::copy_state::Controller::new(),
            search_transfer: Default::default(),
            copy_form: None,
            copy_id: 0,
            copy_last: None,
            recovery_catalog: Default::default(),
            recovery_context: None,
            copy_report_truncated: false,
            copy_path: None,
            copy_destination: None,
            copy_locations: HashMap::new(),
            preview: crate::text_preview_state::State::new(),
            thumbnails: crate::thumbnails::State::new(),
            media_visible: true,
            search: Default::default(),
            search_locate: None,
            search_reveal: false,
            sizes: Default::default(),
            du_requested: None,
            media_guard: crate::media_watch::Guard::new(),
            text_viewport: Default::default(),
            space_map: crate::space_map_state::State::new(),
            space_geometry: Default::default(),
            space_requested: false,
            mutations: crate::rename_job::Worker::new(),
            rename_dialog: None,
            properties_dialog: None,
            mutation_id: 0,
            mutation_applied: false,
            can_undo: false,
            can_redo: false,
            operation_message: None,
            operation_details: Vec::new(),
            notice_open: false,
            trash_browser: None,
            copy_notice_id: None,
            notice_scroll: 0,
            notice_pan: 0,
            notice_width: 0,
            notice_rows: 1,
            notice_pan_limit: 0,
            du_job: None,
            du_target: None,
            du_text: None,
            generation: 1,
            visible_generation: 0,
            selection: HashSet::new(),
            anchor: None,
            layout: Arc::default(),
            layout_revision: 0,
            layout_path: None,
            next_revision: 0,
            saves: Default::default(),
            closing: false,
            discard_close: false,
            config_revision: 0,
            config_in_flight: None,
            config_dirty: false,
            config_error: None,
            layout_ready: false,
            layout_requested: false,
            layout_dirty: false,
            pan_x: 0.0,
            drag: None,
            aux: crate::aux_io::AuxIo::new(),
            free: None,
            path,
            history: Default::default(),
            entries: Vec::new(),
            order: Vec::new(),
            selected: None,
            preserve_empty_selection: false,
            view: View::Groups,
            sort: Sort::Name,
            descending: false,
            query: String::new(),
            filter_edit: Default::default(),
            editing: false,
            edit: String::new(),
            replace_edit: false,
            path_edit: Default::default(),
            scroll: 0,
            wheel_scroll: crate::wheel_scroll::State::default(),
            content_height: 0,
            group_ends: [0; 5],
            rail: false,
            inspector: true,
            settings: false,
            help: None,
            commands: None,
            sftp: Default::default(),
            sftp_profiles: Default::default(),
            size_panel: None,
            actions_menu: false,
            context_menu: None,
            toolkit_popup: false,
            show_hidden: prefs.show_hidden,
            operation_bar: prefs.operation_bar,
            preview_muted_configured: prefs.preview_muted.is_some(),
            grid_zoom: 1.0,
            status_tips: Default::default(),
            open_job: None,
            launch_notice: None,
            window_mode: prefs.window_mode,
            alternate_theme: false,
            palette: palette.clone(),
            system_palette: palette,
            corners: prefs.corners,
            badge_radius: 8,
            radius: if prefs.corners == Corners::Rounded {
                1
            } else {
                0
            },
            checksums: None,
            comparison: None,
            bookmarks: Default::default(),
            folder_views: Default::default(),
            folder_restore: None,
            saved_searches: Default::default(),
            places: places::xdg_places(),
            mounts: places::mounts(),
            hits: Vec::new(),
            visible: Vec::new(),
            hovered: None,
            worker,
            loaded: false,
            error: String::new(),
            pss: None,
            summaries: HashMap::new(),
            requested: HashSet::new(),
            stats_requested: HashSet::new(),
            all_stat: false,
        };
        model
            .preview
            .set_muted(prefs.preview_muted.unwrap_or(false));
        model.arm_folder_restore();
        model
    }
    /// Same-directory async reload. Retain layout/filter/scroll and one focused
    /// lossless names and focus; invalidate every entry index before drawing.
    pub fn selection_refreshing(&self) -> bool {
        self.refresh_selection.is_some() && !self.loaded
    }
    pub fn refresh_current(&mut self) {
        self.text_paste.invalidate();
        if let Some(job) = &self.du_job {
            job.cancel();
        }
        self.sizes.invalidate();
        self.du_requested = None;
        self.du_target = None;
        self.du_text = None;
        if self.thumbnails.directory_target().is_some() {
            self.media_guard.refresh();
            self.thumbnails.invalidate();
        }
        if self.preview.document_plan().is_some() {
            self.media_guard.refresh();
            self.preview.invalidate_document();
        } else if self.preview.source_target().is_some() {
            self.media_guard.refresh();
            self.preview.invalidate_source();
        }
        self.context_menu = None;
        if self.refresh_selection.is_none() {
            let names = self
                .entries
                .iter()
                .enumerate()
                .filter(|(i, _)| self.selection.contains(i))
                .map(|(_, e)| e.name.as_os_str());
            let focus = self
                .selected
                .and_then(|i| self.entries.get(i))
                .map(|e| e.name.as_os_str());
            let anchor = self
                .anchor
                .and_then(|i| self.entries.get(i))
                .map(|e| e.name.as_os_str());
            let snapshot = if self.view == View::Columns {
                let limit = crate::columns::DATA_LIMIT
                    .saturating_sub(self.columns.retained())
                    .saturating_sub(crate::columns::charge(
                        &self.entries,
                        &self.order,
                        &self.selection,
                    ))
                    .saturating_sub(self.query.capacity())
                    .saturating_sub(4160)
                    .min(8 * 1024 * 1024);
                crate::refresh_selection::Snapshot::capture_with_anchor(names, focus, anchor, limit)
            } else {
                crate::refresh_selection::Snapshot::capture_preserving_anchor(names, focus, anchor)
            };
            match snapshot {
                Ok(snapshot) => {
                    self.refresh_selection = Some(snapshot);
                    self.refresh_viewport = crate::listing_sync::Viewport::capture(self);
                }
                Err(error) => {
                    self.error = error.into();
                    self.refresh_deferred = true;
                    return;
                }
            }
        }
        self.refresh_deferred = false;
        self.reset_space_map();
        self.generation = self.generation.wrapping_add(1);
        self.pane_revision = self.pane_revision.wrapping_add(1);
        self.worker.restart(self.path.clone());
        self.entries.clear();
        self.order.clear();
        self.group_ends = [0; 5];
        self.visible.clear();
        self.hits.clear();
        self.visible_generation = 0;
        self.hovered = None;
        self.selected = None;
        self.selection.clear();
        self.anchor = None;
        self.drag = None;
        self.loaded = false;
        self.error.clear();
        self.summaries.clear();
        self.requested.clear();
        self.stats_requested.clear();
        self.all_stat = false;
        self.free = None;
        let owner = self.pane_owner();
        self.listing_sync.refresh_started(owner, &self.path);
        if !self.layout_ready {
            self.layout_requested = false;
        }
    }
    pub fn navigate(&mut self, path: PathBuf) {
        if let Err(e) = files_core::navigation_history::validate(&path) {
            self.error = e.into();
            return;
        }
        if path == self.path {
            self.refresh_current();
            return;
        }
        if !self.save_layout() {
            return;
        }
        if let Err(e) = self.history.visit(&self.path, &path) {
            self.error = e.into();
            return;
        }
        self.load_location(path);
    }
    pub fn travel_history(&mut self, forward: bool) {
        if !self.save_layout() {
            return;
        }
        match self.history.travel(&self.path, forward) {
            Ok(Some(path)) => self.load_location(path),
            Ok(None) => {}
            Err(e) => self.error = e.into(),
        }
    }
    pub(crate) fn load_location(&mut self, path: PathBuf) {
        self.search.close();
        self.search_locate = None;
        self.text_paste.invalidate();
        self.context_menu = None;
        self.refresh_selection = None;
        self.refresh_viewport = None;
        self.refresh_deferred = false;
        self.reset_space_map();
        self.preview.close();
        self.generation = self.generation.wrapping_add(1);
        self.pane_revision = self.pane_revision.wrapping_add(1);
        if let Some(job) = &self.du_job {
            job.cancel();
        }
        self.sizes.invalidate();
        self.du_requested = None;
        self.du_target = None;
        self.du_text = None;
        self.visible.clear();
        self.hits.clear();
        self.hovered = None;
        self.visible_generation = 0;
        self.selection.clear();
        self.anchor = None;
        self.drag = None;
        self.layout = Arc::default();
        self.layout_revision = 0;
        self.layout_path = None;
        self.layout_ready = false;
        self.layout_requested = false;
        self.layout_dirty = false;
        self.pan_x = 0.0;
        self.preserve_empty_selection = false;
        self.path = path;
        self.tabs.update_path(&self.path);
        self.worker.restart(self.path.clone());
        self.entries.clear();
        self.order.clear();
        self.group_ends = [0; 5];
        self.selected = None;
        self.query.clear();
        self.end_path_edit();
        self.scroll = 0;
        self.loaded = false;
        self.error.clear();
        self.summaries.clear();
        self.requested.clear();
        self.stats_requested.clear();
        self.all_stat = false;
        self.free = None;
        self.arm_folder_restore();
    }
    pub fn rebuild(&mut self) {
        let keep = self.selected;
        let group_focus_rank = if self.view == View::Groups && !self.dual.enabled {
            keep.and_then(|i| self.order.iter().position(|&j| j == i))
        } else {
            None
        };
        let filter = files_core::name_filter::Filter::new(&self.query);
        self.order.clear();
        self.group_ends = [0; 5];
        if self.view == View::Columns {
            self.order.reserve_exact(self.entries.len());
        }
        self.order.extend((0..self.entries.len()).filter(|&i| {
            files_core::visibility::accepts(&self.entries[i].name, self.show_hidden)
                && (self.view == View::SpaceMap
                    || self.query.is_empty()
                    || filter.score(&self.entries[i]).is_some())
        }));
        self.order.sort_by_cached_key(|&i| {
            files_core::natural_sort::NaturalKeyRef::new(&self.entries[i].name)
        });
        if self.sort != Sort::Name {
            let measured = if self.sort == Sort::Size {
                self.sizes.measured_map(&self.path)
            } else {
                HashMap::new()
            };
            self.order.sort_by(|&a, &b| {
                let a = &self.entries[a];
                let b = &self.entries[b];
                let c = match self.sort {
                    Sort::Size => {
                        let value = |e: &Entry| {
                            if e.is_dir() {
                                measured.get(e.name.as_os_str()).copied()
                            } else {
                                e.metadata.as_ref().map(|m| m.size)
                            }
                        };
                        return files_core::listing::compare_sizes(
                            value(a),
                            value(b),
                            self.descending,
                        );
                    }
                    // Stable sort: within a kind the natural name order stays.
                    Sort::Type => files_core::listing::type_key(&a.name)
                        .cmp(&files_core::listing::type_key(&b.name)),
                    _ => a
                        .metadata
                        .as_ref()
                        .map(|m| m.modified)
                        .cmp(&b.metadata.as_ref().map(|m| m.modified)),
                };
                if self.descending { c.reverse() } else { c }
            });
        } else if self.descending {
            self.order.reverse();
        }
        if !self.query.is_empty() {
            self.order.sort_by_cached_key(|&i| {
                std::cmp::Reverse(filter.score(&self.entries[i]).unwrap_or_default())
            });
        }
        self.order.sort_by_key(|&i| !self.entries[i].is_dir());
        // Groups paints date sections after folder tiles; navigation/ranges must
        // use that same order. Dual pane always paints ordinary lists.
        if self.view == View::Groups && !self.dual.enabled {
            let now = SystemTime::now();
            let bucket = |i: usize| {
                let e = &self.entries[i];
                if e.is_dir() {
                    0
                } else {
                    1 + e
                        .metadata
                        .as_ref()
                        .map(|m| files_core::system::date_group(m.modified, now))
                        .unwrap_or(3)
                }
            };
            self.order.sort_by_cached_key(|&i| bucket(i));
            for group in 0..5 {
                self.group_ends[group] = self.order.partition_point(|&i| bucket(i) <= group);
            }
        }
        self.selected = keep.filter(|i| self.order.contains(i)).or_else(|| {
            if self.preserve_empty_selection {
                None
            } else {
                self.order.first().copied()
            }
        });
        if group_focus_rank.is_some_and(|rank| self.order.get(rank).copied() != self.selected) {
            self.search_reveal = true;
        }
        self.selection.retain(|i| {
            self.entries.get(*i).is_some_and(|e| {
                files_core::visibility::accepts(&e.name, self.show_hidden)
                    && (self.query.is_empty() || filter.score(e).is_some())
            })
        });
        if self.anchor.is_some_and(|i| !self.order.contains(&i)) {
            self.anchor = None;
        }
        if !self.preserve_empty_selection
            && self.selection.is_empty()
            && let Some(i) = self.selected
        {
            self.selection.insert(i);
        }
    }
    pub fn set_media_visible(&mut self, visible: bool) {
        if self.media_visible && !visible {
            self.thumbnails.suspend();
            self.preview.suspend_media();
        }
        if !self.media_visible && visible {
            self.preview.resume_media();
        }
        self.media_visible = visible;
    }
    pub fn update_thumbnail_demand(&mut self) {
        if self.closing
            || self.search.busy()
            || !self.media_visible
            || self.preview.visible()
            || !self.loaded
            || self.view == View::SpaceMap
            || self.du_job.is_some()
            || self.du_requested.is_some()
            || self.sizes.has_work()
            || self.help.is_some()
            || self.bookmarks.panel.is_some()
            || self.commands.is_some()
            || self.size_panel.is_some()
            || self.trash_browser.is_some()
            || self.checksums.is_some()
            || self.comparison.is_some()
            || self.sftp.form.is_some()
        {
            self.thumbnails.suspend();
            return;
        }
        let mut indices = Vec::with_capacity(32);
        if self.inspector
            && let Some(i) = self.selected
        {
            indices.push(i);
        }
        if self.view == View::Grid && self.visible_generation == self.generation {
            for &i in &self.visible {
                if indices.len() == 32 {
                    break;
                }
                if !indices.contains(&i) {
                    indices.push(i);
                }
            }
        }
        let mut bytes = 0usize;
        let mut names = Vec::with_capacity(32);
        for i in indices {
            let Some(entry) = self.entries.get(i) else {
                continue;
            };
            if entry.is_dir()
                || !files_core::visibility::accepts(&entry.name, self.show_hidden)
                || !crate::thumbnail_job::supported(std::path::Path::new(&entry.name))
            {
                continue;
            }
            let size = self
                .path
                .as_os_str()
                .len()
                .saturating_add(1)
                .saturating_add(entry.name.len());
            bytes = bytes.saturating_add(size);
            if size > 4096 || bytes > 64 * 1024 {
                self.thumbnails.suspend();
                self.thumbnails.notice =
                    Some("Reduce las miniaturas: máximo 64KiB de rutas visibles".into());
                return;
            }
            names.push(&entry.name);
        }
        let paths = names
            .into_iter()
            .map(|name| self.path.join(name))
            .collect::<Vec<_>>();
        self.thumbnails
            .demand(&self.path, paths.iter().map(|p| p.as_path()));
    }
    pub fn pump_media_source(&mut self) -> bool {
        if self.closing {
            self.thumbnails.begin_close();
            self.media_guard.stop();
            return false;
        }
        if !self.media_visible {
            self.media_guard.set_target(None);
            return false;
        }
        let document_plan = self.preview.document_plan();
        let document = document_plan.is_some();
        let selected = document || self.preview.source_target().is_some();
        if let Some((token, path, plan)) = document_plan {
            self.media_guard.set_document(token, path, plan);
        } else if selected {
            self.media_guard.set_target(self.preview.source_target());
        } else if let Some((token, path)) = self.thumbnails.directory_target() {
            self.media_guard.set_directory(token, path);
        } else {
            self.media_guard.set_target(None);
        }
        let mut changed = false;
        for _ in 0..2 {
            match self.media_guard.poll() {
                Some(crate::media_watch::Update::Dirty) => {
                    if document {
                        changed |= self.preview.invalidate_document();
                    } else if selected {
                        changed |= self.preview.invalidate_source();
                    } else {
                        self.thumbnails.invalidate();
                        changed = true;
                    }
                }
                Some(crate::media_watch::Update::Verified(source)) if document => {
                    changed |= self.preview.verify_document(source)
                }
                Some(crate::media_watch::Update::Verified(source)) if selected => {
                    changed |= self.preview.verify_source(source)
                }
                Some(crate::media_watch::Update::DirectoryReady) if !selected => {
                    self.thumbnails.authorize()
                }
                Some(crate::media_watch::Update::WatchUnavailable(error)) if document => {
                    changed |= self.preview.document_watch_unavailable(error);
                }
                Some(
                    crate::media_watch::Update::Unavailable(error)
                    | crate::media_watch::Update::WatchUnavailable(error),
                ) => {
                    if document {
                        changed |= self.preview.document_unavailable(error);
                    } else if selected {
                        changed |= self.preview.source_error(error);
                    } else {
                        self.thumbnails.unavailable(error);
                        changed = true;
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
        changed
    }
    pub fn media_probe_stopped(&self) -> bool {
        self.media_guard.stopped()
    }
    pub fn pump(&mut self) -> bool {
        let deleted = self.pump_delete() | self.pump_open_with() | self.pump_devices();
        if let Some((id, control)) = &self.archive_preparing
            && (self.closing
                || self
                    .copy_form
                    .as_ref()
                    .is_none_or(|d| d.prepare_id != Some(*id)))
        {
            control.cancel();
        }
        if let Some((id, cancel)) = &self.move_preparing
            && (self.closing
                || self
                    .copy_form
                    .as_ref()
                    .is_none_or(|d| d.prepare_id != Some(*id)))
        {
            cancel.store(true, std::sync::atomic::Ordering::Release);
        }
        if self.filter_edit.active && (self.closing || !self.filter_edit.current(self)) {
            self.end_filter_edit();
        }
        let mut dirty = self.pump_mutations();
        dirty |= self.pump_search_transfer();
        let mut sizes_repaint = false;
        if self.closing {
            self.sftp.close();
            self.sftp_profiles.begin_close(self.discard_close);
            self.bookmarks.begin_close(self.discard_close);
            self.folder_views.begin_close(self.discard_close);
            self.saved_searches.begin_close(self.discard_close);
            self.commands = None;
            self.size_panel = None;
            self.trash_browser = None;
            self.sizes.cancel_all();
        }
        if self.closing && self.search.visible {
            self.search.close();
        }
        if self.search.busy() && self.preview.visible() {
            self.preview.close();
        }
        if let Some(outcome) = self.open_job.as_ref().and_then(|job| job.poll()) {
            let kind = self.open_job.as_ref().expect("launcher").kind();
            self.open_job = None;
            let terminal = kind == crate::open_job::Kind::Terminal;
            if terminal && outcome == crate::open_job::Outcome::Opened {
                self.launch_notice = Some("Terminal solicitada");
            }
            let (status, message) = match outcome {
                crate::open_job::Outcome::Opened => ("opened", None),
                crate::open_job::Outcome::Unavailable => (
                    "unavailable",
                    Some(
                        if terminal {
                            "Terminal: uwsm-app no disponible · integración pendiente"
                        } else {
                            "uwsm-app no disponible: no se puede lanzar la aplicación"
                        }
                        .into(),
                    ),
                ),
                crate::open_job::Outcome::Failed(error) => ("failed", Some(error)),
            };
            if let Some(message) = message {
                self.error = message;
            }
            if std::env::var_os("FILES_TEST_OPEN").is_some() {
                if terminal {
                    eprintln!("FILES_TERMINAL_OUTCOME={status}");
                } else {
                    eprintln!("FILES_OPEN_OUTCOME={status}");
                }
            }
            dirty = true;
        }
        dirty |= self.pump_copy();
        self.update_thumbnail_demand();
        dirty |= self.thumbnails.poll(false);
        if self.closing && self.preview.visible() {
            self.preview.close();
            dirty = true;
        }
        if self.closing
            || self.view != View::SpaceMap
            || self.preview.visible()
            || self.search.busy()
            || self.checksums.is_some()
            || self.comparison.is_some()
        {
            if self.space_map.visible() {
                self.space_map.close();
                self.space_geometry.clear();
                dirty = true;
                self.space_requested = self.view == View::SpaceMap && !self.closing;
            }
            if self.closing || self.view != View::SpaceMap {
                self.space_requested = false;
            }
        } else if self.space_requested
            && !self.search.busy()
            && self.loaded
            && self.preview.is_drained()
            && self.du_job.is_none()
            && self.thumbnails.drained()
        {
            self.space_requested = false;
            self.space_map.open(self.path.clone());
            self.space_geometry.clear();
            dirty = true;
        }
        if self.space_map.pump() {
            self.space_geometry.clear();
            dirty = true;
        }
        dirty |= self.pump_media_source();
        if self.space_map.is_drained()
            && self.du_job.is_none()
            && self.du_requested.is_none()
            && self.thumbnails.drained()
        {
            dirty |= self.preview.pump();
        }
        dirty |= self.pump_media_source();
        let allow_search = self.comparison.is_none()
            && !self.closing
            && self.preview.is_drained()
            && !self.preview.visible()
            && self.du_job.is_none()
            && self.space_map.is_drained()
            && self.thumbnails.drained();
        let search_was_running = self.search.running();
        let search_dirty = self.search.pump(allow_search);
        if search_dirty && self.search.visible {
            self.hits.clear();
        }
        dirty |= search_dirty;
        let search_released = search_was_running && self.search.is_drained();
        let may_thumbnail = self.comparison.is_none()
            && !self.closing
            && !search_released
            && !self.search.busy()
            && !self.preview.visible()
            && self.preview.is_drained()
            && self.space_map.is_drained()
            && self.du_job.is_none()
            && self.du_requested.is_none()
            && !self.sizes.has_work();
        dirty |= self.thumbnails.poll(may_thumbnail);
        dirty |= self.pump_media_source();
        if let crate::text_preview_state::View::Markdown(ready) = self.preview.view() {
            dirty |= ready.rendered.needs_layout();
        }
        let mut size_ack = false;
        if let Some(job) = &self.du_job {
            if let Some(progress) = job.progress() {
                sizes_repaint |= self.sizes.progress(progress);
            }
            if let Some(result) = job.poll() {
                self.du_job = None;
                let measured = self.sizes.finish_measurement(result);
                if measured {
                    self.rebuild_inactive_size_order();
                }
                size_ack = true;
                sizes_repaint = true;
                dirty |= measured && self.sort == Sort::Size;
            }
        }
        if self.comparison.is_none()
            && !self.closing
            && !size_ack
            && self.du_job.is_none()
            && !search_released
            && !self.search.busy()
            && self.thumbnails.drained()
            && self.preview.is_drained()
            && !self.preview.visible()
            && self.space_map.is_drained()
            && let Some(work) = self.sizes.dispatch_work()
        {
            match crate::du_job::Job::start_tracked(work.path, work.mode) {
                Ok(job) => self.du_job = Some(job),
                Err(error) => {
                    self.sizes.finish_measurement(Err(error));
                }
            }
            sizes_repaint = true;
        }
        if !self.sizes.pending_matches(self.du_requested.as_deref()) {
            self.du_requested = self.sizes.pending_path();
        }
        let selected_size = self
            .selected
            .and_then(|i| self.entries.get(i))
            .and_then(|e| {
                self.sizes.status(&self.path, &e.name).map(|status| {
                    let text = match status {
                        crate::size_queue::Status::Measured { bytes, entries, .. } => format!(
                            "Aparente: {} · {entries} entradas",
                            files_core::format_size(*bytes)
                        ),
                        _ => self.sizes.detail(&self.path, &e.name).unwrap_or_default(),
                    };
                    (self.path.join(&e.name), text)
                })
            });
        if let Some((path, text)) = selected_size {
            self.du_target = Some(path);
            self.du_text = Some(text);
        } else {
            self.du_target = None;
            self.du_text = None;
        }
        for _ in 0..64 {
            let Ok(u) = self.worker.rx.try_recv() else {
                break;
            };
            dirty = true;
            match u {
                Update::FreeSpace(free) => self.free = free,
                Update::Rounding(r) => {
                    if self.corners == Corners::Omarchy {
                        self.radius = r;
                    }
                }
                Update::Entries(b) => {
                    if self.dual.enabled && !self.dual.admit(self, &b, true) {
                        self.worker.suspend();
                        self.loaded = true;
                        self.error =
                            "Listado parcial: presupuesto conjunto de doble panel agotado".into();
                    } else if self.view == View::Columns
                        && !self.columns.accept_batch(self, &b, true)
                    {
                        self.worker.suspend();
                        self.loaded = true;
                        self.error="Listado parcial: presupuesto de Columnas agotado; cambia a Lista para abrir completo".into();
                    } else {
                        if self.view == View::Columns || self.dual.enabled {
                            self.entries.reserve_exact(b.len());
                        }
                        self.entries.extend(b);
                    }
                }
                Update::Done => self.loaded = true,
                Update::Error(e) => self.error = e,
                Update::Waiting(waiting) => {
                    if waiting && self.error.is_empty() {
                        self.error = crate::worker::CAPACITY_MESSAGE.into();
                    } else if !waiting && self.error == crate::worker::CAPACITY_MESSAGE {
                        self.error.clear();
                    }
                }
                Update::Stats(batch) => {
                    for (i, m) in batch {
                        if let Some(e) = self.entries.get_mut(i) {
                            e.metadata = m;
                            e.stat_attempted = true;
                        }
                    }
                }
                Update::Summary(i, summary) => {
                    self.summaries.insert(i, summary);
                }
            }
        }
        loop {
            let result = match self.aux.rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    dirty |= self.saved_searches.worker_lost();
                    dirty |= self.sftp_profiles.worker_lost();
                    dirty |= self.search_capture_worker_lost();
                    dirty |= self.move_review_worker_lost();
                    dirty |= self.archive_review_worker_lost();
                    break;
                }
            };
            match result {
                r @ (crate::aux_io::Response::NativeDragPrepared(..)
                | crate::aux_io::Response::NativeEncoded(..)
                | crate::aux_io::Response::NativeImported(..)) => {
                    self.native_response(r);
                    dirty = true;
                }
                crate::aux_io::Response::SelectionCaptured(id, result) => {
                    self.clipboard_state.receive(id, result);
                    dirty = true;
                    if !self.closing {
                        self.operation_message = Some(if self.clipboard_state.error.is_empty() {
                            format!(
                                "Portapapeles de files · {} elementos · {}",
                                self.clipboard_state.count(),
                                if self.clipboard_state.kind()
                                    == Some(crate::file_clipboard::Kind::Cut)
                                {
                                    "cortar"
                                } else {
                                    "copiar"
                                }
                            )
                        } else {
                            self.clipboard_state.error.clone()
                        });
                    }
                }
                crate::aux_io::Response::MoveCatalog(id, result) => {
                    if self
                        .recovery_catalog
                        .pending
                        .as_ref()
                        .is_some_and(|(pending, _)| *pending == id)
                    {
                        self.recovery_catalog.pending = None;
                        if self.recovery_catalog.open && !self.closing {
                            match result {
                                Ok(c) => {
                                    self.recovery_catalog.entries = c.entries;
                                    self.recovery_catalog.truncated = c.truncated;
                                    self.recovery_catalog.error.clear();
                                }
                                Err(e) => self.recovery_catalog.error = e,
                            }
                        }
                        dirty = true;
                    }
                }
                crate::aux_io::Response::SearchCaptured(id, result) => {
                    self.receive_search_capture(id, result);
                    dirty = true;
                }
                crate::aux_io::Response::ArchivePrepared(id, result) => {
                    self.receive_archive(id, result);
                    dirty = true;
                }
                crate::aux_io::Response::MovePrepared(id, result) => {
                    if self
                        .move_preparing
                        .as_ref()
                        .is_some_and(|(pending, _)| *pending == id)
                    {
                        self.move_preparing = None;
                        self.move_review_fds = 0;
                    }
                    if !self.closing
                        && let Some(form) =
                            self.copy_form.as_mut().filter(|d| d.prepare_id == Some(id))
                    {
                        form.preparing = false;
                        form.touch();
                        match result {
                            Ok(prepared) => {
                                form.prepared = Some(prepared);
                                form.error.clear();
                            }
                            Err(e) => form.error = e,
                        }
                    }
                    dirty = true;
                }
                crate::aux_io::Response::SessionOpened(opened) => {
                    self.session_opened(opened);
                    dirty = true;
                }
                crate::aux_io::Response::SessionSaved(revision, result) => {
                    let durable = matches!(result, Ok(crate::session_store::Published::Durable));
                    let accepted = self.session.acknowledge(revision, result);
                    if accepted
                        && durable
                        && (self.error.starts_with("Sesión")
                            || self.error.starts_with("Guardado de sesión"))
                    {
                        self.error.clear();
                    }
                    dirty |= accepted;
                }
                crate::aux_io::Response::Layout(g, result) if g == self.generation => {
                    match result {
                        Ok((path, layout)) => {
                            if let Some((revision, pending)) = self.saves.latest_snapshot(&path) {
                                self.layout = pending;
                                self.layout_revision = revision;
                                self.layout_dirty = true;
                            } else {
                                self.layout = Arc::new(layout);
                            }
                            self.layout_path = Some(path);
                            self.layout_ready = true;
                        }
                        Err(e) => self.error = format!("No se pudo cargar la disposición: {e}"),
                    };
                    dirty = true;
                }
                crate::aux_io::Response::Saved(path, revision, result) => {
                    let success = result.is_ok();
                    if self.saves.acknowledge(&path, revision, result) {
                        if success
                            && self.layout_path.as_ref() == Some(&path)
                            && revision == self.layout_revision
                        {
                            self.layout_dirty = false;
                        }
                        dirty = true;
                    }
                }
                crate::aux_io::Response::Config(revision, result)
                    if self.config_in_flight == Some(revision) =>
                {
                    self.config_in_flight = None;
                    match result {
                        Ok(()) if self.config_revision == revision => self.config_dirty = false,
                        Err(e) => {
                            self.config_error =
                                Some(format!("No se pudo guardar la preferencia: {e}"))
                        }
                        _ => {}
                    }
                    dirty = true;
                }
                crate::aux_io::Response::SearchDate(id, result) => {
                    dirty |= self.search.receive_date(id, result);
                }
                crate::aux_io::Response::SftpCapability(id, result) => {
                    dirty |= self.sftp.receive(id, result);
                }
                crate::aux_io::Response::SftpProfiles(id, result) => {
                    dirty |= self.sftp_profiles.receive(id, result);
                }
                crate::aux_io::Response::SavedSearches(id, result) => {
                    dirty |= self.saved_searches.receive(id, result);
                }
                crate::aux_io::Response::FolderViews(id, result) => {
                    dirty |= self.folder_views.receive(id, result);
                }
                crate::aux_io::Response::TrashCatalog(id, result) => {
                    dirty |= self.receive_trash_catalog(id, result);
                }
                crate::aux_io::Response::Bookmarks(id, result) => {
                    dirty |= self.bookmarks.receive(id, result);
                }
                _ => {}
            }
        }
        if !self.closing {
            dirty |= self.search.dispatch_date(&self.aux.tx);
            dirty |= self.sftp.dispatch(&self.aux.tx);
        }
        dirty |= self.pump_sftp_transport();
        dirty |= self.sftp_profiles.tick();
        dirty |= self.sftp_profiles.dispatch(&self.aux.tx);
        dirty |= self.saved_searches.tick();
        dirty |= self.saved_searches.dispatch(&self.aux.tx);
        dirty |= self.pump_folder_restore();
        dirty |= self.folder_views.tick();
        dirty |= self.folder_views.dispatch(&self.aux.tx);
        dirty |= self.bookmarks.tick();
        dirty |= self.bookmarks.dispatch(&self.aux.tx);
        self.saves.dispatch(&self.aux.tx);
        self.session.dispatch(&self.aux.tx);
        self.dispatch_config();
        if self.view == View::Spatial
            && !self.layout_requested
            && self.loaded
            && self
                .aux
                .tx
                .try_send(crate::aux_io::Request::Load(
                    self.generation,
                    self.path.clone(),
                ))
                .is_ok()
        {
            self.layout_requested = true;
        }
        if dirty {
            self.rebuild();
        }
        if self.loaded
            && let Some(snapshot) = self.refresh_selection.take()
        {
            self.selection.clear();
            self.selected = None;
            self.anchor = None;
            let filter = files_core::name_filter::Filter::new(&self.query);
            for (index, entry) in self.entries.iter().enumerate() {
                let visible = files_core::visibility::accepts(&entry.name, self.show_hidden)
                    && (self.view == View::SpaceMap
                        || self.query.is_empty()
                        || filter.score(entry).is_some());
                if snapshot.contains(&entry.name) && visible {
                    self.selection.insert(index);
                }
                if snapshot.focused(&entry.name) && visible {
                    self.selected = Some(index);
                }
                if snapshot.anchored(&entry.name) && visible {
                    self.anchor = Some(index);
                }
            }
            if self.selected.is_none() && !self.selection.is_empty() {
                self.selected = self
                    .order
                    .iter()
                    .copied()
                    .find(|i| self.selection.contains(i));
            }
            if self.anchor.is_none() {
                self.anchor = self.selected;
            }
            self.preserve_empty_selection = self.selection.is_empty() && self.selected.is_none();
            dirty = true;
        } else if self.refresh_selection.is_some() {
            self.selection.clear();
            self.selected = None;
        }
        if self.loaded
            && let Some((generation, name)) = self.search_locate.take()
            && generation == self.generation
        {
            dirty = true;
            if let Some(i) = self.entries.iter().position(|e| {
                e.name == name && files_core::visibility::accepts(&e.name, self.show_hidden)
            }) {
                self.select_entry(i, false, false);
                self.search_reveal = true;
            } else {
                self.selection.clear();
                self.selected = None;
                self.anchor = None;
                self.preserve_empty_selection = true;
                self.error =
                    "El resultado ya no está visible; activa ocultos o actualiza la carpeta".into();
            }
        }
        if self.loaded
            && (self.view != View::Groups && self.sort == Sort::Name
                || self
                    .entries
                    .iter()
                    .filter(|e| files_core::visibility::accepts(&e.name, self.show_hidden))
                    .all(|e| e.stat_attempted))
            && let Some(viewport) = self.refresh_viewport.take()
        {
            viewport.restore(self);
        }
        self.request_stats();
        dirty |= self.pump_clipboard();
        dirty |= self.pump_native_transfer();
        dirty |= self.pump_checksums();
        dirty |= self.pump_comparison();
        dirty |= self.pump_dual();
        dirty |= self.pump_columns();
        dirty |= self.pump_listing_watch();
        dirty |= self.pump_child_watch();
        dirty || sizes_repaint || deleted
    }
    pub fn clamp_scroll(&mut self, available: i32) {
        let geometry_ready = self.loaded
            && match self.view {
                View::Spatial => self.layout_ready,
                View::SpaceMap => matches!(
                    self.space_map.view(),
                    crate::space_map_state::View::Ready(_) | crate::space_map_state::View::Error(_)
                ),
                _ => true,
            };
        if geometry_ready {
            if self.view == View::SpaceMap && !self.dual.enabled {
                self.scroll = self.space_geometry.clamp_scroll(self.scroll);
                return;
            }
            self.scroll = self
                .scroll
                .clamp(0, (self.content_height - available.max(0)).max(0));
        }
    }
    pub fn scene_ready(&self) -> bool {
        self.loaded
            && self.error.is_empty()
            && (self.view != View::Groups
                || self
                    .entries
                    .iter()
                    .filter(|e| files_core::visibility::accepts(&e.name, self.show_hidden))
                    .all(|e| e.stat_attempted && e.metadata.is_some()))
            && (self.view != View::Spatial || self.layout_ready)
            && (self.view != View::SpaceMap
                || matches!(
                    self.space_map.view(),
                    crate::space_map_state::View::Ready(_)
                ))
    }
    pub fn request_stats(&mut self) {
        if !self.loaded {
            return;
        }
        if (self.view == View::Groups || self.view != View::SpaceMap && self.sort != Sort::Name)
            && !self.all_stat
        {
            let names = self
                .entries
                .iter()
                .enumerate()
                .filter(|(i, e)| {
                    files_core::visibility::accepts(&e.name, self.show_hidden)
                        && !e.stat_attempted
                        && !self.stats_requested.contains(i)
                })
                .map(|(i, e)| (i, e.name.clone()))
                .collect::<Vec<_>>();
            if names.is_empty() {
                self.all_stat = true;
            } else if self.worker.tx.try_send(Job::Stat(names)).is_ok() {
                self.stats_requested.extend(
                    self.entries
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| files_core::visibility::accepts(&e.name, self.show_hidden))
                        .map(|(i, _)| i),
                );
                self.all_stat = true;
            }
        }
        if self.visible_generation != self.generation {
            return;
        }
        let names = self
            .visible
            .iter()
            .copied()
            .filter(|i| {
                self.entries.get(*i).is_some_and(|e| !e.stat_attempted)
                    && !self.stats_requested.contains(i)
            })
            .map(|i| (i, self.entries[i].name.clone()))
            .collect::<Vec<_>>();
        if !names.is_empty() {
            let ids = names.iter().map(|(i, _)| *i).collect::<Vec<_>>();
            if self.worker.tx.try_send(Job::Stat(names)).is_ok() {
                self.stats_requested.extend(ids);
            }
        }
        if self.view == View::Columns {
            return;
        }
        for &i in &self.visible {
            if self.entries.get(i).is_some_and(|e| e.is_dir())
                && !self.requested.contains(&i)
                && self
                    .worker
                    .tx
                    .try_send(Job::Summary(i, self.entries[i].name.clone()))
                    .is_ok()
            {
                self.requested.insert(i);
            }
        }
    }
    pub fn sanitize_selection(&mut self) {
        let filter = files_core::name_filter::Filter::new(&self.query);
        let allowed = |i: &usize| {
            self.entries.get(*i).is_some_and(|e| {
                files_core::visibility::accepts(&e.name, self.show_hidden)
                    && (self.view == View::SpaceMap
                        || self.query.is_empty()
                        || filter.score(e).is_some())
            })
        };
        self.selection.retain(allowed);
        if self.selected.is_some_and(|i| !allowed(&i)) {
            self.selected = None;
        }
        if self.anchor.is_some_and(|i| !allowed(&i)) {
            self.anchor = None;
        }
    }
    pub fn toggle_hidden(&mut self) {
        if self.mutation_applied || self.mutations.busy || self.copy_busy() {
            self.error = "Espera a que termine la operación antes de cambiar ocultos".into();
            return;
        }
        self.show_hidden = !self.show_hidden;
        self.preview.close();
        if let Some(job) = &self.du_job {
            job.cancel();
        }
        self.sizes.invalidate();
        self.du_requested = None;
        self.du_target = None;
        self.du_text = None;
        self.drag = None;
        self.hovered = None;
        self.hits.clear();
        self.visible.clear();
        self.space_geometry.clear();
        self.all_stat = false;
        self.rebuild();
        self.scroll = 0;
        self.config_revision += 1;
        self.config_dirty = true;
        self.config_error = None;
        self.dispatch_config();
        self.request_stats();
    }
    pub fn reset_space_map(&mut self) {
        self.space_map.close();
        self.space_map.clear_invalidation();
        self.space_geometry.clear();
        self.space_requested = self.view == View::SpaceMap;
    }
    pub fn move_selection(&mut self, delta: i32) {
        self.move_browser_focus(delta, false, false);
    }
    pub fn move_browser_focus(&mut self, delta: i32, extend: bool, keep_marks: bool) {
        self.folder_restore = None;
        self.text_paste.invalidate();
        self.columns.deferred_menu = None;
        self.session.restore_allowed = false;
        if self.closing || self.selection_refreshing() {
            return;
        }
        let target = if self.view == View::SpaceMap && !self.dual.enabled {
            let indices = self.space_geometry.navigation_indices();
            crate::keyboard_nav::target(indices, self.selected, delta).map(|p| indices[p])
        } else {
            crate::keyboard_nav::target(&self.order, self.selected, delta).map(|p| self.order[p])
        };
        let Some(i) = target else {
            return;
        };
        if keep_marks && !extend {
            if self.selected != Some(i) {
                self.preview.close();
            }
            self.selected = Some(i);
            self.anchor = Some(i);
            self.preserve_empty_selection = self.selection.is_empty();
        } else {
            self.select_entry(i, keep_marks, extend);
        }
    }
    /// MC-style marking: toggle the focused row, then advance focus without
    /// replacing the marked batch. Never opens the next directory implicitly.
    pub fn invalidate_sizes(&mut self) {
        self.sizes.invalidate();
        self.du_requested = None;
        self.du_target = None;
        self.du_text = None;
        if let Some(job) = &self.du_job {
            job.cancel();
        }
    }
    pub fn marked_size_count(&self) -> usize {
        self.order
            .iter()
            .filter(|&&i| {
                self.selection.contains(&i) && self.entries.get(i).is_some_and(|e| e.is_dir())
            })
            .count()
    }
    pub fn request_marked_folder_sizes(&mut self) -> bool {
        self.request_marked_folder_sizes_mode(files_core::folder_usage::Mode::Apparent)
    }
    fn request_marked_folder_sizes_mode(&mut self, mode: files_core::folder_usage::Mode) -> bool {
        let unavailable = if !self.loaded || self.selection_refreshing() {
            Some("Espera a que termine la carga")
        } else if self.view == View::SpaceMap || !self.space_map.is_drained() {
            Some("Espacio mide aparte; vuelve a un listado")
        } else if self.sizes.has_work() {
            Some("Termina o cancela el lote anterior antes de calcular marcadas")
        } else {
            None
        };
        if let Some(error) = unavailable {
            self.error = error.into();
            return false;
        }
        let order = &self.order;
        let selection = &self.selection;
        let entries = &self.entries;
        let names = order
            .iter()
            .filter(|&&i| selection.contains(&i))
            .filter_map(|&i| entries.get(i))
            .filter(|e| e.is_dir())
            .map(|e| e.name.as_os_str());
        match self.sizes.enqueue_mode(&self.path, names, mode) {
            Err(error) => {
                self.error = if self.marked_size_count() == 0 {
                    "Marca carpetas del listado para calcular; no se usa el foco".into()
                } else {
                    error.into()
                };
                false
            }
            Ok(()) => {
                self.error.clear();
                self.thumbnails.suspend();
                self.du_requested = self.sizes.pending_path();
                self.rebuild_size_orders();
                true
            }
        }
    }
    pub fn request_allocated_folder_sizes(&mut self, marked: bool) -> bool {
        if marked {
            return self
                .request_marked_folder_sizes_mode(files_core::folder_usage::Mode::Allocated);
        }
        let name = self
            .selected
            .filter(|i| self.order.contains(i))
            .and_then(|i| self.entries.get(i))
            .filter(|e| e.is_dir())
            .map(|e| e.name.clone());
        match name {
            Some(name) => self.request_allocated_name(&name),
            None => {
                self.error = "Selecciona una carpeta visible para medir bloques".into();
                false
            }
        }
    }
    pub(crate) fn request_allocated_name(&mut self, name: &std::ffi::OsStr) -> bool {
        let error = if !self.loaded || self.selection_refreshing() {
            Some("Espera a que termine la carga")
        } else if self.view == View::SpaceMap || !self.space_map.is_drained() {
            Some("Espacio mide aparte; vuelve a un listado")
        } else if self.sizes.has_work() {
            Some("Termina o cancela el lote anterior antes de medir bloques")
        } else if !self.order.iter().any(|&i| {
            self.entries
                .get(i)
                .is_some_and(|e| e.is_dir() && e.name == name)
        }) {
            Some("La carpeta ya no está en el listado visible")
        } else {
            None
        };
        if let Some(error) = error {
            self.error = error.into();
            return false;
        }
        match self.sizes.enqueue_mode(
            &self.path,
            std::iter::once(name),
            files_core::folder_usage::Mode::Allocated,
        ) {
            Err(error) => {
                self.error = error.into();
                false
            }
            Ok(()) => {
                self.error.clear();
                self.thumbnails.suspend();
                self.du_requested = self.sizes.pending_path();
                self.rebuild_size_orders();
                true
            }
        }
    }
    pub fn request_folder_sizes(&mut self, listed: bool) {
        if self.view == View::SpaceMap || !self.space_map.is_drained() {
            self.error = "Espacio ya mide tamaños; espera su cancelación".into();
            return;
        }
        if listed && self.sizes.has_work() {
            self.sizes.cancel_all();
            if let Some(job) = &self.du_job {
                job.cancel();
            }
            self.du_requested = None;
            return;
        }
        let result = if listed {
            self.sizes.enqueue(
                &self.path,
                self.order
                    .iter()
                    .filter_map(|&i| self.entries.get(i))
                    .filter(|e| e.is_dir())
                    .map(|e| e.name.as_os_str()),
            )
        } else if let Some(entry) = self
            .selected
            .and_then(|i| self.entries.get(i))
            .filter(|e| e.is_dir())
        {
            if matches!(
                self.sizes.status(&self.path, &entry.name),
                Some(crate::size_queue::Status::Queued | crate::size_queue::Status::Running { .. })
            ) {
                if self.sizes.cancel(&entry.name)
                    && let Some(job) = &self.du_job
                {
                    job.cancel();
                }
                return;
            }
            self.sizes
                .enqueue(&self.path, std::iter::once(entry.name.as_os_str()))
        } else {
            Err("Selecciona una carpeta para calcular")
        };
        match result {
            Ok(()) => {
                self.error.clear();
                self.thumbnails.suspend();
                self.du_requested = self.sizes.pending_path();
                if !listed && !self.dual.enabled {
                    self.inspector = true;
                }
                self.rebuild_size_orders();
            }
            Err(error) => self.error = error.into(),
        }
    }
    pub fn mark_and_advance(&mut self) {
        self.folder_restore = None;
        if self.selection_refreshing() {
            return;
        }
        let map = crate::selection_tools::is_map(self);
        if map {
            if crate::selection_tools::blocked(self) {
                return;
            }
            if let Some(reason) =
                crate::selection_tools::map_reason(self, crate::selection_tools::Command::All)
            {
                self.error = format!("Selección: {reason}");
                return;
            }
        }
        let order = if map {
            self.space_geometry.navigation_indices()
        } else {
            self.order.as_slice()
        };
        let Some(i) = self.selected.filter(|i| order.contains(i)) else {
            return;
        };
        self.text_paste.invalidate();
        self.columns.deferred_menu = None;
        self.session.restore_allowed = false;
        if !self.selection.insert(i) {
            self.selection.remove(&i);
        }
        if let Some(rank) = order.iter().position(|&index| index == i) {
            self.selected = order.get(rank + 1).copied().or(Some(i));
        }
        self.anchor = self.selected;
        self.preserve_empty_selection = self.selection.is_empty();
        if map {
            self.scroll = self
                .space_geometry
                .reveal_scroll(self.selected, self.scroll);
        }
        if self.error.starts_with("Selección:") {
            self.error.clear();
        }
    }
    pub fn open_selected(&mut self) {
        if let Some(why) = crate::map_actions::reason(self, &Action::Open) {
            self.error = format!("Espacio: {why}");
            return;
        }
        let Some(entry) = self
            .selected
            .and_then(|i| self.entries.get(i))
            .filter(|e| files_core::visibility::accepts(&e.name, self.show_hidden))
        else {
            return;
        };
        if self
            .path
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add(entry.name.as_encoded_bytes().len())
            .and_then(|n| n.checked_add(1))
            .is_none_or(|n| n > 4096)
        {
            self.error = "Ruta de apertura demasiado larga (máximo 4096 bytes)".into();
            return;
        }
        if entry.is_dir() && self.view == View::Columns {
            self.column_step(true);
            return;
        }
        if entry.is_dir() {
            self.navigate(self.path.join(&entry.name));
            return;
        }
        if self.open_job.is_some() {
            self.error = "Ya hay una apertura en curso".into();
            return;
        }
        self.error.clear();
        match crate::open_job::Job::start(self.path.join(&entry.name)) {
            Ok(job) => self.open_job = Some(job),
            Err(error) => self.error = error,
        }
    }
    pub fn open_terminal(&mut self) {
        if self.closing {
            return;
        }
        if self.open_job.is_some() {
            self.error = "Ya hay una solicitud de apertura o terminal pendiente".into();
            return;
        }
        self.error.clear();
        match crate::open_job::Job::start_terminal(self.path.clone()) {
            Ok(job) => self.open_job = Some(job),
            Err(error) => self.error = error,
        }
    }
    pub fn set_corners(&mut self, c: Corners) {
        self.corners = c;
        self.radius = theme::rounding(c);
        self.config_revision += 1;
        self.config_dirty = true;
        self.config_error = None;
        self.dispatch_config();
    }
    pub fn dispatch_config(&mut self) {
        if self.config_dirty
            && self.config_in_flight.is_none()
            && self.config_error.is_none()
            && self
                .aux
                .tx
                .try_send(crate::aux_io::Request::Preferences(
                    self.config_revision,
                    self.corners,
                    self.window_mode,
                    self.show_hidden,
                    self.operation_bar,
                    self.preview.muted(),
                ))
                .is_ok()
        {
            self.config_in_flight = Some(self.config_revision);
        }
    }
    pub fn reload_theme(&mut self) -> bool {
        let accepted = std::fs::read_to_string(theme::theme_path())
            .ok()
            .and_then(|s| Palette::parse(&s).ok());
        let valid = accepted.is_some();
        if let Some(p) = accepted {
            self.system_palette = p;
            self.apply_theme();
        }
        self.radius = theme::rounding(self.corners);
        valid
    }
    pub fn apply_theme(&mut self) {
        self.palette = if self.alternate_theme {
            Palette::parse(include_str!("../../../themes/tokyo-night.toml"))
                .unwrap_or_else(|_| self.system_palette.clone())
        } else {
            self.system_palette.clone()
        };
    }
    pub fn action(&mut self, a: Action) {
        if let Action::DeleteAnswer(confirmed) = a {
            self.answer_delete(confirmed);
            return;
        }
        if self.delete.confirm.is_some() {
            return;
        }
        if let Action::OpenWithCmd(command) = a {
            self.open_with_command(command);
            return;
        }
        if self.open_with.is_some() {
            return;
        }
        if let Action::TrashBrowser(id, command) = a {
            self.trash_browser_command(id, command);
            return;
        }
        if self.trash_browser.is_some()
            && !matches!(
                a,
                Action::None | Action::CancelClose | Action::DiscardClose | Action::RetrySaves
            )
        {
            return;
        }
        if let Action::Checksum(stamp, command) = a {
            self.checksum_command(stamp, command);
            return;
        }
        if let Action::Comparison(stamp, command) = a {
            self.comparison_command(stamp, command);
            return;
        }
        if self.comparison.is_some()
            && !matches!(
                a,
                Action::None | Action::CancelClose | Action::DiscardClose | Action::RetrySaves
            )
        {
            return;
        }
        if self.checksums.is_some()
            && !matches!(
                a,
                Action::None | Action::CancelClose | Action::DiscardClose | Action::RetrySaves
            )
        {
            return;
        }

        if !matches!(a, Action::None) {
            self.launch_notice = None;
        }
        if let Action::SavedSearchCommand(stamp, command) = a {
            self.saved_search_command(stamp, command);
            return;
        }
        if self.saved_searches.panel.is_some()
            && !matches!(a, Action::SavedSearches(_) | Action::None)
        {
            return;
        }
        if let Action::SavedSearches(save) = a {
            self.open_saved_searches(save);
            return;
        }

        if let Action::SftpCommand(stamp, cmd) = a {
            self.sftp_command(stamp, cmd);
            return;
        }
        if self.sftp.form.is_some() && !matches!(a, Action::Sftp | Action::None) {
            return;
        }

        if let Action::Bookmark(stamp, command) = a {
            self.bookmark_command(stamp, command);
            return;
        }

        if self.bookmarks.panel.is_some()
            && !matches!(
                a,
                Action::Bookmarks | Action::BookmarkAdd | Action::Bookmark(..) | Action::None
            )
        {
            return;
        }

        if let Action::OperationBar(owner, generation, index) = a {
            self.operation_bar_run(owner, generation, index);
            return;
        }
        if matches!(a, Action::ToggleOperationBar) {
            self.toggle_operation_bar();
            return;
        }
        if !matches!(a, Action::None) && self.error.starts_with("MC: ") {
            self.error.clear();
        }
        if !matches!(a, Action::None | Action::Filter) {
            self.end_filter_edit();
        }
        if self.closing {
            self.commands = None;
            self.size_panel = None;
            self.trash_browser = None;
        }
        if self.size_panel.is_some()
            && !matches!(a, Action::Sizes | Action::SizePanel(_, _) | Action::None)
        {
            return;
        }
        if self.commands.is_some()
            && !matches!(
                a,
                Action::Commands
                    | Action::CommandSelect(_, _)
                    | Action::CommandActivate(_)
                    | Action::CommandRun
                    | Action::CommandClose
                    | Action::CommandFocus
                    | Action::CommandRecapture
                    | Action::None
            )
        {
            return;
        }

        if !matches!(a, Action::None) {
            self.cancel_outgoing_drag();
            self.native_transfer.cancel_pending();
            self.text_paste.invalidate();
            self.columns.deferred_menu = None;
        }
        if matches!(
            a,
            Action::View(_)
                | Action::Sort(_)
                | Action::GridZoom(_)
                | Action::PaneSort(..)
                | Action::DualPane
        ) {
            self.folder_restore = None;
        }
        self.session.restore_allowed = false;
        let review = crate::copy_review::supported(&a);
        let rejection = if review {
            crate::copy_review::validate(self, &a)
        } else {
            crate::map_actions::validate(self, &a)
        };
        if let Some(why) = rejection {
            self.error = format!("{}: {why}", if review { "Revisión" } else { "Espacio" });
            return;
        }
        if review && self.error.starts_with("Revisión:") {
            self.error.clear();
        }
        if crate::selection_tools::is_map(self)
            && crate::map_actions::supported(&a)
            && self.error.starts_with("Espacio:")
        {
            self.error.clear();
        }
        if matches!(
            a,
            Action::Open
                | Action::Preview
                | Action::Properties
                | Action::CopyFiles
                | Action::MoveFiles
                | Action::ExtractArchive
                | Action::CreateArchive
                | Action::Rename
                | Action::Trash
        ) {
            self.sanitize_selection();
        }
        if !matches!(a, Action::ContextItem(_) | Action::None) {
            self.context_menu = None;
        }

        if !matches!(a, Action::None | Action::Actions) {
            self.actions_menu = false;
        }
        match a {
            Action::ToggleOperationBar | Action::OperationBar(_, _, _) => unreachable!(),
            Action::CompareFolders => self.open_comparison(),
            Action::ReviewSyncFolders => self.open_sync_review(),
            Action::Comparison(..) => unreachable!("comparison command handled first"),
            Action::Checksums => self.open_checksums(),
            Action::Duplicates => self.open_duplicates(),
            Action::Checksum(..) => unreachable!(),
            Action::Sftp => self.open_sftp(),
            Action::SavedSearches(_) | Action::SavedSearchCommand(..) => {
                unreachable!("saved catalogue handled before modal guards")
            }
            Action::SftpCommand(..) => unreachable!(),
            Action::Bookmarks => self.open_bookmarks(),
            Action::BookmarkAdd => self.add_bookmark(),
            Action::SaveFolderView => self.save_folder_view(false),
            Action::ForgetFolderView => self.save_folder_view(true),
            Action::RetryFolderView => self.folder_views.refresh(),
            Action::DiscardFolderView => {
                self.folder_restore = None;
                if self.folder_views.can_discard() {
                    self.folder_views.discard();
                }
            }
            Action::Bookmark(..) => unreachable!(),
            Action::NativePaste => self.native_paste(),
            a @ (Action::ClipboardCopy | Action::ClipboardCut | Action::ClipboardPaste) => {
                self.clipboard_action(a)
            }
            Action::DualPane => {
                self.settings = false;
                self.toggle_dual();
            }
            Action::PaneFocus(id) => {
                if id != self.dual.active_id && self.dual.other.as_ref().is_some_and(|b| b.id == id)
                {
                    self.switch_dual();
                }
            }
            Action::PaneFilter(id, rev, clear) => self.filter_pane(id, rev, clear),
            Action::PaneSort(id, rev, sort) => self.sort_pane(id, rev, sort),
            Action::PaneEntry(id, rev, i) => self.pane_select(id, rev, i, false, false),
            Action::PaneBackground(id, rev) => {
                self.pane_background(id, rev);
            }
            Action::ColumnFocus(id) => self.column_focus(id, None, false, false),
            Action::ColumnEntry(id, i) => self.column_focus(id, Some(i), false, false),
            Action::ColumnScroll(delta) => {
                self.columns.horizontal = (self.columns.horizontal + delta).max(0)
            }
            Action::Commands => self.open_commands(),
            Action::CommandClose => {
                self.commands = None;
                self.hits.clear();
            }
            Action::CommandFocus => {
                if let Some(s) = &mut self.commands {
                    s.select_input();
                }
            }
            Action::CommandSelect(epoch, i) => {
                if let Some(s) = &mut self.commands
                    && s.epoch == epoch
                    && i < s.matches.len()
                {
                    s.selected = i;
                    s.error.clear();
                }
            }
            Action::CommandRun => self.run_command(),
            Action::CommandActivate(epoch) => {
                if self.commands.as_ref().is_some_and(|s| s.epoch == epoch) {
                    self.run_command();
                }
            }
            Action::CommandRecapture => self.recapture_commands(),
            Action::RecoveryCatalog => self.open_move_catalog(),
            Action::RecoveryClose => self.recovery_catalog.close(),
            Action::RecoverySelect(index) => self.select_move_catalog(index),
            Action::RecoverMove(action) => self.request_move_recovery(action),
            Action::SaveSession => {
                self.request_session_save();
            }
            Action::Tab(command) => self.tab_command(command),
            Action::ContextItem(i) => crate::context_menu::activate(self, i),
            Action::ToggleHidden => self.toggle_hidden(),
            Action::Refresh => {
                self.listing_sync.retry();
                self.refresh_current();
            }
            Action::Help => {
                self.help = if self.help.is_some() {
                    None
                } else {
                    Some(Default::default())
                };
                self.hits.clear();
                self.settings = false;
                self.context_menu = None;
            }
            Action::HelpClose => {
                self.help = None;
                self.hits.clear();
            }
            Action::HelpScroll(d) => {
                if let Some(help) = &mut self.help {
                    help.scroll(d);
                }
            }
            Action::History(forward) => self.travel_history(forward),
            Action::Terminal => self.open_terminal(),
            Action::CopyFiles => self.open_copy(),
            Action::MoveFiles => self.open_copy_mode(true),
            Action::ExtractArchive => self.open_extract(),
            Action::CreateArchive => self.open_create_archive(),
            Action::ArchiveReview(id, action) => self.review_archive_recovery(id, action),
            Action::ArchiveBack(stamp) => self.back_archive_recovery(stamp),
            Action::ArchivePause(stamp) => self.pause_archive_recovery(stamp),
            Action::ArchiveClearPending(stamp) => self.clear_before_archive_recovery(stamp),
            Action::ArchiveCommit(stamp) => self.commit_archive_recovery(stamp),
            Action::ApplyArchive(epoch, id) => {
                if self
                    .copy_form
                    .as_ref()
                    .is_some_and(|d| d.extracting && d.editor.epoch == epoch && d.prepare_id == id)
                {
                    self.start_copy();
                }
            }
            Action::Trash => self.send_trash(),
            Action::DeletePermanently => self.request_permanent_delete(),
            Action::DeleteAnswer(_) => {}
            Action::CreateLinks => self.create_links(),
            Action::OpenWith => self.open_with(),
            Action::NewFromTemplate(t) => self.new_from_template(t),
            Action::FolderWithSelection => self.folder_with_selection(),
            Action::Device(node, op) => self.device_action(node, op),
            Action::OpenWithCmd(_) => {}
            Action::OpenTrash => self.open_trash_browser(),
            Action::TrashBrowser(..) => {}
            Action::CopyDestination => {
                if let Some(form) = self.copy_form.as_mut().filter(|d| !d.options_open) {
                    form.name_focus = false;
                    form.name_field.editor.touch();
                    form.select_all();
                }
            }
            Action::CopyNameFocus(epoch) => {
                if let Some(d) = self.copy_form.as_mut().filter(|d| {
                    !d.moving
                        && !d.extracting
                        && !d.preparing
                        && !d.options_open
                        && d.name_field.editor.epoch == epoch
                }) {
                    d.name_focus = true;
                    d.name_field.select_all();
                    d.editor.touch();
                }
            }
            Action::CopyNameStep(epoch, delta) => {
                if let Some(d) = self.copy_form.as_mut().filter(|d| {
                    !d.moving
                        && !d.extracting
                        && !d.preparing
                        && !d.options_open
                        && d.name_field.editor.epoch == epoch
                }) {
                    match d.name_field.step(delta) {
                        Ok(()) => {
                            d.name_focus = true;
                            d.error.clear();
                            d.editor.touch();
                        }
                        Err(e) => d.error = e.into(),
                    }
                }
            }
            Action::CopyNameReset(epoch) => {
                if let Some(d) = self.copy_form.as_mut().filter(|d| {
                    !d.moving
                        && !d.extracting
                        && !d.preparing
                        && !d.options_open
                        && d.name_field.editor.epoch == epoch
                }) {
                    d.name_field.reset();
                    d.name_focus = true;
                    d.error.clear();
                    d.editor.touch();
                }
            }
            Action::CopyOptions => {
                if let Some(d) = self
                    .copy_form
                    .as_mut()
                    .filter(|d| !d.moving && !d.extracting && !d.preparing)
                {
                    d.options_open = !d.options_open;
                    d.editor.dragging = false;
                    d.name_field.editor.dragging = false;
                    d.editor.touch();
                    d.name_field.editor.touch();
                    d.error.clear();
                    self.text_paste.invalidate();
                    self.hits.clear();
                }
            }
            Action::CopyVisiblePublication | Action::CopySymlinkTimes => {
                if let Some(d) = self
                    .copy_form
                    .as_mut()
                    .filter(|d| !d.moving && !d.extracting && !d.preparing && d.options_open)
                {
                    if matches!(a, Action::CopyVisiblePublication) {
                        use files_core::copy_engine::Publication as P;
                        d.publication = match d.publication {
                            P::StagedRename => P::VisibleExclusive,
                            P::VisibleExclusive => P::StagedRename,
                        };
                    } else {
                        d.preserve_symlink_times = !d.preserve_symlink_times;
                    }
                    d.error.clear();
                    self.hits.clear();
                }
            }
            Action::CopySkipExisting => {
                if let Some(form) = self
                    .copy_form
                    .as_mut()
                    .filter(|d| !d.moving && !d.extracting && !d.preparing && !d.options_open)
                {
                    use files_core::copy_engine::ExistingDestination as P;
                    form.existing_destination = match form.existing_destination {
                        P::Stop => P::Skip,
                        P::Skip => P::KeepBoth,
                        P::KeepBoth => P::Stop,
                    };
                }
            }
            Action::CopyStartPaused => {
                if let Some(form) = self.copy_form.as_mut().filter(|d| !d.options_open) {
                    form.start_paused = !form.start_paused;
                }
            }
            Action::CopyDialogCancel => self.cancel_copy_dialog(),
            Action::CopyStart => self.start_copy(),
            Action::CopyPause(id, pause) => {
                if pause {
                    self.copy.pause(id);
                } else {
                    self.copy.resume(id);
                }
            }
            Action::CopyCancel(id) => {
                self.copy.cancel(id);
            }
            Action::CopyClearPending => self.cancel_copy_pending(),
            Action::CopyDetails => self.show_copy_details(),
            Action::RetryPendingMove => self.retry_pending_move(),
            Action::NoticeNavigate(command) => crate::mutations::navigate_notice(self, command),
            Action::CopyFullReport => self.copy_full_report(),
            Action::Preview => self.open_preview(),
            Action::ClosePreview => self.preview.close(),
            Action::PreviewNeighbor(delta) => self.preview_neighbor(delta),
            Action::PreviewImageZoom(factor) => {
                if let Some(viewport) = self.preview.bitmap_viewport_mut() {
                    viewport.zoom_by(factor);
                }
            }
            Action::PreviewRotate(delta) => {
                if let Some(viewport) = self.preview.bitmap_viewport_mut() {
                    viewport.rotate_by(delta);
                }
            }
            Action::PreviewImageReset => {
                if let Some(viewport) = self.preview.bitmap_viewport_mut() {
                    viewport.reset();
                }
            }
            Action::PreviewAudioToggle => self.preview.toggle_audio(),
            Action::PreviewAudioSeek(delta) => self.preview.seek_audio_by(delta),
            Action::PreviewVideoAudio => {
                if self.preview.toggle_video_audio() {
                    // Global and saved: every later preview starts like this.
                    self.preview_muted_configured = true;
                    self.config_revision += 1;
                    self.config_dirty = true;
                    self.config_error = None;
                    self.dispatch_config();
                }
            }
            Action::PreviewVideoToggle => self.preview.toggle_video(),
            Action::PreviewPdfPage(delta) => self.preview.change_pdf_page(delta),
            Action::PreviewPdfText => self.preview.toggle_pdf_text(),
            Action::PreviewVideoSeek(delta) => self.preview.seek_video_by(delta),
            Action::CopyPreview => self.copy_preview(),
            Action::SelectPreview => {
                if let Some(document) = self.preview.document_mut() {
                    document.select_all();
                }
            }
            Action::SpaceRefresh => {
                // Same explicit intent as F5: renew listing-to-map name mapping too.
                self.listing_sync.retry();
                self.refresh_current();
            }
            Action::SpaceListScroll(rows) => {
                if self.view == View::SpaceMap && !self.dual.enabled {
                    crate::space_map_view::scroll(self, rows);
                }
            }
            Action::SpaceCancel => {
                self.space_requested = false;
                self.space_map.close();
                self.space_geometry.clear();
            }
            a @ (Action::Search
            | Action::SearchStart
            | Action::SearchPages
            | Action::SearchPageStep(..)
            | Action::SearchCancel
            | Action::SearchClose
            | Action::SearchHidden
            | Action::SearchFocus
            | Action::SearchScopeMenu
            | Action::SearchScope(..)
            | Action::SearchContent
            | Action::SearchRegex
            | Action::SearchOrder
            | Action::SearchFilters
            | Action::SearchField(_)
            | Action::SearchClearFilters
            | Action::SearchCalendar
            | Action::SearchDate(_)
            | Action::SearchSelect(..)
            | Action::SearchToggle(..)
            | Action::SearchMarkAll(_)
            | Action::SearchOpen(_)
            | Action::SearchBackgroundTab
            | Action::SearchTransfer(_)) => self.search_action(a),
            Action::CalculateMarkedSizes => {
                if self.request_marked_folder_sizes() {
                    self.open_size_panel();
                }
            }
            Action::CalculateAllocatedSize | Action::CalculateMarkedAllocatedSizes => {
                if self.request_allocated_folder_sizes(matches!(
                    a,
                    Action::CalculateMarkedAllocatedSizes
                )) {
                    self.open_size_panel();
                }
            }
            Action::Sizes => self.open_size_panel(),
            Action::SizePanel(stamp, command) => self.size_panel_command(stamp, command),
            Action::PauseSizes(paused) => self.sizes.pause(paused),
            Action::RetrySizeErrors => {
                if self.sizes.retry_errors() > 0 {
                    self.du_requested = self.sizes.pending_path();
                    self.thumbnails.suspend();
                }
            }
            Action::CancelListedSizes => {
                self.sizes.cancel_all();
                self.du_requested = None;
                if let Some(job) = &self.du_job {
                    job.cancel();
                }
            }
            Action::CancelSize(name) => {
                if self.sizes.cancel(&name)
                    && let Some(job) = &self.du_job
                {
                    job.cancel();
                }
            }
            Action::CalculateSize | Action::CalculateListedSizes => {
                self.request_folder_sizes(matches!(a, Action::CalculateListedSizes));
            }
            Action::Filter => {
                if self.view == View::SpaceMap {
                    self.action(Action::View(View::List));
                }
                self.begin_filter_edit(true);
            }
            Action::SelectionMenu(w, h) => {
                crate::context_menu::open_selection(self, w, h);
            }
            Action::Selection(command) => self.selection_command(command),
            Action::ClearFilter => {
                self.query.clear();
                self.scroll = 0;
                self.rebuild();
            }
            Action::None => {}
            Action::Properties => self.open_properties(),
            Action::PropertyBit(i) => {
                if let Some(d) = &mut self.properties_dialog {
                    d.toggle(i);
                }
            }
            Action::PropertyApply => self.apply_properties(),
            Action::PropertyReload => self.reload_properties(),
            Action::PropertyClose => self.close_properties(),
            Action::Rename => self.open_rename(crate::rename_dialog::Mode::Rename),
            Action::NewEntry(folder) => self.open_rename(if folder {
                crate::rename_dialog::Mode::Folder
            } else {
                crate::rename_dialog::Mode::File
            }),
            Action::RenameField(i) => {
                if let Some(d) = &mut self.rename_dialog {
                    d.focus = i.min(4);
                    d.replace = true;
                }
            }
            Action::RenameCase => {
                if let Some(d) = &mut self.rename_dialog {
                    d.case = match d.case {
                        files_core::rename_plan::Case::Preserve => {
                            files_core::rename_plan::Case::Lower
                        }
                        files_core::rename_plan::Case::Lower => {
                            files_core::rename_plan::Case::Upper
                        }
                        files_core::rename_plan::Case::Upper => {
                            files_core::rename_plan::Case::Preserve
                        }
                    };
                    d.change();
                }
            }
            Action::RenameRegex => {
                if let Some(d) = &mut self.rename_dialog {
                    d.regex = !d.regex;
                    d.change();
                }
            }
            Action::RenameApply => self.apply_rename(),
            Action::RenameCancel => self.cancel_rename(),
            Action::Undo(redo) => self.undo_rename(redo),
            Action::DismissNotice => {
                self.notice_open = false;
                self.archive_confirmation = None;
            }
            Action::CopyNotice => fltk::app::copy(&format!(
                "{}\n{}",
                self.operation_message.as_deref().unwrap_or_default(),
                self.operation_details.join("\n")
            )),
            Action::RetrySaves => self.retry_saves(),
            Action::CancelClose => {
                self.closing = false;
                self.session.closing_saved = false;
            }
            Action::DiscardClose => self.discard_close = true,
            Action::Entry(i) => self.select_entry(i, false, false),
            Action::GridZoom(factor) => {
                if factor.is_finite() && factor > 0. {
                    self.grid_zoom = (self.grid_zoom * factor).clamp(0.65, 1.8);
                    self.scroll = 0;
                    self.hits.clear();
                    self.visible.clear();
                }
            }
            Action::Zoom(factor) => {
                if self.layout_ready {
                    let zoom = self.layout.zoom * factor;
                    Arc::make_mut(&mut self.layout).set_zoom(zoom);
                    self.layout_changed();
                    self.save_layout();
                }
            }
            Action::Navigate(p) => self.navigate(p),
            Action::View(v) => {
                if self.dual.enabled {
                    self.toggle_dual();
                    if self.dual.enabled {
                        return;
                    }
                }

                if v == View::Columns
                    && crate::columns::charge(&self.entries, &self.order, &self.selection)
                        > crate::columns::DATA_LIMIT
                {
                    self.error="Columnas: listado actual supera16MiB; navega a una carpeta menor o utiliza Lista".into();
                    return;
                }
                self.hits.clear();
                self.visible.clear();
                self.hovered = None;
                self.drag = None;
                self.view = v;
                self.reset_space_map();
                if v == View::SpaceMap {
                    self.preview.close();
                    if let Some(job) = &self.du_job {
                        job.cancel();
                    }
                    self.sizes.cancel_all();
                    self.du_requested = None;
                    self.du_target = None;
                    self.du_text = None;
                }
                self.scroll = 0;
                self.rebuild();
                self.request_stats();
            }
            Action::Inspector => {
                if self.dual.enabled {
                    self.settings = false;
                    self.toggle_dual();
                    if !self.dual.enabled {
                        self.inspector = true;
                    }
                } else {
                    self.inspector = !self.inspector;
                }
            }
            Action::Settings => {
                self.settings = !self.settings;
                self.actions_menu = false;
            }
            Action::Actions => {
                self.actions_menu = !self.actions_menu;
                self.settings = false;
            }
            Action::WindowMode(mode) => {
                self.window_mode = mode;
                self.config_revision += 1;
                self.config_dirty = true;
                self.config_error = None;
                self.dispatch_config();
            }
            Action::Corner(c) => self.set_corners(c),
            Action::Theme(alt) => {
                self.alternate_theme = alt;
                self.apply_theme();
            }
            Action::Sort(s) => {
                if self.sort == s {
                    self.descending = !self.descending;
                } else {
                    self.sort = s;
                    self.descending = matches!(s, Sort::Size | Sort::Modified);
                }
                self.rebuild();
                self.request_stats();
            }
            Action::Open => self.open_selected(),
            Action::CopyPath => {
                self.copy_path_with(fltk::app::copy);
            }
            Action::Omni => self.begin_path_edit(),
            Action::PathCommit => {
                if self.editing {
                    self.commit_path_edit();
                }
            }
            Action::PathCancel => self.end_path_edit(),
            Action::ViewMenu(w, h) => crate::context_menu::open_views(self, w, h),
        }
    }
}
fn toml_parse(s: &str) -> Option<Preferences> {
    // El núcleo mantiene el parser y la UI sólo obtiene la preferencia.
    files_core::theme::parse_preferences(s).ok()
}
