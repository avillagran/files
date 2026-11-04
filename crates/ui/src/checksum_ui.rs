//! Herramienta SHA-256 de selección explícita, lectura verificada y comparación sin mutaciones.
use crate::{
    checksum_job::{Job, Progress},
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::checksum::{self, Name, Observation};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
fn epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("checksum IDs exhausted")
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Start,
    Cancel,
    Recapture,
    Copy,
    Close,
    Select(usize),
    Move(i32),
    Page(i32),
    Group(i32),
    Locate,
}
impl Command {
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "Iniciar cálculo o búsqueda de duplicados, F5",
            Self::Cancel => "Cancelar y esperar ACK del lector",
            Self::Recapture => "Recapturar carpeta o selección actual, F6",
            Self::Copy => "Solicitar copia del informe o página visible, Ctrl+C",
            Self::Close => "Cancelar y cerrar tras ACK, Esc",
            Self::Locate => "Ubicar archivo observado en su carpeta, Enter",
            Self::Group(_) => "Grupo anterior/siguiente, Ctrl+Arriba/Abajo",
            Self::Page(_) => "Página de duplicados anterior/siguiente, Izquierda/Derecha",
            Self::Select(_) | Self::Move(_) => "Recorrer archivos del cálculo SHA-256",
        }
    }
}
struct Group {
    first: usize,
    count: usize,
}
pub struct Panel {
    pub scan_mode: bool,
    pub scan_progress: files_core::duplicates::Progress,
    pub scan_report: Option<files_core::duplicates::Report>,
    scan_job: Option<crate::duplicate_job::Job>,
    scan_page: usize,
    scan_pages: Vec<usize>,
    scan_groups: Vec<Group>,
    scan_membership: Vec<usize>,
    pub stamp: u64,
    pub root: PathBuf,
    pub names: Vec<Name>,
    pub rows: Vec<Result<Observation, String>>,
    pub error: String,
    pub note: String,
    pub pending: bool,
    pub closing: bool,
    pub selected: usize,
    top: usize,
    job: Option<Job>,
    progress: Progress,
    prior_pause: Option<bool>,
    owner: (u64, u64),
    generation: u64,
}
fn reason_text(p: files_core::duplicates::Progress) -> String {
    use files_core::duplicates::*;
    let mut labels = Vec::new();
    for (bit, label) in [
        (LIMIT_ENTRIES, "entradas"),
        (LIMIT_FILES, "archivos"),
        (LIMIT_PATHS, "rutas"),
        (LIMIT_DEPTH, "profundidad"),
        (LIMIT_DIRS, "carpetas"),
        (LIMIT_RESULTS, "máximo 128 archivos/8KiB de rutas visibles"),
        (CHANGED, "carpeta cambiada"),
        (BYTE_OVERFLOW, "bytes"),
    ] {
        if p.reasons & bit != 0 {
            labels.push(label);
        }
    }
    if p.errors > 0 {
        labels.push("errores de lectura");
    }
    if labels.is_empty() {
        "ninguno".into()
    } else {
        labels.join(", ")
    }
}
impl Panel {
    fn capture(m: &Model) -> Result<Self, String> {
        if !m.loaded || m.selection_refreshing() {
            return Err("Espera a que termine el listado".into());
        }
        let count = if m.selection.is_empty() {
            usize::from(m.selected.is_some())
        } else {
            m.selection.len()
        };
        if count == 0 || count > checksum::MAX_FILES {
            return Err(
                "Marca de1 a128 archivos regulares o enfoca uno para calcular SHA-256".into(),
            );
        }
        let mut names = Vec::with_capacity(count);
        let focused = m.selected.filter(|_| m.selection.is_empty());
        for index in m.selection.iter().copied().chain(focused) {
            let e = m
                .entries
                .get(index)
                .ok_or("La selección cambió; vuelve a marcar los archivos")?;
            if e.kind != files_core::listing::Kind::File {
                return Err(
                    "SHA-256: selecciona sólo archivos regulares, sin carpetas ni enlaces".into(),
                );
            }
            names.push(Name {
                path: e.name.clone().into(),
            });
        }
        names.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        checksum::validate(&m.path, &names).map_err(|e| e.to_string())?;
        Ok(Self::new(m, names, false))
    }
    fn directory(m: &Model) -> Result<Self, String> {
        if !m.loaded || m.selection_refreshing() {
            return Err("Espera a que termine el listado".into());
        }
        files_core::duplicates::validate_root(&m.path).map_err(|e| e.to_string())?;
        Ok(Self::new(m, Vec::new(), true))
    }
    fn new(m: &Model, names: Vec<Name>, scan_mode: bool) -> Self {
        Self {
            scan_mode,
            scan_progress: Default::default(),
            scan_report: None,
            scan_job: None,
            scan_page: 0,
            scan_pages: Vec::new(),
            scan_groups: Vec::new(),
            scan_membership: Vec::new(),
            stamp: epoch(),
            root: m.path.clone(),
            names,
            rows: Vec::new(),
            error: String::new(),
            note: if scan_mode {
                "F5 buscar · F6 carpeta · Ctrl+C informe · Esc cerrar"
            } else {
                "F5 calcular · F6 captura · Ctrl+C informe · Esc cerrar"
            }
            .into(),
            pending: false,
            closing: false,
            selected: 0,
            top: 0,
            job: None,
            progress: Progress::default(),
            prior_pause: None,
            owner: m.pane_owner(),
            generation: m.generation,
        }
    }
    fn show_scan_page(&mut self, page: usize) {
        let Some(report) = &self.scan_report else {
            return;
        };
        self.scan_page = page.min(self.scan_pages.len().saturating_sub(1));
        let start = self.scan_pages.get(self.scan_page).copied().unwrap_or(0);
        let end = files_core::duplicates::page_end(&report.rows, start);
        self.names = report.rows[start..end]
            .iter()
            .map(|r| Name {
                path: r.path.clone(),
            })
            .collect();
        self.rows = report.rows[start..end]
            .iter()
            .map(|r| Ok(r.observation))
            .collect();
        self.selected = 0;
        self.top = 0;
    }
    fn selected_group(&self) -> Option<(usize, &Group)> {
        let global = self
            .scan_pages
            .get(self.scan_page)?
            .checked_add(self.selected)?;
        let index = *self.scan_membership.get(global)?;
        Some((index, self.scan_groups.get(index)?))
    }
    fn show_group(&mut self, delta: i32) {
        let Some((index, _)) = self.selected_group() else {
            return;
        };
        let index = (index as i64 + i64::from(delta))
            .clamp(0, self.scan_groups.len().saturating_sub(1) as i64) as usize;
        let global = self.scan_groups[index].first;
        let page = self
            .scan_pages
            .partition_point(|start| *start <= global)
            .saturating_sub(1);
        self.show_scan_page(page);
        self.selected = global - self.scan_pages[page];
    }
    fn accept_scan(&mut self, report: files_core::duplicates::Report) {
        let mut groups = std::collections::HashMap::new();
        self.scan_groups.clear();
        self.scan_membership.clear();
        for (position, row) in report.rows.iter().enumerate() {
            let key = (row.observation.sha256, row.observation.proof.file.bytes);
            let index = *groups.entry(key).or_insert_with(|| {
                self.scan_groups.push(Group {
                    first: position,
                    count: 0,
                });
                self.scan_groups.len() - 1
            });
            self.scan_groups[index].count += 1;
            self.scan_membership.push(index);
        }
        self.scan_progress = report.progress;
        self.scan_pages.clear();
        let mut start = 0;
        while start < report.rows.len() {
            self.scan_pages.push(start);
            start = files_core::duplicates::page_end(&report.rows, start);
        }
        self.scan_report = Some(report);
        self.show_scan_page(0);
    }
    pub fn busy(&self) -> bool {
        self.pending || self.job.is_some() || self.scan_job.is_some()
    }
    pub fn cancel(&mut self, close: bool) {
        self.pending = false;
        self.closing |= close;
        if let Some(j) = &self.job {
            j.cancel();
        }
        if let Some(j) = &self.scan_job {
            j.cancel();
        }
        self.note = "Cancelando · esperando ACK del lector".into();
    }
    fn report(&self) -> Result<String, &'static str> {
        let mut text = format!(
            "{} · observaciones individuales, no instantánea del lote\nCarpeta exacta: {:?}\n",
            if self.scan_mode {
                "Duplicados por carpeta"
            } else {
                "SHA-256"
            },
            self.root.as_os_str()
        );
        if let Some(report) = &self.scan_report {
            let p = report.progress;
            text.push_str(&format!("Estado: {} · entradas {} · archivos {} · candidatos {} · hashes {} · alias {} · errores {} · grupos {} · coincidencias {} · retenidas {}\nLímites/cambios: {}\n",if p.complete(){"recorrido completo"}else{"PARCIAL"},p.entries,p.files,p.candidates,p.hashed,p.aliases,p.errors,p.groups,p.matches,report.rows.len(),reason_text(p)));
        }
        if self.scan_report.is_some() {
            let start = self.scan_pages.get(self.scan_page).copied().unwrap_or(0);
            text.push_str(&format!("Contenido: página {}/{} · filas {}–{} · sólo esta página, el grupo puede continuar en otras páginas\n", self.scan_page + 1, self.scan_pages.len().max(1), if self.names.is_empty() { 0 } else { start + 1 }, start + self.names.len()));
        }
        for (i, name) in self.names.iter().enumerate() {
            let mut line = match self.rows.get(i) {
                Some(Ok(o)) => format!(
                    "{}  {} B  {:?}\n",
                    checksum::hex(&o.sha256),
                    o.proof.file.bytes,
                    name.path.as_os_str()
                ),
                Some(Err(e)) => format!("ERROR {e}  {:?}\n", name.path.as_os_str()),
                None => format!("SIN CALCULAR  {:?}\n", name.path.as_os_str()),
            };
            if let Some(group) = self
                .scan_pages
                .get(self.scan_page)
                .and_then(|start| self.scan_membership.get(start + i))
            {
                line = format!(
                    "G{} ({} archivos): {}",
                    group + 1,
                    self.scan_groups[*group].count,
                    line
                );
            }
            if text.len() + line.len() > 128 * 1024 {
                return Err("Informe SHA-256 superior a128KiB");
            }
            text.push_str(&line);
        }
        if !self.error.is_empty() {
            let line = format!("Estado: {}\n", self.error);
            if text.len() + line.len() > 128 * 1024 {
                return Err("Informe SHA-256 superior a128KiB");
            }
            text.push_str(&line);
        }
        Ok(text)
    }
    fn comparison(&self) -> String {
        if self.scan_mode {
            if self.busy() && self.scan_progress.phase == 1 {
                let p = self.scan_progress;
                return format!(
                    "{}/{} SHA · {} / {}",
                    p.hashed,
                    p.candidates,
                    files_core::format_size(p.bytes_read),
                    files_core::format_size(p.bytes_total)
                );
            }
            return match &self.scan_report {
                Some(r) if r.progress.groups > 0 => format!(
                    "{} grupos · {} coincidencias · {} retenidas",
                    r.progress.groups,
                    r.progress.matches,
                    r.rows.len()
                ),
                Some(r) if r.progress.complete() => {
                    "Sin coincidencias observadas en este recorrido".into()
                }
                Some(_) => "Informe parcial; no concluye ausencia de duplicados".into(),
                None if !self.error.is_empty() => "Sin informe final; puedes reintentar".into(),
                None => "Incluye subcarpetas y ocultos; omite enlaces".into(),
            };
        }
        let mut groups = 0;
        let mut matches = 0;
        for (i, row) in self.rows.iter().enumerate() {
            let Ok(row) = row else { continue };
            let same = |o: &Observation| {
                o.sha256 == row.sha256 && o.proof.file.bytes == row.proof.file.bytes
            };
            if self.rows[..i]
                .iter()
                .filter_map(|r| r.as_ref().ok())
                .any(same)
            {
                matches += 1;
            } else if self.rows[i + 1..]
                .iter()
                .filter_map(|r| r.as_ref().ok())
                .any(same)
            {
                groups += 1;
            }
        }
        if groups > 0 {
            format!(
                "{groups} {} · {} archivos",
                if groups == 1 {
                    "grupo coincidente"
                } else {
                    "grupos coincidentes"
                },
                groups + matches
            )
        } else if self.rows.len() > 1 && self.rows.iter().all(Result::is_ok) {
            "Los SHA-256 calculados son diferentes".into()
        } else {
            "Compara observaciones calculadas; no borra archivos".into()
        }
    }
}
impl Model {
    pub fn open_checksums(&mut self) {
        self.open_checksum_panel(false);
    }
    pub fn open_duplicates(&mut self) {
        self.open_checksum_panel(true);
    }
    fn open_checksum_panel(&mut self, directory: bool) {
        if self.comparison.is_some()
            || self.closing
            || self.editing
            || self.filter_edit.active
            || self.search.visible
            || self.commands.is_some()
            || self.size_panel.is_some()
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
        if !directory {
            self.sanitize_selection();
        }
        match if directory {
            Panel::directory(self)
        } else {
            Panel::capture(self)
        } {
            Ok(panel) => {
                self.error.clear();
                self.folder_restore = None;
                self.settings = false;
                self.actions_menu = false;
                self.context_menu = None;
                self.thumbnails.suspend();
                self.text_paste.invalidate();
                self.checksums = Some(panel);
                self.hits.clear();
            }
            Err(e) => self.error = e,
        }
    }
    pub fn checksum_command(&mut self, stamp: u64, command: Command) {
        let Some(mut panel) = self.checksums.take() else {
            return;
        };
        if stamp != panel.stamp {
            self.checksums = Some(panel);
            return;
        }
        match command {
            Command::Start if !panel.busy() && !panel.closing => {
                panel.rows.clear();
                if panel.scan_mode {
                    panel.names.clear();
                    panel.selected = 0;
                    panel.top = 0;
                    panel.scan_progress = Default::default();
                    panel.scan_report = None;
                    panel.scan_pages.clear();
                    panel.scan_groups.clear();
                    panel.scan_membership.clear();
                    panel.scan_page = 0;
                }
                panel.error.clear();
                panel.note = "Esperando lectores anteriores · tamaños pendientes pausados".into();
                panel.pending = true;
                panel.progress = Progress::default();
                panel.stamp = epoch();
                panel.prior_pause = Some(self.sizes.paused());
                self.sizes.pause(true);
            }
            Command::Cancel if panel.busy() => panel.cancel(false),
            Command::Close => panel.cancel(true),
            Command::Recapture if !panel.busy() => match if panel.scan_mode {
                Panel::directory(self)
            } else {
                Panel::capture(self)
            } {
                Ok(new) => panel = new,
                Err(e) => panel.error = e,
            },
            Command::Copy if !panel.rows.is_empty() || panel.scan_report.is_some() => {
                match panel.report() {
                    Ok(text) => {
                        app::copy(&text);
                        panel.note = "Copia del informe solicitada".into();
                    }
                    Err(e) => panel.error = e.into(),
                }
            }
            Command::Page(delta)
                if !panel.busy() && !panel.closing && panel.scan_report.is_some() =>
            {
                let page = (panel.scan_page as i64 + i64::from(delta))
                    .clamp(0, panel.scan_pages.len().saturating_sub(1) as i64)
                    as usize;
                panel.show_scan_page(page);
                panel.note = "Página observada · ←/→ cambiar · Ctrl+C copia esta página".into();
            }
            Command::Group(delta) if !panel.busy() && !panel.closing => panel.show_group(delta),
            Command::Locate
                if panel.scan_mode
                    && !self.closing
                    && !panel.busy()
                    && !panel.closing
                    && panel.scan_report.is_some() =>
            {
                if let Some(name) = panel.names.get(panel.selected) {
                    let path = panel.root.join(&name.path);
                    if let Some((parent, name)) = path.parent().zip(path.file_name()) {
                        let parent = parent.to_path_buf();
                        let name = name.to_os_string();
                        self.navigate(parent.clone());
                        if self.path == parent {
                            self.query.clear();
                            self.rebuild();
                            self.search_locate = Some((self.generation, name));
                            panel.closing = true;
                        } else {
                            panel.error = self.error.clone();
                        }
                    }
                }
            }
            Command::Select(i) if i < panel.names.len() => panel.selected = i,
            Command::Move(delta) => {
                panel.selected = (panel.selected as i64 + i64::from(delta))
                    .clamp(0, panel.names.len().saturating_sub(1) as i64)
                    as usize
            }
            _ => {}
        }
        if !panel.busy()
            && let Some(paused) = panel.prior_pause.take()
            && !self.closing
        {
            self.sizes.pause(paused);
        }
        if !(panel.closing && !panel.busy()) {
            self.checksums = Some(panel);
        }
        self.hits.clear();
    }
    pub fn pump_checksums(&mut self) -> bool {
        let Some(mut panel) = self.checksums.take() else {
            return false;
        };
        let mut dirty = false;
        if self.closing {
            panel.cancel(true);
        }
        if let Some(job) = &panel.job {
            let progress = job.progress();
            if panel.progress != progress {
                panel.progress = progress;
                dirty = true;
            }
            if let Some(outcome) = job.poll() {
                panel.job = None;
                panel.rows = outcome.rows;
                panel.error = outcome.error.unwrap_or_default();
                panel.note = "Terminado · lecturas verificadas; Ctrl+C informe".into();
                dirty = true;
            }
        }
        if let Some(job) = &panel.scan_job {
            let stats = job.progress();
            if stats != panel.scan_progress {
                panel.scan_progress = stats;
                dirty = true;
            }
            if let Some(outcome) = job.poll() {
                panel.scan_job = None;
                panel.error = outcome.error.unwrap_or_default();
                if let Some(report) = outcome.report {
                    if !report.progress.complete() {
                        panel.error = format!(
                            "Informe parcial · {} errores · Ctrl+C detalle",
                            report.progress.errors
                        );
                    }
                    panel.note = "Enter ubicar · Ctrl+↑/↓ grupos · Ctrl+C página".into();
                    panel.accept_scan(report);
                }
                dirty = true;
            }
        }
        if panel.pending
            && !panel.closing
            && !self.closing
            && self.preview.is_drained()
            && self.du_job.is_none()
            && !self.search.busy()
            && self.space_map.is_drained()
            && self.thumbnails.drained()
        {
            panel.pending = false;
            if panel.scan_mode {
                match crate::duplicate_job::Job::start(panel.stamp, panel.root.clone()) {
                    Ok(job) => {
                        panel.scan_job = Some(job);
                        panel.note = "Buscando duplicados · Cancelar disponible".into();
                    }
                    Err(e) => panel.error = e,
                }
            } else {
                match Job::start(panel.stamp, panel.root.clone(), panel.names.clone()) {
                    Ok(job) => {
                        panel.job = Some(job);
                        panel.note = "Calculando · Cancelar conserva sólo filas completas".into();
                    }
                    Err(e) => panel.error = e,
                }
            }
            dirty = true;
        }
        if !panel.busy()
            && let Some(paused) = panel.prior_pause.take()
            && !self.closing
        {
            self.sizes.pause(paused);
        }
        if panel.closing && !panel.busy() {
            self.hits.clear();
            return true;
        }
        self.checksums = Some(panel);
        dirty
    }
}
fn button(
    p: &mut dyn Painter,
    m: &mut Model,
    rect: Rect,
    label: &str,
    cmd: Command,
    stamp: u64,
    enabled: bool,
) {
    p.rounded_rect(
        rect,
        4 * m.radius,
        if enabled {
            m.palette.lighter_background
        } else {
            m.palette.background
        },
    );
    p.text(
        Rect::new(rect.x + 4, rect.y, rect.w - 8, rect.h),
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
            rect,
            action: Action::Checksum(stamp, cmd),
            generation: m.generation,
        });
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut panel) = m.checksums.take() else {
        return;
    };
    m.hits.clear();
    let r = Rect::new(12, 66, w - 24, h - 106);
    let c = m.palette.clone();
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.text(
        Rect::new(r.x + 12, r.y + 4, r.w - 24, 28),
        if panel.scan_mode {
            " Duplicados · carpeta actual"
        } else {
            " SHA-256 · archivos elegidos"
        },
        16,
        c.foreground,
        true,
        false,
    );
    let root_label = format!("{:?}", panel.root.as_os_str());
    let columns = ((r.w - 24) / 7).max(1) as usize;
    let root_label = if root_label.chars().count() > columns {
        let tail: Vec<_> = root_label
            .chars()
            .rev()
            .take(columns.saturating_sub(1))
            .collect();
        format!("…{}", tail.into_iter().rev().collect::<String>())
    } else {
        root_label
    };
    p.text(
        Rect::new(r.x + 12, r.y + 34, r.w - 24, 20),
        &root_label,
        11,
        c.dark_foreground,
        true,
        false,
    );
    let ok = panel.rows.iter().filter(|r| r.is_ok()).count();
    let errors = panel.rows.iter().filter(|r| r.is_err()).count();
    let summary = if panel.scan_mode {
        let q = panel.scan_progress;
        format!(
            "{} · {} entradas · {} archivos · {} SHA",
            if panel.busy() {
                if q.phase == 0 {
                    "Recorriendo"
                } else {
                    "Leyendo"
                }
            } else if panel
                .scan_report
                .as_ref()
                .is_some_and(|r| r.progress.complete())
            {
                "Completo"
            } else if panel.scan_report.is_some() {
                "PARCIAL"
            } else if !panel.error.is_empty() {
                "Sin informe"
            } else {
                "Revisión"
            },
            q.entries,
            q.files,
            q.hashed
        )
    } else if panel.busy() {
        format!(
            "{}/{} completos · {} / {} B",
            panel.progress.completed,
            panel.names.len(),
            panel.progress.bytes,
            panel.progress.total
        )
    } else {
        format!("{ok}/{} hashes · {errors} errores", panel.names.len())
    };
    p.text(
        Rect::new(r.x + 12, r.y + 56, r.w - 24, 22),
        &summary,
        11,
        c.dark_foreground,
        true,
        false,
    );
    p.text(
        Rect::new(r.x + 12, r.y + 78, r.w - 24, 20),
        &panel.comparison(),
        11,
        c.dark_foreground,
        true,
        false,
    );
    let navigation = panel.scan_pages.len() > 1 || panel.scan_groups.len() > 1;
    if navigation {
        let y = r.y + 102;
        let group = panel.selected_group().map(|(i, _)| i).unwrap_or(0);
        for (x, label, cmd, enabled) in [
            (r.x + 12, "G−", Command::Group(-1), group > 0),
            (r.x + 60, "←", Command::Page(-1), panel.scan_page > 0),
            (
                r.x + r.w - 100,
                "→",
                Command::Page(1),
                panel.scan_page + 1 < panel.scan_pages.len(),
            ),
            (
                r.x + r.w - 56,
                "G+",
                Command::Group(1),
                group + 1 < panel.scan_groups.len(),
            ),
        ] {
            button(
                p,
                m,
                Rect::new(x, y, 40, 24),
                label,
                cmd,
                panel.stamp,
                !panel.busy() && enabled,
            );
        }
        let start = panel.scan_pages[panel.scan_page];
        p.text(
            Rect::new(r.x + 104, y, r.w - 208, 24),
            &format!(
                "{}/{} · {}–{}",
                panel.scan_page + 1,
                panel.scan_pages.len(),
                start + 1,
                start + panel.names.len()
            ),
            11,
            c.accent,
            true,
            false,
        );
    }
    let extra = if navigation { 30 } else { 0 };
    let body = Rect::new(
        r.x + 12,
        r.y + 102 + extra,
        r.w - 24,
        (r.h - 238 - extra).max(28),
    );
    let page = (body.h / 28).max(1) as usize;
    if panel.selected < panel.top {
        panel.top = panel.selected;
    }
    if panel.selected >= panel.top + page {
        panel.top = panel.selected + 1 - page;
    }
    p.clip(body);
    for (i, name) in panel.names.iter().enumerate().skip(panel.top).take(page) {
        let rect = Rect::new(body.x, body.y + (i - panel.top) as i32 * 28, body.w, 26);
        if i == panel.selected {
            p.rounded_rect(rect, 3 * m.radius, c.selection);
        }
        p.text(
            Rect::new(rect.x + 4, rect.y, (rect.w - 106).max(0), 26),
            &format!("{:?}", name.path.as_os_str()),
            11,
            c.foreground,
            true,
            false,
        );
        let group_status = panel
            .scan_pages
            .get(panel.scan_page)
            .and_then(|start| panel.scan_membership.get(start + i))
            .map(|g| format!("G{} · {}", g + 1, panel.scan_groups[*g].count));
        let status = group_status.as_deref().unwrap_or(match panel.rows.get(i) {
            Some(Ok(_)) => "Calculado",
            Some(Err(_)) => "Error",
            None if panel.busy() => "Pendiente",
            None => "Sin calcular",
        });
        p.text(
            Rect::new(rect.x + rect.w - 100, rect.y, 96, 26),
            status,
            11,
            c.dark_foreground,
            true,
            false,
        );
        m.hits.push(Hit {
            rect,
            action: Action::Checksum(panel.stamp, Command::Select(i)),
            generation: m.generation,
        });
    }
    p.unclip();
    let detail_y = r.y + r.h - 132;
    if let Some(row) = panel.rows.get(panel.selected) {
        match row {
            Ok(o) => {
                let digest = checksum::hex(&o.sha256);
                for (i, line) in [&digest[..32], &digest[32..]].iter().enumerate() {
                    p.text(
                        Rect::new(r.x + 12, detail_y + i as i32 * 18, r.w - 24, 18),
                        line,
                        11,
                        c.accent,
                        true,
                        false,
                    );
                }
                p.text(
                    Rect::new(r.x + 12, detail_y + 38, r.w - 24, 20),
                    &if let Some((group, info)) = panel.selected_group() {
                        format!(
                            "Grupo {}/{} · {} archivos · {} B",
                            group + 1,
                            panel.scan_groups.len(),
                            info.count,
                            o.proof.file.bytes
                        )
                    } else {
                        format!(
                            "{} B · FD/revisión verificados · Ctrl+C informe",
                            o.proof.file.bytes
                        )
                    },
                    10,
                    c.dark_foreground,
                    true,
                    false,
                );
            }
            Err(error) => {
                let chars: Vec<_> = error.chars().collect();
                let cols = ((r.w - 24) / 7).max(1) as usize;
                for (i, line) in chars.chunks(cols).take(3).enumerate() {
                    p.text(
                        Rect::new(r.x + 12, detail_y + i as i32 * 18, r.w - 24, 18),
                        &line.iter().collect::<String>(),
                        11,
                        c.red,
                        true,
                        false,
                    );
                }
            }
        }
    } else {
        let text = if panel.scan_mode {
            if let Some(report) = &panel.scan_report {
                format!(
                    "{} · alias {} · errores {}",
                    reason_text(report.progress),
                    report.progress.aliases,
                    report.progress.errors
                )
            } else {
                "Busca en subcarpetas y ocultos. Omite enlaces y alias del mismo archivo. No borra archivos.".into()
            }
        } else {
            "Espera lectores activos y pausa tamaños pendientes. No modifica archivos ni sigue enlaces finales.".into()
        };
        let chars: Vec<_> = text.chars().collect();
        let cols = ((r.w - 24) / 7).max(1) as usize;
        for (i, line) in chars.chunks(cols).take(3).enumerate() {
            p.text(
                Rect::new(r.x + 12, detail_y + i as i32 * 18, r.w - 24, 18),
                &line.iter().collect::<String>(),
                11,
                c.dark_foreground,
                true,
                false,
            );
        }
    }
    p.text(
        Rect::new(r.x + 12, r.y + r.h - 68, r.w - 24, 24),
        if panel.error.is_empty() {
            &panel.note
        } else {
            &panel.error
        },
        10,
        if panel.error.is_empty() {
            c.dark_foreground
        } else {
            c.red
        },
        true,
        false,
    );
    let mut buttons = vec![
        (
            if panel.scan_mode {
                "Buscar"
            } else {
                "Calcular"
            },
            Command::Start,
            !panel.busy() && !panel.closing,
        ),
        ("Cancelar", Command::Cancel, panel.busy()),
        ("Captura", Command::Recapture, !panel.busy()),
        (
            "Copiar",
            Command::Copy,
            !panel.rows.is_empty() || panel.scan_report.is_some(),
        ),
        ("Cerrar", Command::Close, true),
    ];
    if panel.scan_mode {
        buttons.insert(
            4,
            (
                "Ubicar",
                Command::Locate,
                !panel.busy()
                    && !panel.closing
                    && panel.scan_report.is_some()
                    && panel.names.get(panel.selected).is_some(),
            ),
        );
    }
    let bw = (r.w - 24 - (buttons.len() as i32 - 1) * 4) / buttons.len() as i32;
    for (i, (label, cmd, enabled)) in buttons.into_iter().enumerate() {
        button(
            p,
            m,
            Rect::new(r.x + 12 + i as i32 * (bw + 4), r.y + r.h - 38, bw, 28),
            label,
            cmd,
            panel.stamp,
            enabled,
        );
    }
    p.unclip();
    m.checksums = Some(panel);
}
pub fn event(m: &mut Model, event: Event) -> bool {
    let Some(stamp) = m.checksums.as_ref().map(|p| p.stamp) else {
        return false;
    };
    let command = match event {
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
                m.action(a);
            }
            None
        }
        Event::KeyDown | Event::Shortcut => {
            let key = app::event_key();
            let ctrl = app::event_state().intersects(Shortcut::Ctrl | Shortcut::Meta);
            match key {
                Key::Escape => Some(Command::Close),
                Key::F5 => Some(Command::Start),
                Key::F6 => Some(Command::Recapture),
                Key::Left => Some(Command::Page(-1)),
                Key::Right => Some(Command::Page(1)),
                Key::Enter => Some(Command::Locate),
                Key::Up if ctrl => Some(Command::Group(-1)),
                Key::Down if ctrl => Some(Command::Group(1)),
                Key::Up => Some(Command::Move(-1)),
                Key::Down => Some(Command::Move(1)),
                Key::Home => Some(Command::Move(i32::MIN)),
                Key::End => Some(Command::Move(i32::MAX)),
                Key::PageUp => Some(Command::Move(-5)),
                Key::PageDown => Some(Command::Move(5)),
                _ if ctrl && key == Key::from_char('c') => Some(Command::Copy),
                _ if ctrl && key == Key::from_char('w') => Some(Command::Close),
                _ => None,
            }
        }
        Event::MouseWheel => Some(Command::Move(if app::event_dy_value() < 0 {
            -1
        } else {
            1
        })),
        _ => None,
    };
    if let Some(command) = command {
        m.checksum_command(stamp, command);
    }
    true
}
pub fn fixture(m: &Model) -> serde_json::Value {
    m.checksums.as_ref().map_or(serde_json::Value::Null,|p|serde_json::json!({"scan_mode":p.scan_mode,"scan_page":p.scan_page,"selected_group":p.selected_group().map(|(i,g)|serde_json::json!({"index":i,"count":g.count,"first":g.first})),"scan_pages":p.scan_pages,"scan_progress":p.scan_progress,"scan_report":p.scan_report,"stamp":p.stamp,"root_raw":p.root.as_os_str().as_encoded_bytes(),"owner":p.owner,"generation":p.generation,"busy":p.busy(),"pending":p.pending,"closing":p.closing,"error":p.error,"note":p.note,"selected":p.selected,"comparison":p.comparison(),"names":p.names.iter().map(|n|n.path.as_os_str().as_encoded_bytes()).collect::<Vec<_>>(),"rows":p.rows,"progress":{"completed":p.progress.completed,"bytes":p.progress.bytes,"total":p.progress.total},"buttons":m.hits.iter().filter_map(|h|if let Action::Checksum(stamp,cmd)=h.action{Some(serde_json::json!({"stamp":stamp,"command":format!("{cmd:?}"),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}else{None}).collect::<Vec<_>>()}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use files_core::listing::{Entry, Kind};
    use std::sync::mpsc;
    struct Fixture {
        root: PathBuf,
        m: Model,
    }
    impl Fixture {
        fn new() -> Self {
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!("checksum-ui-{}-{}", std::process::id(), epoch()));
            std::fs::create_dir_all(&root).unwrap();
            let mut m = Model::new(root.clone());
            let (tx, _) = mpsc::sync_channel(4);
            let (_, rx) = mpsc::channel();
            m.aux = crate::aux_io::AuxIo { tx, rx };
            m.folder_views = Default::default();
            m.folder_views.loaded = true;
            m.folder_restore = None;
            m.loaded = true;
            m.entries = ["a", "b", "c"]
                .iter()
                .map(|n| Entry {
                    name: (*n).into(),
                    kind: Kind::File,
                    metadata: None,
                    stat_attempted: false,
                })
                .collect();
            m.rebuild();
            m.selection.clear();
            m.selected = None;
            Self { root, m }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn explicit_clear_is_empty_but_focused_file_and_marks_match_browser_selection_policy() {
        let mut f = Fixture::new();
        assert!(Panel::capture(&f.m).is_err());
        f.m.selected = Some(0);
        assert_eq!(Panel::capture(&f.m).unwrap().names.len(), 1);
        f.m.selection.insert(0);
        assert!(Panel::capture(&f.m).is_ok());
        f.m.entries[0].kind = Kind::Symlink;
        assert!(Panel::capture(&f.m).err().unwrap().contains("enlaces"));
    }
    #[test]
    fn frozen_capture_cannot_redirect_after_owner_or_path_change_and_stale_hits_do_nothing() {
        let mut f = Fixture::new();
        f.m.selection.extend([0, 1]);
        f.m.open_checksums();
        let stamp = f.m.checksums.as_ref().unwrap().stamp;
        f.m.path = f.root.join("other");
        f.m.tabs.active += 1;
        f.m.selection.clear();
        f.m.checksum_command(stamp + 1, Command::Start);
        assert!(!f.m.checksums.as_ref().unwrap().busy());
        let panel = f.m.checksums.as_ref().unwrap();
        assert_eq!(panel.root, f.root);
        assert_eq!(panel.names.len(), 2);
        f.m.action(Action::Navigate(f.root.join("blocked")));
        assert_eq!(f.m.path, f.root.join("other"));
    }
    #[test]
    fn cancel_before_dispatch_restores_original_size_pause_without_starting_worker() {
        let mut f = Fixture::new();
        f.m.selection.insert(0);
        f.m.open_checksums();
        let stamp = f.m.checksums.as_ref().unwrap().stamp;
        f.m.sizes
            .enqueue(&f.root, std::iter::once(std::ffi::OsStr::new("dir")))
            .unwrap();
        assert!(!f.m.sizes.paused());
        f.m.checksum_command(stamp, Command::Start);
        assert!(f.m.sizes.paused());
        let stamp = f.m.checksums.as_ref().unwrap().stamp;
        f.m.checksum_command(stamp, Command::Cancel);
        assert!(!f.m.sizes.paused() && !f.m.checksums.as_ref().unwrap().busy());
    }
    #[test]
    fn close_keeps_slot_until_exact_worker_outcome_and_preserves_pause() {
        let mut f = Fixture::new();
        f.m.selection.insert(0);
        f.m.sizes
            .enqueue(&f.root, std::iter::once(std::ffi::OsStr::new("dir")))
            .unwrap();
        f.m.sizes.pause(true);
        f.m.open_checksums();
        let (job, tx) = Job::pending();
        let panel = f.m.checksums.as_mut().unwrap();
        panel.job = Some(job);
        panel.prior_pause = Some(true);
        let stamp = panel.stamp;
        f.m.checksum_command(stamp, Command::Close);
        assert!(f.m.checksums.as_ref().unwrap().busy());
        assert!(f.m.sizes.paused());
        assert!(!f.m.pump_checksums());
        assert!(f.m.checksums.is_some());
        tx.send(crate::checksum_job::Outcome {
            rows: Vec::new(),
            error: Some("cancelado".into()),
        })
        .unwrap();
        assert!(f.m.pump_checksums());
        assert!(f.m.checksums.is_none() && f.m.sizes.paused());
    }
    #[test]
    fn root_close_waits_for_hash_worker_ack_even_without_other_jobs() {
        let mut f = Fixture::new();
        f.m.selection.insert(0);
        f.m.open_checksums();
        let (job, tx) = Job::pending();
        f.m.checksums.as_mut().unwrap().job = Some(job);
        f.m.closing = true;
        assert!(!f.m.close_ready());
        tx.send(crate::checksum_job::Outcome {
            rows: Vec::new(),
            error: Some("cancelado".into()),
        })
        .unwrap();
        f.m.pump_checksums();
        assert!(f.m.checksums.is_none());
        assert!(f.m.close_ready());
    }

    #[test]
    fn pages_keep_large_group_and_copy_scope_without_restarting_reader() {
        let mut f = Fixture::new();
        let root = f.root.join("paged");
        std::fs::create_dir(&root).unwrap();
        for i in 0..129 {
            std::fs::write(root.join(format!("copy-{i:03}")), b"paged-group").unwrap();
        }
        let report = files_core::duplicates::scan_all(
            &root,
            &std::sync::atomic::AtomicBool::new(false),
            |_| Ok(()),
        )
        .unwrap();
        f.m.open_duplicates();
        let panel = f.m.checksums.as_mut().unwrap();
        panel.accept_scan(report);
        assert_eq!(panel.scan_pages, vec![0, 128]);
        assert_eq!(panel.names.len(), 128);
        let stamp = panel.stamp;
        f.m.checksum_command(stamp, Command::Page(1));
        let panel = f.m.checksums.as_ref().unwrap();
        assert_eq!(panel.names.len(), 1);
        assert_eq!(panel.names[0].path, PathBuf::from("copy-128"));
        assert!(
            panel
                .report()
                .unwrap()
                .contains("página 2/2 · filas 129–129")
        );
        assert!(!panel.busy());
        assert_eq!(panel.scan_report.as_ref().unwrap().rows.len(), 129);
        for (w, h) in [(440, 440), (1000, 650)] {
            paint(&mut Recorder, &mut f.m, w, h);
            assert!(
                f.m.hits
                    .iter()
                    .all(|h| h.rect.x >= 12 && h.rect.x + h.rect.w <= w - 12)
            );
        }
        f.m.checksum_command(stamp, Command::Page(-1));
        assert_eq!(f.m.checksums.as_ref().unwrap().names.len(), 128);
    }
    #[test]
    fn groups_index_interleaved_rows_and_jump_across_pages_without_io() {
        let mut f = Fixture::new();
        let root = f.root.join("groups");
        std::fs::create_dir(&root).unwrap();
        for i in 0..130 {
            std::fs::write(root.join(format!("a-{i:03}")), b"abc").unwrap();
        }
        for i in 0..2 {
            std::fs::write(root.join(format!("z-{i}")), b"xyz").unwrap();
        }
        let mut report = files_core::duplicates::scan_all(
            &root,
            &std::sync::atomic::AtomicBool::new(false),
            |_| Ok(()),
        )
        .unwrap();
        // Validation allows transport order to interleave groups: UI membership
        // must follow digest+length, rather than adjacency or page boundaries.
        report.rows.swap(1, 130);
        files_core::duplicates::validate_all(&report).unwrap();
        f.m.open_duplicates();
        let panel = f.m.checksums.as_mut().unwrap();
        panel.accept_scan(report);
        assert_eq!(panel.scan_groups.len(), 2);
        let stamp = panel.stamp;
        let first = panel.selected_group().unwrap().1.count;
        f.m.checksum_command(stamp, Command::Group(1));
        let panel = f.m.checksums.as_ref().unwrap();
        assert_eq!(panel.selected_group().unwrap().0, 1);
        assert_ne!(panel.selected_group().unwrap().1.count, first);
        assert!(!panel.busy());
        assert!(
            panel.report().unwrap().contains("G1 (") && panel.report().unwrap().contains("G2 (")
        );
        f.m.checksum_command(stamp, Command::Group(-1));
        assert_eq!(
            f.m.checksums.as_ref().unwrap().selected_group().unwrap().0,
            0
        );
    }
    #[test]
    #[cfg(unix)]
    fn locate_captures_raw_name_and_waits_for_current_listing_without_mutation() {
        use std::os::unix::ffi::OsStringExt;
        let mut f = Fixture::new();
        let sub = f.root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let name = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        std::fs::write(sub.join(&name), b"abc").unwrap();
        std::fs::write(f.root.join("peer"), b"abc").unwrap();
        let report = files_core::duplicates::scan_all(
            &f.root,
            &std::sync::atomic::AtomicBool::new(false),
            |_| Ok(()),
        )
        .unwrap();
        f.m.open_duplicates();
        let panel = f.m.checksums.as_mut().unwrap();
        panel.accept_scan(report);
        panel.selected = panel
            .names
            .iter()
            .position(|n| n.path == PathBuf::from("sub").join(&name))
            .unwrap();
        let stamp = panel.stamp;
        let hidden = f.m.show_hidden;
        f.m.checksum_command(stamp, Command::Locate);
        assert!(f.m.checksums.is_none());
        assert_eq!(f.m.path, sub);
        assert_eq!(f.m.search_locate.as_ref().unwrap().1, name);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while f.m.search_locate.is_some() && std::time::Instant::now() < until {
            f.m.pump();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(f.m.loaded && f.m.search_locate.is_none());
        assert_eq!(f.m.entries[f.m.selected.unwrap()].name, name);
        assert_eq!(f.m.show_hidden, hidden);
        assert_eq!(std::fs::read(sub.join(name)).unwrap(), b"abc");
    }
    #[test]
    fn locate_hidden_or_removed_candidate_leaves_no_unrelated_selection() {
        for removed in [false, true] {
            let mut f = Fixture::new();
            f.m.show_hidden = false;
            let candidate = if removed { "visible" } else { ".hidden" };
            for name in [candidate, "peer"] {
                std::fs::write(f.root.join(name), b"abc").unwrap();
            }
            let report = files_core::duplicates::scan_all(
                &f.root,
                &std::sync::atomic::AtomicBool::new(false),
                |_| Ok(()),
            )
            .unwrap();
            f.m.open_duplicates();
            let panel = f.m.checksums.as_mut().unwrap();
            panel.accept_scan(report);
            panel.selected = panel
                .names
                .iter()
                .position(|n| n.path == std::path::Path::new(candidate))
                .unwrap();
            let stamp = panel.stamp;
            if removed {
                std::fs::remove_file(f.root.join(candidate)).unwrap();
            }
            let generation = f.m.generation;
            f.m.closing = true;
            f.m.checksum_command(stamp, Command::Locate);
            assert!(f.m.checksums.is_some() && f.m.generation == generation);
            f.m.closing = false;
            f.m.checksum_command(stamp, Command::Locate);
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while f.m.search_locate.is_some() && std::time::Instant::now() < until {
                f.m.pump();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(f.m.loaded && f.m.search_locate.is_none());
            assert!(f.m.selected.is_none() && f.m.selection.is_empty());
            assert!(!f.m.error.is_empty() && !f.m.show_hidden);
        }
    }
    #[test]
    fn directory_mode_accepts_explicit_empty_selection_and_copy_waits_for_report() {
        let mut f = Fixture::new();
        f.m.selection.clear();
        f.m.selected = None;
        f.m.open_duplicates();
        let panel = f.m.checksums.as_ref().unwrap();
        assert!(panel.scan_mode && panel.names.is_empty() && !panel.busy());
        let stamp = panel.stamp;
        let note = panel.note.clone();
        f.m.checksum_command(stamp, Command::Copy);
        assert_eq!(f.m.checksums.as_ref().unwrap().note, note);
        let p = files_core::duplicates::Progress {
            phase: 2,
            entries: 2,
            files: 2,
            candidates: 2,
            errors: 1,
            ..Default::default()
        };
        let panel = f.m.checksums.as_mut().unwrap();
        panel.scan_progress = p;
        panel.scan_report = Some(files_core::duplicates::Report {
            root: files_core::relative_source::DirectoryIdentity {
                device: 1,
                inode: 1,
            },
            progress: p,
            rows: Vec::new(),
        });
        assert!(panel.comparison().contains("parcial"));
        assert!(panel.report().unwrap().contains("PARCIAL"));
    }
    #[test]
    fn directory_reader_keeps_root_close_pending_until_outcome() {
        let mut f = Fixture::new();
        f.m.open_duplicates();
        let (job, tx) = crate::duplicate_job::Job::pending();
        f.m.checksums.as_mut().unwrap().scan_job = Some(job);
        f.m.closing = true;
        assert!(!f.m.close_ready());
        tx.send(crate::duplicate_job::Outcome {
            report: None,
            error: Some("cancelado".into()),
        })
        .unwrap();
        f.m.pump_checksums();
        assert!(f.m.checksums.is_none());
        assert!(f.m.close_ready());
    }
    struct Recorder;
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            _: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    #[cfg(unix)]
    fn minimum_panel_targets_fit_and_report_escapes_raw_control_names() {
        use std::os::unix::ffi::OsStringExt;
        let mut f = Fixture::new();
        f.m.entries[0].name = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        f.m.selection.insert(0);
        f.m.open_checksums();
        for (w, h) in [(440, 440), (1000, 650)] {
            paint(&mut Recorder, &mut f.m, w, h);
            assert!(f.m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y >= 66
                && hit.rect.y + hit.rect.h <= h - 40));
        }
        let report = f.m.checksums.as_ref().unwrap().report().unwrap();
        assert!(report.contains("\\xFF") || report.contains("\\xff"));
        assert!(report.contains("\\n") && report.len() < 128 * 1024);
    }
}
