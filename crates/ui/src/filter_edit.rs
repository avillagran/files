//! Current browser filter input. Text is still owned by Model/query and live filtering.
use crate::{
    line_edit::{Cursor, Motion},
    line_field::Geometry,
    model::Model,
    painter::{Painter, Rect},
    path_edit::KeyResult,
};
use fltk::enums::{Key, Shortcut};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
fn epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut n = NEXT.load(Ordering::Relaxed);
    loop {
        let next = n.checked_add(1).expect("filter input epoch exhausted");
        match NEXT.compare_exchange_weak(n, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(value) => return value,
            Err(value) => n = value,
        }
    }
}
pub struct State {
    pub active: bool,
    pub cursor: Cursor,
    pub geometry: Geometry,
    pub epoch: u64,
    owner: (u64, u64),
    root: PathBuf,
    dragging: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            active: false,
            cursor: Default::default(),
            geometry: Default::default(),
            epoch: epoch(),
            owner: (0, 0),
            root: PathBuf::new(),
            dragging: false,
        }
    }
}
impl State {
    pub fn current(&self, m: &Model) -> bool {
        self.active && self.owner == m.pane_owner() && self.root == m.path
    }
    fn touch(&mut self) {
        self.epoch = epoch();
    }
}
impl Model {
    pub fn end_filter_edit(&mut self) {
        self.text_paste.invalidate();
        self.filter_edit = State::default();
    }
    pub fn begin_filter_edit(&mut self, all: bool) -> bool {
        if self.closing
            || self.bookmarks.panel.is_some()
            || self.commands.is_some()
            || self.size_panel.is_some()
            || self.sftp.form.is_some()
            || self.search.visible
            || self.preview.visible()
            || self.help.is_some()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.properties_dialog.is_some()
            || self.notice_open
            || self.mutation_applied
            || self.recovery_catalog.open
        {
            return false;
        }
        if self.view == crate::model::View::SpaceMap {
            self.action(crate::model::Action::View(crate::model::View::List));
        }
        if self.filter_edit.current(self) {
            self.text_paste.invalidate();
            if all {
                self.filter_edit.cursor.all(&self.query);
                self.filter_edit.touch();
            }
            return true;
        }
        if !self.filter_edit.current(self) {
            if files_core::navigation_history::validate(&self.path).is_err() {
                return false;
            }
            self.end_path_edit();
            self.filter_edit = State {
                active: true,
                owner: self.pane_owner(),
                root: self.path.clone(),
                ..Default::default()
            };
            self.filter_edit
                .cursor
                .select(&self.query, self.query.len(), self.query.len());
        }
        self.drag = None;
        self.text_paste.invalidate();
        self.session.restore_allowed = false;
        if all {
            self.filter_edit.cursor.all(&self.query);
        }
        self.filter_edit.touch();
        true
    }
    fn filter_changed(&mut self) {
        self.filter_edit.touch();
        self.preserve_empty_selection = false;
        self.scroll = 0;
        if self.error == crate::omnibar_input::TOO_LONG || self.error.starts_with("Filtro:") {
            self.error.clear();
        }
        self.rebuild();
    }
    pub fn filter_input(&mut self, text: &str) -> Result<(), &'static str> {
        self.filter_compose(text, 0)
    }
    pub fn filter_compose(&mut self, text: &str, delete: usize) -> Result<(), &'static str> {
        if !self.begin_filter_edit(false) {
            return Err("Filtro: termina la edición o diálogo actual");
        }
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\0'))
        {
            return Err("Filtro: una sola línea sin controles");
        }
        let before = self.filter_edit.cursor.revision;
        self.filter_edit
            .cursor
            .compose(&mut self.query, text, delete, false)?;
        if self.filter_edit.cursor.revision != before {
            self.filter_changed();
        }
        Ok(())
    }
    pub fn filter_key(&mut self, key: Key, mods: Shortcut, text: &str, delete: usize) -> KeyResult {
        if !self.filter_edit.active {
            return KeyResult::Unhandled;
        }
        if !self.filter_edit.current(self) {
            self.end_filter_edit();
            return KeyResult::Handled;
        }
        let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
        let shift = mods.intersects(Shortcut::Shift);
        if ctrl && matches!(key,k if k==Key::from_char('l') || k==Key::from_char('f')) {
            return KeyResult::Unhandled;
        }
        if ctrl && shift && (key == Key::from_char('a') || key == Key::from_char('d')) {
            self.action(crate::model::Action::Selection(
                if key == Key::from_char('a') {
                    crate::selection_tools::Command::Filtered
                } else {
                    crate::selection_tools::Command::Clear
                },
            ));
            return KeyResult::Handled;
        }
        self.text_paste.invalidate();
        let cursor = &mut self.filter_edit.cursor;
        match key {
            Key::Enter | Key::KPEnter | Key::Tab => self.end_filter_edit(),
            Key::Escape => {
                self.end_filter_edit();
                self.action(crate::model::Action::ClearFilter);
            }
            Key::Up | Key::Down | Key::PageUp | Key::PageDown => {
                self.end_filter_edit();
                return KeyResult::Unhandled;
            }
            Key::Left | Key::Right | Key::Home | Key::End => {
                cursor.motion(
                    &self.query,
                    match key {
                        Key::Home => Motion::Start,
                        Key::End => Motion::End,
                        Key::Left if ctrl => Motion::WordLeft,
                        Key::Right if ctrl => Motion::WordRight,
                        Key::Left => Motion::Left,
                        _ => Motion::Right,
                    },
                    shift,
                );
                self.filter_edit.touch();
            }
            Key::BackSpace | Key::Delete => {
                let before = self.query.len();
                cursor.delete(&mut self.query, key == Key::BackSpace, ctrl, false);
                if self.query.len() != before {
                    self.filter_changed();
                } else {
                    self.filter_edit.touch();
                }
            }
            k if ctrl && k == Key::from_char('a') => {
                cursor.all(&self.query);
                self.filter_edit.touch();
            }
            k if ctrl && (k == Key::from_char('z') || k == Key::from_char('y')) => {
                let before = cursor.revision;
                cursor.undo(&mut self.query, shift || k == Key::from_char('y'));
                if before != cursor.revision {
                    self.filter_changed();
                } else {
                    self.filter_edit.touch();
                }
            }
            k if ctrl && (k == Key::from_char('c') || k == Key::from_char('x')) => {
                let copy = self.query[cursor.range(&self.query)].to_string();
                if k == Key::from_char('x') && !copy.is_empty() {
                    cursor.delete(&mut self.query, false, false, false);
                    self.filter_changed();
                }
                return KeyResult::Copy(copy);
            }
            _ if ctrl => {}
            _ if !text.is_empty() && text.chars().all(|c| !c.is_control()) => {
                if let Err(e) = self.filter_compose(text, delete) {
                    self.error = e.into();
                }
            }
            _ => {} // A focused input never falls through to destructive file shortcuts.
        }
        KeyResult::Handled
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, area: Rect) {
    let c = m.palette.clone();
    if m.query.is_empty() && m.filter_edit.active {
        p.literal_text(
            Rect::new(area.x + 12, area.y, area.w - 16, area.h),
            "Filtrar… · Enter",
            13,
            c.dark_foreground,
        );
    }
    m.filter_edit.geometry.paint(
        p,
        &m.query,
        &m.filter_edit.cursor,
        area,
        &c,
        (m.filter_edit.active, m.filter_edit.epoch),
    );
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
    if !down && !drag {
        m.filter_edit.dragging = false;
        return false;
    }
    if down
        && (!m.filter_edit.geometry.contains(x, y) || (!m.filter_edit.active && m.query.is_empty()))
    {
        return false;
    }
    if drag && !m.filter_edit.dragging {
        return false;
    }
    let Some(at) = m.filter_edit.geometry.position(x, m.filter_edit.epoch) else {
        return m.filter_edit.active;
    };
    if !m.begin_filter_edit(false) {
        return false;
    }
    let c = &mut m.filter_edit.cursor;
    if down && double {
        c.component(&m.query, at);
    } else {
        let anchor = if drag || shift { c.anchor } else { at };
        c.select(&m.query, anchor, at);
    }
    m.filter_edit.dragging = true;
    m.filter_edit.touch();
    true
}
