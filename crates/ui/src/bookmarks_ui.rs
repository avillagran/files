//! Modal favourites/places/history navigation; all persistence stays on AuxIo.
use crate::{
    line_edit::{Cursor, Motion},
    line_field::Geometry,
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::bookmarks::{self, Entry, Op};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
use std::{
    borrow::Cow,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
fn epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut n = NEXT.load(Ordering::Relaxed);
    loop {
        let next = n.checked_add(1).expect("bookmark epochs exhausted");
        match NEXT.compare_exchange_weak(n, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(v) => return v,
            Err(v) => n = v,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Favourite(u64),
    Place(usize),
    Back(usize),
    Forward(usize),
}
#[derive(Clone, Debug)]
pub enum Command {
    Close,
    SaveLabel,
    Select(Row),
    Jump,
    Add,
    Edit,
    Remove,
    Reload,
    Discard,
    Details,
}
pub fn name(cmd: &Command) -> &'static str {
    match cmd {
        Command::SaveLabel => "Guardar etiqueta del favorito, Enter",
        Command::Close => "Cerrar lugares, Esc",
        Command::Select(_) => "Seleccionar lugar",
        Command::Jump => "Entrar en la carpeta, Enter",
        Command::Add => "Añadir carpeta actual a favoritos, Ctrl+D",
        Command::Edit => "Editar etiqueta del favorito, F2",
        Command::Remove => "Quitar favorito; no borra la carpeta, Supr",
        Command::Reload => "Recargar o reintentar favoritos, F5",
        Command::Discard => "Omitir intento pendiente; no revierte publicado, F8",
        Command::Details => "Ver diagnóstico de favoritos, F1",
    }
}
pub struct Panel {
    pub stamp: u64,
    pub edit_epoch: u64,
    owner: (u64, u64),
    pub selected: Option<Row>,
    pub query: String,
    pub query_cursor: Cursor,
    pub query_geometry: Geometry,
    pub label: String,
    pub label_cursor: Cursor,
    pub label_geometry: Geometry,
    pub editing: Option<Entry>,
    pub input: bool,
    pub local_error: String,
    top: usize,
    details: bool,
    detail_top: usize,
    dragging: bool,
}
impl Panel {
    fn new(owner: (u64, u64)) -> Self {
        Self {
            stamp: epoch(),
            edit_epoch: epoch(),
            owner,
            selected: None,
            query: String::new(),
            query_cursor: Cursor::default(),
            query_geometry: Geometry::default(),
            label: String::new(),
            label_cursor: Cursor::default(),
            label_geometry: Geometry::default(),
            editing: None,
            input: true,
            local_error: String::new(),
            top: 0,
            details: false,
            detail_top: 0,
            dragging: false,
        }
    }
    pub fn invalidate(&mut self) {
        self.dragging = false;
        self.stamp = epoch();
        self.edit_epoch = epoch();
    }
    fn text(&self) -> &str {
        if self.editing.is_some() {
            &self.label
        } else {
            &self.query
        }
    }
    fn cursor(&self) -> &Cursor {
        if self.editing.is_some() {
            &self.label_cursor
        } else {
            &self.query_cursor
        }
    }
    fn selected_text(&self) -> &str {
        let text = self.text();
        &text[self.cursor().range(text)]
    }
    fn change(&mut self) {
        self.stamp = epoch();
        self.local_error.clear();
        self.edit_epoch = epoch();
        self.input = true;
        self.details = false;
        if self.editing.is_none() {
            self.selected = None;
            self.top = 0;
        }
    }
    pub fn input(&mut self, text: &str) -> Result<(), &'static str> {
        if text.chars().any(char::is_control) {
            return Err("Escribe una sola línea sin controles");
        }
        if self.editing.is_some() {
            self.label_cursor
                .insert_bounded(&mut self.label, text, false, bookmarks::MAX_LABEL)?;
        } else {
            self.query_cursor
                .insert_bounded(&mut self.query, text, false, 256)?;
        }
        self.change();
        Ok(())
    }
    fn all(&mut self) {
        if self.editing.is_some() {
            self.label_cursor.all(&self.label);
        } else {
            self.query_cursor.all(&self.query);
        }
        self.input = true;
        self.edit_epoch = epoch();
    }
    fn motion(&mut self, motion: Motion, shift: bool) {
        if self.editing.is_some() {
            self.label_cursor.motion(&self.label, motion, shift);
        } else {
            self.query_cursor.motion(&self.query, motion, shift);
        }
        self.input = true;
        self.edit_epoch = epoch();
    }
    fn delete(&mut self, back: bool, word: bool) {
        if self.editing.is_some() {
            self.label_cursor.delete(&mut self.label, back, word, false);
        } else {
            self.query_cursor.delete(&mut self.query, back, word, false);
        }
        self.change();
    }
    fn undo(&mut self, redo: bool) {
        if self.editing.is_some() {
            self.label_cursor.undo(&mut self.label, redo);
        } else {
            self.query_cursor.undo(&mut self.query, redo);
        }
        self.change();
    }
}
fn data(m: &Model, row: Row) -> Option<(Cow<'_, str>, &Path, &'static str)> {
    match row {
        Row::Favourite(id) => m
            .bookmarks
            .document
            .rows
            .iter()
            .find(|e| e.id == id)
            .map(|e| (Cow::Borrowed(e.label.as_str()), e.path.as_path(), "")),
        Row::Place(i) => m
            .places
            .get(i)
            .map(|e| (Cow::Borrowed(e.label.as_str()), e.path.as_path(), "")),
        Row::Back(i) | Row::Forward(i) => {
            let (back, forward) = m.history.paths();
            let path = if matches!(row, Row::Back(_)) {
                back.get(i)
            } else {
                forward.get(i)
            }?;
            Some((
                files_core::name_label::label(path.file_name().unwrap_or(path.as_os_str())),
                path.as_path(),
                "",
            ))
        }
    }
}
fn display_path(path: &Path) -> Cow<'_, str> {
    files_core::name_label::label(path.as_os_str())
}

