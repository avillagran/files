//! Two-root comparison and synchronization review, without applying changes.
//! The panel retains either reader until its reap ACK, including window close.
use crate::{
    directory_compare_job::Job,
    line_edit::{Cursor, Motion},
    line_field::Geometry,
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::{
    directory_compare::{self as compare, Difference, Progress, Report, Row},
    directory_sync::{self as sync, Choice, Conflict, Mode, Proposal, Side},
    directory_sync_wire,
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
use std::{
    fmt::{self, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
fn epoch() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("comparison IDs exhausted")
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Start,
    Cancel,
    Close,
    Focus(usize),
    Results,
    Move(i32),
    Select(usize),
    Filter,
    Budget,
    Mode(i32),
    Choice(i32),
    ResetChoice,
    Locate(usize),
    Copy,
    Pan(i32),
}
impl Command {
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "Comparar o preparar revisión fresca de las dos raíces, F5",
            Self::Mode(_) => "Cambiar modo o dirección, F6; Shift+F6 retrocede",
            Self::Choice(_) => "Alternar decisión de esta fila, F9; Shift+F9 retrocede",
            Self::ResetChoice => "Dejar decisión pendiente, F10",
            Self::Cancel => "Cancelar y esperar ACK del lector",
            Self::Close => "Cerrar tras ACK, Esc",
            Self::Focus(_) => "Editar raíz local, Ctrl+L",
            Self::Results => "Volver a resultados, Tab",
            Self::Move(_) | Self::Select(_) => "Recorrer resultados de comparación",
            Self::Filter => "Alternar diferencias, desconocidos o todos, F7",
            Self::Budget => "Cambiar presupuesto de datos hash, F8",
            Self::Locate(0) => "Ubicar observación en raíz A, Enter",
            Self::Locate(_) => "Ubicar observación en raíz B, Shift+Enter",
            Self::Copy => "Solicitar copia del informe completo, Ctrl+C",
            Self::Pan(_) => "Desplazar la ruta seleccionada, Izquierda/Derecha",
        }
    }
}
#[derive(Default)]
struct Editor {
    text: String,
    cursor: Cursor,
    geometry: Geometry,
}
enum Reader {
    Compare(Job),
    Sync(crate::directory_sync_job::Job),
}
enum Data {
    Compare(Report),
    Sync(sync::Review),
}
impl Reader {
    fn cancel(&self) {
        match self {
            Self::Compare(j) => j.cancel(),
            Self::Sync(j) => j.cancel(),
        }
    }
    fn progress(&self) -> Progress {
        match self {
            Self::Compare(j) => j.progress(),
            Self::Sync(j) => j.progress(),
        }
    }
    fn poll(&self) -> Option<Result<Data, String>> {
        match self {
            Self::Compare(j) => j.poll().map(|r| r.map(Data::Compare)),
            Self::Sync(j) => j.poll().map(|r| r.map(Data::Sync)),
        }
    }
}
const MODES: [Option<Mode>; 6] = [
    None,
    Some(Mode::Update(Side::Left)),
    Some(Mode::Update(Side::Right)),
    Some(Mode::Mirror(Side::Left)),
    Some(Mode::Mirror(Side::Right)),
    Some(Mode::Merge),
];
fn mode_label(mode: Option<Mode>) -> &'static str {
    match mode {
        None => "Comparar",
        Some(Mode::Update(Side::Left)) => "Actualizar A → B",
        Some(Mode::Update(Side::Right)) => "Actualizar B → A",
        Some(Mode::Mirror(Side::Left)) => "Espejo A → B",
        Some(Mode::Mirror(Side::Right)) => "Espejo B → A",
        Some(Mode::Merge) => "Combinar A ↔ B",
    }
}
fn proposal_label(p: Proposal) -> &'static str {
    match p {
        Proposal::Unchanged => "Sin acción",
        Proposal::Retain => "Conservar",
        Proposal::CopyFrom(Side::Left) => "Copiar A→B",
        Proposal::CopyFrom(Side::Right) => "Copiar B→A",
        Proposal::Review(_) => "Conflicto",
        Proposal::CoveredBy(_) => "En ancestro",
        Proposal::Blocked => "Bloqueado",
    }
}
fn choice_label(c: Choice) -> &'static str {
    match c {
        Choice::Pending => "Pendiente",
        Choice::Skip => "Omitir",
        Choice::PreserveAndReplaceFrom(Side::Left) => "Reemplazar B desde A con respaldo",
        Choice::PreserveAndReplaceFrom(Side::Right) => "Reemplazar A desde B con respaldo",
        Choice::PreserveAndSetMetadataFrom(Side::Left) => "Metadatos A→B con respaldo",
        Choice::PreserveAndSetMetadataFrom(Side::Right) => "Metadatos B→A con respaldo",
        Choice::PreserveAndRemove => "Retirar destino con respaldo",
    }
}
pub struct Panel {
    pub stamp: u64,
    pub pending: bool,
    pub closing: bool,
    pub error: String,
    pub progress: Progress,
    pub report: Option<Report>,
    review: Option<sync::Review>,
    mode: Option<Mode>,
    cancelled: bool,
    pub selected: usize,
    pub focus: Option<usize>,
    editors: [Editor; 2],
    base: PathBuf,
    owner: (u64, u64),
    roots: Option<[PathBuf; 2]>,
    job: Option<Reader>,
    prior_pause: Option<bool>,
    budget: usize,
    filter: usize,
    visible: Vec<usize>,
    top: usize,
    rows: usize,
    pan: i32,
    pan_limit: i32,
    drag: Option<usize>,
}
const BUDGETS: [u64; 4] = [
    64 * 1024 * 1024,
    1024 * 1024 * 1024,
    16 * 1024 * 1024 * 1024,
    compare::MAX_READ,
];
const BUDGET_LABELS: [&str; 4] = ["64 MiB", "1 GiB", "16 GiB", "1 TiB"];
impl Panel {
    fn capture(m: &Model) -> Result<Self, String> {
        let first = crate::path_edit::display(&m.path).map_err(str::to_owned)?;
        let second = m
            .opposite_destination()
            .map(crate::path_edit::display)
            .transpose()
            .map_err(str::to_owned)?;
        let focus = second.is_none().then_some(1);
        let mut editors = [
            Editor {
                text: first,
                ..Editor::default()
            },
            Editor {
                text: second.unwrap_or_default(),
                ..Editor::default()
            },
        ];
        for e in &mut editors {
            e.cursor.all(&e.text);
        }
        Ok(Self {
            stamp: epoch(),
            pending: false,
            closing: false,
            error: String::new(),
            progress: Progress::default(),
            report: None,
            review: None,
            mode: None,
            cancelled: false,
            selected: 0,
            focus,
            editors,
            base: m.path.clone(),
            owner: m.pane_owner(),
            roots: None,
            job: None,
            prior_pause: None,
            budget: 1,
            filter: 0,
            visible: Vec::new(),
            top: 0,
            rows: 1,
            pan: 0,
            pan_limit: 0,
            drag: None,
        })
    }
    pub fn busy(&self) -> bool {
        self.pending || self.job.is_some()
    }
    pub fn cancel(&mut self, close: bool) {
        self.cancelled |= self.busy();
        self.pending = false;
        self.closing |= close;
        if let Some(j) = &self.job {
            j.cancel();
        }
    }
    pub fn text_key(&self) -> Option<(u64, usize, u64)> {
        if self.busy() || self.closing {
            return None;
        }
        let side = self.focus?;
        Some((self.stamp, side, self.editors[side].cursor.revision))
    }
    fn reset(&mut self) {
        self.report = None;
        self.review = None;
        self.cancelled = false;
        self.roots = None;
        self.progress = Progress::default();
        self.drag = None;
        self.visible.clear();
        self.selected = 0;
        self.top = 0;
        self.pan = 0;
        self.error.clear();
        self.stamp = epoch();
    }
    fn input(&mut self, text: &str, compose: Option<usize>) -> Result<(), &'static str> {
        if self.busy() || self.closing {
            return Err("Espera el ACK antes de editar raíces");
        }
        let side = self.focus.ok_or("Enfoca una raíz primero")?;
        let e = &mut self.editors[side];
        let range = compose.map_or_else(
            || e.cursor.range(&e.text),
            |n| e.cursor.composition_range(&e.text, n, false),
        );
        let limit = crate::path_edit::edit_limit(&e.text, range, text);
        if let Some(n) = compose {
            e.cursor
                .compose_bounded(&mut e.text, text, n, false, limit)?;
        } else {
            e.cursor.insert_bounded(&mut e.text, text, false, limit)?;
        }
        self.reset();
        Ok(())
    }
    fn resolve(&self) -> Result<[PathBuf; 2], String> {
        let mut roots = Vec::new();
        for e in &self.editors {
            let p = crate::path_edit::destination(&e.text, &self.base).map_err(str::to_owned)?;
            files_core::navigation_history::validate(&p).map_err(str::to_owned)?;
            roots.push(p);
        }
        Ok([roots.remove(0), roots.remove(0)])
    }
    fn copy_with(&self, sink: impl FnOnce(&str)) -> Result<(), &'static str> {
        let text = self.report_text()?;
        sink(&text);
        Ok(())
    }
    fn row_count(&self) -> usize {
        self.review.as_ref().map_or_else(
            || self.report.as_ref().map_or(0, |r| r.rows.len()),
            |r| r.entries().len(),
        )
    }
    fn row_at(&self, i: usize) -> Option<&Row> {
        if let Some(r) = &self.review {
            r.entries().get(i).map(|e| e.observation())
        } else {
            self.report.as_ref()?.rows.get(i)
        }
    }
    fn entry(&self) -> Option<&sync::Entry> {
        self.review
            .as_ref()?
            .entries()
            .get(*self.visible.get(self.selected)?)
    }
    fn filter_label(&self) -> &'static str {
        [
            "diferencias",
            "desconocidos",
            "todos",
            "pendientes",
            "propuestas",
        ][self.filter]
    }
    fn rebuild(&mut self) {
        self.visible = (0..self.row_count())
            .filter(|&i| {
                let row = self.row_at(i).unwrap();
                match self.filter {
                    0 => row.difference != Difference::Equal || row.metadata_differs,
                    1 => row.difference == Difference::Unknown,
                    2 => true,
                    3 => self.review.as_ref().is_some_and(|r| {
                        let e = &r.entries()[i];
                        matches!(e.proposal(), Proposal::Review(_)) && e.choice() == Choice::Pending
                    }),
                    _ => self.review.as_ref().is_some_and(|r| {
                        matches!(
                            r.entries()[i].proposal(),
                            Proposal::CopyFrom(_) | Proposal::Review(_)
                        )
                    }),
                }
            })
            .collect();
        self.selected = 0;
        self.top = 0;
        self.pan = 0;
    }
    fn row(&self) -> Option<&Row> {
        self.row_at(*self.visible.get(self.selected)?)
    }
    fn choices(&self) -> Vec<Choice> {
        let Some(e) = self.entry() else {
            return Vec::new();
        };
        let Proposal::Review(conflict) = e.proposal() else {
            return Vec::new();
        };
        let mut out = vec![Choice::Pending, Choice::Skip];
        if conflict == Conflict::DestinationOnly {
            out.push(Choice::PreserveAndRemove);
        } else {
            for side in [Side::Left, Side::Right] {
                if matches!(self.mode, Some(Mode::Merge))
                    || matches!(self.mode,Some(Mode::Update(s)|Mode::Mirror(s)) if s==side)
                {
                    out.push(if conflict == Conflict::Metadata {
                        Choice::PreserveAndSetMetadataFrom(side)
                    } else {
                        Choice::PreserveAndReplaceFrom(side)
                    });
                }
            }
        }
        out
    }
    fn choose(&mut self, dir: Option<i32>) -> Result<(), String> {
        let choices = self.choices();
        let e = self.entry().ok_or("Selecciona un conflicto")?;
        if choices.is_empty() {
            return Err("Esta fila depende de su ancestro o no admite una decisión".into());
        }
        let at = choices.iter().position(|&c| c == e.choice()).unwrap_or(0);
        let choice = dir.map_or(Choice::Pending, |d| {
            choices[(at as i32 + d).rem_euclid(choices.len() as i32) as usize]
        });
        let i = self.visible[self.selected];
        self.review
            .as_mut()
            .unwrap()
            .choose(i, choice)
            .map_err(|e| e.to_string())?;
        if self.filter == 3 {
            self.rebuild();
        }
        self.error.clear();
        self.stamp = epoch();
        Ok(())
    }
    fn reveal(&mut self) {
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        self.top = self
            .top
            .min(self.selected)
            .max(self.selected.saturating_add(1).saturating_sub(self.rows));
        self.top = self.top.min(self.visible.len().saturating_sub(self.rows));
    }
    pub fn report_text(&self) -> Result<String, &'static str> {
        struct Bounded(String);
        impl fmt::Write for Bounded {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                if s.len() > 16 * 1024 * 1024 - self.0.len() {
                    return Err(fmt::Error);
                }
                self.0.push_str(s);
                Ok(())
            }
        }
        let mut out = Bounded(String::new());
        if let Some(r) = &self.review {
            writeln!(out,"Revisión de sincronización · sólo propuestas; NO aplica cambios\nModo: {}\nRaíces: {:?}\nIdentidades: {:?}\nEstado: {} · decisiones pendientes: {}\nPresupuesto de datos hash: {}\nMotivos: {}\nNo es autorización de escritura. Respaldos, ejecución y recuperación aún pendientes.\n",mode_label(self.mode),self.roots,r.roots(),if r.blocked(){"BLOQUEADO"}else{"OBSERVACIÓN COMPLETA; revisión no equivale a ejecución"},r.unresolved(),BUDGET_LABELS[self.budget],reasons(r.progress())).map_err(|_|"Informe supera16MiB; no se copia versión incompleta")?;
            for (i, e) in r.entries().iter().enumerate() {
                let row = e.observation();
                writeln!(
                    out,
                    "{}\t{:?}\t{}\t{}/{}\tpropuesta={:?}\tdecisión={:?}",
                    i,
                    row.path,
                    status(row),
                    kind_label(row.left),
                    kind_label(row.right),
                    e.proposal(),
                    e.choice()
                )
                .map_err(|_| "Informe supera16MiB; no se copia versión incompleta")?;
            }
            return Ok(out.0);
        }
        let Some(r) = &self.report else {
            writeln!(out,"Sin informe final aceptado · no existe un veredicto de comparación\nBase de rutas relativas: {:?}\nModo: {}\nTexto de raíz A: {:?}\nTexto de raíz B: {:?}\nRaíces solicitadas: {:?}\nPresupuesto de datos hash: {}\nPendiente/lector activo: {}\nDiagnóstico: {}\nBytes de progreso, sin veredicto: {}\n",self.base,mode_label(self.mode),self.editors[0].text,self.editors[1].text,self.roots,BUDGET_LABELS[self.budget],self.busy(),if self.error.is_empty(){"Sin error registrado"}else{&self.error},self.progress.bytes_read).map_err(|_|"Diagnóstico fuera del límite")?;
            return Ok(out.0);
        };
        let roots = self.roots.as_ref().ok_or("Raíces no capturadas")?;
        writeln!(out,"Comparación de observaciones · no instantánea ni autorización de sincronización\nRaíz A: {:?}\nRaíz B: {:?}\nIdentidades (dispositivo,inodo): A=({},{}) B=({},{})\nEstado: {}\nPresupuesto de datos hash: {}\nIguales: {} · diferentes: {} · desconocidos: {} · bytes leídos: {}\nMotivos de parcialidad: {}\n",roots[0],roots[1],r.roots[0].device,r.roots[0].inode,r.roots[1].device,r.roots[1].inode,if r.progress.complete(){"COMPLETO"}else{"PARCIAL; no prueba ausencia ni igualdad del resto"},BUDGET_LABELS[self.budget],r.progress.equal,r.progress.different,r.progress.unknown,r.progress.bytes_read,if r.progress.reasons==0{"ninguno".into()}else{reasons(r.progress)}).map_err(|_|"Informe supera16MiB; no se copia una versión incompleta")?;
        for row in &r.rows {
            writeln!(
                out,
                "{:?}\t{}\t{}/{}\tmetadatos={}",
                row.path,
                status(row),
                kind_label(row.left),
                kind_label(row.right),
                if row.metadata_differs {
                    "distintos"
                } else {
                    "sin diferencia observada"
                }
            )
            .map_err(|_| "Informe supera16MiB; no se copia una versión incompleta")?;
        }
        Ok(out.0)
    }
}
fn reasons(p: Progress) -> String {
    let mut labels = Vec::new();
    for (bit, label) in [
        (compare::LIMIT, "límites de inventario"),
        (compare::UNREADABLE, "sin lectura"),
        (compare::CHANGED, "cambios observados"),
        (compare::READ_LIMIT, "límite de datos hash"),
        (compare::UNSUPPORTED, "tipo no comparable"),
        (compare::ALIAS, "alias de directorio"),
    ] {
        if p.reasons & bit != 0 {
            labels.push(label);
        }
    }
    labels.join(", ")
}
impl Model {
    pub fn open_comparison(&mut self) {
        if self.closing
            || self.editing
            || self.filter_edit.active
            || self.search.visible
            || self.commands.is_some()
            || self.size_panel.is_some()
            || self.checksums.is_some()
            || self.comparison.is_some()
            || self.preview.visible()
            || self.help.is_some()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.properties_dialog.is_some()
            || self.notice_open
            || self.recovery_catalog.open
            || self.bookmarks.panel.is_some()
            || self.saved_searches.panel.is_some()
            || self.sftp.form.is_some()
        {
            return;
        }
        match Panel::capture(self) {
            Ok(panel) => {
                self.folder_restore = None;
                self.settings = false;
                self.actions_menu = false;
                self.context_menu = None;
                self.thumbnails.suspend();
                self.text_paste.invalidate();
                self.comparison = Some(panel);
                self.hits.clear();
            }
            Err(e) => self.error = e,
        }
    }
    pub fn open_sync_review(&mut self) {
        if self.comparison.is_some() {
            return;
        }
        self.open_comparison();
        if let Some(p) = &mut self.comparison {
            p.mode = Some(Mode::Update(Side::Left));
            p.reset();
        }
    }
    pub fn comparison_input(&mut self, text: &str) {
        if let Some(p) = &mut self.comparison
            && let Err(e) = p.input(text, None)
        {
            p.error = e.into();
        }
    }
    pub fn comparison_command(&mut self, stamp: u64, command: Command) {
        let Some(mut p) = self.comparison.take() else {
            return;
        };
        if p.stamp != stamp {
            self.comparison = Some(p);
            return;
        }
        self.text_paste.invalidate();
        match command {
            Command::Start
                if !p.busy()
                    && !p.closing
                    && (p.owner != self.pane_owner() || p.base != self.path) =>
            {
                p.reset();
                p.error = "El contexto cambió; vuelve a abrir la herramienta".into();
            }
            Command::Mode(dir) if !p.busy() && !p.closing => {
                let i = MODES.iter().position(|m| *m == p.mode).unwrap();
                p.mode = MODES[(i as i32 + dir).rem_euclid(MODES.len() as i32) as usize];
                p.filter = 0;
                p.reset();
            }
            Command::Choice(dir) if !p.busy() && !p.closing => {
                if let Err(e) = p.choose(Some(dir)) {
                    p.error = e;
                }
            }
            Command::ResetChoice if !p.busy() && !p.closing => {
                if let Err(e) = p.choose(None) {
                    p.error = e;
                }
            }
            Command::Start if !p.busy() && !p.closing => match p.resolve() {
                Ok(roots) => {
                    p.reset();
                    p.roots = Some(roots);
                    p.progress = Progress::default();
                    p.focus = None;
                    p.pending = true;
                    p.prior_pause = Some(self.sizes.paused());
                    self.sizes.pause(true);
                    self.thumbnails.suspend();
                    self.space_map.close();
                    self.space_geometry.clear();
                }
                Err(e) => p.error = e,
            },
            Command::Cancel => p.cancel(false),
            Command::Close => p.cancel(true),
            Command::Focus(side) if side < 2 && !p.busy() && !p.closing => {
                p.focus = Some(side);
                let e = &mut p.editors[side];
                e.cursor.all(&e.text);
                p.stamp = epoch();
            }
            Command::Results => {
                p.focus = None;
                p.stamp = epoch();
            }
            Command::Filter if !p.busy() => {
                p.filter = (p.filter + 1) % if p.mode.is_some() { 5 } else { 3 };
                p.rebuild();
                p.stamp = epoch();
            }
            Command::Budget if !p.busy() => {
                p.budget = (p.budget + 1) % BUDGETS.len();
                p.reset();
            }
            Command::Move(n) => {
                p.focus = None;
                let before = p.selected;
                p.selected = (p.selected as i64 + i64::from(n))
                    .clamp(0, p.visible.len().saturating_sub(1) as i64)
                    as usize;
                if p.selected != before {
                    p.pan = 0;
                    if p.mode.is_some() {
                        p.stamp = epoch();
                    }
                }
                p.reveal();
            }
            Command::Select(i) if i < p.visible.len() => {
                p.focus = None;
                if p.selected != i {
                    p.pan = 0;
                    if p.mode.is_some() {
                        p.stamp = epoch();
                    }
                }
                p.selected = i;
                p.reveal();
            }
            Command::Pan(n) => p.pan = p.pan.saturating_add(n).clamp(0, p.pan_limit),
            Command::Copy => match p.copy_with(app::copy) {
                Ok(()) => {
                    self.launch_notice = Some("Copia del informe solicitada");
                }
                Err(e) => p.error = e.into(),
            },
            Command::Locate(side) if side < 2 && !p.busy() && !p.closing && !self.closing => {
                if p.owner != self.pane_owner() || p.base != self.path {
                    p.error = "El contexto cambió; vuelve a abrir Comparar".into();
                } else if let Some(row) = p.row() {
                    let present = if side == 0 { row.left } else { row.right };
                    if present.is_some() {
                        if let Some(roots) = &p.roots {
                            let path = roots[side].join(&row.path);
                            if let Some((parent, name)) = path.parent().zip(path.file_name()) {
                                match files_core::navigation_history::validate(parent) {
                                    Ok(()) => {
                                        let parent = parent.to_path_buf();
                                        let name = name.to_os_string();
                                        self.navigate(parent.clone());
                                        if self.path == parent {
                                            self.query.clear();
                                            self.rebuild();
                                            self.search_locate = Some((self.generation, name));
                                            p.closing = true;
                                        } else {
                                            p.error = self.error.clone();
                                        }
                                    }
                                    Err(e) => p.error = e.into(),
                                }
                            }
                        }
                    } else {
                        p.error = "Esta observación no contiene ese lado".into();
                    }
                }
            }
            _ => {}
        }
        if !p.busy()
            && let Some(paused) = p.prior_pause.take()
            && !self.closing
        {
            self.sizes.pause(paused);
        }
        if !(p.closing && !p.busy()) {
            self.comparison = Some(p);
        }
        self.hits.clear();
    }
    pub fn pump_comparison(&mut self) -> bool {
        let Some(mut p) = self.comparison.take() else {
            return false;
        };
        let mut dirty = false;
        if self.closing {
            p.cancel(true);
        }
        if let Some(j) = &p.job {
            let progress = j.progress();
            if progress != p.progress {
                p.progress = progress;
                dirty = true;
            }
            if let Some(result) = j.poll() {
                p.job = None;
                match result {
                    Ok(_) if p.cancelled || p.owner != self.pane_owner() || p.base != self.path => {
                        p.error="Lectura descartada tras cancelación o cambio de contexto; sin informe final".into();
                    }
                    Ok(data) => {
                        match data {
                            Data::Compare(report) => {
                                p.progress = report.progress;
                                p.report = Some(report);
                            }
                            Data::Sync(review) => {
                                p.progress = review.progress();
                                p.review = Some(review);
                            }
                        }
                        p.error.clear();
                        p.rebuild();
                    }
                    Err(e) => p.error = e,
                }
                if std::env::var_os("FILES_CLOSE_TRACE").as_deref()
                    == Some(std::ffi::OsStr::new("1"))
                {
                    eprintln!(
                        "FILES_COMPARE_ACK id={} closing={} root_closing={}",
                        p.stamp, p.closing, self.closing
                    );
                }
                p.stamp = epoch();
                dirty = true;
            }
        }
        if p.pending
            && !p.closing
            && !self.closing
            && self.preview.is_drained()
            && self.du_job.is_none()
            && !self.search.busy()
            && self.space_map.is_drained()
            && self.thumbnails.drained()
        {
            p.pending = false;
            let roots = p.roots.as_ref().unwrap();
            let job = if let Some(mode) = p.mode {
                crate::directory_sync_job::Job::start(directory_sync_wire::Intent {
                    version: directory_sync_wire::VERSION,
                    id: p.stamp,
                    left: roots[0].clone(),
                    right: roots[1].clone(),
                    expected: None,
                    mode,
                    read_limit: BUDGETS[p.budget],
                })
                .map(Reader::Sync)
            } else {
                Job::start(
                    p.stamp,
                    roots[0].clone(),
                    roots[1].clone(),
                    None,
                    BUDGETS[p.budget],
                )
                .map(Reader::Compare)
            };
            match job {
                Ok(j) => p.job = Some(j),
                Err(e) => p.error = e,
            }
            dirty = true;
        }
        if !p.busy()
            && let Some(paused) = p.prior_pause.take()
            && !self.closing
        {
            self.sizes.pause(paused);
        }
        if p.closing && !p.busy() {
            self.hits.clear();
            return true;
        }
        self.comparison = Some(p);
        dirty
    }
}
fn kind_label(kind: Option<compare::Kind>) -> &'static str {
    match kind {
        Some(compare::Kind::File) => "archivo",
        Some(compare::Kind::Directory) => "carpeta",
        Some(compare::Kind::Symlink) => "enlace",
        Some(compare::Kind::Other) => "otro",
        None => "no observado",
    }
}
fn status(row: &Row) -> &'static str {
    match row.difference {
        Difference::Equal if row.metadata_differs => "Metadatos",
        Difference::Equal => "Igual",
        Difference::OnlyLeft => "Sólo A",
        Difference::OnlyRight => "Sólo B",
        Difference::Type => "Tipo",
        Difference::Content => "Contenido",
        Difference::Children => "Descendientes",
        Difference::Link => "Enlace",
        Difference::Unknown => "Desconocido",
    }
}
fn button(
    p: &mut dyn Painter,
    m: &mut Model,
    r: Rect,
    label: &str,
    stamp: u64,
    cmd: Command,
    enabled: bool,
) {
    p.rounded_rect(
        r,
        4 * m.radius,
        if enabled {
            m.palette.lighter_background
        } else {
            m.palette.background
        },
    );
    p.text(
        Rect::new(r.x + 3, r.y, r.w - 6, r.h),
        label,
        11,
        if enabled {
            m.palette.accent
        } else {
            m.palette.muted
        },
        true,
        false,
    );
    if enabled {
        m.hits.push(Hit {
            rect: r,
            action: Action::Comparison(stamp, cmd),
            generation: m.generation,
        });
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut panel) = m.comparison.take() else {
        return;
    };
    m.hits.clear();
    let r = Rect::new(12, 66, (w - 24).max(1), (h - 102).max(1));
    p.rounded_rect(r, 6 * m.radius, m.palette.background);
    p.clip(r);
    button(
        p,
        m,
        Rect::new(r.x + 12, r.y + 4, r.w - 24, 26),
        &format!(
            "{} · {} · F6 modo",
            mode_label(panel.mode),
            if panel.mode.is_some() {
                "sólo revisión"
            } else {
                "sólo lectura"
            }
        ),
        panel.stamp,
        Command::Mode(1),
        !panel.busy() && !panel.closing,
    );
    let editing_enabled = !panel.busy();
    for side in 0..2 {
        let field = Rect::new(r.x + 74, r.y + 36 + side as i32 * 38, r.w - 86, 30);
        p.text(
            Rect::new(r.x + 12, field.y, 60, 30),
            if panel.editors[side]
                .text
                .get(..7)
                .is_some_and(|s| s.eq_ignore_ascii_case("file://"))
            {
                if side == 0 { "A URI" } else { "B URI" }
            } else if side == 0 {
                "Raíz A"
            } else {
                "Raíz B"
            },
            12,
            m.palette.dark_foreground,
            true,
            false,
        );
        p.rounded_rect(field, 4 * m.radius, m.palette.lighter_background);
        let e = &mut panel.editors[side];
        if panel.focus != Some(side) {
            e.geometry.reveal_end();
        }
        e.geometry.paint_bounded(
            p,
            &e.text,
            &e.cursor,
            (field, crate::path_edit::URI_LIMIT),
            &m.palette,
            (panel.focus == Some(side) && editing_enabled, panel.stamp),
        );
    }
    let state = if panel.pending {
        "Esperando ACK de lectores anteriores".to_string()
    } else if panel.job.is_some() {
        format!(
            "Comparando · {} entradas · {} objetos",
            panel.progress.entries.iter().sum::<u64>(),
            panel.progress.compared
        )
    } else if let Some(review) = &panel.review {
        format!(
            "{} · {} pendientes",
            if review.blocked() {
                "BLOQUEADO; sin acciones"
            } else {
                "Revisión sin cambios"
            },
            review.unresolved()
        )
    } else if let Some(report) = &panel.report {
        format!(
            "{} · {} distintos · {} desconocidos",
            if report.progress.complete() {
                "Completo"
            } else {
                "PARCIAL"
            },
            report.progress.different,
            report.progress.unknown
        )
    } else {
        "Dos raíces · F5 prepara · Ctrl+L edita".into()
    };
    p.text(
        Rect::new(r.x + 12, r.y + 112, r.w - 24, 22),
        &state,
        12,
        m.palette.accent,
        true,
        false,
    );
    let detail = if !panel.error.is_empty() {
        panel.error.clone()
    } else if panel.progress.reasons != 0 {
        format!("No prueba el resto: {}", reasons(panel.progress))
    } else if let Some(e) = panel.entry() {
        match e.proposal() {
            Proposal::Review(_) => {
                format!("{} · {}", status(e.observation()), choice_label(e.choice()))
            }
            Proposal::CoveredBy(i) => {
                format!("Incluido en la decisión del ancestro, fila {}", i + 1)
            }
            proposal => format!(
                "Propuesta: {} · no aplica cambios",
                proposal_label(proposal)
            ),
        }
    } else {
        "Tab raíces/resultados · Enter A · ⇧Enter B · ←/→ ruta".into()
    };
    p.text(
        Rect::new(r.x + 12, r.y + 134, r.w - 24, 22),
        &detail,
        11,
        if panel.error.is_empty() {
            m.palette.dark_foreground
        } else {
            m.palette.red
        },
        true,
        false,
    );
    p.text(
        Rect::new(r.x + 12, r.y + 156, r.w - 90, 22),
        &format!(
            "{} · F7 {} · {} visibles",
            BUDGET_LABELS[panel.budget],
            panel.filter_label(),
            panel.visible.len()
        ),
        11,
        m.palette.dark_foreground,
        true,
        false,
    );
    let compact = r.w < 700;
    let is_sync = panel.mode.is_some();
    let controls = if is_sync {
        if compact { 94 } else { 64 }
    } else if compact {
        64
    } else {
        34
    };
    let top = r.y + 184;
    let bottom = r.y + r.h - controls - 12;
    panel.rows = ((bottom - top).max(0) / 26).max(1) as usize;
    panel.reveal();
    let name_width = (r.w - 132).max(1);
    panel.pan_limit = panel.row().map_or(0, |row| {
        let label = files_core::name_label::label(row.path.as_os_str());
        (p.text_width(&label, 12, true).ceil() as i32 - name_width).max(0)
    });
    panel.pan = panel.pan.clamp(0, panel.pan_limit);
    for (slot, i) in panel
        .visible
        .iter()
        .enumerate()
        .skip(panel.top)
        .take(panel.rows)
    {
        let row = panel.row_at(*i).unwrap();
        let y = top + (slot - panel.top) as i32 * 26;
        let rect = Rect::new(r.x + 10, y, r.w - 20, 24);
        if slot == panel.selected {
            p.rounded_rect(rect, 3 * m.radius, m.palette.selection);
        }
        let label = files_core::name_label::label(row.path.as_os_str());
        p.clip(Rect::new(r.x + 14, y, name_width, 24));
        p.text(
            Rect::new(
                r.x + 14 - if slot == panel.selected { panel.pan } else { 0 },
                y,
                (panel.pan_limit + name_width).max(name_width),
                24,
            ),
            &label,
            12,
            m.palette.foreground,
            true,
            false,
        );
        p.unclip();
        p.text(
            Rect::new(r.x + r.w - 112, y, 100, 24),
            if let Some(review) = &panel.review {
                let e = &review.entries()[*i];
                if matches!(e.proposal(), Proposal::Review(_)) {
                    match e.choice() {
                        Choice::Pending => "Pendiente",
                        Choice::Skip => "Omitir",
                        Choice::PreserveAndReplaceFrom(Side::Left)
                        | Choice::PreserveAndSetMetadataFrom(Side::Left) => "A → B",
                        Choice::PreserveAndReplaceFrom(Side::Right)
                        | Choice::PreserveAndSetMetadataFrom(Side::Right) => "B → A",
                        Choice::PreserveAndRemove => "Retirar",
                    }
                } else {
                    proposal_label(e.proposal())
                }
            } else {
                status(row)
            },
            10,
            m.palette.accent,
            true,
            true,
        );
        m.hits.push(Hit {
            rect,
            action: Action::Comparison(panel.stamp, Command::Select(slot)),
            generation: m.generation,
        });
    }
    if (panel.report.is_some() || panel.review.is_some()) && panel.visible.is_empty() {
        p.text(
            Rect::new(r.x + 14, top, r.w - 28, 30),
            "No hay filas en este filtro",
            12,
            m.palette.dark_foreground,
            true,
            false,
        );
    }
    button(
        p,
        m,
        Rect::new(r.x + r.w - 72, r.y + 156, 28, 22),
        "‹",
        panel.stamp,
        Command::Pan(-120),
        panel.pan > 0,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 40, r.y + 156, 28, 22),
        "›",
        panel.stamp,
        Command::Pan(120),
        panel.pan < panel.pan_limit,
    );
    let mut buttons = vec![
        (
            if is_sync { "Preparar" } else { "Comparar" },
            Command::Start,
            !panel.busy() && !panel.closing,
        ),
        ("Cancelar", Command::Cancel, panel.busy()),
        ("Filtro", Command::Filter, !panel.busy()),
        ("Límite", Command::Budget, !panel.busy()),
        (
            "Ubicar A",
            Command::Locate(0),
            !panel.busy() && panel.row().is_some_and(|r| r.left.is_some()),
        ),
        (
            "Ubicar B",
            Command::Locate(1),
            !panel.busy() && panel.row().is_some_and(|r| r.right.is_some()),
        ),
        ("Informe", Command::Copy, true),
        ("Cerrar", Command::Close, true),
    ];
    if is_sync {
        let decision_enabled = !panel.busy() && !panel.closing && !panel.choices().is_empty();
        buttons.push(("Decisión F9", Command::Choice(1), decision_enabled));
        buttons.push(("Pend. F10", Command::ResetChoice, decision_enabled));
    }
    let cols = if compact {
        4
    } else if is_sync {
        5
    } else {
        8
    };
    let bw = (r.w - 24 - (cols - 1) * 4) / cols;
    for (i, (label, cmd, enabled)) in buttons.into_iter().enumerate() {
        button(
            p,
            m,
            Rect::new(
                r.x + 12 + (i as i32 % cols) * (bw + 4),
                r.y + r.h - controls + (i as i32 / cols) * 30,
                bw,
                26,
            ),
            label,
            panel.stamp,
            cmd,
            enabled,
        );
    }
    p.unclip();
    m.comparison = Some(panel);
}
pub fn event(m: &mut Model, event: Event) -> bool {
    let Some(stamp) = m.comparison.as_ref().map(|p| p.stamp) else {
        return false;
    };
    if event == Event::Push && app::event_button() == 1 {
        let (x, y) = (app::event_x(), app::event_y());
        let field = m
            .comparison
            .as_ref()
            .and_then(|p| p.editors.iter().position(|e| e.geometry.contains(x, y)));
        if let Some(side) = field {
            let p = m.comparison.as_mut().unwrap();
            if !p.busy() {
                let byte = p.editors[side].geometry.position(x, p.stamp);
                let same = p.focus == Some(side);
                p.focus = Some(side);
                p.drag = Some(side);
                if let Some(byte) = byte {
                    let e = &mut p.editors[side];
                    if app::event_clicks() {
                        e.cursor.component(&e.text, byte);
                    } else {
                        let anchor = if same && app::event_state().intersects(Shortcut::Shift) {
                            e.cursor.anchor
                        } else {
                            byte
                        };
                        e.cursor.select(&e.text, anchor, byte);
                    }
                }
                p.stamp = epoch();
            }
            return true;
        }
        if let Some(action) = m
            .hits
            .iter()
            .rev()
            .find(|h| h.generation == m.generation && h.rect.contains(x, y))
            .map(|h| h.action.clone())
        {
            m.action(action);
        }
        return true;
    }
    if event == Event::Released {
        if let Some(p) = &mut m.comparison {
            p.drag = None;
        }
        return true;
    }
    if event == Event::Drag {
        if let Some(p) = &mut m.comparison
            && !p.busy()
            && let Some(side) = p.drag
            && let Some(byte) = p.editors[side].geometry.position(app::event_x(), p.stamp)
        {
            let e = &mut p.editors[side];
            e.cursor.select(&e.text, e.cursor.anchor, byte);
            p.stamp = epoch();
        }
        return true;
    }
    if event == Event::MouseWheel {
        m.comparison_command(
            stamp,
            Command::Move(if app::event_dy_value() < 0 { -3 } else { 3 }),
        );
        return true;
    }
    if event != Event::KeyDown && event != Event::Shortcut {
        return true;
    }
    let key = app::event_key();
    let mods = app::event_state();
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = mods.intersects(Shortcut::Shift);
    let command = if key == Key::Escape {
        Some(Command::Close)
    } else if key == Key::F5 {
        Some(Command::Start)
    } else if key == Key::F7 {
        Some(Command::Filter)
    } else if key == Key::F6 {
        Some(Command::Mode(if shift { -1 } else { 1 }))
    } else if key == Key::F9 {
        Some(Command::Choice(if shift { -1 } else { 1 }))
    } else if key == Key::F10 {
        Some(Command::ResetChoice)
    } else if key == Key::F8 {
        Some(Command::Budget)
    } else if ctrl && key == Key::from_char('l') {
        Some(Command::Focus(0))
    } else if key == Key::Tab {
        let p = m.comparison.as_ref().unwrap();
        match (p.focus, shift) {
            (None, false) | (Some(1), true) => Some(Command::Focus(0)),
            (Some(0), false) | (None, true) => Some(Command::Focus(1)),
            _ => Some(Command::Results),
        }
    } else {
        None
    };
    if let Some(c) = command {
        m.comparison_command(stamp, c);
        return true;
    }
    let p = m.comparison.as_mut().unwrap();
    if let Some(side) = p.focus.filter(|_| !p.busy()) {
        let e = &mut p.editors[side];
        let motion = match key {
            Key::Left => Some(if ctrl { Motion::WordLeft } else { Motion::Left }),
            Key::Right => Some(if ctrl {
                Motion::WordRight
            } else {
                Motion::Right
            }),
            Key::Home => Some(Motion::Start),
            Key::End => Some(Motion::End),
            _ => None,
        };
        if let Some(motion) = motion {
            e.cursor.motion(&e.text, motion, shift);
            p.stamp = epoch();
            return true;
        }
        if ctrl && key == Key::from_char('a') {
            e.cursor.all(&e.text);
            p.stamp = epoch();
            return true;
        }
        if ctrl && key == Key::from_char('c') {
            let range = e.cursor.range(&e.text);
            if !range.is_empty() {
                app::copy(&e.text[range]);
            }
            return true;
        }
        if ctrl && key == Key::from_char('x') {
            let range = e.cursor.range(&e.text);
            if !range.is_empty() {
                let copied = e.text[range].to_owned();
                match p.input("", None) {
                    Ok(()) => app::copy(&copied),
                    Err(error) => p.error = error.into(),
                };
            }
            return true;
        }
        if ctrl && key == Key::from_char('z') {
            e.cursor.undo(&mut e.text, shift);
            p.reset();
            return true;
        }
        if key == Key::BackSpace || key == Key::Delete {
            e.cursor
                .delete(&mut e.text, key == Key::BackSpace, ctrl, false);
            p.reset();
            return true;
        }
        if key == Key::Enter {
            m.comparison_command(stamp, Command::Start);
            return true;
        }
        if !ctrl {
            let text = app::event_text();
            if !text.is_empty()
                && text.chars().all(|c| !c.is_control())
                && let Err(error) = p.input(&text, app::compose().map(|n| n.max(0) as usize))
            {
                p.error = error.into();
            }
        }
    } else {
        let cmd = match key {
            Key::Up => Some(Command::Move(-1)),
            Key::Down => Some(Command::Move(1)),
            Key::Home => Some(Command::Move(i32::MIN)),
            Key::End => Some(Command::Move(i32::MAX)),
            Key::PageUp => Some(Command::Move(-(p.rows as i32))),
            Key::PageDown => Some(Command::Move(p.rows as i32)),
            Key::Left => Some(Command::Pan(-80)),
            Key::Right => Some(Command::Pan(80)),
            Key::Enter => Some(Command::Locate(usize::from(shift))),
            _ if ctrl && key == Key::from_char('c') => Some(Command::Copy),
            _ => None,
        };
        if let Some(cmd) = cmd {
            m.comparison_command(stamp, cmd);
        }
    }
    true
}

