//! One search intent/one helper. Cancellation keeps the active job until reap ACK.
use files_core::{
    protocol::Request,
    search::{Hit, Progress, Report},
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
trait Task {
    fn invalidated(&self) -> bool {
        false
    }
    fn candidates(&self) -> Option<Arc<[Hit]>> {
        None
    }
    fn page(&self) -> Option<files_core::search_page::Page> {
        None
    }
    fn finish_page(&self) -> Option<Result<files_core::search_page::Page, String>> {
        None
    }
    fn next_page(&self, _sequence: u64) -> bool {
        false
    }
    fn cancel(&self);
    fn poll(&self) -> Option<Result<Report, String>>;
    fn progress(&self) -> Option<Progress> {
        None
    }
}
struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Report, String>>,
    progress: Arc<Mutex<Option<Progress>>>,
    candidates: Arc<Mutex<Option<Arc<[Hit]>>>>,
    invalid: Arc<AtomicBool>,
}
impl Job {
    fn start(path: PathBuf, query: String, hidden: bool) -> Result<Self, String> {
        let (metadata_requested, content_requested, regex_requested) = {
            let filter = files_core::name_filter::Filter::for_search(&query);
            (
                filter.needs_metadata(),
                filter.content().is_some(),
                filter.regex().is_some(),
            )
        };
        let mut request = serde_json::to_vec(&Request::SearchBatches {
            id: 1,
            path,
            query,
            hidden,
        })
        .map_err(|e| e.to_string())?;
        request.push(b'\n');
        if request.len() > 64 * 1024 {
            return Err("Petición de búsqueda demasiado grande".into());
        }
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("files-helper");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let progress = Arc::new(Mutex::new(None));
        let published = progress.clone();
        let candidates = Arc::new(Mutex::new(None));
        let published_candidates = candidates.clone();
        let invalid = Arc::new(AtomicBool::new(false));
        let invalidated = invalid.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-search".into())
            .spawn(move || {
                let mut decoder = crate::search_stream::Stream::new(metadata_requested)
                    .expect_content(content_requested)
                    .expect_regex(regex_requested)
                    .with_batches();
                let value = crate::space_map_job::run_helper_stream(
                    &request,
                    &flag,
                    executable,
                    Duration::from_secs(15),
                    "search_batches",
                    |bytes| {
                        let decoded = decoder.feed(bytes, &published);
                        if decoded.is_err() || decoder.invalidated() {
                            invalidated.store(true, Ordering::Release);
                            published_candidates
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .take();
                        }
                        decoded?;
                        if let Some(snapshot) = decoder.take_snapshot() {
                            let previous = published_candidates
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .replace(snapshot);
                            drop(previous);
                        }
                        Ok(())
                    },
                )
                .and_then(|_| decoder.finish())
                .map_err(|e| e.to_string());
                if value.is_err() {
                    invalidated.store(true, Ordering::Release);
                }
                let _ = tx.send(value);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cancel,
            result,
            progress,
            candidates,
            invalid,
        })
    }
}
impl Task for Job {
    fn invalidated(&self) -> bool {
        self.invalid.load(Ordering::Acquire)
    }
    fn candidates(&self) -> Option<Arc<[Hit]>> {
        self.candidates.try_lock().ok()?.take()
    }
    fn progress(&self) -> Option<Progress> {
        self.progress.try_lock().ok().and_then(|p| *p)
    }
    fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    fn poll(&self) -> Option<Result<Report, String>> {
        match self.result.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("Búsqueda terminó sin respuesta".into())),
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Task for crate::search_pages_job::Job {
    fn invalidated(&self) -> bool {
        self.invalidated()
    }
    fn page(&self) -> Option<files_core::search_page::Page> {
        self.take_page()
    }
    fn finish_page(&self) -> Option<Result<files_core::search_page::Page, String>> {
        self.poll()
    }
    fn next_page(&self, sequence: u64) -> bool {
        self.next(sequence)
    }
    fn cancel(&self) {
        self.cancel();
    }
    fn poll(&self) -> Option<Result<Report, String>> {
        None
    }
}
struct Active {
    revision: u64,
    job: Box<dyn Task>,
}
#[derive(Clone, Copy)]
struct DateRequest {
    id: u64,
    epoch: u64,
    preset: files_core::search_dates::Preset,
    reference: std::time::SystemTime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Parent,
    Back,
    Result(usize),
}
#[derive(Default)]
pub struct State {
    pub scope_menu: bool,
    pub scope_choice: usize,
    scope_epoch: u64,
    pub(crate) scope_result: Option<(usize, u64)>,
    scope_history: std::collections::VecDeque<PathBuf>,
    pub visible: bool,
    pub root: PathBuf,
    pub query: String,
    pub content: String,
    pub content_mode: bool,
    pub content_regex: bool,
    pub hidden: bool,
    pub replace: bool,
    pub filters_open: bool,
    pub calendar_open: bool,
    pub date_range: Option<files_core::search_dates::Range>,
    date_next: u64,
    date_pending: Option<DateRequest>,
    date_active: Option<DateRequest>,
    pub fields: [String; 4],
    pub field: usize,
    pub editing_input: bool,
    pub editors: [crate::line_edit::Cursor; 6],
    pub geometry: [crate::line_field::Geometry; 6],
    dragging: Option<usize>,
    edit_epoch: u64,
    pub report: Option<Report>,
    pub page_mode: bool,
    pub pages: crate::search_page_cache::Cache,
    pub provisional: Option<Arc<[Hit]>>,
    pub progress: Option<Progress>,
    pub error: String,
    pub status: &'static str,
    pub selected: usize,
    pub marks: crate::search_marks::Marks,
    pub result_epoch: u64,
    pub background_tab: Option<u64>,
    pub background_count: usize,
    pub order: crate::search_order::Index,
    pub scroll: usize,
    revision: u64,
    active: Option<Active>,
    pending: Option<(PathBuf, String, bool)>,
}
impl State {
    pub fn saved_criteria(&self) -> Result<files_core::saved_search::Criteria, String> {
        if !self.visible {
            return Err("Abre Buscar antes de guardar criterios".into());
        }
        if self.query.len() > 4096 || self.query.contains('\0') {
            return Err("El nombre de búsqueda excede4096 bytes o contiene NUL".into());
        }
        if self.date_busy() {
            return Err("Espera la fecha local antes de guardar".into());
        }
        let query = self.composed_query()?;
        if query.is_empty() {
            return Err("Escribe criterios antes de guardar".into());
        }
        if let Some(error) = files_core::name_filter::Filter::for_search(&query).error() {
            return Err(error.into());
        }
        if self.content_mode && self.content.is_empty() {
            return Err(if self.content_regex {
                "Escribe el patrón regex o vuelve a Nombre antes de guardar"
            } else {
                "Escribe texto literal o vuelve a Nombre antes de guardar"
            }
            .into());
        }
        files_core::navigation_history::validate(&self.root).map_err(str::to_string)?;
        Ok(files_core::saved_search::Criteria {
            root: self.root.clone(),
            query: self.query.clone(),
            content: self.content.clone(),
            fields: self.fields.clone(),
            hidden: self.hidden,
            content_mode: self.content_mode,
            content_regex: self.content_regex,
            order: match self.order.order {
                crate::search_order::Order::Path => 0,
                crate::search_order::Order::Name => 1,
                crate::search_order::Order::Kind => 2,
            },
        })
    }
    pub fn load_saved_criteria(
        &mut self,
        c: &files_core::saved_search::Criteria,
        here: bool,
    ) -> Result<(), String> {
        files_core::saved_search::validate_criteria(c).map_err(|e| e.to_string())?;
        if here {
            files_core::navigation_history::validate(&self.root).map_err(str::to_string)?;
        }
        let root = if here {
            self.root.clone()
        } else {
            c.root.clone()
        };
        self.open(root, c.query.clone(), c.hidden);
        self.content = c.content.clone();
        self.fields = c.fields.clone();
        self.content_mode = c.content_mode;
        self.content_regex = c.content_regex;
        self.filters_open = self.fields.iter().any(|s| !s.is_empty());
        self.order.order = match c.order {
            1 => crate::search_order::Order::Name,
            2 => crate::search_order::Order::Kind,
            _ => crate::search_order::Order::Path,
        };
        self.field = if c.content_mode { 5 } else { 0 };
        let value = if c.content_mode {
            &self.content
        } else {
            &self.query
        };
        self.editors[self.field].all(value);
        self.error = self.composed_query().err().unwrap_or_default();
        self.status = "Criterios cargados · fechas UTC guardadas · Enter busca";
        Ok(())
    }
    pub fn date_busy(&self) -> bool {
        self.date_pending.is_some() || self.date_active.is_some_and(|r| r.epoch == self.edit_epoch)
    }
    pub fn toggle_calendar(&mut self) {
        if !self.visible {
            return;
        }
        self.calendar_open = !self.calendar_open;
        self.status = if self.calendar_open {
            "Elige fechas u horas transcurridas · criterios sin cambiar"
        } else {
            "Criterios conservados · Enter busca"
        };
        self.date_pending = None;
        self.dragging = None;
        self.edit_changed();
    }
    pub fn request_date(&mut self, preset: files_core::search_dates::Preset) {
        self.request_date_at(preset, std::time::SystemTime::now());
    }
    fn request_date_at(
        &mut self,
        preset: files_core::search_dates::Preset,
        reference: std::time::SystemTime,
    ) {
        if !self.visible || !self.calendar_open {
            return;
        }
        self.cancel();
        self.marks = Default::default();
        self.report = None;
        self.provisional = None;
        self.date_next = self
            .date_next
            .checked_add(1)
            .expect("search date id exhausted");
        self.date_pending = Some(DateRequest {
            id: self.date_next,
            epoch: self.edit_epoch,
            preset,
            reference,
        });
        self.status = "Calculando límites de fecha…";
        self.error.clear();
    }
    pub fn dispatch_date(
        &mut self,
        tx: &std::sync::mpsc::SyncSender<crate::aux_io::Request>,
    ) -> bool {
        if self.date_active.is_some() {
            return false;
        }
        let Some(request) = self.date_pending.take() else {
            return false;
        };
        if !self.visible || !self.calendar_open || request.epoch != self.edit_epoch {
            return false;
        }
        match tx.try_send(crate::aux_io::Request::SearchDate(
            request.id,
            request.preset,
            request.reference,
        )) {
            Ok(()) => {
                self.date_active = Some(request);
                true
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                self.date_pending = Some(request);
                false
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                self.error = "Trabajador de calendario no disponible".into();
                true
            }
        }
    }
    pub fn receive_date(
        &mut self,
        id: u64,
        result: Result<files_core::search_dates::Range, String>,
    ) -> bool {
        let Some(request) = self.date_active.filter(|r| r.id == id) else {
            return false;
        };
        self.date_active = None;
        if !self.visible || !self.calendar_open || request.epoch != self.edit_epoch {
            return false;
        }
        match result {
            Ok(range) if range.matches_request(request.preset, request.reference) => {
                let old_after = std::mem::replace(&mut self.fields[2], range.after.clone());
                let old_before = std::mem::replace(&mut self.fields[3], range.before.clone());
                if let Err(error) = self.composed_query() {
                    self.fields[2] = old_after;
                    self.fields[3] = old_before;
                    self.error = error;
                    return true;
                }
                self.editors[3] = Default::default();
                self.editors[4] = Default::default();
                self.geometry[3] = Default::default();
                self.geometry[4] = Default::default();
                self.calendar_open = false;
                self.filters_open = true;
                self.field = self.primary();
                self.editing_input = false;
                self.status = range.preset.label();
                self.date_range = Some(range);
                self.error.clear();
                self.edit_changed();
            }
            Ok(_) => {
                self.error = "Respuesta de calendario incoherente; criterios conservados".into()
            }
            Err(error) => self.error = error,
        }
        true
    }
    pub fn input_revision(&self) -> u64 {
        self.edit_epoch
    }
    pub fn is_drained(&self) -> bool {
        self.active.is_none() && self.pending.is_none()
    }
    pub fn busy(&self) -> bool {
        self.visible || !self.is_drained()
    }
    pub fn searching_current(&self) -> bool {
        self.pending.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|a| a.revision == self.revision)
    }
    pub fn running(&self) -> bool {
        !self.is_drained()
    }
    pub fn scope_stamp(&self, scope: Scope) -> u64 {
        if matches!(scope, Scope::Result(_)) {
            self.result_epoch
        } else {
            self.scope_epoch
        }
    }
    pub fn scope_result_index(&self) -> Option<usize> {
        self.scope_result
            .filter(|(_, epoch)| *epoch == self.result_epoch)
            .map(|(i, _)| i)
    }
    pub fn scope_target(&self, scope: Scope) -> Option<PathBuf> {
        if !self.visible {
            return None;
        }
        let candidate = match scope {
            Scope::Parent => self.root.parent()?,
            Scope::Back => self.scope_history.back()?.as_path(),
            Scope::Result(i) => {
                let hit = self.hits().get(i)?;
                if hit.relative.as_os_str().is_empty()
                    || !hit
                        .relative
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    return None;
                }
                let relative = if hit.kind == files_core::listing::Kind::Directory {
                    hit.relative.as_path()
                } else {
                    hit.relative.parent()?
                };
                if self
                    .root
                    .as_os_str()
                    .len()
                    .checked_add(1)?
                    .checked_add(relative.as_os_str().len())?
                    > 4096
                {
                    return None;
                }
                let next = self.root.join(relative);
                return (next != self.root
                    && next.is_absolute()
                    && !next.as_os_str().as_encoded_bytes().contains(&0))
                .then_some(next);
            }
        };
        (candidate != self.root
            && candidate.is_absolute()
            && candidate.as_os_str().len() <= 4096
            && !candidate.as_os_str().as_encoded_bytes().contains(&0))
        .then(|| candidate.to_path_buf())
    }
    pub fn change_scope(&mut self, scope: Scope, epoch: u64) {
        if epoch != self.scope_stamp(scope) {
            return;
        }
        if self.date_busy() {
            self.error = "Espera la fecha pendiente antes de cambiar raíz".into();
            return;
        }
        let Some(next) = self.scope_target(scope) else {
            return;
        };
        if self.root.as_os_str().len() > 4096 {
            return;
        }
        if scope == Scope::Back {
            self.scope_history.pop_back();
        } else {
            let previous = self.root.as_path().to_path_buf();
            while self.scope_history.len() >= 8
                || self
                    .scope_history
                    .iter()
                    .map(|p| p.capacity())
                    .sum::<usize>()
                    .saturating_add(previous.capacity())
                    > 32 * 1024
            {
                if self.scope_history.pop_front().is_none() {
                    break;
                }
            }
            if previous.capacity() <= 4096 {
                self.scope_history.push_back(previous);
            }
        }
        self.cancel(); // Keep active until its reap ACK; revoke late results/date replies.
        self.scope_epoch = self
            .scope_epoch
            .checked_add(1)
            .expect("search root epoch exhausted");
        self.root = next;
        self.report = None;
        self.provisional = None;
        self.marks = Default::default();
        self.background_tab = None;
        self.background_count = 0;
        self.selected = 0;
        self.scroll = 0;
        self.scope_menu = false;
        self.scope_result = None;
        self.calendar_open = false;
        self.filters_open = false;
        self.field = self.primary();
        self.editing_input = true;
        self.replace = false;
        self.dragging = None;
        self.geometry = Default::default();
        self.error.clear();
        self.status = "Raíz cambiada · criterios conservados · Enter busca";
    }
    pub fn open(&mut self, root: PathBuf, query: String, hidden: bool) {
        self.close();
        self.visible = true;
        self.order = Default::default();
        self.page_mode = false;
        self.root = root;
        self.query = query;
        self.content.clear();
        self.content_mode = false;
        self.content_regex = false;
        self.fields = Default::default();
        self.filters_open = false;
        self.calendar_open = false;
        self.date_range = None;
        self.field = 0;
        self.hidden = hidden;
        self.replace = true;
        self.editing_input = true;
        self.editors = Default::default();
        self.geometry = Default::default();
        self.editors[0].all(&self.query);
        self.error.clear();
        self.status = "Enter busca · flechas recorren · Alt+Enter localiza";
    }
    fn edit_changed(&mut self) {
        self.background_tab = None;
        self.edit_epoch = self
            .edit_epoch
            .checked_add(1)
            .expect("search edit epoch exhausted");
    }
    pub fn focus(&mut self, field: usize) {
        if self.calendar_open {
            self.status = "Criterios conservados · Enter busca";
            self.date_pending = None;
        }
        self.calendar_open = false;
        if field > 5 || (field != 0 && field != 5 && !self.filters_open) {
            return;
        }
        self.field = field;
        if field == 0 {
            self.content_mode = false;
        } else if field == 5 {
            self.content_mode = true;
        }
        self.editing_input = true;
        self.replace = true;
        let text = if field == 0 {
            &self.query
        } else if field == 5 {
            &self.content
        } else {
            &self.fields[field - 1]
        };
        self.editors[field].all(text);
        self.edit_changed();
    }
    pub fn select_input(&mut self) {
        self.focus(self.field);
    }
    pub fn toggle_filters(&mut self) {
        self.calendar_open = false;
        self.filters_open = !self.filters_open;
        self.focus(if self.filters_open { 1 } else { self.primary() });
    }
    pub fn primary(&self) -> usize {
        if self.content_mode { 5 } else { 0 }
    }
    pub fn toggle_content(&mut self) {
        self.focus(if self.content_mode { 0 } else { 5 });
    }
    pub fn toggle_regex(&mut self) {
        self.content_regex = !self.content_regex;
        self.focus(5);
        self.invalidate_input();
        self.status = if self.content_regex {
            "Patrón regex · Enter busca · Ctrl+F6 cambia a literal"
        } else {
            "Texto literal · Enter busca · Ctrl+F6 cambia a regex"
        };
    }
    pub fn clear_filters(&mut self) {
        self.calendar_open = false;
        self.date_range = None;
        for value in &mut self.fields {
            value.clear();
        }
        self.editors[1..]
            .iter_mut()
            .for_each(|c| *c = Default::default());
        self.invalidate_input();
        self.focus(self.primary());
    }
    fn invalidate_input(&mut self) {
        self.marks = Default::default();
        if matches!(self.field, 3 | 4) {
            self.date_range = None;
        }
        self.cancel();
        self.order.invalidate();
        self.report = None;
        self.provisional = None;
        self.selected = 0;
        self.scroll = 0;
        self.status = "Enter busca en el alcance mostrado";
        self.error.clear();
    }
    pub fn text(&self, field: usize) -> &str {
        if field == 0 {
            &self.query
        } else if field == 5 {
            &self.content
        } else {
            &self.fields[field - 1]
        }
    }
    pub fn paint_input(
        &mut self,
        p: &mut dyn crate::painter::Painter,
        field: usize,
        area: crate::painter::Rect,
        colors: &files_core::theme::Palette,
    ) {
        let text = if field == 0 {
            &self.query
        } else if field == 5 {
            &self.content
        } else {
            &self.fields[field - 1]
        };
        self.geometry[field].paint(
            p,
            text,
            &self.editors[field],
            area,
            colors,
            (self.field == field && self.editing_input, self.edit_epoch),
        );
    }
    pub fn motion(&mut self, motion: crate::line_edit::Motion, shift: bool) {
        self.editing_input = true;
        let text = if self.field == 0 {
            &self.query
        } else if self.field == 5 {
            &self.content
        } else {
            &self.fields[self.field - 1]
        };
        self.editors[self.field].motion(text, motion, shift);
        self.replace = false;
        self.edit_changed();
    }
    pub fn pointer(
        &mut self,
        x: i32,
        y: i32,
        down: bool,
        drag: bool,
        shift: bool,
        double: bool,
    ) -> bool {
        if self.calendar_open {
            return false;
        }
        if !down && !drag {
            self.dragging = None;
            return false;
        }
        if down {
            self.dragging = None;
        }
        let Some(field) = (if drag {
            self.dragging
        } else {
            std::iter::once(self.primary())
                .chain(1..if self.filters_open { 5 } else { 1 })
                .find(|&i| self.geometry[i].contains(x, y))
        }) else {
            return false;
        };
        let Some(at) = self.geometry[field].position(x, self.edit_epoch) else {
            return true;
        };
        self.field = field;
        self.editing_input = true;
        self.replace = false;
        let text = if field == 0 {
            &self.query
        } else if field == 5 {
            &self.content
        } else {
            &self.fields[field - 1]
        };
        let cursor = &mut self.editors[field];
        if down && double {
            cursor.component(text, at);
        } else {
            let anchor = if shift || drag { cursor.anchor } else { at };
            cursor.select(text, anchor, at);
        }
        self.dragging = Some(field);
        self.edit_changed();
        true
    }
    pub fn delete(&mut self, backward: bool, word: bool) {
        self.editing_input = true;
        let value = if self.field == 0 {
            &mut self.query
        } else if self.field == 5 {
            &mut self.content
        } else {
            &mut self.fields[self.field - 1]
        };
        let before = value.len();
        self.editors[self.field].delete(value, backward, word, self.replace);
        self.replace = false;
        if before != value.len() {
            self.invalidate_input();
        } else {
            self.edit_changed();
        }
    }
    pub fn undo(&mut self, redo: bool) {
        let value = if self.field == 0 {
            &mut self.query
        } else if self.field == 5 {
            &mut self.content
        } else {
            &mut self.fields[self.field - 1]
        };
        let cursor = &mut self.editors[self.field];
        let before = cursor.revision;
        cursor.undo(value, redo);
        self.replace = false;
        self.editing_input = true;
        if before != cursor.revision {
            self.invalidate_input();
        }
    }
    pub fn selected_text(&self) -> &str {
        let text = self.text(self.field);
        &text[self.editors[self.field].range(text)]
    }
    pub fn input(&mut self, text: &str) {
        if self.calendar_open {
            return;
        }
        if text
            .chars()
            .any(|c| c.is_control() && (self.field != 0 || !matches!(c, '\n' | '\r' | '\0')))
        {
            self.error = "Escribe una sola línea, sin controles".into();
            return;
        }
        let limit = if self.field == 0 {
            4096
        } else if self.field == 5 {
            1024
        } else {
            32
        };
        let value = if self.field == 0 {
            &mut self.query
        } else if self.field == 5 {
            &mut self.content
        } else {
            &mut self.fields[self.field - 1]
        };
        if let Err(e) = self.editors[self.field].insert_bounded(value, text, self.replace, limit) {
            self.error = if self.field == 0 {
                e.into()
            } else if self.field == 5 {
                "Texto literal: máximo1024 bytes, sin saltos de línea".into()
            } else {
                "Cada criterio admite hasta 32 bytes, sin saltos de línea".into()
            };
            return;
        }
        self.replace = false;
        self.editing_input = true;
        self.invalidate_input();
    }
    pub fn composed_query(&self) -> Result<String, String> {
        const PREFIX: [&str; 4] = ["size:>=", "size:<=", "after:", "before:"];
        let mut length = self.query.trim().len();
        for (i, value) in self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_empty())
        {
            if value.len() > 32
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b':')
            {
                return Err(
                    "Criterio inválido: usa tamaño o fecha UTC (AAAA-MM-DD / ...THH:MM:SSZ)".into(),
                );
            }
            // Tiny bounded fragments validate before allocating the combined query.
            let term = format!("{}{value}", PREFIX[i]);
            if let Some(e) = files_core::name_filter::Filter::for_search(&term).error() {
                return Err(e.into());
            }
            length = length
                .checked_add(term.len() + usize::from(length != 0))
                .ok_or(crate::omnibar_input::TOO_LONG)?;
        }
        if !self.fields[0].is_empty() && !self.fields[1].is_empty() {
            let range = format!("size:{}..{}", self.fields[0], self.fields[1]);
            if files_core::name_filter::Filter::for_search(&range)
                .error()
                .is_some()
            {
                return Err("El tamaño mínimo no puede superar al máximo".into());
            }
        }
        if !self.fields[2].is_empty()
            && !self.fields[3].is_empty()
            && files_core::search_dates::utc_seconds(&self.fields[2])
                >= files_core::search_dates::utc_seconds(&self.fields[3])
        {
            return Err("Desde debe ser anterior a Antes de (UTC)".into());
        }
        if !self.content.is_empty() {
            if self.content.len() > 1024 || self.content.contains('\0') {
                return Err("Texto literal: máximo1024 bytes, sin NUL".into());
            }
            let escaped = self
                .content
                .chars()
                .filter(|c| matches!(c, '"' | '\\' | '\n' | '\r' | '\t'))
                .count();
            length = length
                .checked_add(
                    self.content.len()
                        + escaped
                        + if self.content_regex { 8 } else { 10 }
                        + usize::from(length != 0),
                )
                .ok_or(crate::omnibar_input::TOO_LONG)?;
        }
        if length > crate::omnibar_input::LIMIT {
            return Err(crate::omnibar_input::TOO_LONG.into());
        }
        let mut query = String::with_capacity(length);
        for (i, value) in self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_empty())
        {
            if !query.is_empty() {
                query.push(' ');
            }
            query.push_str(PREFIX[i]);
            query.push_str(value);
        }
        if !self.content.is_empty() {
            if !query.is_empty() {
                query.push(' ');
            }
            query.push_str(if self.content_regex {
                "regex:\""
            } else {
                "content:\""
            });
            for c in self.content.chars() {
                match c {
                    '"' => query.push_str("\\\""),
                    '\\' => query.push_str("\\\\"),
                    '\n' => query.push_str("\\n"),
                    '\r' => query.push_str("\\r"),
                    '\t' => query.push_str("\\t"),
                    _ => query.push(c),
                }
            }
            query.push('"');
        }
        if !self.query.trim().is_empty() {
            if !query.is_empty() {
                query.push(' ');
            }
            query.push_str(self.query.trim());
        }
        Ok(query)
    }
    pub fn start(&mut self) {
        self.marks = Default::default();
        self.background_tab = None;
        if self.date_busy() {
            self.error = "Espera la fecha local o cambia el criterio para descartarla".into();
            return;
        }
        if self.calendar_open {
            self.toggle_calendar();
            return;
        }
        if self.content_mode && self.content.is_empty() {
            self.error = if self.content_regex {
                "Escribe el patrón regex o vuelve a Nombre (F6)"
            } else {
                "Escribe el texto literal o vuelve a Nombre (F6)"
            }
            .into();
            return;
        }

        if !self.visible {
            return;
        }
        let query = match self.composed_query() {
            Ok(q) => q,
            Err(e) => {
                self.error = e;
                return;
            }
        };
        let filter = files_core::name_filter::Filter::for_search(&query);
        if let Some(e) = filter.error() {
            self.error = e.into();
            return;
        }
        if query.is_empty() {
            self.error = "Escribe un nombre, texto o filtro".into();
            return;
        }
        self.cancel();
        self.order.invalidate();
        self.report = None;
        self.provisional = None;
        self.selected = 0;
        self.scroll = 0;
        self.error.clear();
        self.replace = false;
        self.pending = Some((self.root.clone(), query, self.hidden));
        self.filters_open = false;
        self.field = self.primary();
        self.editing_input = false;
        let field = self.primary();
        let text = self.text(field);
        let len = text.len();
        if field == 5 {
            self.editors[5].select(&self.content, len, len);
        } else {
            self.editors[0].select(&self.query, len, len);
        }
        self.status = "Buscando… · cancelar mantiene la ventana disponible";
    }
    fn bump_results(&mut self) {
        self.result_epoch = self
            .result_epoch
            .checked_add(1)
            .expect("search results epoch exhausted");
    }
    pub fn cancel_visible(&mut self) {
        if self.is_drained() {
            return;
        }
        let valid = self
            .active
            .as_ref()
            .is_none_or(|a| a.revision != self.revision || !a.job.invalidated());
        let mut pages = std::mem::take(&mut self.pages);
        if !valid {
            self.report = None;
            self.marks = Default::default();
            if self.error.is_empty() {
                self.error = "Respuesta de búsqueda inválida; vuelve a buscar".into();
            }
        }
        self.cancel();
        if valid && pages.current.is_some() && self.report.is_some() {
            pages.stopped();
            self.pages = pages;
        }
    }
    pub fn cancel(&mut self) {
        self.pages = Default::default();
        self.bump_results();
        self.date_pending = None;
        self.order.invalidate();
        self.provisional = None;
        self.marks
            .reconcile(self.report.as_ref().map_or(&[], |r| r.hits.as_slice()));
        self.progress = None;
        self.edit_changed();
        self.revision = self
            .revision
            .checked_add(1)
            .expect("search revision exhausted");
        self.pending = None;
        if let Some(active) = &self.active {
            active.job.cancel();
        }
        self.status = "Cancelado · puedes cambiar el filtro y volver a buscar";
    }
    pub fn close(&mut self) {
        self.scope_epoch = self
            .scope_epoch
            .checked_add(1)
            .expect("search root epoch exhausted");
        self.scope_menu = false;
        self.scope_result = None;
        self.scope_history.clear();
        self.marks = Default::default();
        self.background_tab = None;
        self.cancel();
        self.order = Default::default();
        self.visible = false;
        self.calendar_open = false;
        self.geometry = Default::default();
        self.editors = Default::default();
        self.dragging = None;
        self.order.invalidate();
        self.report = None;
        self.provisional = None;
        self.error.clear();
    }
    pub fn hits(&self) -> &[Hit] {
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.revision == self.revision && a.job.invalidated())
        {
            return &[];
        }
        self.report
            .as_ref()
            .map(|r| r.hits.as_slice())
            .or(self.provisional.as_deref())
            .unwrap_or(&[])
    }
    pub fn sync_order(&mut self) {
        let mut order = std::mem::take(&mut self.order);
        order.sync(self.hits());
        self.order = order;
    }
    pub fn cycle_order(&mut self) {
        self.order.cycle();
        self.sync_order();
        self.scroll = 0;
        self.results_focus();
    }
    pub fn results_focus(&mut self) {
        self.editing_input = false;
        self.dragging = None;
        self.edit_changed();
    }
    pub fn select_result(&mut self, index: usize, toggle: bool, extend: bool, add: bool) {
        self.sync_order();
        let mut marks = std::mem::take(&mut self.marks);
        let result = marks.select(
            self.hits(),
            self.order.visible(self.hits()),
            self.selected,
            index,
            (toggle, extend, add),
        );
        self.marks = marks;
        match result {
            Ok(()) => {
                self.selected = index;
                self.background_tab = None;
                self.results_focus();
                self.error.clear();
            }
            Err(e) => self.error = e.into(),
        }
    }
    pub fn mark_all(&mut self) {
        self.background_tab = None;
        self.sync_order();
        let mut marks = std::mem::take(&mut self.marks);
        let result = marks.all(self.hits(), self.order.visible(self.hits()), self.selected);
        self.marks = marks;
        if let Err(e) = result {
            self.error = e.into();
        } else {
            self.error.clear();
        }
    }
    pub fn step_range(&mut self, delta: i32, extend: bool, add: bool) {
        let old = self.selected;
        self.step(delta);
        if extend {
            let next = self.selected;
            self.selected = old;
            self.select_result(next, false, true, add);
        }
    }
    pub fn step(&mut self, delta: i32) {
        self.background_tab = None;
        self.results_focus();
        self.sync_order();
        let indices = self.order.visible(self.hits());
        if !indices.is_empty() {
            let position = indices
                .iter()
                .position(|&i| i == self.selected)
                .unwrap_or(0);
            let next =
                (position as i64 + i64::from(delta)).clamp(0, indices.len() as i64 - 1) as usize;
            self.selected = indices[next];
        }
    }
    pub fn pump(&mut self, allow: bool) -> bool {
        let pages = self.page_mode;
        self.pump_with(allow, |p, q, h| {
            if pages {
                crate::search_pages_job::Job::start(p, q, h).map(|j| Box::new(j) as Box<dyn Task>)
            } else {
                Job::start(p, q, h).map(|j| Box::new(j) as Box<dyn Task>)
            }
        })
    }
    pub fn start_pages(&mut self) {
        let old = self.page_mode;
        let revision = self.revision;
        self.page_mode = true;
        self.start();
        if revision == self.revision {
            self.page_mode = old;
        }
    }
    fn page_changed(&mut self, position: crate::search_page_cache::Position, focus: bool) {
        self.selected = position.selected;
        self.scroll = position.scroll;
        self.marks = Default::default();
        self.order.invalidate();
        self.bump_results();
        self.background_tab = None;
        if focus {
            self.results_focus();
        }
        self.progress = None;
        self.provisional = None;
        self.status = "Página observada · marcas sólo de la página visible";
    }
    pub fn page_step(&mut self, delta: i8, epoch: u64) {
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.revision == self.revision && a.job.invalidated())
        {
            return;
        }
        if epoch != self.result_epoch || !self.visible || self.calendar_open {
            return;
        }
        let position = crate::search_page_cache::Position {
            selected: self.selected,
            scroll: self.scroll,
        };
        if let Some(position) = self.pages.seek(delta, &mut self.report, position) {
            self.page_changed(position, true);
            return;
        }
        if delta > 0 && self.pages.can_request() {
            self.pages.reserve(&self.report);
            if let Some(active) = &self.active
                && active.revision == self.revision
                && active.job.next_page(self.pages.current.unwrap().sequence)
            {
                self.pages.requested();
                self.results_focus();
                self.status = "Página siguiente solicitada · espera ACK para reemplazar";
            }
        }
    }
    fn admit_page(&mut self, page: files_core::search_page::Page) {
        let position = crate::search_page_cache::Position {
            selected: self.selected,
            scroll: self.scroll,
        };
        match self.pages.admit(page, &mut self.report, position) {
            Ok(Some(position)) => self.page_changed(position, false),
            Ok(None) => self.status = "Recorrido actualizado · página visible conservada",
            Err(error) => {
                self.error = error;
                self.report = None;
                self.pages = Default::default();
                self.marks = Default::default();
                if let Some(active) = &self.active {
                    active.job.cancel();
                }
            }
        }
    }
    fn pump_with(
        &mut self,
        allow: bool,
        mut start: impl FnMut(PathBuf, String, bool) -> Result<Box<dyn Task>, String>,
    ) -> bool {
        if let Some(result) = self.active.as_ref().and_then(|a| a.job.finish_page()) {
            let active = self.active.take().unwrap();
            if self.visible && active.revision == self.revision {
                match result {
                    Ok(page) => self.admit_page(page),
                    Err(error) => {
                        if self.error.is_empty() {
                            self.error = error;
                        }
                        self.report = None;
                        self.pages = Default::default();
                        self.marks = Default::default();
                        self.bump_results();
                        self.status = "Búsqueda paginada detenida";
                    }
                }
            }
            return true; // A page was not an ACK; only this result means child reap.
        }
        if let Some(active) = &self.active
            && self.visible
            && active.revision == self.revision
            && !active.job.invalidated()
            && let Some(page) = active.job.page()
        {
            self.admit_page(page);
            return true;
        }
        if let Some(result) = self.active.as_ref().and_then(|a| a.job.poll()) {
            let active = self.active.take().unwrap();
            if self.visible && active.revision == self.revision {
                let provisional = self.provisional.take();
                match result {
                    Ok(report) => {
                        if let Some(hit) = provisional
                            .as_ref()
                            .and_then(|hits| hits.get(self.selected))
                            && let Some(index) =
                                report.hits.iter().position(|h| h.relative == hit.relative)
                        {
                            self.selected = index;
                        }
                        self.progress = None;
                        self.status = if report.content.is_some() {
                            "Coincidencias UTF8 literales; líneas observadas"
                        } else {
                            "Resultados de nombres; no se leyó contenido"
                        };
                        self.order.invalidate();
                        self.bump_results();
                        self.marks.reconcile(&report.hits);
                        self.report = Some(report);
                    }
                    Err(e) => {
                        self.marks.reconcile(&[]);
                        self.error = e;
                        self.status = "Búsqueda detenida";
                    }
                }
            }
            return true; // No replacement on the ACK pump.
        }
        if let Some(active) = &self.active
            && self.visible
            && active.revision == self.revision
            && active.job.invalidated()
        {
            let changed = self.provisional.take().is_some()
                | self.progress.take().is_some()
                | self.report.take().is_some();
            self.pages = Default::default();
            if changed {
                self.bump_results();
                self.order.invalidate();
            }
            self.marks.reconcile(&[]);
            self.status = "Búsqueda inválida; esperando cierre del helper";
            return changed;
        }
        if let Some(active) = &self.active
            && self.visible
            && active.revision == self.revision
            && let Some(candidates) = active.job.candidates()
        {
            self.order.invalidate();
            self.bump_results();
            self.marks.reconcile(&candidates);
            self.provisional = Some(candidates);
            self.status = "Coincidencias provisionales; recorrido en curso";
            return true;
        }
        if let Some(active) = &self.active
            && self.visible
            && active.revision == self.revision
            && let Some(progress) = active.job.progress()
            && self.progress != Some(progress)
        {
            self.progress = Some(progress);
            return true;
        }
        if allow
            && self.active.is_none()
            && let Some((p, q, h)) = self.pending.take()
        {
            match start(p, q, h) {
                Ok(job) => {
                    self.active = Some(Active {
                        revision: self.revision,
                        job,
                    })
                }
                Err(e) => self.error = e,
            }
            return true;
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Fake {
        cancel: Rc<Cell<bool>>,
        reply: Rc<Cell<bool>>,
    }
    impl Task for Fake {
        fn cancel(&self) {
            self.cancel.set(true);
        }
        fn poll(&self) -> Option<Result<Report, String>> {
            self.reply.replace(false).then(|| {
                Ok(Report {
                    complete: true,
                    ..Default::default()
                })
            })
        }
    }
    #[test]
    fn explicit_regex_switch_preserves_raw_text_revokes_receipts_and_roundtrips_criteria() {
        let mut state = State::default();
        state.open("/fixture".into(), "ext:txt".into(), true);
        state.content = r#"(?m)^a\.[0-9]+"q"$"#.into();
        state.focus(5);
        let text = state.content.clone();
        let epoch = state.edit_epoch;
        let literal = state.saved_criteria().unwrap();
        assert!(!literal.content_regex);
        state.toggle_regex();
        assert!(state.content_regex && state.content_mode && state.edit_epoch > epoch);
        assert_eq!(state.content, text);
        assert_eq!(state.query, "ext:txt");
        assert!(!state.running() && state.pending.is_none());
        let composed = state.composed_query().unwrap();
        let filter = files_core::name_filter::Filter::for_search(&composed);
        assert_eq!(filter.content(), Some(text.as_str()));
        assert!(filter.regex().is_some());
        let regex = state.saved_criteria().unwrap();
        assert_ne!(regex, literal);
        let mut restored = State::default();
        restored.load_saved_criteria(&regex, false).unwrap();
        assert_eq!(restored.saved_criteria().unwrap(), regex);
        assert!(!restored.running() && restored.pending.is_none());
        restored.toggle_regex();
        assert_eq!(restored.content, text);
        assert!(!restored.content_regex);
        assert!(
            files_core::name_filter::Filter::for_search(&restored.composed_query().unwrap())
                .regex()
                .is_none()
        );
    }
    #[test]
    fn editing_content_cancels_and_replacement_waits_for_old_ack() {
        let cancelled = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        let mut s = State::default();
        s.open("/fixture".into(), "ext:txt".into(), false);
        s.toggle_content();
        s.input("old");
        s.start();
        s.pump_with(true, |_, q, _| {
            assert!(q.contains("old"));
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.focus(5);
        s.input("new");
        s.start();
        assert!(cancelled.get());
        s.pump_with(true, |_, _, _| panic!("before ACK"));
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("on ACK pump"));
        assert!(s.report.is_none());
        s.pump_with(true, |_, q, _| {
            assert!(q.contains("new") && !q.contains("old"));
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.close();
        assert!(!s.is_drained());
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("after close"));
        assert!(s.is_drained());
    }
    #[test]
    fn mouse_content_field_keeps_name_fields_cursor_undo_and_preflight() {
        let mut s = State::default();
        s.open("/fixture".into(), "ext:rs".into(), false);
        s.fields[0] = "1B".into();
        s.toggle_content();
        s.start();
        assert!(s.pending.is_none() && s.error.contains("texto literal"));
        assert_eq!(s.field, 5);
        assert_eq!(s.query, "ext:rs");
        s.input("Hola \"mundo\" \\");
        let q = s.composed_query().unwrap();
        let f = files_core::name_filter::Filter::for_search(&q);
        assert!(f.error().is_none());
        assert_eq!(f.content(), Some("Hola \"mundo\" \\"));
        s.start();
        assert_eq!(s.pending.as_ref().unwrap().1, q);
        assert_eq!(s.field, 5);
        s.toggle_content();
        assert_eq!(s.field, 0);
        assert_eq!(s.query, "ext:rs");
        assert!(!s.content.is_empty());
        s.cancel();
        s.toggle_content();
        s.input(&"x".repeat(1025));
        assert!(s.error.contains("1024"));
        s.content.clear();
        s.editors = Default::default();
        s.replace = false;
        s.input("abc");
        s.undo(false);
        assert!(s.content.is_empty());
    }
    #[test]
    fn rejected_query_paste_keeps_selection_epoch_capacity_and_undo_history() {
        use crate::line_edit::Motion;
        let mut s = State::default();
        s.open("/fixture".into(), "draft".into(), false);
        s.input("keep");
        s.motion(Motion::Start, false);
        s.motion(Motion::Right, true);
        let epoch = s.input_revision();
        let cap = s.query.capacity();
        s.input(&"x".repeat(1024 * 1024));
        assert_eq!(s.query, "keep");
        assert_eq!(s.selected_text(), "k");
        assert_eq!(s.query.capacity(), cap);
        assert_eq!(s.input_revision(), epoch);
        s.undo(false);
        assert_eq!(s.query, "draft");
    }
    #[test]
    fn scalar_middle_edit_selection_undo_and_field_limits_preserve_drafts() {
        use crate::line_edit::Motion;
        let mut s = State::default();
        s.open("/fixture".into(), "日本 old.rs".into(), false);
        s.motion(Motion::Start, false);
        s.motion(Motion::Right, false);
        s.motion(Motion::Right, false);
        s.input("語");
        assert_eq!(s.query, "日本語 old.rs");
        s.motion(Motion::Right, true);
        assert_eq!(s.selected_text(), " ");
        s.input("_");
        assert_eq!(s.query, "日本語_old.rs");
        s.undo(false);
        assert_eq!(s.query, "日本語 old.rs");
        s.undo(true);
        assert_eq!(s.query, "日本語_old.rs");
        s.delete(true, true);
        assert_eq!(s.query, "old.rs");
        s.undo(false);
        assert_eq!(s.query, "日本語_old.rs");
        s.toggle_filters();
        s.input("12MiB");
        s.motion(Motion::Start, false);
        s.delete(false, false);
        s.input("2");
        assert_eq!(s.fields[0], "22MiB");
        s.focus(2);
        s.input("3MiB");
        s.undo(false);
        assert_eq!(s.fields[1], "");
        s.focus(1);
        s.motion(Motion::End, false);
        let epoch = s.input_revision();
        let cursor = (s.editors[1].caret, s.editors[1].anchor);
        s.input(&"x".repeat(1024 * 1024));
        assert_eq!(s.fields[0], "22MiB");
        assert_eq!(s.input_revision(), epoch);
        assert_eq!((s.editors[1].caret, s.editors[1].anchor), cursor);
        s.undo(false);
        assert_eq!(s.fields[0], "2MiB");
    }
    #[test]
    fn cursor_motion_preserves_live_job_and_noop_delete_keeps_final_report() {
        use crate::line_edit::Motion;
        let cancelled = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        let mut s = State::default();
        s.open("/fixture".into(), "name".into(), false);
        s.start();
        s.pump_with(true, |_, _, _| {
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.motion(Motion::Start, false);
        s.motion(Motion::Right, true);
        assert!(!cancelled.get() && s.searching_current());
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("extra child"));
        assert!(s.report.is_some());
        s.motion(Motion::Start, false);
        s.delete(true, false);
        assert!(
            s.report.is_some(),
            "delete before first scalar did not edit"
        );
        s.input("x");
        assert!(s.report.is_none());
    }
    #[test]
    fn provisional_counters_never_survive_cancel_or_publish_before_final_ack() {
        struct Tracked {
            inner: Fake,
            stats: Rc<Cell<Option<Progress>>>,
        }
        impl Task for Tracked {
            fn cancel(&self) {
                self.inner.cancel();
            }
            fn poll(&self) -> Option<Result<Report, String>> {
                self.inner.poll()
            }
            fn progress(&self) -> Option<Progress> {
                self.stats.get()
            }
        }
        let cancelled = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        let stats = Rc::new(Cell::new(None));
        let mut s = State::default();
        s.open("/fixture".into(), "first".into(), false);
        s.start();
        s.pump_with(true, |_, _, _| {
            Ok(Box::new(Tracked {
                inner: Fake {
                    cancel: cancelled.clone(),
                    reply: reply.clone(),
                },
                stats: stats.clone(),
            }))
        });
        let p = Progress {
            visited: 128,
            ..Default::default()
        };
        stats.set(Some(p));
        assert!(s.pump_with(false, |_, _, _| panic!()));
        assert_eq!(s.progress, Some(p));
        assert!(s.report.is_none());
        s.focus(0);
        assert!(!s.pump_with(false, |_, _, _| panic!()));
        assert_eq!(s.progress, Some(p));
        s.cancel();
        assert!(cancelled.get());
        assert!(s.progress.is_none() && !s.searching_current() && s.running());
        stats.set(Some(Progress {
            visited: 999,
            ..Default::default()
        }));
        assert!(!s.pump_with(false, |_, _, _| panic!()));
        assert!(s.progress.is_none());
        s.input("next");
        s.start();
        assert!(!s.pump_with(true, |_, _, _| panic!("replacement before ACK")));
        reply.set(true);
        assert!(s.pump_with(true, |_, _, _| panic!("replacement on ACK pump")));
        assert!(s.progress.is_none() && s.report.is_none());
        s.pump_with(true, |_, q, _| {
            assert_eq!(q, "next");
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.close();
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("spawn after close"));
        assert!(s.is_drained() && s.progress.is_none() && s.report.is_none());
    }
    #[test]
    fn guided_fields_compose_before_unfinished_name_quote_and_keep_advanced_terms() {
        let mut s = State::default();
        s.open("/fixture".into(), "\"report".into(), false);
        s.fields = [
            "1KiB".into(),
            "2MiB".into(),
            "2026-01-01".into(),
            "2027-01-01".into(),
        ];
        let query = s.composed_query().unwrap();
        assert_eq!(
            query,
            "size:>=1KiB size:<=2MiB after:2026-01-01 before:2027-01-01 \"report"
        );
        let filter = files_core::name_filter::Filter::for_search(&query);
        assert!(filter.error().is_none() && filter.needs_metadata());
        s.start();
        assert_eq!(s.pending.as_ref().unwrap().1, query);
        assert_eq!(s.query, "\"report");
        assert!(!s.filters_open);
        s.clear_filters();
        assert_eq!(s.composed_query().unwrap(), "\"report");
    }
    #[test]
    fn invalid_or_oversized_composition_never_admits_helper_and_retains_draft() {
        let mut s = State::default();
        s.open("/fixture".into(), "keep".into(), false);
        for fields in [
            ["3MiB", "2KiB", "", ""],
            ["", "", "2026-02-30", ""],
            ["", "", "2027-01-01", "2026-01-01"],
            ["10MiB !name:a", "", "", ""],
        ] {
            s.fields = fields.map(String::from);
            s.start();
            assert!(!s.error.is_empty() && s.pending.is_none());
            s.pump_with(true, |_, _, _| panic!("invalid form admitted"));
        }
        s.fields = ["1B".into(), String::new(), String::new(), String::new()];
        s.query = "x".repeat(4096);
        s.start();
        assert!(s.pending.is_none());
        assert_eq!(s.query.len(), 4096);
        s.toggle_filters();
        let capacity = s.fields[0].capacity();
        s.input(&"x".repeat(1024 * 1024));
        assert_eq!(s.fields[0], "1B");
        assert_eq!(s.fields[0].capacity(), capacity);
        assert!(s.replace);
    }
    #[test]
    fn moving_field_does_not_invalidate_running_search_but_editing_cancels_it() {
        let cancelled = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        let mut s = State::default();
        s.open("/fixture".into(), "name".into(), false);
        s.start();
        s.pump_with(true, |_, _, _| {
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        let epoch = s.input_revision();
        s.toggle_filters();
        s.focus(2);
        assert_ne!(s.input_revision(), epoch);
        assert!(!cancelled.get());
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("extra spawn"));
        assert!(s.report.is_some());
        s.input("20MiB");
        assert!(s.report.is_none());
    }
    #[test]
    fn latest_intent_waits_for_cancel_ack_and_does_not_publish_old_results() {
        let cancelled = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        let mut s = State::default();
        s.open("/fixture".into(), "first".into(), false);
        s.start();
        s.pump_with(true, |_, _, _| {
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.query = "second".into();
        s.start();
        assert!(cancelled.get());
        assert!(!s.is_drained());
        s.pump_with(true, |_, _, _| panic!("replacement before ACK"));
        reply.set(true);
        assert!(s.pump_with(true, |_, _, _| panic!("replacement on ACK")));
        assert!(s.report.is_none());
        s.pump_with(true, |_, q, _| {
            assert_eq!(q, "second");
            Ok(Box::new(Fake {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.close();
        assert!(!s.is_drained());
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("spawn after close"));
        assert!(s.is_drained());
        assert!(s.report.is_none());
    }
    #[test]
    fn blocked_gate_keeps_one_pending_intent_and_input_is_bounded() {
        let mut s = State::default();
        s.open("/fixture".into(), "keep".into(), true);
        s.input(&"x".repeat(4097));
        assert_eq!(s.query, "keep");
        assert!(s.replace);
        s.start();
        s.pump_with(false, |_, _, _| panic!("gate is closed"));
        assert!(!s.is_drained());
        s.cancel();
        assert!(s.is_drained());
    }
    struct Incremental {
        slot: Rc<std::cell::RefCell<Option<Arc<[Hit]>>>>,
        reply: Rc<std::cell::RefCell<Option<Result<Report, String>>>>,
        invalid: Rc<Cell<bool>>,
        cancelled: Rc<Cell<bool>>,
    }
    impl Task for Incremental {
        fn cancel(&self) {
            self.cancelled.set(true);
        }
        fn poll(&self) -> Option<Result<Report, String>> {
            self.reply.borrow_mut().take()
        }
        fn candidates(&self) -> Option<Arc<[Hit]>> {
            self.slot.borrow_mut().take()
        }
        fn invalidated(&self) -> bool {
            self.invalid.get()
        }
    }
    fn named_hit(name: &str) -> Hit {
        Hit {
            relative: name.into(),
            kind: files_core::listing::Kind::File,
            content: None,
            snippet: None,
        }
    }
    #[test]
    fn provisional_selection_follows_raw_path_into_final_sorted_report() {
        let slot = Rc::new(std::cell::RefCell::new(Some(Arc::from(vec![
            named_hit("b"),
            named_hit("a"),
        ]))));
        let reply = Rc::new(std::cell::RefCell::new(None));
        let mut s = State {
            visible: true,
            ..Default::default()
        };
        s.active = Some(Active {
            revision: s.revision,
            job: Box::new(Incremental {
                slot,
                reply: reply.clone(),
                invalid: Rc::new(Cell::new(false)),
                cancelled: Rc::new(Cell::new(false)),
            }),
        });
        assert!(s.pump_with(false, |_, _, _| unreachable!()));
        assert!(s.report.is_none() && s.hits().len() == 2);
        s.selected = 0;
        *reply.borrow_mut() = Some(Ok(Report {
            hits: vec![named_hit("a"), named_hit("b")],
            visited: 2,
            complete: true,
            ..Default::default()
        }));
        assert!(s.pump_with(false, |_, _, _| unreachable!()));
        assert!(s.provisional.is_none() && s.report.is_some());
        assert_eq!(s.selected, 1);
    }
    #[test]
    fn invalidation_hides_before_ack_and_edit_cannot_restore_old_candidates() {
        let slot = Rc::new(std::cell::RefCell::new(Some(Arc::from(vec![named_hit(
            "a",
        )]))));
        let reply = Rc::new(std::cell::RefCell::new(None));
        let invalid = Rc::new(Cell::new(false));
        let cancelled = Rc::new(Cell::new(false));
        let mut s = State {
            visible: true,
            ..Default::default()
        };
        s.active = Some(Active {
            revision: s.revision,
            job: Box::new(Incremental {
                slot: slot.clone(),
                reply: reply.clone(),
                invalid: invalid.clone(),
                cancelled: cancelled.clone(),
            }),
        });
        s.pump_with(false, |_, _, _| unreachable!());
        assert_eq!(s.hits().len(), 1);
        invalid.set(true);
        assert!(s.hits().is_empty()); // paint/access guard before next pump
        s.pump_with(false, |_, _, _| unreachable!());
        assert!(s.provisional.is_none() && !s.is_drained());
        invalid.set(false);
        s.input("changed");
        assert!(cancelled.get());
        *slot.borrow_mut() = Some(Arc::from(vec![named_hit("stale")]));
        s.pump_with(false, |_, _, _| unreachable!());
        assert!(s.hits().is_empty());
        *reply.borrow_mut() = Some(Err("old failure".into()));
        s.pump_with(false, |_, _, _| unreachable!());
        assert!(s.is_drained() && s.hits().is_empty() && s.error.is_empty());
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;
    #[test]
    fn order_changes_keep_selected_path_and_keyboard_follows_visual_positions() {
        let mut s = State::default();
        s.open("/owned".into(), "*".into(), false);
        let hit = |p: &str, kind| Hit {
            relative: p.into(),
            kind,
            content: None,
            snippet: None,
        };
        s.report = Some(Report {
            hits: vec![
                hit("z/file10", files_core::listing::Kind::File),
                hit("a/file2", files_core::listing::Kind::File),
                hit("b/folder", files_core::listing::Kind::Directory),
            ],
            ..Default::default()
        });
        s.selected = 0;
        s.cycle_order();
        assert_eq!(
            s.hits()[s.selected].relative,
            std::path::Path::new("z/file10")
        );
        s.step(-1);
        assert_eq!(s.selected, 1);
        s.cycle_order();
        assert_eq!(s.selected, 1);
        s.step(i32::MIN);
        assert_eq!(s.selected, 2);
        s.step(i32::MAX);
        assert_eq!(s.selected, 0);
        s.close();
        assert!(s.order.visible(s.hits()).is_empty());
    }
}

#[cfg(test)]
mod calendar_tests {
    use super::*;
    use files_core::search_dates::{Preset, Range};
    fn reference() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + Duration::from_secs(3)
    }
    fn range(preset: Preset) -> Range {
        let (after, before, start, end) = if preset == Preset::Yesterday {
            ("1969-12-31T00:00:00Z", "1970-01-01T00:00:00Z", -86400, 0)
        } else {
            ("1970-01-01T00:00:00Z", "1970-01-02T00:00:00Z", 0, 86400)
        };
        Range {
            after: after.into(),
            before: before.into(),
            reference: 3,
            start,
            end,
            preset,
        }
    }
    fn open() -> State {
        let mut s = State::default();
        s.open("/calendar-fixture".into(), "ext:txt".into(), false);
        s.toggle_calendar();
        s
    }
    #[test]
    #[cfg(unix)]
    fn elapsed_choice_preserves_non_date_criteria_and_never_starts_search_on_ack() {
        let mut s = open();
        s.fields[0] = "1B".into();
        s.content = "literal".into();
        s.content_mode = true;
        let (tx, rx) = mpsc::sync_channel(1);
        s.request_date_at(Preset::Last24Hours, reference());
        assert!(s.dispatch_date(&tx));
        let crate::aux_io::Request::SearchDate(id, preset, time) = rx.try_recv().unwrap() else {
            panic!()
        };
        let result = files_core::search_dates::resolve(preset, time).unwrap();
        assert!(s.receive_date(id, Ok(result.clone())));
        assert_eq!(s.fields[0], "1B");
        assert_eq!(s.query, "ext:txt");
        assert_eq!(s.content, "literal");
        assert_eq!(&s.fields[2..], [result.after, result.before]);
        assert!(s.pending.is_none() && !s.running());
        let query = s.composed_query().unwrap();
        assert!(query.contains("size:>=1B") && query.contains("content:"));
    }
    #[test]
    fn latest_calendar_choice_waits_old_ack_and_retains_captured_clock() {
        let mut s = open();
        let (tx, rx) = mpsc::sync_channel(1);
        s.request_date_at(Preset::Today, reference());
        assert!(s.dispatch_date(&tx));
        let id = match rx.try_recv().unwrap() {
            crate::aux_io::Request::SearchDate(id, preset, time) => {
                assert_eq!(preset, Preset::Today);
                assert_eq!(time, reference());
                id
            }
            _ => panic!(),
        };
        s.request_date_at(Preset::Yesterday, reference());
        assert!(!s.dispatch_date(&tx));
        assert!(!s.receive_date(id + 99, Ok(range(Preset::Today))));
        assert!(s.date_active.is_some());
        assert!(!s.receive_date(id, Ok(range(Preset::Today))));
        assert!(s.fields[2].is_empty());
        assert!(s.dispatch_date(&tx));
        let next = match rx.try_recv().unwrap() {
            crate::aux_io::Request::SearchDate(id, _, _) => id,
            _ => panic!(),
        };
        assert!(s.receive_date(next, Ok(range(Preset::Yesterday))));
        assert!(!s.calendar_open && s.filters_open);
        assert_eq!(s.query, "ext:txt");
        assert_eq!(s.fields[2], "1969-12-31T00:00:00Z");
        assert_eq!(s.date_range.as_ref().unwrap().reference, 3);
        assert_eq!(
            s.composed_query().unwrap(),
            "after:1969-12-31T00:00:00Z before:1970-01-01T00:00:00Z ext:txt"
        );
        assert!(s.pending.is_none(), "preset must not admit a search helper");
    }
    #[test]
    fn close_reopen_and_edit_revoke_late_calendar_without_faking_ack() {
        let mut s = open();
        let (tx, rx) = mpsc::sync_channel(1);
        s.request_date_at(Preset::Today, reference());
        s.dispatch_date(&tx);
        let id = match rx.try_recv().unwrap() {
            crate::aux_io::Request::SearchDate(id, _, _) => id,
            _ => panic!(),
        };
        s.close();
        assert!(s.date_active.is_some());
        s.open("/other".into(), "name".into(), false);
        assert!(!s.receive_date(id, Ok(range(Preset::Today))));
        assert!(s.date_active.is_none());
        assert!(s.fields[2].is_empty());
        s.toggle_calendar();
        s.request_date_at(Preset::Today, reference());
        s.dispatch_date(&tx);
        let id = match rx.try_recv().unwrap() {
            crate::aux_io::Request::SearchDate(id, _, _) => id,
            _ => panic!(),
        };
        s.focus(0);
        s.input("edited");
        let query = s.query.clone();
        assert!(!s.receive_date(id, Ok(range(Preset::Today))));
        assert_eq!(s.query, query);
        assert!(s.fields[2].is_empty());
    }
    #[test]
    fn malformed_reference_error_or_oversized_composition_preserves_date_fields() {
        let mut s = open();
        let (tx, rx) = mpsc::sync_channel(1);
        s.fields[2] = "1960-01-01".into();
        s.fields[3] = "1980-01-01".into();
        for mode in 0..3 {
            s.calendar_open = true;
            s.request_date_at(Preset::Today, reference());
            s.dispatch_date(&tx);
            let id = match rx.try_recv().unwrap() {
                crate::aux_io::Request::SearchDate(id, _, _) => id,
                _ => panic!(),
            };
            let mut value = range(Preset::Today);
            if mode == 0 {
                value.reference = 4;
            }
            if mode == 2 {
                s.query = "a".repeat(4096);
            }
            let result = if mode == 1 {
                Err("calendar failure".into())
            } else {
                Ok(value)
            };
            assert!(s.receive_date(id, result));
            assert_eq!(s.fields[2], "1960-01-01");
            assert_eq!(s.fields[3], "1980-01-01");
            assert!(!s.error.is_empty());
            assert!(s.pending.is_none());
        }
    }
    #[test]
    fn calendar_reply_preserves_content_editor_and_does_not_redirect_text_into_name() {
        let mut s = open();
        s.content = "raw calendar".into();
        s.content_mode = true;
        s.editors[5].select(&s.content, s.content.len(), s.content.len());
        let (tx, rx) = mpsc::sync_channel(1);
        s.request_date_at(Preset::Today, reference());
        s.dispatch_date(&tx);
        let id = match rx.try_recv().unwrap() {
            crate::aux_io::Request::SearchDate(id, _, _) => id,
            _ => panic!(),
        };
        s.receive_date(id, Ok(range(Preset::Today)));
        assert_eq!(s.field, 5);
        s.replace = false;
        s.input(" bytes");
        assert_eq!(s.content, "raw calendar bytes");
        assert_eq!(s.query, "ext:txt");
        assert!(
            s.composed_query()
                .unwrap()
                .contains("content:\"raw calendar bytes\"")
        );
    }
    #[test]
    fn mixed_utc_date_timestamp_bounds_compare_instants_and_pending_preset_blocks_start() {
        let mut s = open();
        s.fields[2] = "1970-01-01".into();
        s.fields[3] = "1970-01-01T00:00:00Z".into();
        assert!(s.composed_query().is_err());
        s.fields[3] = "1970-01-01T00:00:01Z".into();
        assert!(s.composed_query().is_ok());
        s.request_date_at(Preset::Today, reference());
        s.start();
        assert!(s.pending.is_none());
        assert!(s.error.contains("Espera"));
        s.toggle_calendar();
        assert!(!s.date_busy());
        s.start();
        assert!(s.pending.is_some());
    }
    #[test]
    fn saved_load_cancels_old_search_and_never_admits_a_new_helper_until_enter() {
        use std::{cell::Cell, rc::Rc};
        struct Held {
            cancel: Rc<Cell<bool>>,
            reply: Rc<Cell<bool>>,
        }
        impl Task for Held {
            fn cancel(&self) {
                self.cancel.set(true);
            }
            fn poll(&self) -> Option<Result<Report, String>> {
                self.reply.replace(false).then(|| {
                    Ok(Report {
                        complete: true,
                        ..Default::default()
                    })
                })
            }
        }
        let cancelled = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        let mut s = State::default();
        s.open("/saved".into(), "ext:rs".into(), true);
        s.content = "literal ñ".into();
        s.fields = [
            "1B".into(),
            "20MiB".into(),
            "2026-01-01".into(),
            "2027-01-01".into(),
        ];
        s.content_mode = true;
        s.order.order = crate::search_order::Order::Kind;
        let c = s.saved_criteria().unwrap();
        s.start();
        s.pump_with(true, |_, _, _| {
            Ok(Box::new(Held {
                cancel: cancelled.clone(),
                reply: reply.clone(),
            }))
        });
        s.load_saved_criteria(&c, false).unwrap();
        assert!(cancelled.get() && s.running() && !s.searching_current());
        s.pump_with(true, |_, _, _| panic!("loading spawned a helper"));
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("late ACK spawned a helper"));
        assert!(!s.running() && s.hits().is_empty() && s.report.is_none());
        assert_eq!(s.fields, c.fields);
        assert_eq!(s.content, c.content);
        assert_eq!(s.order.order, crate::search_order::Order::Kind);
    }
    #[test]
    fn invalid_save_and_oversize_load_fail_before_cloning_or_replacing_draft() {
        let mut s = State::default();
        s.open("/original".into(), "ext:rs".into(), false);
        let mut c = s.saved_criteria().unwrap();
        c.query = " ".repeat(4097);
        assert!(s.load_saved_criteria(&c, false).is_err());
        assert_eq!(s.query, "ext:rs");
        s.query = " ".repeat(100000);
        assert!(s.saved_criteria().is_err());
        s.query = "before:no-date".into();
        assert!(s.saved_criteria().is_err());
        s.query = "".into();
        assert!(s.saved_criteria().is_err());
        s.query = "ext:rs".into();
        s.toggle_calendar();
        s.request_date(files_core::search_dates::Preset::Today);
        assert!(s.saved_criteria().is_err());
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;
    use files_core::search_page::Page;
    use std::cell::RefCell;
    struct Paged {
        ready: std::rc::Rc<RefCell<Option<Page>>>,
        done: std::rc::Rc<RefCell<Option<Result<Page, String>>>>,
        cancelled: Arc<AtomicBool>,
        invalid: Arc<AtomicBool>,
        credits: Arc<Mutex<Vec<u64>>>,
    }
    impl Task for Paged {
        fn invalidated(&self) -> bool {
            self.invalid.load(Ordering::Acquire)
        }
        fn page(&self) -> Option<Page> {
            self.ready.borrow_mut().take()
        }
        fn finish_page(&self) -> Option<Result<Page, String>> {
            self.done.borrow_mut().take()
        }
        fn next_page(&self, sequence: u64) -> bool {
            self.credits.lock().unwrap().push(sequence);
            true
        }
        fn cancel(&self) {
            self.cancelled.store(true, Ordering::Release);
        }
        fn poll(&self) -> Option<Result<Report, String>> {
            None
        }
    }
    fn page(sequence: u64, more: bool) -> Page {
        Page {
            version: 1,
            id: 1,
            sequence,
            found_total: (sequence + 1) * 10,
            more,
            report: Report {
                hits: (0..10)
                    .map(|i| Hit {
                        relative: format!("{sequence}-{i}.txt").into(),
                        kind: files_core::listing::Kind::File,
                        content: None,
                        snippet: None,
                    })
                    .collect(),
                visited: (sequence + 1) * 10,
                complete: !more,
                reason: more.then(|| "page".into()),
                ..Default::default()
            },
        }
    }
    fn pump(s: &mut State) {
        s.pump_with(true, |_, _, _| panic!("replacement before drain"));
    }
    #[test]
    fn pages_preserve_active_job_until_reap_clear_marks_on_switch_and_reject_old_keys() {
        let mut s = State::default();
        s.open("/fixture".into(), "ext:txt".into(), false);
        s.page_mode = true;
        let ready = std::rc::Rc::new(RefCell::new(Some(page(0, true))));
        let done = std::rc::Rc::new(RefCell::new(None));
        let credits = Arc::new(Mutex::new(Vec::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
        s.active = Some(Active {
            revision: s.revision,
            job: Box::new(Paged {
                ready: ready.clone(),
                done: done.clone(),
                credits: credits.clone(),
                cancelled: cancelled.clone(),
                invalid: Arc::new(AtomicBool::new(false)),
            }),
        });
        pump(&mut s);
        assert_eq!(s.hits().len(), 10);
        assert!(!s.is_drained());
        s.selected = 2;
        s.scroll = 1;
        s.mark_all();
        assert_eq!(s.marks.len(), 10);
        let old = s.result_epoch;
        s.page_step(1, old);
        assert_eq!(*credits.lock().unwrap(), [0]);
        assert!(s.pages.loading);
        assert_eq!(s.marks.len(), 10);
        s.page_step(1, old);
        assert_eq!(credits.lock().unwrap().len(), 1);
        s.focus(5);
        let edit = s.input_revision();
        *ready.borrow_mut() = Some(page(1, true));
        pump(&mut s);
        assert!(
            s.editing_input && s.field == 5 && s.input_revision() == edit,
            "async page must not steal editor focus/paste epoch"
        );
        assert_eq!(s.pages.current.unwrap().sequence, 1);
        assert_eq!(s.marks.len(), 0);
        s.page_step(1, old);
        assert_eq!(credits.lock().unwrap().len(), 1);
        s.page_step(1, s.result_epoch);
        assert_eq!(*credits.lock().unwrap(), [0, 1]);
        s.page_step(-1, s.result_epoch);
        assert_eq!((s.selected, s.scroll), (2, 1));
        let mut terminal = page(2, false);
        terminal.found_total = 20;
        terminal.report.hits.clear();
        *done.borrow_mut() = Some(Ok(terminal));
        pump(&mut s);
        assert!(s.is_drained());
        assert_eq!(s.pages.current.unwrap().sequence, 0);
        assert!(s.pages.ended);
        s.page_step(1, s.result_epoch);
        assert_eq!(s.pages.current.unwrap().sequence, 1);
        assert_eq!(s.hits().len(), 10);
        assert!(s.report.as_ref().unwrap().complete);
        assert!(!cancelled.load(Ordering::Acquire));
    }
    #[test]
    fn paged_failure_hides_old_targets_and_close_never_fabricates_reap() {
        let mut s = State::default();
        s.open("/fixture".into(), "ext:txt".into(), false);
        s.page_mode = true;
        let ready = std::rc::Rc::new(RefCell::new(Some(page(0, true))));
        let done = std::rc::Rc::new(RefCell::new(None));
        let invalid = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        s.active = Some(Active {
            revision: s.revision,
            job: Box::new(Paged {
                ready,
                done: done.clone(),
                invalid: invalid.clone(),
                cancelled: cancel.clone(),
                credits: Default::default(),
            }),
        });
        pump(&mut s);
        invalid.store(true, Ordering::Release);
        assert!(s.hits().is_empty());
        pump(&mut s);
        *done.borrow_mut() = Some(Err("bad frame".into()));
        pump(&mut s);
        assert!(s.report.is_none() && s.is_drained() && s.pages.current.is_none());
        let ready = std::rc::Rc::new(RefCell::new(Some(page(0, true))));
        let done = std::rc::Rc::new(RefCell::new(None));
        s.active = Some(Active {
            revision: s.revision,
            job: Box::new(Paged {
                ready,
                done: done.clone(),
                invalid: Arc::new(AtomicBool::new(false)),
                cancelled: cancel.clone(),
                credits: Default::default(),
            }),
        });
        pump(&mut s);
        s.close();
        assert!(cancel.load(Ordering::Acquire) && !s.is_drained() && s.hits().is_empty());
        pump(&mut s);
        assert!(!s.is_drained());
        *done.borrow_mut() = Some(Err("cancelled".into()));
        pump(&mut s);
        assert!(s.is_drained() && s.error.is_empty());
    }
    #[test]
    fn user_stop_keeps_only_valid_observations_and_cached_navigation_without_fake_ack() {
        for invalid in [false, true] {
            let mut s = State::default();
            s.open("/fixture".into(), "ext:txt".into(), false);
            s.page_mode = true;
            let ready = std::rc::Rc::new(RefCell::new(Some(page(0, true))));
            let done = std::rc::Rc::new(RefCell::new(None));
            let failed = Arc::new(AtomicBool::new(false));
            let cancel = Arc::new(AtomicBool::new(false));
            let credits = Arc::new(Mutex::new(Vec::new()));
            s.active = Some(Active {
                revision: s.revision,
                job: Box::new(Paged {
                    ready: ready.clone(),
                    done: done.clone(),
                    invalid: failed.clone(),
                    cancelled: cancel.clone(),
                    credits: credits.clone(),
                }),
            });
            pump(&mut s);
            if !invalid {
                s.page_step(1, s.result_epoch);
                *ready.borrow_mut() = Some(page(1, true));
                pump(&mut s);
                s.page_step(-1, s.result_epoch);
            }
            failed.store(invalid, Ordering::Release);
            s.cancel_visible();
            assert!(cancel.load(Ordering::Acquire) && !s.is_drained());
            assert_eq!(s.report.is_some(), !invalid);
            assert!(!s.pages.can_request());
            if !invalid {
                assert!(s.pages.cancelled && s.pages.current.is_some_and(|p| p.sequence == 0));
                s.page_step(1, s.result_epoch);
                assert!(s.pages.current.is_some_and(|p| p.sequence == 1));
                s.page_step(1, s.result_epoch);
                assert_eq!(*credits.lock().unwrap(), [0]);
            }
            *done.borrow_mut() = Some(Err("cancelled".into()));
            pump(&mut s);
            assert!(s.is_drained());
            assert_eq!(s.report.is_some(), !invalid);
            assert_eq!(s.error.is_empty(), !invalid);
        }
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Fake {
        cancel: Rc<Cell<bool>>,
        reply: Rc<Cell<bool>>,
    }
    impl Task for Fake {
        fn cancel(&self) {
            self.cancel.set(true);
        }
        fn poll(&self) -> Option<Result<Report, String>> {
            self.reply.replace(false).then(|| {
                Ok(Report {
                    complete: true,
                    ..Default::default()
                })
            })
        }
    }
    #[test]
    fn scope_change_preserves_criteria_but_revokes_results_marks_and_stale_controls() {
        let mut s = State::default();
        s.open("/root".into(), "needle".into(), true);
        s.fields[0] = "2KiB".into();
        s.content = "exact".into();
        s.content_mode = true;
        s.order.order = crate::search_order::Order::Kind;
        s.report = Some(Report {
            hits: vec![Hit {
                relative: "child/item".into(),
                kind: files_core::listing::Kind::File,
                content: None,
                snippet: None,
            }],
            ..Default::default()
        });
        s.mark_all();
        let old = s.result_epoch;
        s.change_scope(Scope::Result(0), old);
        assert_eq!(s.root, PathBuf::from("/root/child"));
        assert!(s.hits().is_empty() && s.marks.paths().next().is_none());
        assert_eq!(
            (&s.query, &s.content, s.hidden),
            (&"needle".to_string(), &"exact".to_string(), true)
        );
        assert_eq!(s.fields[0], "2KiB");
        assert_eq!(s.order.order, crate::search_order::Order::Kind);
        assert!(s.pending.is_none() && s.is_drained());
        s.change_scope(Scope::Parent, old);
        assert_eq!(s.root, PathBuf::from("/root/child"));
        s.change_scope(Scope::Back, s.scope_stamp(Scope::Back));
        assert_eq!(s.root, PathBuf::from("/root"));
        assert!(s.pending.is_none());
    }
    #[test]
    fn scope_change_waits_old_ack_without_accepting_late_report_or_spawning_second_task() {
        let mut s = State::default();
        s.open("/root/child".into(), "needle".into(), false);
        let cancel = Rc::new(Cell::new(false));
        let reply = Rc::new(Cell::new(false));
        s.start();
        s.pump_with(true, |_, _, _| {
            Ok(Box::new(Fake {
                cancel: cancel.clone(),
                reply: reply.clone(),
            }))
        });
        s.change_scope(Scope::Parent, s.scope_stamp(Scope::Parent));
        assert!(cancel.get() && !s.is_drained());
        s.start();
        s.pump_with(true, |_, _, _| panic!("old task not reaped"));
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("ACK pump must not spawn"));
        assert!(s.report.is_none());
        s.pump_with(true, |path, q, _| {
            assert_eq!(path, PathBuf::from("/root"));
            assert_eq!(q, "needle");
            Ok(Box::new(Fake {
                cancel: cancel.clone(),
                reply: reply.clone(),
            }))
        });
        s.close();
        reply.set(true);
        s.pump_with(true, |_, _, _| panic!("closed"));
        assert!(s.is_drained());
    }
    #[test]
    fn scope_history_is_bounded_and_close_reopen_does_not_reuse_it() {
        let mut s = State::default();
        s.open("/a/b/c/d/e/f/g/h/i/j/k/l".into(), "x".into(), false);
        for _ in 0..11 {
            s.change_scope(Scope::Parent, s.scope_stamp(Scope::Parent));
        }
        assert_eq!(s.scope_history.len(), 8);
        assert!(s.scope_history.iter().map(|p| p.capacity()).sum::<usize>() <= 32 * 1024);
        s.close();
        s.open("/other".into(), "x".into(), false);
        assert!(s.scope_target(Scope::Back).is_none());
        s.root = "/".into();
        assert!(s.scope_target(Scope::Parent).is_none());
    }
    #[cfg(unix)]
    #[test]
    fn scope_paths_are_raw_and_unsafe_or_oversized_results_never_change_root() {
        use std::os::unix::ffi::OsStringExt;
        let mut s = State::default();
        s.open("/root".into(), "x".into(), false);
        let raw = PathBuf::from(std::ffi::OsString::from_vec(b"dir-\xff/file".to_vec()));
        s.report = Some(Report {
            hits: vec![Hit {
                relative: raw,
                kind: files_core::listing::Kind::File,
                content: None,
                snippet: None,
            }],
            ..Default::default()
        });
        s.change_scope(Scope::Result(0), s.result_epoch);
        assert_eq!(s.root.as_os_str().as_encoded_bytes(), b"/root/dir-\xff");
        s.report = Some(Report {
            hits: vec![Hit {
                relative: "../escape".into(),
                kind: files_core::listing::Kind::Directory,
                content: None,
                snippet: None,
            }],
            ..Default::default()
        });
        assert!(s.scope_target(Scope::Result(0)).is_none());
        s.report.as_mut().unwrap().hits[0].relative = "z".repeat(4096).into();
        assert!(s.scope_target(Scope::Result(0)).is_none());
    }
    #[test]
    fn scope_menu_result_is_revoked_when_results_change() {
        let mut s = State::default();
        s.open("/root".into(), "x".into(), false);
        s.scope_result = Some((0, s.result_epoch));
        assert_eq!(s.scope_result_index(), Some(0));
        s.bump_results();
        assert!(s.scope_result_index().is_none());
    }
    #[test]
    fn parent_scope_remains_responsive_when_only_results_refresh() {
        let mut s = State::default();
        s.open("/root/child".into(), "x".into(), false);
        let stamp = s.scope_stamp(Scope::Parent);
        s.bump_results();
        s.change_scope(Scope::Parent, stamp);
        assert_eq!(s.root, PathBuf::from("/root"));
    }
    #[test]
    fn pending_calendar_choice_is_not_silently_discarded_by_scope_change() {
        let mut s = State::default();
        s.open("/root/child".into(), "x".into(), false);
        s.date_pending = Some(DateRequest {
            id: 1,
            epoch: s.edit_epoch,
            preset: files_core::search_dates::Preset::Today,
            reference: std::time::SystemTime::UNIX_EPOCH,
        });
        s.change_scope(Scope::Parent, s.scope_stamp(Scope::Parent));
        assert_eq!(s.root, PathBuf::from("/root/child"));
        assert!(s.date_pending.is_some() && s.error.contains("fecha pendiente"));
    }
}
