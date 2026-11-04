//! Explicit saved-criteria catalogue. Storage ACK never applies a search draft.
use crate::{
    line_edit::{Cursor, Motion},
    line_field::Geometry,
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::saved_search::{self, Criteria, Entry, Op};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
use std::sync::atomic::{AtomicU64, Ordering};
fn epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut n = NEXT.load(Ordering::Relaxed);
    loop {
        let next = n.checked_add(1).expect("saved search epoch exhausted");
        match NEXT.compare_exchange_weak(n, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(v) => return v,
            Err(v) => n = v,
        }
    }
}
#[derive(Clone, Debug)]
pub enum Command {
    Close,
    Select(u64),
    Load(bool),
    New,
    Edit,
    Remove,
    Commit,
    Cancel,
    Reload,
    Discard,
    Details,
}
pub fn name(c: &Command) -> &'static str {
    match c {
        Command::Close => "Cerrar sin cambiar criterios, Esc",
        Command::Select(_) => "Seleccionar búsqueda guardada",
        Command::Load(false) => "Cargar criterios y raíz guardados; Enter, no ejecuta",
        Command::Load(true) => {
            "Cargar criterios en la raíz de búsqueda actual; Alt+Enter, no ejecuta"
        }
        Command::New => "Guardar captura de los criterios actuales, Ctrl+S",
        Command::Edit => "Editar etiqueta de la búsqueda, F2",
        Command::Remove => "Quitar búsqueda guardada, no borra archivos; Supr",
        Command::Commit => "Guardar etiqueta, Enter",
        Command::Cancel => "Cancelar etiqueta, Esc",
        Command::Reload => "Recargar o reintentar guardado, F5",
        Command::Discard => "Omitir intento pendiente; no revierte publicado, F8",
        Command::Details => "Leer diagnóstico, F1",
    }
}
enum Editing {
    New(Criteria),
    Rename(Entry),
}
pub struct Panel {
    pub stamp: u64,
    pub edit_epoch: u64,
    owner: (u64, u64),
    intent: u64,
    root: std::path::PathBuf,
    selected: Option<u64>,
    selection_request: Option<u64>,
    editing: Option<Editing>,
    label: String,
    cursor: Cursor,
    geometry: Geometry,
    top: usize,
    rows: usize,
    reveal: bool,
    dragging: bool,
    error: String,
}
impl Panel {
    pub fn acknowledged(&mut self, id: u64, op: &Op, doc: &saved_search::Document) {
        if self.selection_request != Some(id) {
            return;
        }
        self.selection_request = None;
        let selected = match op {
            Op::Add { criteria, .. } => doc
                .rows
                .iter()
                .find(|e| e.criteria == *criteria)
                .map(|e| e.id),
            Op::Rename { expected, .. } => {
                doc.rows.iter().find(|e| e.id == expected.id).map(|e| e.id)
            }
            _ => None,
        };
        if let Some(id) = selected {
            self.selected = Some(id);
            self.reveal = true;
        }
    }
    pub fn editing(&self) -> bool {
        self.editing.is_some()
    }
    pub fn invalidate(&mut self) {
        self.stamp = epoch();
        self.edit_epoch = epoch();
        self.dragging = false;
    }
    pub(crate) fn current(&self, m: &Model) -> bool {
        m.search.visible
            && self.owner == m.pane_owner()
            && self.intent == m.search.input_revision()
            && self.root == m.search.root
    }
    fn input(&mut self, s: &str) {
        if !self.editing() {
            return;
        }
        if let Err(e) =
            self.cursor
                .insert_bounded(&mut self.label, s, false, saved_search::MAX_LABEL)
        {
            self.error = e.into()
        } else {
            self.error.clear();
            self.edit_epoch = epoch()
        }
    }
    fn new_edit(&mut self, c: Criteria) {
        self.editing = Some(Editing::New(c));
        self.label = "Nueva búsqueda".into();
        self.cursor.all(&self.label);
        self.invalidate();
    }
}
impl Model {
    pub fn open_saved_searches(&mut self, save: bool) {
        if self.search_transfer.busy() {
            self.search.error = "Espera la captura de orígenes antes de abrir guardadas".into();
            return;
        }
        if self.closing
            || !self.search.visible
            || self.copy_form.is_some()
            || self.notice_open
            || self.commands.is_some()
            || self.bookmarks.panel.is_some()
            || self.sftp.form.is_some()
        {
            return;
        }
        if self.saved_searches.panel.is_some() {
            self.saved_searches.panel = None;
            self.text_paste.invalidate();
            return;
        }
        let mut p = Panel {
            stamp: epoch(),
            edit_epoch: epoch(),
            owner: self.pane_owner(),
            intent: self.search.input_revision(),
            root: self.search.root.clone(),
            selected: None,
            selection_request: None,
            editing: None,
            label: String::new(),
            cursor: Cursor::default(),
            geometry: Geometry::default(),
            top: 0,
            rows: 1,
            reveal: true,
            dragging: false,
            error: String::new(),
        };
        if save {
            match self.search.saved_criteria() {
                Ok(c) => p.new_edit(c),
                Err(e) => p.error = e,
            }
        }
        if !self.saved_searches.loaded && self.saved_searches.can_request() {
            self.saved_searches.request(Op::Read);
        }
        self.saved_searches.panel = Some(p);
        self.text_paste.invalidate();
        self.hits.clear();
    }
    pub fn saved_search_input(&mut self, text: &str) {
        if let Some(p) = &mut self.saved_searches.panel {
            p.input(text)
        }
    }
    pub fn saved_search_command(&mut self, stamp: u64, command: Command) {
        let Some(mut p) = self.saved_searches.panel.take() else {
            return;
        };
        if p.stamp != stamp {
            self.saved_searches.panel = Some(p);
            return;
        }
        if matches!(command, Command::Close) {
            self.text_paste.invalidate();
            self.hits.clear();
            return;
        }
        if self.closing || !p.current(self) {
            p.error = "La búsqueda o pestaña cambió; cierra y vuelve a abrir guardadas".into();
            self.saved_searches.panel = Some(p);
            return;
        }
        self.text_paste.invalidate();
        let selected = p.selected.and_then(|id| {
            self.saved_searches
                .document
                .rows
                .iter()
                .find(|e| e.id == id)
        });
        match command {
            Command::Load(here) if !p.editing() => {
                if let Some(e) = selected {
                    let c = e.criteria.clone();
                    match self.search.load_saved_criteria(&c, here) {
                        Ok(()) => {
                            self.hits.clear();
                            return;
                        }
                        Err(error) => p.error = error,
                    }
                }
            }
            Command::Select(id) if !p.editing() => {
                if self.saved_searches.document.rows.iter().any(|e| e.id == id) {
                    p.selection_request = None;
                    p.selected = Some(id);
                    p.reveal = true;
                    p.invalidate();
                }
            }
            Command::New => {
                if self.saved_searches.can_request() {
                    match self.search.saved_criteria() {
                        Ok(c) => p.new_edit(c),
                        Err(e) => p.error = e,
                    }
                } else {
                    p.error = "Espera, reintenta u omite el guardado anterior".into()
                }
            }
            Command::Edit if !p.editing() && self.saved_searches.can_request() => {
                if let Some(e) = selected {
                    p.label = e.label.clone();
                    p.cursor.all(&p.label);
                    p.editing = Some(Editing::Rename(e.clone()));
                    p.invalidate();
                }
            }
            Command::Commit => {
                if p.label.trim().is_empty()
                    || p.label.chars().any(char::is_control)
                    || p.label.len() > saved_search::MAX_LABEL
                {
                    p.error = "La etiqueta admite1–96 bytes, sin controles".into()
                } else if let Some(editing) = &p.editing {
                    let op = match editing {
                        Editing::New(c) => Op::Add {
                            criteria: c.clone(),
                            label: p.label.clone(),
                        },
                        Editing::Rename(e) => Op::Rename {
                            expected: e.clone(),
                            label: p.label.clone(),
                        },
                    };
                    if self.saved_searches.request(op) {
                        p.selection_request = self.saved_searches.request_id();
                        p.editing = None;
                        p.error.clear();
                        p.invalidate()
                    } else {
                        p.error = "Espera, reintenta u omite el intento anterior".into()
                    }
                }
            }
            Command::Cancel => {
                p.editing = None;
                p.error.clear();
                p.invalidate();
            }
            Command::Remove if !p.editing() && self.saved_searches.can_request() => {
                if let Some(e) = selected {
                    self.saved_searches.request(Op::Remove(e.clone()));
                }
            }
            Command::Reload => {
                self.saved_searches.refresh();
                if p.selection_request.is_some() {
                    p.selection_request = self.saved_searches.request_id();
                }
            }
            Command::Discard => {
                if self.saved_searches.can_discard() {
                    self.saved_searches.discard();
                    p.selection_request = None;
                }
            }
            Command::Details => {
                let error = if !p.error.is_empty() {
                    p.error.clone()
                } else {
                    self.saved_searches.error.clone()
                };
                self.copy_notice_id = None;
                self.operation_message = Some("Búsquedas guardadas · diagnóstico".into());
                self.operation_details = vec![
                    error,
                    "F2 vuelve al catálogo; ningún criterio se aplica por un ACK de guardado"
                        .into(),
                ];
                self.notice_open = true;
                self.notice_scroll = 0;
                self.notice_pan = 0;
                return;
            }
            _ => {}
        }
        self.saved_searches.panel = Some(p);
    }
}
fn target(
    p: &mut dyn Painter,
    m: &mut Model,
    r: Rect,
    label: &str,
    command: Command,
    enabled: bool,
    stamp: u64,
) {
    let c = &m.palette;
    p.rounded_rect(r, 4 * m.radius, c.selection);
    p.text(
        Rect::new(r.x + 7, r.y, r.w - 14, r.h),
        label,
        11,
        if enabled { c.accent } else { c.dark_foreground },
        true,
        false,
    );
    if enabled {
        m.hits.push(Hit {
            generation: m.generation,
            rect: r,
            action: Action::SavedSearchCommand(stamp, command),
        })
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut panel) = m.saved_searches.panel.take() else {
        return;
    };
    m.hits.clear();
    let r = Rect::new(18, 25, w - 36, h - 50);
    let c = m.palette.clone();
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.text(
        Rect::new(r.x + 14, r.y + 8, r.w - 104, 28),
        if panel.editing() {
            " Guardar búsqueda"
        } else {
            " Búsquedas guardadas"
        },
        15,
        c.foreground,
        true,
        false,
    );
    target(
        p,
        m,
        Rect::new(r.x + r.w - 78, r.y + 8, 64, 28),
        " Esc",
        Command::Close,
        true,
        panel.stamp,
    );
    let doc = &m.saved_searches.document;
    if panel
        .selected
        .is_none_or(|id| !doc.rows.iter().any(|e| e.id == id))
    {
        panel.selected = doc.rows.first().map(|e| e.id);
        panel.reveal = true;
    }
    let selected = panel
        .selected
        .and_then(|id| doc.rows.iter().find(|e| e.id == id));
    let root = match &panel.editing {
        Some(Editing::New(c)) => &c.root,
        Some(Editing::Rename(e)) => &e.criteria.root,
        None => selected.map_or(&panel.root, |e| &e.criteria.root),
    };
    p.literal_text(
        Rect::new(r.x + 14, r.y + 40, r.w - 28, 24),
        &format!(
            "Raíz guardada: {} · {}",
            root.file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy(),
            root.to_string_lossy()
        ),
        10,
        c.dark_foreground,
    );
    if panel.editing() {
        let field = Rect::new(r.x + 14, r.y + 70, r.w - 28, 32);
        p.rounded_rect(field, 4 * m.radius, c.background);
        panel.geometry.paint(
            p,
            &panel.label,
            &panel.cursor,
            field,
            &c,
            (true, panel.edit_epoch),
        );
    } else {
        p.literal_text(
            Rect::new(r.x + 14, r.y + 70, r.w - 28, 24),
            &format!(
                "Actual: {} · {}",
                m.search
                    .root
                    .file_name()
                    .unwrap_or(m.search.root.as_os_str())
                    .to_string_lossy(),
                m.search.root.to_string_lossy()
            ),
            10,
            c.accent,
        );
    }
    let body = Rect::new(r.x + 14, r.y + 110, r.w - 28, (r.h - 256).max(0));
    panel.rows = (body.h / 44).max(1) as usize;
    let last = doc.rows.len().saturating_sub(panel.rows);
    panel.top = panel.top.min(last);
    if panel.reveal {
        if let Some(i) = panel
            .selected
            .and_then(|id| doc.rows.iter().position(|e| e.id == id))
        {
            if i < panel.top {
                panel.top = i
            }
            if i >= panel.top + panel.rows {
                panel.top = i + 1 - panel.rows
            }
        }
        panel.reveal = false;
    }
    if !panel.editing() {
        for (i, e) in doc.rows.iter().enumerate().skip(panel.top).take(panel.rows) {
            let area = Rect::new(body.x, body.y + (i - panel.top) as i32 * 44, body.w, 42);
            if Some(e.id) == panel.selected {
                p.rounded_rect(area, 4 * m.radius, c.selection)
            }
            p.literal_text(
                Rect::new(area.x + 6, area.y, area.w - 12, 22),
                &e.label,
                12,
                c.foreground,
            );
            p.literal_text(
                Rect::new(area.x + 6, area.y + 22, area.w - 12, 18),
                if e.criteria.query.is_empty() {
                    "(nombre: todos)"
                } else {
                    &e.criteria.query
                },
                10,
                c.dark_foreground,
            );
            m.hits.push(Hit {
                generation: m.generation,
                rect: area,
                action: Action::SavedSearchCommand(panel.stamp, Command::Select(e.id)),
            });
        }
        if doc.rows.is_empty() {
            p.text(
                body,
                if m.saved_searches.busy() {
                    "Cargando…"
                } else {
                    "Sin búsquedas guardadas · Ctrl+S captura criterios"
                },
                11,
                c.dark_foreground,
                true,
                false,
            );
        }
    }
    if let Some(editing) = &panel.editing {
        let c = match editing {
            Editing::New(c) => c,
            Editing::Rename(e) => &e.criteria,
        };
        let summaries = [
            format!("Nombre: {:?}", c.query),
            format!(
                "{}: {:?}",
                if c.content_regex {
                    "Regex"
                } else {
                    "Texto literal"
                },
                c.content
            ),
            format!(
                "Tamaño: {} .. {}",
                if c.fields[0].is_empty() {
                    "—"
                } else {
                    &c.fields[0]
                },
                if c.fields[1].is_empty() {
                    "—"
                } else {
                    &c.fields[1]
                }
            ),
            format!(
                "UTC: {} .. {}",
                if c.fields[2].is_empty() {
                    "—"
                } else {
                    &c.fields[2]
                },
                if c.fields[3].is_empty() {
                    "—"
                } else {
                    &c.fields[3]
                }
            ),
            format!(
                "Ocultos: {} · {} · editor {}",
                if c.hidden { "sí" } else { "no" },
                match c.order {
                    1 => "Nombre",
                    2 => "Tipo",
                    _ => "Ruta",
                },
                if c.content_mode {
                    if c.content_regex {
                        "regex"
                    } else {
                        "texto literal"
                    }
                } else {
                    "nombre"
                }
            ),
        ];
        for (i, line) in summaries
            .iter()
            .take((body.h / 20).max(0) as usize)
            .enumerate()
        {
            p.literal_text(
                Rect::new(body.x, body.y + i as i32 * 20, body.w, 20),
                line,
                10,
                m.palette.foreground,
            );
        }
    }
    let summary = format!(
        "Cargar no busca · {}/32",
        m.saved_searches.document.rows.len()
    );
    let status = if !panel.error.is_empty() {
        panel.error.as_str()
    } else {
        m.saved_searches.status().unwrap_or(&summary)
    };
    let err = !panel.error.is_empty() || !m.saved_searches.error.is_empty();
    p.literal_text(
        Rect::new(r.x + 14, r.y + r.h - 136, r.w - 184, 32),
        status,
        10,
        if err { c.red } else { c.dark_foreground },
    );
    target(
        p,
        m,
        Rect::new(r.x + r.w - 162, r.y + r.h - 134, 68, 28),
        " F1",
        Command::Details,
        err,
        panel.stamp,
    );
    target(
        p,
        m,
        Rect::new(r.x + r.w - 86, r.y + r.h - 134, 72, 28),
        " F8",
        Command::Discard,
        m.saved_searches.can_discard(),
        panel.stamp,
    );
    let idle = m.saved_searches.can_request();
    let selected = panel.selected.is_some();
    let editing = panel.editing();
    let bw = (r.w - 36) / 3;
    let controls = if editing {
        [
            (" Guardar", Command::Commit, idle),
            (" Cancelar", Command::Cancel, true),
            (" F5", Command::Reload, !m.saved_searches.busy()),
            ("", Command::Close, false),
            ("", Command::Close, false),
            ("", Command::Close, false),
        ]
    } else {
        [
            (" Cargar", Command::Load(false), selected),
            (" Aquí", Command::Load(true), selected),
            (" Nueva", Command::New, idle),
            (" F2", Command::Edit, idle && selected),
            (" Quitar", Command::Remove, idle && selected),
            (" F5", Command::Reload, !m.saved_searches.busy()),
        ]
    };
    for (i, (label, command, enabled)) in controls
        .into_iter()
        .take(if editing { 3 } else { 6 })
        .enumerate()
    {
        target(
            p,
            m,
            Rect::new(
                r.x + 14 + (i % 3) as i32 * (bw + 4),
                r.y + r.h - 94 + (i / 3) as i32 * 36,
                bw,
                30,
            ),
            label,
            command,
            enabled,
            panel.stamp,
        );
    }
    p.unclip();
    m.saved_searches.panel = Some(panel);
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut, text: &str) {
    let Some(mut p) = m.saved_searches.panel.take() else {
        return;
    };
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = mods.intersects(Shortcut::Shift);
    let mut command = None;
    if key == Key::Escape {
        command = Some(if p.editing() {
            Command::Cancel
        } else {
            Command::Close
        })
    } else if key == Key::Enter || key == Key::KPEnter {
        command = Some(if p.editing() {
            Command::Commit
        } else {
            Command::Load(mods.intersects(Shortcut::Alt))
        })
    } else if ctrl && key == Key::from_char('s') {
        command = Some(Command::New)
    } else if key == Key::F5 {
        command = Some(Command::Reload)
    } else if key == Key::F8 {
        command = Some(Command::Discard)
    } else if key == Key::F1 {
        command = Some(Command::Details)
    } else if key == Key::F2 && !p.editing() {
        command = Some(Command::Edit)
    } else if p.editing() {
        if ctrl && key == Key::from_char('a') {
            p.cursor.all(&p.label)
        } else if ctrl && key == Key::from_char('c') {
            app::copy(&p.label[p.cursor.range(&p.label)])
        } else if ctrl && key == Key::from_char('z') {
            p.cursor.undo(&mut p.label, shift)
        } else if matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
            let motion = match key {
                Key::Left if ctrl => Motion::WordLeft,
                Key::Right if ctrl => Motion::WordRight,
                Key::Left => Motion::Left,
                Key::Right => Motion::Right,
                Key::Home => Motion::Start,
                _ => Motion::End,
            };
            p.cursor.motion(&p.label, motion, shift)
        } else if key == Key::BackSpace || key == Key::Delete {
            p.cursor
                .delete(&mut p.label, key == Key::BackSpace, ctrl, false)
        } else if !ctrl && !text.is_empty() {
            p.input(text)
        }
        p.edit_epoch = epoch();
    } else if key == Key::Delete {
        command = Some(Command::Remove)
    } else if matches!(
        key,
        Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown
    ) {
        let len = m.saved_searches.document.rows.len();
        let at = p
            .selected
            .and_then(|id| {
                m.saved_searches
                    .document
                    .rows
                    .iter()
                    .position(|e| e.id == id)
            })
            .unwrap_or(0);
        let i = match key {
            Key::Home => 0,
            Key::End => len.saturating_sub(1),
            Key::Up => at.saturating_sub(1),
            Key::Down => at.saturating_add(1).min(len.saturating_sub(1)),
            Key::PageUp => at.saturating_sub(p.rows),
            _ => at.saturating_add(p.rows).min(len.saturating_sub(1)),
        };
        p.selection_request = None;
        p.selected = m.saved_searches.document.rows.get(i).map(|e| e.id);
        p.reveal = true;
        p.invalidate();
    }
    let stamp = p.stamp;
    m.saved_searches.panel = Some(p);
    if let Some(c) = command {
        m.saved_search_command(stamp, c)
    }
}
pub fn event(m: &mut Model, e: Event) -> bool {
    match e {
        Event::KeyDown | Event::Shortcut => {
            key(m, app::event_key(), app::event_state(), &app::event_text())
        }
        Event::Push if app::event_button() == 1 => {
            let x = app::event_x();
            let y = app::event_y();
            let mut field = false;
            if let Some(p) = m
                .saved_searches
                .panel
                .as_mut()
                .filter(|p| p.editing() && p.geometry.contains(x, y))
                && let Some(at) = p.geometry.position(x, p.edit_epoch)
            {
                let anchor = if app::event_state().intersects(Shortcut::Shift) {
                    p.cursor.anchor
                } else {
                    at
                };
                if app::event_clicks() {
                    p.cursor.component(&p.label, at)
                } else {
                    p.cursor.select(&p.label, anchor, at)
                }
                p.dragging = true;
                p.edit_epoch = epoch();
                field = true;
            }
            if !field
                && let Some(a) = m
                    .hits
                    .iter()
                    .rev()
                    .find(|h| h.generation == m.generation && h.rect.contains(x, y))
                    .map(|h| h.action.clone())
            {
                m.action(a)
            }
        }
        Event::Drag => {
            if let Some(p) = m.saved_searches.panel.as_mut().filter(|p| p.dragging)
                && let Some(at) = p.geometry.position(app::event_x(), p.edit_epoch)
            {
                p.cursor.select(&p.label, p.cursor.anchor, at);
                p.edit_epoch = epoch();
            }
        }
        Event::Released => {
            if let Some(p) = &mut m.saved_searches.panel {
                p.dragging = false
            }
        }
        Event::MouseWheel => {
            if let Some(p) = &mut m.saved_searches.panel {
                p.top = p
                    .top
                    .saturating_add_signed(app::event_dy_value().saturating_mul(3) as isize)
                    .min(m.saved_searches.document.rows.len().saturating_sub(p.rows));
                p.reveal = false;
            }
        }
        _ => {}
    }
    true
}
/// Existing owned-window fixture only; never synthesizes an OS gesture.
pub(crate) fn fixture_command(m: &mut Model, action: &str) -> Result<(), &'static str> {
    if let Some(text) = action.strip_prefix("saved-label:") {
        m.saved_search_input(text);
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("saved-key:") {
        let (key, mods) = match name {
            "enter" => (Key::Enter, Shortcut::None),
            "here" => (Key::Enter, Shortcut::Alt),
            "escape" => (Key::Escape, Shortcut::None),
            "f2" => (Key::F2, Shortcut::None),
            "f5" => (Key::F5, Shortcut::None),
            "f8" => (Key::F8, Shortcut::None),
            "save" => (Key::from_char('s'), Shortcut::Ctrl),
            "up" => (Key::Up, Shortcut::None),
            "down" => (Key::Down, Shortcut::None),
            "home" => (Key::Home, Shortcut::None),
            "end" => (Key::End, Shortcut::None),
            _ => return Err("Tecla guardadas desconocida"),
        };
        if m.saved_searches.panel.is_some() {
            key_event(m, key, mods)
        } else {
            crate::search_ui::key(m, key, mods, "", 440, 440)
        }
        return Ok(());
    }
    let selected = action
        .strip_prefix("saved-select:")
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| "ID guardado inválido")?;
    let target = m
        .hits
        .iter()
        .rev()
        .find(|h| {
            h.generation == m.generation
                && match (&h.action, action) {
                    (Action::SavedSearches(false), "saved-open") => true,
                    (Action::SavedSearchCommand(_, command), _) => {
                        matches!(
                            (command, action),
                            (Command::Commit, "saved-commit")
                                | (Command::Close, "saved-close")
                                | (Command::Load(false), "saved-load")
                                | (Command::Load(true), "saved-here")
                                | (Command::New, "saved-new")
                                | (Command::Edit, "saved-edit")
                                | (Command::Remove, "saved-remove")
                                | (Command::Reload, "saved-reload")
                                | (Command::Discard, "saved-omit")
                                | (Command::Details, "saved-details")
                        ) || matches!(command,Command::Select(id)if Some(*id)==selected)
                    }
                    _ => false,
                }
        })
        .map(|h| h.action.clone())
        .ok_or("Control de búsqueda guardada no visible")?;
    m.action(target);
    Ok(())
}
fn key_event(m: &mut Model, key: Key, mods: Shortcut) {
    self::key(m, key, mods, "")
}
pub(crate) fn snapshot(m: &Model) -> serde_json::Value {
    serde_json::json!({"loaded":m.saved_searches.loaded,"busy":m.saved_searches.busy(),"unresolved":m.saved_searches.needs_resolution(),"unsynced":m.saved_searches.unsynced,"error":m.saved_searches.error,"notice":m.saved_searches.notice,
 "rows":m.saved_searches.document.rows.iter().map(|e|serde_json::json!({"id":e.id,"label":e.label,"root":e.criteria.root.as_os_str().as_encoded_bytes(),"query":e.criteria.query,"content":e.criteria.content,"fields":e.criteria.fields,"hidden":e.criteria.hidden,"content_mode":e.criteria.content_mode,"content_regex":e.criteria.content_regex,"order":e.criteria.order})).collect::<Vec<_>>(),
 "panel":m.saved_searches.panel.as_ref().map(|p|serde_json::json!({"stamp":p.stamp,"epoch":p.edit_epoch,"selected":p.selected,"editing":p.editing(),"label":p.label,"error":p.error,"top":p.top,"rows":p.rows,"current":p.current(m),"input_painted":!p.editing()||p.geometry.position(0,p.edit_epoch).is_some()})),
 "buttons":m.hits.iter().filter_map(|h|if let Action::SavedSearchCommand(stamp,ref command)=h.action{Some(serde_json::json!({"stamp":stamp,"command":format!("{command:?}"),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}else{None}).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn model() -> Model {
        let mut m = Model::new("/browser".into());
        m.search.open("/captured".into(), "ext:rs".into(), false);
        m
    }
    fn loaded(m: &mut Model) {
        let c = m.search.saved_criteria().unwrap();
        m.saved_searches.loaded = true;
        m.saved_searches.document = Arc::new(saved_search::Document {
            next: 2,
            rows: vec![Entry {
                id: 1,
                label: "Código".into(),
                criteria: c,
            }],
        });
    }
    #[test]
    fn catalogue_load_is_explicit_no_autoexecution_and_root_here_is_distinct() {
        let mut m = model();
        m.search.content = "ñ literal".into();
        m.search.fields = [
            "1B".into(),
            "20MiB".into(),
            "2026-01-01".into(),
            "2027-01-01".into(),
        ];
        m.search.hidden = true;
        m.search.content_mode = true;
        loaded(&mut m);
        let c = m.saved_searches.document.rows[0].criteria.clone();
        m.search.open("/here".into(), "wrong".into(), false);
        let path = m.path.clone();
        m.open_saved_searches(false);
        let p = m.saved_searches.panel.as_mut().unwrap();
        p.selected = Some(1);
        let stamp = p.stamp;
        m.saved_search_command(stamp, Command::Load(true));
        assert!(m.saved_searches.panel.is_none());
        assert_eq!(m.search.root, std::path::Path::new("/here"));
        assert_eq!(m.search.query, c.query);
        assert_eq!(m.search.content, c.content);
        assert_eq!(m.search.fields, c.fields);
        assert!(m.search.hidden && !m.search.running());
        assert_eq!(m.path, path);
        m.open_saved_searches(false);
        let p = m.saved_searches.panel.as_mut().unwrap();
        p.selected = Some(1);
        let stamp = p.stamp;
        m.saved_search_command(stamp, Command::Load(false));
        assert_eq!(m.search.root, c.root);
        assert!(!m.search.running());
        assert!(m.search.report.is_none() && m.search.hits().is_empty());
    }
    #[test]
    fn old_hits_changed_intent_and_late_paste_never_apply_another_draft() {
        let mut m = model();
        loaded(&mut m);
        m.open_saved_searches(true);
        let old = m.saved_searches.panel.as_ref().unwrap().stamp;
        let ctx = m.text_context();
        assert!(ctx.is_some());
        m.text_paste.request(ctx).unwrap();
        m.saved_search_command(old, Command::Cancel);
        assert!(!m.receive_text_paste("STALE"));
        assert_eq!(m.search.query, "ext:rs");
        let p = m.saved_searches.panel.as_mut().unwrap();
        p.selected = Some(1);
        p.invalidate();
        let fresh = p.stamp;
        m.saved_search_command(old, Command::Load(false));
        assert!(m.saved_searches.panel.is_some());
        m.search.open("/changed".into(), "new".into(), false);
        m.saved_search_command(fresh, Command::Load(false));
        assert_eq!(m.search.query, "new");
        assert_eq!(m.search.root, std::path::Path::new("/changed"));
        assert!(
            m.saved_searches
                .panel
                .as_ref()
                .unwrap()
                .error
                .contains("cambió")
        );
    }
    #[test]
    fn captured_save_survives_storage_reply_without_mutating_search_and_label_is_bounded() {
        let mut m = model();
        loaded(&mut m);
        m.open_saved_searches(true);
        m.saved_search_input("á");
        let p = m.saved_searches.panel.as_ref().unwrap();
        assert_eq!(p.label, "á");
        let before = p.label.clone();
        m.saved_search_input(&"x".repeat(100000));
        assert_eq!(m.saved_searches.panel.as_ref().unwrap().label, before);
        let stamp = m.saved_searches.panel.as_ref().unwrap().stamp;
        m.saved_search_command(stamp, Command::Commit);
        assert!(m.saved_searches.busy());
        assert_eq!(m.search.query, "ext:rs");
        assert!(!m.search.running());
        m.saved_search_command(
            m.saved_searches.panel.as_ref().unwrap().stamp,
            Command::Close,
        );
        assert!(m.saved_searches.panel.is_none() && m.saved_searches.busy());
    }
    #[test]
    fn add_ack_selects_new_entry_only_for_the_same_unsuperseded_panel_intent() {
        for supersede in 0..3 {
            let mut m = model();
            loaded(&mut m);
            m.search.open("/captured".into(), "ext:md".into(), false);
            m.open_saved_searches(true);
            m.saved_search_input("Nueva");
            let stamp = m.saved_searches.panel.as_ref().unwrap().stamp;
            m.saved_search_command(stamp, Command::Commit);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            m.saved_searches.dispatch(&tx);
            let crate::aux_io::Request::SavedSearches(id, op) = rx.try_recv().unwrap() else {
                panic!()
            };
            let Op::Add { criteria, label } = &*op else {
                panic!()
            };
            if supersede == 1 {
                let stamp = m.saved_searches.panel.as_ref().unwrap().stamp;
                m.saved_search_command(stamp, Command::Select(1));
            }
            if supersede == 2 {
                let stamp = m.saved_searches.panel.as_ref().unwrap().stamp;
                m.saved_search_command(stamp, Command::Close);
                m.open_saved_searches(false);
            }
            let mut rows = m.saved_searches.document.rows.clone();
            rows.push(Entry {
                id: 2,
                label: label.clone(),
                criteria: criteria.clone(),
            });
            m.saved_searches.receive(
                id,
                Ok(saved_search::Updated {
                    document: saved_search::Document { next: 3, rows },
                    unsynced: false,
                    detail: None,
                    notice: "Guardado",
                }),
            );
            let selected = m.saved_searches.panel.as_ref().unwrap().selected;
            if supersede == 0 {
                assert_eq!(selected, Some(2));
            } else {
                assert_ne!(selected, Some(2));
            }
            assert_eq!(m.search.query, "ext:md");
            assert!(!m.search.running());
        }
    }
}