/// Private scene commands: use actual model commands, never native clipboard/input.
pub fn fixture_command(m: &mut Model, action: &str) -> Result<bool, String> {
    let Some(p) = m.comparison.as_ref() else {
        return Ok(false);
    };
    let stamp = p.stamp;
    if let Some(rest) = action.strip_prefix("fixture-compare-root:") {
        let (side, hex) = rest.split_once(':').ok_or("Raíz fixture inválida")?;
        let side: usize = side.parse().map_err(|_| "Lado fixture inválido")?;
        if side > 1 {
            return Err("Lado inválido".into());
        }
        let bytes = fixture_bytes(hex)?;
        let root = files_core::media_source::raw_path(bytes).map_err(|e| e.to_string())?;
        let text = crate::path_edit::display(&root).map_err(str::to_owned)?;
        m.comparison_command(stamp, Command::Focus(side));
        m.comparison_input(&text);
        return Ok(true);
    }
    if let Some(hex) = action.strip_prefix("fixture-compare-select:") {
        let bytes = fixture_bytes(hex)?;
        let p = m.comparison.as_ref().unwrap();
        let index = p
            .visible
            .iter()
            .position(|&i| p.row_at(i).unwrap().path.as_os_str().as_encoded_bytes() == bytes)
            .ok_or("Fila fixture no visible en filtro")?;
        m.comparison_command(stamp, Command::Select(index));
        return Ok(true);
    }
    if action == "fixture-compare-report" {
        let text = p.report_text().map_err(str::to_owned)?;
        eprintln!("FILES_COMPARE_REPORT {}", serde_json::json!({"text":text}));
        return Ok(true);
    }
    let command = match action {
        "fixture-compare-start" => Command::Start,
        "fixture-compare-cancel" => Command::Cancel,
        "fixture-compare-close" => Command::Close,
        "fixture-compare-filter" => Command::Filter,
        "fixture-compare-budget" => Command::Budget,
        "fixture-compare-mode" => Command::Mode(1),
        "fixture-compare-choice" => Command::Choice(1),
        "fixture-compare-pending" => Command::ResetChoice,
        "fixture-compare-end" => Command::Move(i32::MAX),
        "fixture-compare-home" => Command::Move(i32::MIN),
        "fixture-compare-pan" => Command::Pan(i32::MAX),
        "fixture-compare-a" => Command::Locate(0),
        "fixture-compare-b" => Command::Locate(1),
        _ => return Ok(false),
    };
    if matches!(command, Command::Move(_) | Command::Pan(_)) {
        m.comparison_command(stamp, command);
    } else {
        let hit=m.hits.iter().find(|hit|matches!(hit.action,Action::Comparison(s,c)if s==stamp&&std::mem::discriminant(&c)==std::mem::discriminant(&command)&&match(c,command){(Command::Locate(a),Command::Locate(b))=>a==b,_=>true})).ok_or("Control fixture no disponible")?.action.clone();
        m.action(hit);
    }
    Ok(true)
}
fn fixture_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() > 8192 || !hex.len().is_multiple_of(2) {
        return Err("Hex de comparación fuera del límite".into());
    }
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| "Hex inválido".into())
        })
        .collect()
}
pub fn fixture(m: &Model) -> serde_json::Value {
    m.comparison.as_ref().map_or(serde_json::Value::Null,|p|serde_json::json!({"stamp":p.stamp,"busy":p.busy(),"pending":p.pending,"closing":p.closing,"error":p.error,"progress":p.progress,"focus":p.focus,"filter":p.filter,"budget":BUDGETS[p.budget],"editors":[p.editors[0].text,p.editors[1].text],"roots_raw":p.roots.as_ref().map(|roots|roots.iter().map(|r|r.as_os_str().as_encoded_bytes()).collect::<Vec<_>>()),"rows":p.row_count(),"mode":p.mode,"blocked":p.review.as_ref().map(|r|r.blocked()),"unresolved":p.review.as_ref().map(|r|r.unresolved()),"selected_proposal":p.entry().map(|e|format!("{:?}",e.proposal())),"selected_choice":p.entry().map(|e|format!("{:?}",e.choice())),"visible":p.visible.len(),"selected":p.selected,"selected_raw":p.row().map(|r|r.path.as_os_str().as_encoded_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>()),"selected_kind":p.row().map(|r|format!("{:?}",r.difference)),"top":p.top,"page_rows":p.rows,"pan":p.pan,"pan_limit":p.pan_limit,"buttons":m.hits.iter().filter_map(|h|if let Action::Comparison(s,c)=h.action{Some(serde_json::json!({"stamp":s,"command":format!("{c:?}"),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}else{None}).collect::<Vec<_>>()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::mpsc};
    struct Fixture {
        root: PathBuf,
        m: Model,
    }
    impl Fixture {
        fn new() -> Self {
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!("compare-ui-{}-{}", std::process::id(), epoch()));
            fs::create_dir_all(root.join("other")).unwrap();
            let mut m = Model::new(root.clone());
            let (tx, _) = mpsc::sync_channel(4);
            let (_, rx) = mpsc::channel();
            m.aux = crate::aux_io::AuxIo { tx, rx };
            m.folder_views = Default::default();
            m.folder_views.loaded = true;
            m.folder_restore = None;
            m.loaded = true;
            Self { root, m }
        }
        fn open(&mut self) {
            self.m.open_comparison();
            let p = self.m.comparison.as_mut().unwrap();
            p.input("other", None).unwrap();
        }
        fn cmd(&mut self, c: Command) {
            let stamp = self.m.comparison.as_ref().unwrap().stamp;
            self.m.comparison_command(stamp, c);
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn sync_observed(f: &mut Fixture, mode: Mode, partial: bool) {
        fs::create_dir_all(f.root.join("a")).unwrap();
        fs::create_dir_all(f.root.join("b")).unwrap();
        for name in ["conflict-a", "conflict-b"] {
            fs::write(f.root.join("a").join(name), b"left").unwrap();
            fs::write(f.root.join("b").join(name), b"rite").unwrap();
        }
        fs::write(f.root.join("a/unique"), b"u").unwrap();
        let roots = [f.root.join("a"), f.root.join("b")];
        let review = sync::prepare(
            [&roots[0], &roots[1]],
            None,
            mode,
            compare::Limits {
                read: if partial { 0 } else { 1024 },
                ..compare::Limits::default()
            },
            &std::sync::atomic::AtomicBool::new(false),
            |_| Ok(()),
        )
        .unwrap();
        if f.m.comparison.is_none() {
            f.m.open_sync_review();
        }
        let p = f.m.comparison.as_mut().unwrap();
        p.mode = Some(mode);
        p.reset();
        p.progress = review.progress();
        p.review = Some(review);
        p.editors[0].text = roots[0].to_string_lossy().into();
        p.editors[1].text = roots[1].to_string_lossy().into();
        p.roots = Some(roots);
        p.focus = None;
        p.rebuild();
    }
    #[test]
    fn sync_decisions_are_per_row_stamped_and_never_mutate_files() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Merge, false);
        let old = f.m.comparison.as_ref().unwrap().stamp;
        f.cmd(Command::Choice(1));
        let p = f.m.comparison.as_ref().unwrap();
        assert_eq!(p.entry().unwrap().choice(), Choice::Skip);
        assert_eq!(p.review.as_ref().unwrap().unresolved(), 1);
        f.m.comparison_command(old, Command::Choice(1));
        assert_eq!(
            f.m.comparison.as_ref().unwrap().entry().unwrap().choice(),
            Choice::Skip
        );
        f.cmd(Command::Choice(1));
        assert_eq!(
            f.m.comparison.as_ref().unwrap().entry().unwrap().choice(),
            Choice::PreserveAndReplaceFrom(Side::Left)
        );
        f.cmd(Command::Choice(1));
        assert_eq!(
            f.m.comparison.as_ref().unwrap().entry().unwrap().choice(),
            Choice::PreserveAndReplaceFrom(Side::Right)
        );
        let stale = f.m.comparison.as_ref().unwrap().stamp;
        f.cmd(Command::Move(1));
        f.m.comparison_command(stale, Command::Choice(1));
        assert_eq!(
            f.m.comparison.as_ref().unwrap().entry().unwrap().choice(),
            Choice::Pending
        );
        f.cmd(Command::Move(-1));
        f.cmd(Command::ResetChoice);
        assert_eq!(
            f.m.comparison
                .as_ref()
                .unwrap()
                .review
                .as_ref()
                .unwrap()
                .unresolved(),
            2
        );
        assert_eq!(fs::read(f.root.join("a/conflict-a")).unwrap(), b"left");
        assert_eq!(fs::read(f.root.join("b/conflict-a")).unwrap(), b"rite");
        assert!(!f.m.mutation_applied);
    }
    #[test]
    fn sync_mode_and_budget_and_root_edits_clear_decisions_and_paste_identity() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Update(Side::Left), false);
        f.cmd(Command::Choice(1));
        let stale = f.m.comparison.as_ref().unwrap().stamp;
        f.cmd(Command::Mode(1));
        let p = f.m.comparison.as_ref().unwrap();
        assert_eq!(p.mode, Some(Mode::Update(Side::Right)));
        assert!(p.review.is_none() && p.roots.is_none());
        f.m.comparison_command(stale, Command::Start);
        assert!(!f.m.comparison.as_ref().unwrap().busy());
        sync_observed(&mut f, Mode::Mirror(Side::Left), false);
        f.cmd(Command::Budget);
        assert!(f.m.comparison.as_ref().unwrap().review.is_none());
        sync_observed(&mut f, Mode::Merge, false);
        f.cmd(Command::Focus(1));
        let key = f.m.comparison.as_ref().unwrap().text_key().unwrap();
        f.m.comparison_input("b");
        let p = f.m.comparison.as_ref().unwrap();
        assert!(p.review.is_none() && p.roots.is_none());
        assert_ne!(p.text_key().unwrap(), key);
    }
    #[test]
    fn pending_filter_removes_resolved_row_without_reusing_its_old_action() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Merge, false);
        for _ in 0..3 {
            f.cmd(Command::Filter);
        }
        assert_eq!(f.m.comparison.as_ref().unwrap().visible.len(), 2);
        let old = f.m.comparison.as_ref().unwrap().stamp;
        f.cmd(Command::Choice(1));
        assert_eq!(f.m.comparison.as_ref().unwrap().visible.len(), 1);
        f.m.comparison_command(old, Command::Choice(1));
        assert_eq!(
            f.m.comparison.as_ref().unwrap().entry().unwrap().choice(),
            Choice::Pending
        );
        f.cmd(Command::Choice(1));
        assert!(f.m.comparison.as_ref().unwrap().visible.is_empty());
        assert!(
            f.m.comparison
                .as_ref()
                .unwrap()
                .review
                .as_ref()
                .unwrap()
                .unresolved()
                == 0
        );
        f.cmd(Command::Filter);
        assert_eq!(f.m.comparison.as_ref().unwrap().visible.len(), 3);
    }
    #[test]
    fn blocked_sync_keeps_known_unique_row_blocked_and_has_no_decision_controls() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Mirror(Side::Left), true);
        let mut painter = Recorder::default();
        paint(&mut painter, &mut f.m, 440, 440);
        assert!(!f.m.hits.iter().any(|h| matches!(
            h.action,
            Action::Comparison(_, Command::Choice(_) | Command::ResetChoice)
        )));
        f.cmd(Command::Choice(1));
        let p = f.m.comparison.as_ref().unwrap();
        assert!(p.review.as_ref().unwrap().blocked());
        assert!(
            p.review
                .as_ref()
                .unwrap()
                .entries()
                .iter()
                .all(|e| e.proposal() == Proposal::Blocked)
        );
        let text = p.report_text().unwrap();
        assert!(
            text.contains("BLOQUEADO")
                && text.contains("NO aplica cambios")
                && text.contains("unique")
        );
    }
    #[test]
    fn sync_compact_controls_and_rows_are_disjoint_with_an_explicit_choice() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Update(Side::Left), false);
        let mut painter = Recorder::default();
        paint(&mut painter, &mut f.m, 440, 440);
        let hits = &f.m.hits;
        assert!(
            hits.iter()
                .any(|h| matches!(h.action, Action::Comparison(_, Command::Choice(1))))
        );
        for (i, a) in hits.iter().enumerate() {
            assert!(
                a.rect.w > 0
                    && a.rect.h > 0
                    && a.rect.x + a.rect.w <= 440
                    && a.rect.y + a.rect.h <= 440
            );
            for b in &hits[i + 1..] {
                assert!(
                    a.rect.x + a.rect.w <= b.rect.x
                        || b.rect.x + b.rect.w <= a.rect.x
                        || a.rect.y + a.rect.h <= b.rect.y
                        || b.rect.y + b.rect.h <= a.rect.y
                );
            }
        }
        assert!(f.m.comparison.as_ref().unwrap().rows >= 1);
        assert!(
            painter
                .text
                .iter()
                .any(|t| t.contains("Revisión sin cambios"))
        );
    }
    #[test]
    fn cancelled_sync_discards_successful_queued_result_and_restores_pause_at_ack() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Merge, false);
        let review = f.m.comparison.as_mut().unwrap().review.take().unwrap();
        let (job, tx) = crate::directory_sync_job::Job::pending();
        let p = f.m.comparison.as_mut().unwrap();
        p.job = Some(Reader::Sync(job));
        p.prior_pause = Some(false);
        f.m.sizes
            .enqueue(&f.root, std::iter::once(std::ffi::OsStr::new("a")))
            .unwrap();
        f.m.sizes.pause(true);
        f.cmd(Command::Cancel);
        assert!(f.m.comparison.as_ref().unwrap().busy() && f.m.sizes.paused());
        tx.send(Ok(review)).unwrap();
        assert!(f.m.pump_comparison());
        let p = f.m.comparison.as_ref().unwrap();
        assert!(!p.busy() && p.review.is_none() && p.error.contains("cancelación"));
        assert!(!f.m.sizes.paused());
    }
    #[test]
    fn sync_root_close_and_new_intent_cannot_bypass_retained_reader_ack() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Merge, false);
        let (job, tx) = crate::directory_sync_job::Job::pending();
        let p = f.m.comparison.as_mut().unwrap();
        p.review = None;
        p.job = Some(Reader::Sync(job));
        let old = p.stamp;
        f.cmd(Command::Mode(1));
        f.cmd(Command::Start);
        assert_eq!(f.m.comparison.as_ref().unwrap().stamp, old);
        assert!(!f.m.comparison.as_ref().unwrap().pending);
        f.m.closing = true;
        assert!(!f.m.close_ready());
        f.m.pump_comparison();
        assert!(f.m.comparison.as_ref().unwrap().busy());
        tx.send(Err("ACK".into())).unwrap();
        assert!(f.m.pump_comparison());
        assert!(f.m.comparison.is_none());
        assert!(f.m.close_ready());
    }
    #[test]
    fn sync_diagnostic_contains_all_choices_and_never_claims_apply_authority() {
        let mut f = Fixture::new();
        sync_observed(&mut f, Mode::Mirror(Side::Right), false);
        f.cmd(Command::Move(i32::MAX));
        assert!(matches!(
            f.m.comparison.as_ref().unwrap().entry().unwrap().proposal(),
            Proposal::Review(Conflict::DestinationOnly)
        ));
        f.cmd(Command::Choice(-1));
        let p = f.m.comparison.as_ref().unwrap();
        assert_eq!(p.entry().unwrap().choice(), Choice::PreserveAndRemove);
        let text = p.report_text().unwrap();
        assert!(
            text.contains("Espejo B → A")
                && text.contains("PreserveAndRemove")
                && text.contains("NO aplica cambios")
                && text.contains("Respaldos, ejecución y recuperación aún pendientes")
        );
        assert!(f.root.join("a/unique").exists());
    }
    #[test]
    fn manual_root_capture_is_explicit_and_stale_commands_do_not_start() {
        let mut f = Fixture::new();
        f.m.open_comparison();
        let p = f.m.comparison.as_ref().unwrap();
        assert_eq!(p.focus, Some(1));
        assert!(p.resolve().is_err());
        let stale = p.stamp;
        f.m.comparison_input("other");
        f.m.comparison_command(stale, Command::Start);
        assert!(!f.m.comparison.as_ref().unwrap().busy());
        let expected = [f.root.clone(), f.root.join("other")];
        assert_eq!(
            f.m.comparison.as_ref().unwrap().resolve().unwrap(),
            expected
        );
        f.cmd(Command::Start);
        let p = f.m.comparison.as_ref().unwrap();
        assert!(p.pending && p.job.is_none());
        assert_eq!(p.roots.as_ref().unwrap(), &expected);
        f.m.action(Action::Navigate(f.root.join("blocked")));
        assert_eq!(f.m.path, f.root);
    }
    #[test]
    fn paste_identity_changes_with_root_focus_cursor_and_new_intent() {
        let mut f = Fixture::new();
        f.open();
        let key = f.m.text_context();
        assert!(f.m.text_paste.request(key).is_some());
        f.cmd(Command::Focus(0));
        assert!(!f.m.receive_text_paste("late"));
        f.cmd(Command::Focus(1));
        let key = f.m.text_context();
        assert!(f.m.text_paste.request(key).is_some());
        assert!(f.m.receive_text_paste("other"));
        f.cmd(Command::Start);
        assert!(f.m.text_context().is_none());
        let before = f.m.comparison.as_ref().unwrap().editors[1].text.clone();
        f.m.comparison_input("must-not-change");
        assert_eq!(f.m.comparison.as_ref().unwrap().editors[1].text, before);
    }
    #[test]
    fn cancel_before_dispatch_restores_pause_without_spawning() {
        let mut f = Fixture::new();
        f.open();
        f.m.sizes
            .enqueue(&f.root, std::iter::once(std::ffi::OsStr::new("other")))
            .unwrap();
        assert!(!f.m.sizes.paused());
        f.cmd(Command::Start);
        assert!(f.m.sizes.paused());
        f.cmd(Command::Cancel);
        assert!(!f.m.sizes.paused());
        assert!(!f.m.comparison.as_ref().unwrap().busy());
    }
    #[test]
    fn close_retains_job_and_slot_until_exact_ack() {
        let mut f = Fixture::new();
        f.open();
        f.m.sizes
            .enqueue(&f.root, std::iter::once(std::ffi::OsStr::new("other")))
            .unwrap();
        f.m.sizes.pause(true);
        let (job, tx) = Job::pending();
        let p = f.m.comparison.as_mut().unwrap();
        p.job = Some(Reader::Compare(job));
        p.prior_pause = Some(true);
        f.cmd(Command::Close);
        assert!(f.m.comparison.as_ref().unwrap().busy());
        assert!(!f.m.pump_comparison());
        assert!(f.m.comparison.is_some());
        tx.send(Err("cancelado".into())).unwrap();
        assert!(f.m.pump_comparison());
        assert!(f.m.comparison.is_none());
        assert!(f.m.sizes.paused());
    }
    #[cfg(unix)]
    #[test]
    fn expanded_uri_root_roundtrips_and_oversize_paste_is_transactional() {
        use std::os::unix::ffi::OsStringExt;
        let mut f = Fixture::new();
        f.m.open_comparison();
        let raw = PathBuf::from(std::ffi::OsString::from_vec(
            [b"/".as_slice(), &vec![0xff; 4095]].concat(),
        ));
        let text = crate::path_edit::display(&raw).unwrap();
        assert!(text.len() > 4096);
        f.m.comparison_input(&text);
        let p = f.m.comparison.as_mut().unwrap();
        assert_eq!(p.resolve().unwrap()[1], raw);
        let before = (
            p.editors[1].text.clone(),
            p.editors[1].text.capacity(),
            p.editors[1].cursor.revision,
        );
        assert!(p.input(&"x".repeat(1_000_000), None).is_err());
        assert_eq!(
            (
                &p.editors[1].text,
                p.editors[1].text.capacity(),
                p.editors[1].cursor.revision
            ),
            (&before.0, before.1, before.2)
        );
    }
    fn observed(f: &mut Fixture) {
        use files_core::relative_source::DirectoryIdentity;
        let paths = ["changed", "same", "unknown"];
        let rows = paths
            .into_iter()
            .enumerate()
            .map(|(i, path)| Row {
                path: path.into(),
                left: Some(compare::Kind::File),
                right: Some(compare::Kind::File),
                difference: [Difference::Content, Difference::Equal, Difference::Unknown][i],
                metadata_differs: false,
            })
            .collect();
        let p = f.m.comparison.as_mut().unwrap();
        p.roots = Some([f.root.clone(), f.root.join("other")]);
        p.progress = Progress {
            phase: 3,
            entries: [3, 3],
            compared: 3,
            equal: 1,
            different: 1,
            unknown: 1,
            reasons: compare::READ_LIMIT,
            ..Progress::default()
        };
        p.report = Some(Report {
            roots: [DirectoryIdentity {
                device: 1,
                inode: 2,
            }; 2],
            progress: p.progress,
            rows,
        });
        p.focus = None;
        p.rebuild();
    }
    #[test]
    fn filters_and_movement_use_retained_observations_without_starting_reader() {
        let mut f = Fixture::new();
        f.open();
        observed(&mut f);
        assert_eq!(f.m.comparison.as_ref().unwrap().visible, vec![0, 2]);
        f.cmd(Command::Filter);
        assert_eq!(f.m.comparison.as_ref().unwrap().visible, vec![2]);
        f.cmd(Command::Filter);
        assert_eq!(f.m.comparison.as_ref().unwrap().visible, vec![0, 1, 2]);
        f.cmd(Command::Move(i32::MAX));
        assert_eq!(f.m.comparison.as_ref().unwrap().selected, 2);
        assert!(!f.m.comparison.as_ref().unwrap().busy());
    }
    #[test]
    fn locate_is_explicit_context_bound_and_never_selects_an_unrelated_name() {
        let mut f = Fixture::new();
        f.open();
        observed(&mut f);
        f.cmd(Command::Locate(1));
        assert!(f.m.comparison.is_none());
        assert_eq!(f.m.path, f.root.join("other"));
        assert_eq!(f.m.search_locate.as_ref().unwrap().1, "changed");
        assert!(!f.m.mutation_applied);
        let mut f = Fixture::new();
        f.open();
        observed(&mut f);
        f.m.dual.active_id = f.m.dual.active_id.checked_add(1).unwrap();
        f.cmd(Command::Locate(0));
        assert!(f.m.comparison.as_ref().unwrap().error.contains("contexto"));
        assert_eq!(f.m.path, f.root);
    }
    #[test]
    fn root_close_requests_cancel_and_cannot_skip_comparison_ack() {
        let mut f = Fixture::new();
        f.open();
        let (job, tx) = Job::pending();
        f.m.comparison.as_mut().unwrap().job = Some(Reader::Compare(job));
        f.m.closing = true;
        assert!(!f.m.close_ready());
        assert!(f.m.comparison.as_ref().unwrap().closing);
        assert!(!f.m.pump_comparison());
        tx.send(Err("cancelado".into())).unwrap();
        f.m.pump_comparison();
        assert!(f.m.comparison.is_none());
    }
    #[test]
    fn semantic_budget_changes_clear_old_report_and_revoke_old_hits() {
        let mut f = Fixture::new();
        f.open();
        observed(&mut f);
        let old = f.m.comparison.as_ref().unwrap().stamp;
        f.cmd(Command::Budget);
        let p = f.m.comparison.as_ref().unwrap();
        assert!(p.report.is_none() && p.roots.is_none());
        assert_eq!(p.progress, Progress::default());
        assert_ne!(p.stamp, old);
        f.m.comparison_command(old, Command::Start);
        assert!(!f.m.comparison.as_ref().unwrap().busy());
    }
    #[test]
    fn copied_diagnostic_never_invents_a_report_or_erases_existing_error() {
        let mut f = Fixture::new();
        f.open();
        let p = f.m.comparison.as_mut().unwrap();
        p.error = "Lector cancelado de fixture".into();
        let mut copied = String::new();
        p.copy_with(|s| copied = s.into()).unwrap();
        assert!(
            copied.contains("Sin informe final aceptado")
                && copied.contains("Lector cancelado de fixture")
        );
        assert_eq!(p.error, "Lector cancelado de fixture");
        assert!(p.report.is_none());
    }

    #[derive(Default)]
    struct Recorder {
        text: Vec<String>,
    }
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            s: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
            self.text.push(s.into());
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn compact_results_keep_controls_disjoint_and_long_names_scrollable() {
        let mut f = Fixture::new();
        f.open();
        observed(&mut f);
        let name = format!("changed-{}", "z".repeat(242));
        f.m.comparison
            .as_mut()
            .unwrap()
            .report
            .as_mut()
            .unwrap()
            .rows[0]
            .path = name.clone().into();
        let mut painter = Recorder::default();
        paint(&mut painter, &mut f.m, 440, 440);
        assert!(painter.text.contains(&name));
        let hits =
            f.m.hits
                .iter()
                .filter(|h| matches!(h.action, Action::Comparison(..)))
                .collect::<Vec<_>>();
        for (i, a) in hits.iter().enumerate() {
            assert!(
                a.rect.w > 0
                    && a.rect.h > 0
                    && a.rect.x >= 0
                    && a.rect.y >= 0
                    && a.rect.x + a.rect.w <= 440
                    && a.rect.y + a.rect.h <= 440
            );
            for b in &hits[i + 1..] {
                assert!(
                    a.rect.x + a.rect.w <= b.rect.x
                        || b.rect.x + b.rect.w <= a.rect.x
                        || a.rect.y + a.rect.h <= b.rect.y
                        || b.rect.y + b.rect.h <= a.rect.y
                );
            }
        }
        assert!(f.m.comparison.as_ref().unwrap().pan_limit > 0);
        f.cmd(Command::Pan(i32::MAX));
        assert_eq!(
            f.m.comparison.as_ref().unwrap().pan,
            f.m.comparison.as_ref().unwrap().pan_limit
        );
        f.cmd(Command::Select(1));
        paint(&mut painter, &mut f.m, 440, 440);
        assert_eq!(f.m.comparison.as_ref().unwrap().pan, 0);
        assert_eq!(f.m.comparison.as_ref().unwrap().pan_limit, 0);
    }
    #[test]
    fn inactive_long_roots_reveal_their_tail_without_changing_editor_text() {
        let mut f = Fixture::new();
        f.m.open_comparison();
        f.m.comparison_input(&format!("/{}/tail", "long".repeat(200)));
        let p = f.m.comparison.as_mut().unwrap();
        let expected = p.editors[1].text.clone();
        p.focus = None;
        let mut painter = Recorder::default();
        paint(&mut painter, &mut f.m, 440, 440);
        let p = f.m.comparison.as_ref().unwrap();
        let e = &p.editors[1];
        assert_eq!(e.text, expected);
        assert!(e.geometry.painted_position(e.text.len(), p.stamp).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn report_keeps_raw_bytes_literal_backslashes_and_shadow_names_distinct() {
        use std::os::unix::ffi::OsStringExt;
        let mut f = Fixture::new();
        f.open();
        observed(&mut f);
        let paths = [
            PathBuf::from(std::ffi::OsString::from_vec(b"raw\xff\n".to_vec())),
            PathBuf::from("raw\\xFF"),
            PathBuf::from("raw�\n"),
        ];
        let p = f.m.comparison.as_mut().unwrap();
        for (row, path) in p.report.as_mut().unwrap().rows.iter_mut().zip(&paths) {
            row.path = path.clone();
        }
        let text = p.report_text().unwrap();
        for path in &paths {
            assert!(text.contains(&format!("{path:?}")));
        }
        assert_ne!(format!("{:?}", paths[0]), format!("{:?}", paths[1]));
        assert!(
            text.contains("PARCIAL")
                && text.contains("límite de datos hash")
                && text.contains("no instantánea")
        );
    }
}
