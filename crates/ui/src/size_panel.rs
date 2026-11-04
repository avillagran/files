//! Read-only presentation and explicit controls for the existing directory-size queue.
use crate::{
    model::{Action, Hit, Model, View},
    painter::{Painter, Rect},
    size_queue::Status,
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
fn stamp() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut n = NEXT.load(Ordering::Relaxed);
    loop {
        match NEXT.compare_exchange_weak(
            n,
            n.checked_add(1).expect("size panel epoch exhausted"),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(v) => return v,
            Err(v) => n = v,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DisplayOrder {
    #[default]
    Queue,
    Largest,
    Name,
}
impl DisplayOrder {
    fn next(self) -> Self {
        match self {
            Self::Queue => Self::Largest,
            Self::Largest => Self::Name,
            Self::Name => Self::Queue,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Queue => "Cola",
            Self::Largest => "Mayor",
            Self::Name => "Nombre",
        }
    }
    fn indices(self, rows: &[crate::size_queue::Record]) -> Vec<usize> {
        let mut indices: Vec<_> = (0..rows.len()).collect();
        if self != Self::Queue {
            indices.sort_unstable_by(|&a, &b| {
                let names = || rows[a].name.cmp(&rows[b].name).then_with(|| a.cmp(&b));
                match self {
                    Self::Largest => {
                        // Only completed measurements are comparable. Running progress is not a total.
                        let bytes = |i: usize| match rows[i].status {
                            Status::Measured { bytes, .. } => Some(bytes),
                            _ => None,
                        };
                        bytes(b).cmp(&bytes(a)).then_with(names)
                    }
                    Self::Name => names(),
                    Self::Queue => a.cmp(&b),
                }
            });
        }
        indices
    }
}
#[derive(Clone, Debug)]
pub enum Command {
    Close,
    Recapture,
    Select(u64),
    Move(i32),
    Details,
    Open(u64),
    Pause(bool),
    Cancel(u64),
    Retry(u64),
    Recalculate(u64),
    Stop,
    RetryErrors,
    Listed,
    Marked,
    Blocks(u64),
    MarkedBlocks,
    Order,
    Report,
    RefreshReport,
    ReportScope(bool),
    Forget(u64),
    ForgetMeasured(std::sync::Arc<[u64]>),
    CopyReport,
}
impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Forget(_) => "Olvidar observación · O; no borra archivos ni cancela trabajos",
            Self::ForgetMeasured(_) => {
                "Olvidar sólo mediciones de este informe · Shift+O; no borra archivos"
            }
            Self::Close => "Cerrar gestor, sin cancelar",
            Self::Recapture => "Usar carpeta actual",
            Self::Select(_) => "Seleccionar cálculo",
            Self::Move(_) => "Recorrer cálculos",
            Self::Details => "Mostrar detalle completo, Enter",
            Self::Open(_) => "Abrir carpeta seleccionada; Alt+Enter",
            Self::Pause(true) => "Pausar siguientes cálculos",
            Self::Pause(false) => "Reanudar cálculos",
            Self::Cancel(_) => "Cancelar fila seleccionada, Supr",
            Self::Retry(_) => "Reintentar fila, R",
            Self::Recalculate(_) => "Recalcular sólo esta carpeta medida, F5",
            Self::Stop => "Cancelar pendientes, Ctrl+Supr",
            Self::RetryErrors => "Reintentar sólo errores, Shift+R",
            Self::ReportScope(true) => {
                "Capturar informe sólo carpetas marcadas visibles · Shift+I; no recorre disco"
            }
            Self::ReportScope(false) => "Capturar informe de toda la cola; no recorre disco",
            Self::Report => "Mostrar/volver del informe capturado, I",
            Self::RefreshReport => "Actualizar informe con observaciones actuales, F5",
            Self::CopyReport => "Solicitar copia del informe capturado, Ctrl+C",
            Self::Order => "Cambiar orden visual: cola, mayor tamaño, nombre; S",
            Self::Marked => {
                "Calcular sólo carpetas marcadas visibles, M; no incluye el foco sin marca"
            }
            Self::Listed => "Calcular carpetas del listado visible",
            Self::Blocks(_) => {
                "Medir bloques de archivos regulares de esta fila · B; no es espacio liberable"
            }
            Self::MarkedBlocks => {
                "Medir bloques de carpetas marcadas visibles · Shift+B; no usa foco sin marca"
            }
        }
    }
}
pub struct State {
    pub stamp: u64,
    owner: (u64, u64),
    generation: u64,
    epoch: u64,
    root: PathBuf,
    pub selected: usize,
    order: DisplayOrder,
    top: usize,
    details: bool,
    detail_top: usize,
    report: Option<crate::size_report::Report>,
    report_marked: bool,
    report_layout: crate::size_report::Layout,
    note: String,
    pub error: String,
    report_ids: std::sync::Arc<[u64]>,
}
impl State {
    fn capture_report(
        &self,
        m: &Model,
        marked: bool,
    ) -> Result<crate::size_report::Report, &'static str> {
        let rows = m.sizes.rows(&self.root);
        let now = std::time::SystemTime::now();
        if marked {
            crate::size_report::capture_marked(
                &self.root,
                rows,
                m.order
                    .iter()
                    .filter(|&&i| m.selection.contains(&i))
                    .filter_map(|&i| m.entries.get(i))
                    .filter(|e| e.is_dir())
                    .map(|e| e.name.as_os_str()),
                now,
            )
        } else {
            crate::size_report::capture(&self.root, rows, now)
        }
    }
    fn new(m: &Model) -> Result<Self, &'static str> {
        files_core::navigation_history::validate(&m.path)?;
        Ok(Self {
            stamp: stamp(),
            owner: m.pane_owner(),
            generation: m.generation,
            epoch: m.sizes.epoch(),
            root: m.path.clone(),
            selected: 0,
            order: DisplayOrder::Queue,
            top: 0,
            details: false,
            detail_top: 0,
            report: None,
            report_marked: false,
            report_ids: Default::default(),
            report_layout: Default::default(),
            note: String::new(),
            error: String::new(),
        })
    }
    pub fn current(&self, m: &Model) -> bool {
        self.owner == m.pane_owner()
            && self.generation == m.generation
            && self.root == m.path
            && self.epoch == m.sizes.epoch()
            && (m.sizes.is_empty() || m.sizes.root() == self.root)
    }
}
impl Model {
    pub fn open_size_panel(&mut self) {
        if self.size_panel.is_some() {
            self.size_panel = None;
            return;
        }
        if self.closing
            || self.commands.is_some()
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
        self.end_path_edit();
        self.help = None;
        self.context_menu = None;
        self.actions_menu = false;
        self.settings = false;
        self.thumbnails.suspend();
        match State::new(self) {
            Ok(s) => self.size_panel = Some(s),
            Err(e) => self.error = e.into(),
        }
        self.hits.clear();
    }
    pub fn size_panel_command(&mut self, expected_stamp: u64, command: Command) {
        let Some(mut s) = self.size_panel.take() else {
            return;
        };
        if expected_stamp != s.stamp {
            self.size_panel = Some(s);
            return;
        }
        if matches!(command, Command::Close) {
            self.hits.clear();
            return;
        }
        if matches!(command, Command::Recapture) {
            match State::new(self) {
                Ok(new) => s = new,
                Err(e) => s.error = e.into(),
            }
        } else if !s.current(self) {
            s.error = "El contexto cambió; usa Carpeta actual (Ctrl+R)".into();
        } else if s.report.is_some()
            && !matches!(
                command,
                Command::Report
                    | Command::RefreshReport
                    | Command::ReportScope(_)
                    | Command::CopyReport
                    | Command::Move(_)
                    | Command::ForgetMeasured(_)
            )
        {
            s.error = "Vuelve a la cola para cambiar cálculos".into();
        } else {
            s.error.clear();
            let rows = self.sizes.rows(&s.root);
            s.selected = s.selected.min(rows.len().saturating_sub(1));
            match command {
                Command::Select(id) => {
                    if let Some(i) = rows.iter().position(|r| r.id == id) {
                        s.selected = i;
                        s.detail_top = 0;
                    }
                }
                Command::Move(delta) => {
                    if s.details || s.report.is_some() {
                        s.detail_top =
                            (s.detail_top as i64 + i64::from(delta)).clamp(0, 32 * 1024) as usize;
                    } else {
                        let order = s.order.indices(rows);
                        let position = order.iter().position(|&i| i == s.selected).unwrap_or(0);
                        let next = (position as i64 + i64::from(delta))
                            .clamp(0, rows.len().saturating_sub(1) as i64)
                            as usize;
                        s.selected = order.get(next).copied().unwrap_or(0);
                    }
                }
                Command::Report | Command::RefreshReport | Command::ReportScope(_) => {
                    if matches!(command, Command::Report) && s.report.is_some() {
                        s.report = None;
                        s.report_ids = Default::default();
                        s.report_marked = false;
                    } else {
                        let marked = match command {
                            Command::ReportScope(marked) => marked,
                            Command::RefreshReport => s.report_marked,
                            _ => false,
                        };
                        match s.capture_report(self, marked) {
                            Ok(report) => {
                                s.report_marked = marked;
                                s.report_ids = report.measured_ids.clone();
                                s.report = Some(report);
                            }
                            Err(error) => s.error = error.into(),
                        }
                    }
                    s.report_layout.invalidate();
                    s.details = false;
                    s.detail_top = 0;
                    s.note.clear();
                    self.hits.clear();
                }
                Command::Forget(id) => {
                    let order = s.order.indices(rows);
                    let position = order.iter().position(|&i| i == s.selected).unwrap_or(0);
                    let neighbor = order
                        .iter()
                        .skip(position + 1)
                        .chain(order[..position].iter().rev())
                        .filter_map(|&i| rows.get(i))
                        .map(|r| r.id)
                        .next();
                    if rows.get(s.selected).is_some_and(|r| r.id == id)
                        && self.sizes.forget_id(s.epoch, id)
                    {
                        s.selected = neighbor
                            .and_then(|id| self.sizes.rows(&s.root).iter().position(|r| r.id == id))
                            .unwrap_or(0);
                        s.details = false;
                        s.detail_top = 0;
                        s.stamp = stamp();
                        s.note = "Observación olvidada · archivos intactos".into();
                        self.du_text = None;
                        self.rebuild_size_orders();
                        self.hits.clear();
                    } else {
                        s.error =
                            "Sólo una fila finalizada, sin ACK pendiente; selecciona de nuevo"
                                .into();
                    }
                }
                Command::ForgetMeasured(ids) => {
                    if s.report.is_none() || ids.as_ref() != s.report_ids.as_ref() {
                        s.error = "Abre o actualiza el informe antes de olvidar medidas".into();
                        self.size_panel = Some(s);
                        return;
                    }
                    let selected = rows.get(s.selected).map(|r| r.id);
                    match self.sizes.forget_measured(s.epoch, &ids) {
                        Ok(count) => {
                            s.selected = selected
                                .and_then(|id| {
                                    self.sizes.rows(&s.root).iter().position(|r| r.id == id)
                                })
                                .unwrap_or(0);
                            s.report = None;
                            s.report_ids = Default::default();
                            s.report_layout.invalidate();
                            s.detail_top = 0;
                            match s.capture_report(self, s.report_marked) {
                                Ok(report) => {
                                    s.report_ids = report.measured_ids.clone();
                                    s.report = Some(report);
                                }
                                Err(error) => s.error = error.into(),
                            }
                            s.stamp = stamp();
                            s.note = format!("{count} mediciones olvidadas · archivos intactos");
                            self.du_text = None;
                            self.rebuild_size_orders();
                            self.hits.clear();
                        }
                        Err(e) => s.error = e.into(),
                    }
                }
                Command::CopyReport => {
                    if let Some(report) = &s.report {
                        app::copy(&report.text);
                        s.note = "Solicitud de copia enviada · entrega SO sin verificar".into();
                    }
                }
                Command::Order => {
                    s.order = s.order.next();
                    s.top = 0;
                }
                Command::Open(id) => {
                    if let Some(row) = rows.get(s.selected).filter(|r| r.id == id) {
                        let path = s.root.join(&row.name);
                        if let Err(error) = files_core::navigation_history::validate(&path) {
                            s.error = error.into();
                        } else {
                            let generation = self.generation;
                            self.navigate(path);
                            self.hits.clear();
                            if self.generation == generation {
                                s.error =
                                    "No se pudo cambiar de carpeta; revisa el aviso del navegador"
                                        .into();
                                self.size_panel = Some(s);
                            }
                            return;
                        }
                    } else {
                        s.error = "La fila cambió; vuelve a seleccionarla".into();
                    }
                }
                Command::Details => {
                    if !rows.is_empty() {
                        s.details = !s.details;
                        s.detail_top = 0;
                    }
                }
                Command::Pause(value) => self.sizes.pause(value),
                Command::Cancel(id) => {
                    if self.sizes.cancel_id(s.epoch, id)
                        && let Some(job) = &self.du_job
                    {
                        job.cancel();
                    }
                }
                Command::Recalculate(id) => {
                    if rows.get(s.selected).is_some_and(|r| r.id == id)
                        && self.sizes.recalculate_id(s.epoch, id).is_some()
                    {
                        self.thumbnails.suspend();
                        s.detail_top = 0;
                        self.rebuild_size_orders();
                    } else {
                        s.error = "La medición cambió; selecciona una carpeta ya medida".into();
                    }
                }
                Command::Retry(id) => {
                    if self.sizes.retry_id(s.epoch, id).is_some() {
                        self.thumbnails.suspend();
                    } else {
                        s.error =
                            "Sólo se reintentan errores o cancelaciones; fila antigua ignorada"
                                .into();
                    }
                }
                Command::Stop => {
                    self.sizes.cancel_all();
                    if let Some(job) = &self.du_job {
                        job.cancel();
                    }
                }
                Command::RetryErrors => {
                    if self.sizes.retry_errors() > 0 {
                        self.thumbnails.suspend();
                    }
                }
                Command::Marked => {
                    if self.request_marked_folder_sizes() {
                        s.epoch = self.sizes.epoch();
                        s.selected = 0;
                        s.top = 0;
                        s.details = false;
                        s.note="Carpetas marcadas visibles admitidas; archivos y foco sin marca excluidos".into();
                    } else {
                        s.error = self.error.clone();
                    }
                }
                Command::Blocks(id) => {
                    let name = self
                        .sizes
                        .rows(&s.root)
                        .iter()
                        .find(|r| r.id == id)
                        .map(|r| r.name.clone());
                    if let Some(name) = name {
                        if self.request_allocated_name(&name) {
                            s.epoch = self.sizes.epoch();
                            s.note = "Bloques de archivos regulares solicitados; no es espacio liberable".into();
                        } else {
                            s.error = self.error.clone();
                        }
                    } else {
                        s.error = "La fila cambió; actualiza el gestor".into();
                    }
                }
                Command::MarkedBlocks => {
                    if self.request_allocated_folder_sizes(true) {
                        s.epoch = self.sizes.epoch();
                        s.selected = 0;
                        s.top = 0;
                        s.details = false;
                        s.note = "Bloques de carpetas marcadas visibles solicitados".into();
                    } else {
                        s.error = self.error.clone();
                    }
                }
                Command::Listed => {
                    if !self.loaded || self.selection_refreshing() {
                        s.error = "Espera a que termine la carga".into();
                    } else if self.sizes.has_work() {
                        s.error = "Termina o cancela el lote anterior antes de recalcular".into();
                    } else if self.view == View::SpaceMap || !self.space_map.is_drained() {
                        s.error = "Espacio mide aparte; vuelve a un listado".into();
                    } else {
                        self.request_folder_sizes(true);
                        s.epoch = self.sizes.epoch();
                        s.selected = 0;
                        s.top = 0;
                        s.details = false;
                        s.error = self.error.clone();
                    }
                }
                Command::Close | Command::Recapture => {}
            }
            self.du_requested = self.sizes.pending_path();
        }
        self.size_panel = Some(s);
    }
}
fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::new(x, y, w.max(0), h.max(0))
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
        rect(r.x + 6, r.y, r.w - 12, r.h),
        label,
        12,
        if enabled {
            m.palette.accent
        } else {
            m.palette.dark_foreground
        },
        true,
        false,
    );
    if enabled {
        m.hits.push(Hit {
            rect: r,
            action: Action::SizePanel(stamp, cmd),
            generation: m.generation,
        });
    }
}
fn wrap(text: &str, columns: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut column = 0;
    for c in text.chars().take(32 * 1024) {
        if c == '\n' {
            lines.push(String::new());
            column = 0;
            continue;
        }
        if column == columns {
            lines.push(String::new());
            column = 0;
        }
        lines
            .last_mut()
            .unwrap()
            .push(if c.is_control() { '�' } else { c });
        column += 1;
    }
    lines
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut s) = m.size_panel.take() else {
        return;
    };
    m.hits.clear();
    let r = rect(12, 66, w - 24, h - 106);
    let c = m.palette.clone();
    let valid = s.current(m);
    let stamp = s.stamp;
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.literal_text(
        rect(r.x + 14, r.y + 8, r.w - 306, 28),
        " Tamaños",
        16,
        c.foreground,
    );
    button(
        p,
        m,
        rect(r.x + r.w - 278, r.y + 8, 34, 28),
        "",
        stamp,
        Command::Report,
        valid,
    );
    button(
        p,
        m,
        rect(r.x + r.w - 234, r.y + 8, 90, 28),
        &format!(" {}", s.order.label()),
        stamp,
        Command::Order,
        valid && s.report.is_none(),
    );
    let selected_id = m.sizes.rows(&s.root).get(s.selected).map(|r| r.id);
    button(
        p,
        m,
        rect(r.x + r.w - 136, r.y + 8, 34, 28),
        "",
        stamp,
        Command::Open(selected_id.unwrap_or(0)),
        valid && selected_id.is_some() && s.report.is_none(),
    );
    button(
        p,
        m,
        rect(r.x + r.w - 92, r.y + 8, 34, 28),
        "",
        stamp,
        Command::Details,
        valid && !m.sizes.rows(&s.root).is_empty() && s.report.is_none(),
    );
    button(
        p,
        m,
        rect(r.x + r.w - 48, r.y + 8, 34, 28),
        "",
        stamp,
        Command::Close,
        true,
    );
    p.literal_text(
        rect(r.x + 14, r.y + 40, r.w - 28, 22),
        &format!("Carpeta: {}", s.root.to_string_lossy()),
        11,
        c.dark_foreground,
    );
    let rows = m.sizes.rows(&s.root);
    let queued = rows
        .iter()
        .filter(|r| matches!(r.status, Status::Queued))
        .count();
    let label = if !valid {
        "Contexto cambió · Ctrl+R: carpeta actual".into()
    } else if !s.error.is_empty() {
        s.error.clone()
    } else if s.report.is_some() {
        if s.note.is_empty() {
            if s.report_marked {
                "Informe marcadas · F5 recaptura · Ctrl+C copia"
            } else {
                "Informe cola · F5 actualiza · Ctrl+C copia"
            }
            .into()
        } else {
            s.note.clone()
        }
    } else if m.sizes.has_work()
        && !m.sizes.pending()
        && !rows
            .iter()
            .any(|r| matches!(r.status, Status::Running { .. }))
    {
        "Esperando ACK de cancelación".into()
    } else {
        format!(
            "{} filas · cola: {queued} · errores: {}{}",
            rows.len(),
            m.sizes.errors(),
            if m.sizes.paused() { " · PAUSA" } else { "" }
        )
    };
    p.literal_text(
        rect(r.x + 14, r.y + 68, r.w - 28, 24),
        &label,
        12,
        if valid && s.error.is_empty() {
            c.foreground
        } else {
            c.red
        },
    );
    let summary = s
        .report
        .as_ref()
        .map(|r| r.summary)
        .unwrap_or_else(|| crate::size_report::Summary::from_rows(rows));
    let summary_label = summary.label();
    p.literal_text(
        rect(r.x + 14, r.y + 92, r.w - 28, 18),
        if valid {
            &summary_label
        } else {
            "Subtotal del contexto anterior no aplicable"
        },
        11,
        c.foreground,
    );
    p.literal_text(
        rect(r.x + 14, r.y + 112, r.w - 28, 18),
        if !valid {
            "Ctrl+R: carpeta actual · Esc: cerrar"
        } else if s.report_layout.truncated && s.report.is_some() {
            "Vista recortada; copiar conserva el texto del informe"
        } else if s.report.as_ref().is_some_and(|r| r.truncated) {
            "Informe recortado a64KiB; ver aviso final"
        } else if s.details || s.report.is_some() {
            "↑↓/PgUp/PgDown: desplazar · Esc: volver"
        } else {
            "F5 recalcula · I informe · ⇧I marcadas · S orden"
        },
        11,
        c.dark_foreground,
    );
    let body = rect(
        r.x + 14,
        r.y + 140,
        r.w - 28,
        r.h - if s.report.is_some() { 224 } else { 260 },
    );
    let page = (body.h / 38).max(1) as usize;
    s.selected = s.selected.min(rows.len().saturating_sub(1));
    if let Some(report) = &s.report {
        let font_key = p.text_width("M", 12, true).to_bits();
        s.report_layout
            .ensure(&report.text, body.w, font_key, |text| {
                p.text_width(text, 12, true)
            });
        let n = (body.h / 18).max(1) as usize;
        s.detail_top = s
            .detail_top
            .min(s.report_layout.lines.len().saturating_sub(n));
        p.clip(body);
        for (i, range) in s
            .report_layout
            .lines
            .iter()
            .skip(s.detail_top)
            .take(n)
            .enumerate()
        {
            p.literal_text(
                rect(body.x, body.y + i as i32 * 18, body.w, 18),
                &report.text[range.clone()],
                12,
                c.foreground,
            );
        }
        p.unclip();
    } else if valid && s.details {
        if let Some(row) = rows.get(s.selected) {
            let text = format!(
                "{}\n\nCarpeta: {:?}\nNombre: {:?}\n\nError disponible hasta512B (… indica recorte).\nObservación aparente; no snapshot ni espacio físico.",
                m.sizes.detail(&s.root, &row.name).unwrap_or_default(),
                s.root,
                row.name
            );
            let columns = ((body.w as f32 / p.text_width("M", 12, true).max(1.)) as usize).max(1);
            let lines = wrap(&text, columns);
            let n = (body.h / 18).max(1) as usize;
            s.detail_top = s.detail_top.min(lines.len().saturating_sub(n));
            p.clip(body);
            for (i, line) in lines.iter().skip(s.detail_top).take(n).enumerate() {
                p.literal_text(
                    rect(body.x, body.y + i as i32 * 18, body.w, 18),
                    line,
                    12,
                    c.foreground,
                );
            }
            p.unclip();
        }
    } else if valid && !rows.is_empty() {
        let order = s.order.indices(rows);
        let position = order.iter().position(|&i| i == s.selected).unwrap_or(0);
        if position < s.top {
            s.top = position;
        }
        if position >= s.top + page {
            s.top = position + 1 - page;
        }
        s.top = s.top.min(rows.len().saturating_sub(page));
        p.clip(body);
        for (position, &index) in order.iter().enumerate().skip(s.top).take(page) {
            let row = &rows[index];
            let area = rect(body.x, body.y + (position - s.top) as i32 * 38, body.w, 36);
            if index == s.selected {
                p.rounded_rect(area, 3 * m.radius, c.selection);
            }
            let name = files_core::name_label::label(&row.name);
            p.literal_text(
                rect(area.x + 6, area.y, area.w - 122, 36),
                &name,
                13,
                c.foreground,
            );
            p.literal_text(
                rect(area.x + area.w - 112, area.y, 106, 36),
                &m.sizes.label(&s.root, &row.name).unwrap_or_default(),
                12,
                if matches!(row.status, Status::Failed(_)) {
                    c.red
                } else {
                    c.accent
                },
            );
            m.hits.push(Hit {
                rect: area,
                action: Action::SizePanel(stamp, Command::Select(row.id)),
                generation: m.generation,
            });
        }
        p.unclip();
    } else if body.h >= 40 {
        p.literal_text(body,if valid {"Sin cálculos. Lista visible inicia un lote.\nEsc cierra sin cancelar; P pausa, R reintenta."} else {"Los controles anteriores están invalidados.\nCarpeta actual vuelve a capturar el contexto."},12,c.dark_foreground);
    }
    if s.report.is_some() {
        let width = (r.w - 44) / 3;
        let y = r.y + r.h - 76;
        for (i, (label, command)) in [
            (" Ctrl+C", Command::CopyReport),
            if valid {
                (" F5", Command::RefreshReport)
            } else {
                (" Actual", Command::Recapture)
            },
            (" Volver · I", Command::Report),
        ]
        .into_iter()
        .enumerate()
        {
            button(
                p,
                m,
                rect(r.x + 14 + i as i32 * (width + 8), y, width, 28),
                label,
                stamp,
                command.clone(),
                valid || matches!(command, Command::Recapture),
            );
        }
        button(
            p,
            m,
            rect(r.x + 14, y + 36, (r.w - 36) / 2, 28),
            "󰩺 Olvidar · ⇧O",
            stamp,
            Command::ForgetMeasured(s.report_ids.clone()),
            valid && !s.report_ids.is_empty(),
        );
        button(
            p,
            m,
            rect(r.x + 22 + (r.w - 36) / 2, y + 36, (r.w - 36) / 2, 28),
            if s.report_marked {
                " Toda la cola"
            } else {
                " Sólo marcadas"
            },
            stamp,
            Command::ReportScope(!s.report_marked),
            valid && (s.report_marked || m.marked_size_count() > 0),
        );
        p.unclip();
        m.size_panel = Some(s);
        return;
    }
    let selected = m.sizes.rows(&s.root).get(s.selected);
    let (id, cancel, retry, measured) = selected.map_or((0, false, false, false), |r| {
        (
            r.id,
            matches!(r.status, Status::Queued | Status::Running { .. }),
            matches!(r.status, Status::Failed(_) | Status::Cancelled),
            matches!(r.status, Status::Measured { .. }),
        )
    });
    let width = (r.w - 44) / 3;
    let y = r.y + r.h - 112;
    let items = [
        (
            if m.sizes.paused() {
                " Reanudar"
            } else {
                " Pausar sig."
            },
            Command::Pause(!m.sizes.paused()),
            valid && m.sizes.has_work(),
        ),
        (" Cancelar fila", Command::Cancel(id), valid && cancel),
        if measured {
            (" Recalcular", Command::Recalculate(id), valid)
        } else {
            (" Reintentar", Command::Retry(id), valid && retry)
        },
        (" Parar todo", Command::Stop, valid && m.sizes.has_work()),
        if s.details {
            (
                "󰩺 Olvidar · O",
                Command::Forget(id),
                valid && m.sizes.can_forget(s.epoch, id),
            )
        } else {
            (
                " Sólo errores",
                Command::RetryErrors,
                valid && m.sizes.errors() > 0,
            )
        },
        (
            if valid {
                " Lista visible"
            } else {
                " Usar actual"
            },
            if valid {
                Command::Listed
            } else {
                Command::Recapture
            },
            !valid
                || (!m.sizes.has_work()
                    && m.loaded
                    && !m.selection_refreshing()
                    && m.view != View::SpaceMap
                    && m.space_map.is_drained()),
        ),
        (
            " Marcadas · M",
            Command::Marked,
            valid
                && !m.sizes.has_work()
                && m.loaded
                && !m.selection_refreshing()
                && m.view != View::SpaceMap
                && m.space_map.is_drained()
                && m.marked_size_count() > 0,
        ),
        (
            " Bloques · B",
            Command::Blocks(id),
            valid
                && !m.sizes.has_work()
                && m.loaded
                && !m.selection_refreshing()
                && m.view != View::SpaceMap
                && m.space_map.is_drained()
                && m.sizes
                    .rows(&s.root)
                    .iter()
                    .find(|r| r.id == id)
                    .is_some_and(|r| {
                        m.order.iter().any(|&i| {
                            m.entries
                                .get(i)
                                .is_some_and(|e| e.is_dir() && e.name == r.name)
                        })
                    }),
        ),
        (
            " Marcadas ⇧B",
            Command::MarkedBlocks,
            valid
                && !m.sizes.has_work()
                && m.loaded
                && !m.selection_refreshing()
                && m.view != View::SpaceMap
                && m.space_map.is_drained()
                && m.marked_size_count() > 0,
        ),
    ];
    for (i, (label, cmd, enabled)) in items.into_iter().enumerate() {
        button(
            p,
            m,
            rect(
                r.x + 14 + (i % 3) as i32 * (width + 8),
                y + (i / 3) as i32 * 36,
                width,
                28,
            ),
            label,
            stamp,
            cmd,
            enabled,
        );
    }
    p.unclip();
    m.size_panel = Some(s);
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut) {
    let Some(s) = &m.size_panel else {
        return;
    };
    let stamp = s.stamp;
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = mods.intersects(Shortcut::Shift);
    let alt = mods.intersects(Shortcut::Alt);
    let row = m.sizes.rows(&s.root).get(s.selected).map(|r| r.id);
    if s.report.is_some() {
        let command = match key {
            Key::Escape if !s.current(m) => Command::Close,
            Key::Escape | Key::Enter | Key::KPEnter => Command::Report,
            Key::F5 => Command::RefreshReport,
            Key::Up => Command::Move(-1),
            Key::Down => Command::Move(1),
            Key::PageUp => Command::Move(-5),
            Key::PageDown => Command::Move(5),
            Key::Home => Command::Move(i32::MIN),
            Key::End => Command::Move(i32::MAX),
            k if shift && !ctrl && !alt && k == Key::from_char('o') => {
                Command::ForgetMeasured(s.report_ids.clone())
            }
            k if ctrl && k == Key::from_char('c') => Command::CopyReport,
            k if ctrl && k == Key::from_char('r') => Command::Recapture,
            k if shift && !ctrl && !alt && k == Key::from_char('i') => Command::ReportScope(true),
            k if !ctrl && k == Key::from_char('i') => Command::Report,
            _ => return,
        };
        m.action(Action::SizePanel(stamp, command));
        return;
    }

    let command = match key {
        Key::Escape if s.details && s.current(m) => Command::Details,
        Key::Escape => Command::Close,
        k if ctrl && mods.intersects(Shortcut::Alt) && k == Key::from_char(' ') => Command::Close,
        Key::Enter | Key::KPEnter if alt && !ctrl && !shift => {
            let Some(id) = row else {
                return;
            };
            Command::Open(id)
        }
        Key::Enter | Key::KPEnter => Command::Details,
        Key::Up => Command::Move(-1),
        Key::Down => Command::Move(1),
        Key::PageUp => Command::Move(-5),
        Key::PageDown => Command::Move(5),
        Key::Home => Command::Move(i32::MIN),
        Key::End => Command::Move(i32::MAX),
        Key::F5 if !ctrl && !alt && !shift => {
            let Some(id) = row else {
                return;
            };
            Command::Recalculate(id)
        }
        k if !ctrl && !alt && k == Key::from_char('o') => {
            if shift {
                Command::Report
            } else {
                let Some(id) = row else {
                    return;
                };
                Command::Forget(id)
            }
        }
        Key::Delete if ctrl => Command::Stop,
        Key::Delete => {
            let Some(id) = row else {
                return;
            };
            Command::Cancel(id)
        }
        k if ctrl && !alt && k == Key::from_char('r') => Command::Recapture,
        k if !ctrl && !alt && k == Key::from_char('p') => Command::Pause(!m.sizes.paused()),
        k if !ctrl && !alt && shift && k == Key::from_char('r') => Command::RetryErrors,
        k if !ctrl && !alt && k == Key::from_char('r') => {
            let Some(id) = row else {
                return;
            };
            Command::Retry(id)
        }
        k if shift && !ctrl && !alt && k == Key::from_char('i') => Command::ReportScope(true),
        k if !ctrl && !alt && k == Key::from_char('i') => Command::Report,
        k if !ctrl && !alt && k == Key::from_char('s') => Command::Order,
        k if !ctrl && !alt && k == Key::from_char('l') => Command::Listed,
        k if !ctrl && !alt && k == Key::from_char('m') => Command::Marked,
        k if !ctrl && !alt && shift && k == Key::from_char('b') => Command::MarkedBlocks,
        k if !ctrl && !alt && k == Key::from_char('b') => {
            let Some(id) = row else {
                return;
            };
            Command::Blocks(id)
        }
        _ => return,
    };
    m.action(Action::SizePanel(stamp, command));
}
pub fn event(m: &mut Model, e: Event) -> bool {
    match e {
        Event::Push if app::event_button() == 1 => {
            let action = m
                .hits
                .iter()
                .rev()
                .find(|h| {
                    h.generation == m.generation && h.rect.contains(app::event_x(), app::event_y())
                })
                .map(|h| h.action.clone());
            if let Some(a) = action {
                let row = matches!(a, Action::SizePanel(_, Command::Select(_)));
                m.action(a);
                if row
                    && app::event_clicks()
                    && let Some(s) = &m.size_panel
                {
                    m.action(Action::SizePanel(s.stamp, Command::Details));
                }
            }
        }
        Event::MouseWheel => {
            if let Some(s) = &m.size_panel {
                m.action(Action::SizePanel(
                    s.stamp,
                    Command::Move(app::event_dy_value() * 3),
                ));
            }
        }
        Event::KeyDown | Event::Shortcut => key(m, app::event_key(), app::event_state()),
        _ => {}
    }
    true
}