fn rows(m: &Model, p: &Panel) -> Vec<Row> {
    let (back, forward) = m.history.paths();
    let choices = m
        .bookmarks
        .document
        .rows
        .iter()
        .map(|e| Row::Favourite(e.id))
        .chain((0..m.places.len().min(8)).map(Row::Place))
        .chain((0..back.len()).rev().map(Row::Back))
        .chain((0..forward.len()).rev().map(Row::Forward));
    choices
        .filter(|&r| {
            p.query.is_empty()
                || data(m, r).is_some_and(|(label, _, _)| {
                    files_core::listing::fuzzy_score(&label, &p.query).is_some()
                })
        })
        .collect()
}
fn permitted(m: &Model) -> bool {
    !m.closing
        && !m.editing
        && !m.filter_edit.active
        && !m.search.visible
        && m.commands.is_none()
        && m.size_panel.is_none()
        && !m.preview.visible()
        && m.help.is_none()
        && m.rename_dialog.is_none()
        && m.copy_form.is_none()
        && m.properties_dialog.is_none()
        && !m.notice_open
        && !m.recovery_catalog.open
        && !m.mutation_applied
}
fn auto_label(path: &Path) -> String {
    let leaf = path.file_name().unwrap_or(path.as_os_str());
    let mut s = if leaf.to_str().is_some() {
        leaf.to_string_lossy()
            .chars()
            .map(|c| if c.is_control() { '�' } else { c })
            .collect::<String>()
    } else {
        format!("{leaf:?}")
    };
    if s.trim().is_empty() {
        s = "Carpeta".into();
    }
    if s.len() > 96 {
        let mut n = 93;
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        s.truncate(n);
        s.push('…');
    }
    s
}
impl Model {
    pub fn open_bookmarks(&mut self) {
        if self.bookmarks.panel.is_some() {
            self.bookmarks.panel = None;
            self.hits.clear();
            return;
        }
        if !permitted(self) {
            return;
        }
        self.thumbnails.suspend();
        self.settings = false;
        self.actions_menu = false;
        self.context_menu = None;
        self.text_paste.invalidate();
        self.bookmarks.panel = Some(Panel::new(self.pane_owner()));
        if !self.bookmarks.needs_resolution() {
            self.bookmarks.refresh();
        }
        self.hits.clear();
    }
    pub fn add_bookmark(&mut self) {
        if self.closing
            || (!permitted(self) && self.bookmarks.panel.is_none())
            || !self.loaded
            || self.selection_refreshing()
            || !self.error.is_empty()
        {
            return;
        }
        if let Err(e) = files_core::navigation_history::validate(&self.path) {
            self.bookmarks.error = e.into();
            return;
        }
        let label = auto_label(&self.path);
        if !self.bookmarks.request(Op::Add {
            path: self.path.clone(),
            label,
        }) {
            self.bookmarks.notice = "Espera, reintenta u omite el intento anterior".into();
        }
    }
    pub fn bookmark_command(&mut self, stamp: u64, cmd: Command) {
        if self.closing {
            return;
        }
        let Some(mut panel) = self.bookmarks.panel.take() else {
            return;
        };
        if panel.stamp != stamp {
            self.bookmarks.panel = Some(panel);
            return;
        }
        if matches!(cmd, Command::Close) {
            self.text_paste.invalidate();
            self.hits.clear();
            return;
        }
        if panel.owner != self.pane_owner() {
            panel.local_error = "El panel o pestaña cambió; cierra y vuelve a abrir lugares".into();
            self.bookmarks.panel = Some(panel);
            return;
        }
        self.session.restore_allowed = false;
        self.text_paste.invalidate();
        match cmd {
            Command::Select(row) => {
                if rows(self, &panel).contains(&row) {
                    panel.selected = Some(row);
                    panel.editing = None;
                    panel.input = false;
                    panel.invalidate();
                }
            }
            Command::Jump => {
                if let Some((_, path, _)) = panel.selected.and_then(|r| data(self, r))
                    && files_core::navigation_history::validate(path).is_ok()
                {
                    let target = path.to_path_buf();
                    self.text_paste.invalidate();
                    self.hits.clear();
                    self.navigate(target);
                    return;
                }
            }
            Command::Add => {
                self.bookmarks.panel = Some(panel);
                self.add_bookmark();
                return;
            }
            Command::Edit => {
                if self.bookmarks.can_request()
                    && let Some(Row::Favourite(id)) = panel.selected
                    && let Some(e) = self.bookmarks.document.rows.iter().find(|e| e.id == id)
                {
                    panel.label = e.label.clone();
                    panel.label_cursor = Cursor::default();
                    panel.label_cursor.all(&panel.label);
                    panel.editing = Some(e.clone());
                    panel.input = true;
                    panel.invalidate();
                }
            }
            Command::Remove => {
                if let Some(Row::Favourite(id)) = panel.selected
                    && let Some(e) = self.bookmarks.document.rows.iter().find(|e| e.id == id)
                    && self.bookmarks.request(Op::Remove(e.clone()))
                {
                    panel.editing = None;
                    panel.input = false;
                    panel.invalidate();
                }
            }
            Command::SaveLabel => save_label(self, &mut panel),
            Command::Reload => self.bookmarks.refresh(),
            Command::Discard => {
                self.bookmarks.discard();
                panel.local_error.clear();
                panel.invalidate();
            }
            Command::Details => {
                panel.details = !panel.details;
                panel.detail_top = 0;
                panel.invalidate();
            }
            Command::Close => unreachable!(),
        }
        self.bookmarks.panel = Some(panel);
    }
    pub fn bookmark_input(&mut self, text: &str) {
        if let Some(panel) = &mut self.bookmarks.panel
            && let Err(e) = panel.input(text)
        {
            panel.local_error = e.into();
        }
    }
}
fn target(
    p: &mut dyn Painter,
    m: &mut Model,
    rect: Rect,
    label: &str,
    cmd: Command,
    enabled: bool,
    stamp: u64,
) {
    p.rounded_rect(
        rect,
        4 * m.radius,
        if enabled {
            m.palette.lighter_background
        } else {
            m.palette.darker_background
        },
    );
    p.text(
        Rect::new(rect.x + 6, rect.y, rect.w - 12, rect.h),
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
            action: Action::Bookmark(stamp, cmd),
            generation: m.generation,
        });
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut panel) = m.bookmarks.panel.take() else {
        return;
    };
    m.hits.clear();
    let r = Rect::new(12, 66, w - 24, h - 106);
    let c = m.palette.clone();
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.text(
        Rect::new(r.x + 14, r.y + 4, r.w - 110, 28),
        " Favoritos y lugares",
        16,
        c.foreground,
        true,
        false,
    );
    target(
        p,
        m,
        Rect::new(r.x + r.w - 84, r.y + 6, 70, 28),
        " Esc",
        Command::Close,
        true,
        panel.stamp,
    );
    p.text(
        Rect::new(
            r.x + 14,
            r.y + 36,
            r.w - 28 - if m.bookmarks.error.is_empty() { 0 } else { 84 },
            20,
        ),
        if panel.editing.is_some() {
            "Etiqueta del favorito · no renombra la carpeta"
        } else {
            "Filtrar por etiqueta · favoritos / XDG / historial"
        },
        11,
        c.dark_foreground,
        true,
        false,
    );
    let field = Rect::new(r.x + 14, r.y + 60, r.w - 28, 32);
    p.rounded_rect(field, 4 * m.radius, c.background);
    if panel.editing.is_some() {
        panel.label_geometry.paint(
            p,
            &panel.label,
            &panel.label_cursor,
            field,
            &c,
            (panel.input, panel.edit_epoch),
        );
    } else {
        panel.query_geometry.paint(
            p,
            &panel.query,
            &panel.query_cursor,
            field,
            &c,
            (panel.input, panel.edit_epoch),
        );
    }
    let choices = rows(m, &panel);
    if !panel.selected.is_some_and(|r| choices.contains(&r)) {
        panel.selected = choices.first().copied();
    }
    let body = Rect::new(r.x + 14, r.y + 104, r.w - 28, (r.h - 230).max(0));
    let page = (body.h / 44).max(1) as usize;
    if let Some(rank) = panel
        .selected
        .and_then(|row| choices.iter().position(|r| *r == row))
    {
        if rank < panel.top {
            panel.top = rank;
        }
        if rank >= panel.top + page {
            panel.top = rank + 1 - page;
        }
    }
    p.clip(body);
    if panel.details {
        let text = if !m.bookmarks.error.is_empty() {
            m.bookmarks.error.clone()
        } else if !panel.local_error.is_empty() {
            panel.local_error.clone()
        } else if let Some((label, path, _)) = panel.selected.and_then(|r| data(m, r)) {
            format!(
                "{label}\nRuta exacta: {:?}\nQuitar favorito no borra la carpeta.",
                path.as_os_str()
            )
        } else {
            "No hay selección ni error".into()
        };
        let cols = (body.w / 7).max(1) as usize;
        let lines: Vec<String> = text
            .lines()
            .flat_map(|line| {
                let chars: Vec<char> = line
                    .chars()
                    .map(|c| if c.is_control() { '�' } else { c })
                    .collect();
                chars
                    .chunks(cols)
                    .map(|p| p.iter().collect::<String>())
                    .collect::<Vec<_>>()
            })
            .collect();
        panel.detail_top = panel
            .detail_top
            .min(lines.len().saturating_sub((body.h / 18).max(1) as usize));
        for (line, part) in lines
            .iter()
            .enumerate()
            .skip(panel.detail_top)
            .take((body.h / 18).max(0) as usize)
        {
            p.text(
                Rect::new(
                    body.x,
                    body.y + (line - panel.detail_top) as i32 * 18,
                    body.w,
                    18,
                ),
                part,
                11,
                if m.bookmarks.error.is_empty() {
                    c.dark_foreground
                } else {
                    c.red
                },
                true,
                false,
            );
        }
    } else if choices.is_empty() && !m.bookmarks.loaded && m.bookmarks.busy() {
        p.text(
            body,
            "Cargando favoritos…",
            12,
            c.dark_foreground,
            true,
            false,
        );
    } else {
        for (i, row) in choices
            .iter()
            .copied()
            .enumerate()
            .skip(panel.top)
            .take(page)
        {
            let rect = Rect::new(body.x, body.y + (i - panel.top) as i32 * 44, body.w, 42);
            if rect.y + rect.h > body.y + body.h {
                break;
            }
            if panel.selected == Some(row) {
                p.rounded_rect(rect, 4 * m.radius, c.selection);
            }
            if let Some((label, path, glyph)) = data(m, row) {
                p.text(
                    Rect::new(rect.x + 6, rect.y, rect.w - 12, 22),
                    &format!("{glyph} {label}"),
                    12,
                    c.foreground,
                    true,
                    false,
                );
                p.text(
                    Rect::new(rect.x + 24, rect.y + 22, rect.w - 30, 18),
                    &display_path(path),
                    10,
                    c.dark_foreground,
                    true,
                    false,
                );
            }
            m.hits.push(Hit {
                rect,
                action: Action::Bookmark(panel.stamp, Command::Select(row)),
                generation: m.generation,
            });
        }
    }
    p.unclip();
    let status = if !m.bookmarks.error.is_empty() {
        format!("Error · F1 detalle: {}", m.bookmarks.error)
    } else if !panel.local_error.is_empty() {
        panel.local_error.clone()
    } else if m.bookmarks.busy() {
        "Esperando ACK de favoritos…".into()
    } else if !m.bookmarks.notice.is_empty() {
        m.bookmarks.notice.clone()
    } else if !m.bookmarks.loaded {
        "Favoritos sin cargar · F5 recarga".into()
    } else {
        format!(
            "{} favoritos · F2 etiqueta · Supr quita",
            m.bookmarks.document.rows.len()
        )
    };
    p.text(
        Rect::new(r.x + 14, r.y + r.h - 120, r.w - 28, 30),
        &status,
        11,
        if m.bookmarks.error.is_empty() {
            c.dark_foreground
        } else {
            c.red
        },
        true,
        false,
    );
    let selected = matches!(panel.selected, Some(Row::Favourite(_)));
    let idle = m.bookmarks.can_request();
    let bw = (r.w - 36) / 3;
    let y = r.y + r.h - 84;
    for (i, (label, cmd, enabled)) in [
        (
            if panel.editing.is_some() {
                " Guardar"
            } else {
                " Entrar"
            },
            if panel.editing.is_some() {
                Command::SaveLabel
            } else {
                Command::Jump
            },
            if panel.editing.is_some() {
                idle
            } else {
                panel.selected.is_some()
            },
        ),
        (" Actual", Command::Add, idle && m.loaded),
        (" Etiqueta", Command::Edit, idle && selected),
        (" Quitar", Command::Remove, idle && selected),
        (" F5", Command::Reload, !m.bookmarks.busy()),
        (" Omitir F8", Command::Discard, m.bookmarks.can_discard()),
    ]
    .into_iter()
    .enumerate()
    {
        target(
            p,
            m,
            Rect::new(
                r.x + 14 + (i % 3) as i32 * (bw + 4),
                y + (i / 3) as i32 * 36,
                bw,
                30,
            ),
            label,
            cmd,
            enabled,
            panel.stamp,
        );
    }
    if !m.bookmarks.error.is_empty() || !panel.local_error.is_empty() {
        target(
            p,
            m,
            Rect::new(r.x + r.w - 90, r.y + 35, 76, 22),
            " F1",
            Command::Details,
            true,
            panel.stamp,
        );
    }
    p.unclip();
    m.bookmarks.panel = Some(panel);
}
fn save_label(m: &mut Model, p: &mut Panel) {
    if let Some(expected) = &p.editing {
        if p.label.trim().is_empty() || p.label.len() > 96 {
            p.local_error = "La etiqueta admite de1 a96 bytes".into();
        } else if m.bookmarks.request(Op::Rename {
            expected: expected.clone(),
            label: p.label.clone(),
        }) {
            p.editing = None;
            p.input = false;
            p.invalidate();
        } else {
            p.local_error = "Reintenta u omite el intento anterior".into();
        }
    }
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut, text: &str, w: i32, h: i32) {
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = mods.intersects(Shortcut::Shift);
    let Some(mut p) = m.bookmarks.panel.take() else {
        return;
    };
    let mut command = None;
    if key == Key::Escape {
        if p.editing.is_some() {
            p.editing = None;
            p.input = true;
            p.invalidate();
        } else if p.details {
            p.details = false;
            p.invalidate();
        } else {
            command = Some(Command::Close);
        }
    } else if ctrl && key == Key::from_char('w') {
        command = Some(Command::Close);
    } else if key == Key::Enter || key == Key::KPEnter {
        let editing = p.editing.is_some();
        m.bookmarks.panel = Some(p);
        if editing {
            if let Some(stamp) = m.bookmarks.panel.as_ref().map(|p| p.stamp) {
                m.bookmark_command(stamp, Command::SaveLabel);
            }
        } else if let Some(stamp) = m.bookmarks.panel.as_ref().map(|p| p.stamp) {
            m.bookmark_command(stamp, Command::Jump);
        }
        return;
    } else if key == Key::F2 {
        command = Some(Command::Edit);
    } else if key == Key::F5 || ctrl && key == Key::from_char('r') {
        command = Some(Command::Reload);
    } else if key == Key::F8 {
        command = Some(Command::Discard);
    } else if key == Key::F1 {
        command = Some(Command::Details);
    } else if ctrl && key == Key::from_char('d') {
        command = Some(if mods.intersects(Shortcut::Alt) {
            Command::Close
        } else {
            Command::Add
        });
    } else if ctrl && key == Key::from_char('f') {
        p.editing = None;
        p.query_cursor.all(&p.query);
        p.input = true;
        p.invalidate();
    } else if ctrl && key == Key::from_char('a') {
        p.all();
    } else if ctrl && (key == Key::from_char('c') || key == Key::from_char('x')) && p.input {
        let selected = p.selected_text().to_owned();
        if !selected.is_empty() {
            app::copy(&selected);
            if key == Key::from_char('x') {
                p.delete(false, false);
            }
        }
    } else if ctrl && (key == Key::from_char('z') || key == Key::from_char('y')) && p.input {
        p.undo(shift || key == Key::from_char('y'));
    } else if p.input && matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
        p.motion(
            match key {
                Key::Left if ctrl => Motion::WordLeft,
                Key::Left => Motion::Left,
                Key::Right if ctrl => Motion::WordRight,
                Key::Right => Motion::Right,
                Key::Home => Motion::Start,
                _ => Motion::End,
            },
            shift,
        );
    } else if matches!(
        key,
        Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown
    ) {
        let delta = match key {
            Key::Up => -1,
            Key::Down => 1,
            Key::Home => i32::MIN,
            Key::End => i32::MAX,
            Key::PageUp => -((h - 336) / 44).max(1),
            _ => ((h - 336) / 44).max(1),
        };
        if p.details {
            p.detail_top = if delta < 0 {
                p.detail_top.saturating_sub(delta.unsigned_abs() as usize)
            } else {
                p.detail_top.saturating_add(delta as usize).min(16384)
            };
        } else if p.editing.is_none() {
            let rows = rows(m, &p);
            let indices: Vec<usize> = (0..rows.len()).collect();
            let selected = p.selected.and_then(|r| rows.iter().position(|v| *v == r));
            if let Some(i) = crate::keyboard_nav::target(&indices, selected, delta) {
                p.selected = Some(rows[i]);
            }
            p.input = false;
            p.invalidate();
        }
    } else if key == Key::Delete && !p.input {
        command = Some(Command::Remove);
    } else if matches!(key, Key::Delete | Key::BackSpace) && p.input {
        p.delete(key == Key::BackSpace, ctrl);
    } else if !ctrl
        && !text.is_empty()
        && let Err(e) = p.input(text)
    {
        p.local_error = e.into();
    }
    let stamp = p.stamp;
    m.bookmarks.panel = Some(p);
    if let Some(command) = command {
        m.bookmark_command(stamp, command);
    }
    let _ = w;
}
pub fn pointer(
    m: &mut Model,
    x: i32,
    y: i32,
    down: bool,
    drag: bool,
    shift: bool,
    double: bool,
) -> bool {
    let Some(p) = &mut m.bookmarks.panel else {
        return false;
    };
    if !down && !drag {
        p.dragging = false;
        return false;
    }
    let geometry = if p.editing.is_some() {
        &p.label_geometry
    } else {
        &p.query_geometry
    };
    if !(down && geometry.contains(x, y) || drag && p.dragging) {
        if down {
            p.dragging = false;
        }
        return false;
    }
    if let Some(at) = geometry.position(x, p.edit_epoch) {
        let (cursor, text) = if p.editing.is_some() {
            (&mut p.label_cursor, &p.label)
        } else {
            (&mut p.query_cursor, &p.query)
        };
        if down && double {
            cursor.component(text, at);
        } else {
            let anchor = if drag || shift { cursor.anchor } else { at };
            cursor.select(text, anchor, at);
        }
        p.dragging = true;
        p.input = true;
        p.edit_epoch = epoch();
    }
    true
}
pub fn event(m: &mut Model, e: Event, w: i32, h: i32) -> bool {
    match e {
        Event::Push if app::event_button() == 1 => {
            let selected = pointer(
                m,
                app::event_x(),
                app::event_y(),
                true,
                false,
                app::event_state().intersects(Shortcut::Shift),
                app::event_clicks(),
            );
            if !selected {
                let action = m
                    .hits
                    .iter()
                    .rev()
                    .find(|h| {
                        h.generation == m.generation
                            && h.rect.contains(app::event_x(), app::event_y())
                    })
                    .map(|h| h.action.clone());
                if let Some(action) = action {
                    let twice = matches!(action, Action::Bookmark(_, Command::Select(_)))
                        && app::event_clicks();
                    m.action(action);
                    if twice && let Some(stamp) = m.bookmarks.panel.as_ref().map(|p| p.stamp) {
                        m.bookmark_command(stamp, Command::Jump);
                    }
                }
            }
        }
        Event::Drag => {
            pointer(m, app::event_x(), app::event_y(), false, true, false, false);
        }
        Event::Released => {
            pointer(m, 0, 0, false, false, false, false);
        }
        Event::KeyDown | Event::Shortcut => key(
            m,
            app::event_key(),
            app::event_state(),
            &app::event_text(),
            w,
            h,
        ),
        Event::MouseWheel => {
            let step = if app::event_dy_value() < 0 {
                Key::Up
            } else {
                Key::Down
            };
            key(m, step, Shortcut::None, "", w, h);
        }
        _ => {}
    }
    true
}
pub fn fixture(m: &Model) -> serde_json::Value {
    serde_json::json!({"browser_loaded":m.loaded,"closing":m.closing,"loaded":m.bookmarks.loaded,"busy":m.bookmarks.busy(),"unresolved":m.bookmarks.needs_resolution(),"unsynced":m.bookmarks.unsynced,"error":m.bookmarks.error,"notice":m.bookmarks.notice,"rows":m.bookmarks.document.rows.iter().map(|e|serde_json::json!({"id":e.id,"label":e.label,"path":e.path.as_os_str().as_encoded_bytes()})).collect::<Vec<_>>(),"panel":m.bookmarks.panel.as_ref().map(|p|serde_json::json!({"stamp":p.stamp,"epoch":p.edit_epoch,"query":p.query,"input":p.input,"label":p.label,"editing":p.editing.is_some(),"error":p.local_error,"selected":format!("{:?}",p.selected)})),"buttons":m.hits.iter().filter_map(|h|if let Action::Bookmark(stamp,ref command)=h.action {Some(serde_json::json!({"stamp":stamp,"command":format!("{command:?}"),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}else{None}).collect::<Vec<_>>()})
}

#[cfg(all(test, unix))]
mod path_label_tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn history_caption_and_path_are_escaped_while_destination_remains_exact() {
        let path =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/fixture/raw\xff\n".to_vec()));
        let mut m = Model::new(path.clone());
        m.history
            .visit(&path, std::path::Path::new("/other"))
            .unwrap();
        let (caption, destination, _) = data(&m, Row::Back(0)).unwrap();
        assert_eq!(caption, "raw\\xFF\\n");
        assert_eq!(display_path(destination), "/fixture/raw\\xFF\\n");
        assert_eq!(destination, path);
        let mut panel = Panel::new(m.pane_owner());
        panel.query = "\\xFF".into();
        assert!(rows(&m, &panel).contains(&Row::Back(0)));
        assert_ne!(
            display_path(destination),
            display_path(Path::new("/fixture/raw\\xFF\\n"))
        );
    }
}
