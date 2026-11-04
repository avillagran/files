//! "Abrir con…" (Nautilus Open With): pick an application for the focused
//! file, optionally make it the default for its type. The desktop-entry scan
//! and the mimeapps.list write run off the UI thread.
use crate::model::{Action, Hit, Model};
use crate::painter::{Painter, Rect};
use files_core::apps::{App, Catalog};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Select(usize),
    Open,
    SetDefault,
    ToggleAll,
    Close,
}

struct Loaded {
    mime: String,
    catalog: Catalog,
    default: Option<String>,
    recommended: Vec<String>,
}

pub struct Panel {
    pub path: PathBuf,
    loading: Option<Receiver<Loaded>>,
    saving: Option<Receiver<Result<(), String>>>,
    data: Option<Loaded>,
    pub show_all: bool,
    pub selected: usize,
    scroll: usize,
    pub notice: Option<String>,
}

impl Panel {
    fn rows(&self) -> Vec<&App> {
        let Some(d) = &self.data else {
            return Vec::new();
        };
        // Like GNOME's chooser: NoDisplay entries stay out unless they are
        // the current default.
        let shown = |a: &&App| !a.no_display || d.default.as_deref() == Some(a.id.as_str());
        if self.show_all {
            d.catalog.apps.iter().filter(shown).collect()
        } else {
            d.recommended
                .iter()
                .filter_map(|id| files_core::apps::find(&d.catalog, id))
                .filter(shown)
                .collect()
        }
    }
    pub fn mime(&self) -> Option<&str> {
        self.data.as_ref().map(|d| d.mime.as_str())
    }
    pub fn default_id(&self) -> Option<&str> {
        self.data.as_ref().and_then(|d| d.default.as_deref())
    }
    pub fn row_ids(&self) -> Vec<String> {
        self.rows().iter().map(|a| a.id.clone()).collect()
    }
}

impl Model {
    /// Opens the chooser for the focused file (not folders: they navigate).
    pub fn open_with(&mut self) {
        if self.closing || self.open_with.is_some() {
            return;
        }
        let Some(entry) = self
            .selected
            .and_then(|i| self.entries.get(i))
            .filter(|e| files_core::visibility::accepts(&e.name, self.show_hidden))
        else {
            self.operation_message = Some("Selecciona un archivo para abrir con…".into());
            return;
        };
        let path = self.path.join(&entry.name);
        let (tx, rx) = mpsc::sync_channel(1);
        let probe = path.clone();
        let _ = std::thread::Builder::new()
            .name("files-apps".into())
            .spawn(move || {
                let mime = files_core::apps::detect_mime(&probe);
                let catalog = files_core::apps::scan();
                let mut recommended = files_core::apps::recommended(&mime, &catalog);
                let default = files_core::apps::default_for(&mime, &catalog);
                if let Some(d) = &default {
                    recommended.retain(|id| id != d);
                    recommended.insert(0, d.clone());
                }
                let _ = tx.send(Loaded {
                    mime,
                    catalog,
                    default,
                    recommended,
                });
            });
        self.context_menu = None;
        self.hits.clear();
        self.open_with = Some(Panel {
            path,
            loading: Some(rx),
            saving: None,
            data: None,
            show_all: false,
            selected: 0,
            scroll: 0,
            notice: None,
        });
    }
    pub fn open_with_command(&mut self, command: Command) {
        let Some(panel) = self.open_with.as_mut() else {
            return;
        };
        let rows = panel.row_ids();
        match command {
            Command::Close => {
                self.open_with = None;
                self.hits.clear();
            }
            Command::Select(i) => panel.selected = i.min(rows.len().saturating_sub(1)),
            Command::ToggleAll => {
                let current = rows.get(panel.selected).cloned();
                panel.show_all = !panel.show_all;
                let rows = panel.row_ids();
                panel.selected = current
                    .and_then(|c| rows.iter().position(|r| *r == c))
                    .unwrap_or(0);
                panel.scroll = 0;
            }
            Command::Open => {
                let Some(id) = rows.get(panel.selected).cloned() else {
                    return;
                };
                if self.open_job.is_some() {
                    panel.notice = Some("Ya hay una apertura en curso".into());
                    return;
                }
                let path = panel.path.clone();
                match crate::open_job::Job::start_with(path, Some(id)) {
                    Ok(job) => {
                        self.open_job = Some(job);
                        self.open_with = None;
                        self.hits.clear();
                    }
                    Err(e) => panel.notice = Some(e),
                }
            }
            Command::SetDefault => {
                let (Some(id), Some(mime)) = (
                    rows.get(panel.selected).cloned(),
                    panel.mime().map(str::to_string),
                ) else {
                    return;
                };
                if panel.saving.is_some() {
                    return;
                }
                let (tx, rx) = mpsc::sync_channel(1);
                let _ = std::thread::Builder::new()
                    .name("files-mimeapps".into())
                    .spawn(move || {
                        let _ = tx.send(
                            files_core::apps::set_default(&mime, &id).map_err(|e| e.to_string()),
                        );
                    });
                panel.saving = Some(rx);
                panel.notice = Some("Guardando…".into());
            }
        }
    }
    /// Polls the scan and the default write.
    pub fn pump_open_with(&mut self) -> bool {
        let Some(panel) = self.open_with.as_mut() else {
            return false;
        };
        let mut dirty = false;
        if let Some(rx) = &panel.loading
            && let Ok(data) = rx.try_recv()
        {
            panel.data = Some(data);
            panel.loading = None;
            if panel.row_ids().is_empty() {
                panel.show_all = true;
            }
            dirty = true;
        }
        if let Some(rx) = &panel.saving
            && let Ok(result) = rx.try_recv()
        {
            panel.saving = None;
            let id = panel.row_ids().get(panel.selected).cloned();
            match result {
                Ok(()) => {
                    if let (Some(d), Some(id)) = (panel.data.as_mut(), id) {
                        d.default = Some(id.clone());
                        d.recommended.retain(|r| *r != id);
                        d.recommended.insert(0, id.clone());
                        let name = files_core::apps::find(&d.catalog, &id)
                            .map(|a| a.name.clone())
                            .unwrap_or(id);
                        panel.notice =
                            Some(format!("{name} es ahora la predeterminada para {}", d.mime));
                        eprintln!(
                            "FILES_OPEN_WITH_DEFAULT mime={} app={}",
                            d.mime,
                            d.default.as_deref().unwrap_or("")
                        );
                    }
                    if !panel.show_all {
                        panel.selected = 0;
                    }
                }
                Err(e) => panel.notice = Some(format!("No se pudo fijar: {e}")),
            }
            dirty = true;
        }
        dirty
    }
}

fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action, strong: bool) {
    let c = m.palette.clone();
    p.rounded_rect(r, 6 * m.radius, if strong { c.accent } else { c.selection });
    p.text(
        Rect::new(r.x + 10, r.y, r.w - 20, r.h),
        label,
        12,
        if strong { c.background } else { c.accent },
        true,
        false,
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: r,
        action,
    });
}

pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut panel) = m.open_with.take() else {
        return;
    };
    m.hits.clear();
    let c = m.palette.clone();
    let cw = (w - 40).clamp(300, 620);
    let ch = (h - 60).clamp(260, 520);
    let (x, y) = ((w - cw) / 2, (h - ch) / 2);
    p.rounded_rect(
        Rect::new(x - 1, y - 1, cw + 2, ch + 2),
        10 * m.radius,
        c.muted,
    );
    p.rounded_rect(Rect::new(x, y, cw, ch), 10 * m.radius, c.darker_background);
    let name = files_core::name_label::label(panel.path.file_name().unwrap_or_default());
    p.text(
        Rect::new(x + 20, y + 14, cw - 40, 28),
        &format!("Abrir «{name}» con…"),
        16,
        c.bright_foreground,
        true,
        false,
    );
    let subtitle = match (&panel.data, panel.show_all) {
        (None, _) => "Buscando aplicaciones…".to_string(),
        (Some(d), false) => format!("Recomendadas para {}", d.mime),
        (Some(d), true) => format!("Todas las aplicaciones · tipo {}", d.mime),
    };
    p.text(
        Rect::new(x + 20, y + 42, cw - 40, 22),
        &subtitle,
        11,
        c.dark_foreground,
        false,
        false,
    );
    let body = Rect::new(x + 14, y + 70, cw - 28, ch - 70 - 96);
    let page = (body.h / 34).max(1) as usize;
    let rows: Vec<(String, String)> = panel
        .rows()
        .iter()
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();
    if panel.selected < panel.scroll {
        panel.scroll = panel.selected;
    } else if panel.selected >= panel.scroll + page {
        panel.scroll = panel.selected + 1 - page;
    }
    p.clip(body);
    if panel.data.is_some() && rows.is_empty() {
        p.text(
            body,
            "No hay aplicaciones para este tipo · Tab muestra todas",
            12,
            c.dark_foreground,
            false,
            false,
        );
    }
    let default = panel.default_id().map(str::to_string);
    for (k, (id, label)) in rows.iter().enumerate().skip(panel.scroll).take(page) {
        let r = Rect::new(body.x, body.y + (k - panel.scroll) as i32 * 34, body.w, 32);
        if k == panel.selected {
            p.rounded_rect(r, 4 * m.radius, c.selection);
        }
        let tag = if default.as_deref() == Some(id.as_str()) {
            "  · predeterminada"
        } else {
            ""
        };
        p.text(
            Rect::new(r.x + 10, r.y, r.w - 20, r.h),
            &format!("{label}{tag}"),
            12,
            c.foreground,
            false,
            false,
        );
        m.hits.push(Hit {
            generation: m.generation,
            rect: r,
            action: Action::OpenWithCmd(Command::Select(k)),
        });
    }
    p.unclip();
    if let Some(n) = &panel.notice {
        p.text(
            Rect::new(x + 20, y + ch - 92, cw - 40, 22),
            n,
            11,
            c.accent,
            false,
            false,
        );
    }
    let by = y + ch - 56;
    let bw = ((cw - 40 - 24) / 4).max(60);
    let compact = bw < 140;
    let labels = if compact {
        [
            "Abrir",
            "Siempre",
            if panel.show_all { "Recom." } else { "Todas" },
            "Cancelar",
        ]
    } else {
        [
            "Abrir · Enter",
            "Usar siempre · Ctrl+Enter",
            if panel.show_all {
                "Recomendadas · Tab"
            } else {
                "Todas · Tab"
            },
            "Cancelar · Esc",
        ]
    };
    let actions = [
        Command::Open,
        Command::SetDefault,
        Command::ToggleAll,
        Command::Close,
    ];
    m.open_with = Some(panel);
    for (i, (label, cmd)) in labels.iter().zip(actions).enumerate() {
        button(
            p,
            m,
            Rect::new(x + 20 + i as i32 * (bw + 8), by, bw, 32),
            label,
            Action::OpenWithCmd(cmd),
            i == 0,
        );
    }
}