pub fn fixture_state(m: &Model) -> Option<serde_json::Value> {
    let s = m.size_panel.as_ref()?;
    let rows = m.sizes.rows(&s.root);
    let summary = s
        .report
        .as_ref()
        .map(|r| r.summary)
        .unwrap_or_else(|| crate::size_report::Summary::from_rows(rows));
    let summary = serde_json::json!({"measured":summary.measured,"queued":summary.queued,"running":summary.running,"failed":summary.failed,"cancelled":summary.cancelled,"unobserved":summary.unobserved,"bytes":summary.bytes,"overflow":summary.overflow});
    Some(
        serde_json::json!({"blocks_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Blocks(_)))),"marked_blocks_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::MarkedBlocks))),"marked_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Marked))),"report_marked":s.report_marked,"summary":summary,"report":s.report.as_ref().map(|r|serde_json::json!({"text":r.text,"truncated":r.truncated})),"report_lines":s.report_layout.lines.len(),"note":s.note,"copy_report_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::CopyReport))),"forget_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Forget(_)))),"forget_measured_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::ForgetMeasured(_)))),"report_ids":s.report_ids.as_ref(),"stamp":s.stamp,"current":s.current(m),"selected":s.selected,"display_order":s.order.label(),"display_ids":s.order.indices(m.sizes.rows(&s.root)).iter().map(|&i|m.sizes.rows(&s.root)[i].id).collect::<Vec<_>>(),"details":s.details,"detail_top":s.detail_top,"error":s.error,"rows":m.sizes.rows(&s.root).iter().map(|r|serde_json::json!({"id":r.id,"mode":match r.mode {files_core::folder_usage::Mode::Apparent=>"apparent",files_core::folder_usage::Mode::Allocated=>"allocated"},"allocation":r.allocation,"name":r.name.as_encoded_bytes(),"label":m.sizes.label(&s.root,&r.name),"detail":m.sizes.detail(&s.root,&r.name)})).collect::<Vec<_>>(),"row_hits":m.hits.iter().filter(|h|matches!(h.action,Action::SizePanel(_,Command::Select(_)))).count(),"open_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Open(_)))),"recalculate_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Recalculate(_)))),"recalculate_id":m.hits.iter().find_map(|h|if let Action::SizePanel(_,Command::Recalculate(id))=h.action{Some(id)}else{None}),"retry_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Retry(_)))),"cancel_hit":m.hits.iter().any(|h|matches!(h.action,Action::SizePanel(_,Command::Cancel(_)))),"control_rects":m.hits.iter().filter(|h|matches!(h.action,Action::SizePanel(_,_))).map(|h|[h.rect.x,h.rect.y,h.rect.w,h.rect.h]).collect::<Vec<_>>()}),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn forgetting_marked_report_measurements_keeps_unselected_and_recaptures_missing() {
        use super::*;
        use files_core::listing::{Entry, Kind};
        let mut model = Model::new("/marked-report-fixture".into());
        model.loaded = true;
        model.entries = ["A", "B", "C", "missing"]
            .into_iter()
            .map(|n| Entry {
                name: n.into(),
                kind: Kind::Directory,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        model.order = vec![0, 1, 2, 3];
        model.selection = std::collections::HashSet::from([0, 2, 3]);
        model.selected = Some(1);
        model
            .sizes
            .enqueue(
                &model.path,
                [
                    std::ffi::OsStr::new("A"),
                    std::ffi::OsStr::new("B"),
                    std::ffi::OsStr::new("C"),
                ]
                .into_iter(),
            )
            .unwrap();
        for bytes in [8, 100, 9] {
            model.sizes.dispatch().unwrap();
            model.sizes.finish(Ok((bytes, 1)));
        }
        model.open_size_panel();
        let stamp = model.size_panel.as_ref().unwrap().stamp;
        model.size_panel_command(stamp, Command::Report);
        let all_ids = model.size_panel.as_ref().unwrap().report_ids.clone();
        model.size_panel_command(stamp, Command::ReportScope(true));
        let panel = model.size_panel.as_ref().unwrap();
        assert!(panel.report_marked);
        assert_eq!(panel.report.as_ref().unwrap().summary.bytes, Some(17));
        assert_eq!(panel.report.as_ref().unwrap().summary.unobserved, 1);
        let selected = panel.report_ids.clone();
        assert_eq!(selected.len(), 2);
        model.size_panel_command(stamp, Command::ForgetMeasured(all_ids));
        assert_eq!(model.sizes.rows(&model.path).len(), 3);
        model.size_panel_command(stamp, Command::ForgetMeasured(selected));
        assert_eq!(model.sizes.rows(&model.path).len(), 1);
        assert_eq!(
            model.sizes.measured(&model.path, std::ffi::OsStr::new("B")),
            Some(100)
        );
        let panel = model.size_panel.as_ref().unwrap();
        assert!(panel.report_marked && panel.report_ids.is_empty());
        let summary = panel.report.as_ref().unwrap().summary;
        assert_eq!(summary.unobserved, 3);
        assert_eq!(summary.bytes, None);
        assert!(model.du_job.is_none() && model.du_requested.is_none());
        assert_eq!(model.selection, std::collections::HashSet::from([0, 2, 3]));
        assert_eq!(model.selected, Some(1));
    }
    #[test]
    fn display_order_does_not_mutate_queue_or_rank_partial_progress_as_total() {
        use super::DisplayOrder;
        use crate::size_queue::{Record, Status};
        let rows = vec![
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 10,
                name: "z".into(),
                status: Status::Running {
                    bytes: u64::MAX,
                    entries: 1,
                },
            },
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 20,
                name: "b".into(),
                status: Status::Measured {
                    bytes: 9,
                    entries: 1,
                    at: std::time::SystemTime::UNIX_EPOCH,
                },
            },
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 30,
                name: "a".into(),
                status: Status::Measured {
                    bytes: 9,
                    entries: 1,
                    at: std::time::SystemTime::UNIX_EPOCH,
                },
            },
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 40,
                name: "c".into(),
                status: Status::Measured {
                    bytes: 0,
                    entries: 0,
                    at: std::time::SystemTime::UNIX_EPOCH,
                },
            },
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 50,
                name: "d".into(),
                status: Status::Failed("not a total".into()),
            },
        ];
        assert_eq!(DisplayOrder::Queue.indices(&rows), [0, 1, 2, 3, 4]);
        assert_eq!(DisplayOrder::Largest.indices(&rows), [2, 1, 3, 4, 0]);
        assert_eq!(DisplayOrder::Name.indices(&rows), [2, 1, 3, 4, 0]);
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            [10, 20, 30, 40, 50]
        );
        assert_eq!(DisplayOrder::Name.next(), DisplayOrder::Queue);
    }
    #[test]
    fn diagnostic_wrap_preserves_unicode_and_never_interprets_controls_as_markup() {
        let lines = super::wrap("ñ@abc\nerror\tend", 3);
        assert_eq!(lines, vec!["ñ@a", "bc", "err", "or�", "end"]);
    }
}