pub fn event(m: &mut Model, ev: fltk::enums::Event) -> bool {
    use fltk::{
        app,
        enums::{Event, Key, Shortcut},
    };
    let Some(panel) = m.open_with.as_ref() else {
        return false;
    };
    let n = panel.row_ids().len();
    let sel = panel.selected;
    match ev {
        Event::KeyDown | Event::Shortcut => {
            let ctrl = app::event_state().intersects(Shortcut::Ctrl);
            match app::event_key() {
                Key::Escape => m.open_with_command(Command::Close),
                Key::Enter | Key::KPEnter if ctrl => m.open_with_command(Command::SetDefault),
                Key::Enter | Key::KPEnter => m.open_with_command(Command::Open),
                Key::Tab => m.open_with_command(Command::ToggleAll),
                Key::Down => {
                    m.open_with_command(Command::Select((sel + 1).min(n.saturating_sub(1))))
                }
                Key::Up => m.open_with_command(Command::Select(sel.saturating_sub(1))),
                Key::Home => m.open_with_command(Command::Select(0)),
                Key::End => m.open_with_command(Command::Select(n.saturating_sub(1))),
                Key::PageDown => {
                    m.open_with_command(Command::Select((sel + 8).min(n.saturating_sub(1))))
                }
                Key::PageUp => m.open_with_command(Command::Select(sel.saturating_sub(8))),
                _ => {}
            }
        }
        Event::Push => {
            let before = m.open_with.as_ref().map(|p| p.selected);
            m.pointer_down(app::event_x(), app::event_y(), false, false, false);
            // Double click on a row opens with it.
            if app::event_clicks() && m.open_with.as_ref().map(|p| p.selected) == before {
                m.open_with_command(Command::Open);
            }
        }
        Event::MouseWheel => {
            let step = if app::event_dy_value() > 0 { 1 } else { -1i32 };
            let next = (sel as i32 + step).clamp(0, n.saturating_sub(1) as i32) as usize;
            m.open_with_command(Command::Select(next));
        }
        _ => {}
    }
    true
}
