//! Read-only home-trash catalog. Filesystem work stays on the existing AuxIo.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::trash::Catalog;
use fltk::{
    app,
    enums::{Event, Key},
    frame::Frame,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Clone)]
pub enum Command {
    Close,
    Reload,
    Select(usize),
    /// Back to its original folder (never replacing anything there).
    Restore,
    /// Delete the selected item permanently (asks first).
    Purge,
    /// Delete everything permanently (asks first).
    Empty,
}
pub struct Panel {
    pub id: u64,
    cancel: Arc<AtomicBool>,
    pub catalog: Catalog,
    error: String,
    loading: bool,
    selected: usize,
    scroll: i32,
    motion: fltk_omarchy::scroll::Scroll,
    pan: i32,
    pan_max: i32,
    page: i32,
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Panel {
    pub fn ready(&self) -> bool {
        !self.loading
    }
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut id = NEXT.load(Ordering::Relaxed);
        loop {
            let next = id.checked_add(1).expect("trash catalog IDs exhausted");
            match NEXT.compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => id = current,
            }
        }
        Self {
            id,
            cancel: Arc::new(AtomicBool::new(false)),
            catalog: Catalog::default(),
            error: String::new(),
            loading: false,
            selected: 0,
            scroll: 0,
            motion: Default::default(),
            pan: 0,
            pan_max: 0,
            page: 1,
        }
    }
    fn max(&self) -> i32 {
        (self.catalog.items.len() as i32 * 44 - self.page * 44).max(0)
    }
    fn select(&mut self, index: usize) {
        self.selected = index.min(self.catalog.items.len().saturating_sub(1));
        let y = self.selected as i32 * 44;
        if y < self.scroll {
            self.scroll = y;
        } else if y + 44 > self.scroll + self.page * 44 {
            self.scroll = y + 44 - self.page * 44;
        }
        self.motion.jump(self.scroll as i64);
        self.pan = 0;
    }
}
impl Model {
    pub fn open_trash_browser(&mut self) {
        if self.closing
            || self.search.visible
            || self.preview.visible()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.properties_dialog.is_some()
            || self.notice_open
            || self.mutation_applied
            || self.recovery_catalog.open
            || self.commands.is_some()
            || self.size_panel.is_some()
            || self.checksums.is_some()
            || self.comparison.is_some()
            || self.bookmarks.panel.is_some()
            || self.saved_searches.panel.is_some()
            || self.sftp.form.is_some()
        {
            return;
        }
        self.end_path_edit();
        self.help = None;
        self.context_menu = None;
        self.actions_menu = false;
        self.settings = false;
        self.wheel_scroll.cancel();
        self.thumbnails.suspend();
        self.hits.clear();
        self.trash_browser = Some(Panel::new());
        self.reload_trash_browser();
    }
    pub(crate) fn reload_trash_browser(&mut self) {
        let Some(old) = self.trash_browser.take() else {
            return;
        };
        drop(old);
        let mut panel = Panel::new();
        match crate::trash_history::root().and_then(|root| {
            self.aux
                .tx
                .try_send(crate::aux_io::Request::TrashCatalog(
                    panel.id,
                    root,
                    panel.cancel.clone(),
                ))
                .map_err(|_| "No se pudo leer la Papelera; pulsa F5 para reintentar".into())
        }) {
            Ok(()) => panel.loading = true,
            Err(error) => panel.error = error,
        }
        self.trash_browser = Some(panel);
    }
    pub fn trash_browser_command(&mut self, id: u64, command: Command) {
        let Some(panel) = self.trash_browser.as_mut().filter(|p| p.id == id) else {
            return;
        };
        match command {
            Command::Close => {
                self.trash_browser = None;
                self.hits.clear();
            }
            Command::Reload => {
                self.reload_trash_browser();
                self.hits.clear();
            }
            Command::Select(index) => panel.select(index),
            Command::Restore | Command::Purge | Command::Empty => {
                if panel.loading {
                    return;
                }
                let count = panel.catalog.items.len();
                let item = panel.catalog.items.get(panel.selected).map(|i| {
                    let label = files_core::name_label::label(
                        i.original
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .unwrap_or(&i.name),
                    );
                    (i.name.clone(), label)
                });
                let target = match (command, item) {
                    (Command::Empty, _) if count > 0 => {
                        crate::delete_ui::Target::TrashEmpty { count }
                    }
                    (Command::Restore, Some((name, _))) => {
                        crate::delete_ui::Target::TrashRestore { name }
                    }
                    (Command::Purge, Some((name, label))) => crate::delete_ui::Target::TrashItem {
                        name,
                        label: label.into_owned(),
                    },
                    _ => return,
                };
                self.request_trash_operation(target);
            }
        }
    }
    pub fn receive_trash_catalog(&mut self, id: u64, result: Result<Catalog, String>) -> bool {
        let Some(p) = self.trash_browser.as_mut().filter(|p| p.id == id) else {
            return false;
        };
        p.loading = false;
        match result {
            Ok(catalog) => {
                p.catalog = catalog;
                p.error.clear();
            }
            Err(e) => p.error = e,
        }
        true
    }
}
pub fn advance(m: &mut Model, frame: &Frame) {
    if let Some(p) = m.trash_browser.as_mut() {
        p.scroll = p.motion.position(p.scroll as i64, p.max() as i64) as i32;
        if p.motion.running() {
            fltk_omarchy::anim::request_redraw(frame);
        }
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut s) = m.trash_browser.take() else {
        return;
    };
    let c = m.palette.clone();
    let rect = Rect::new(12, 20, (w - 24).max(0), (h - 40).max(0));
    m.hits.clear();
    p.rounded_rect(rect, 8, c.darker_background);
    p.clip(rect);
    p.text(
        Rect::new(26, 28, w - 260, 32),
        "Papelera de Inicio",
        18,
        c.foreground,
        true,
        false,
    );
    let empty = s.catalog.items.is_empty() || s.loading;
    let mut buttons = vec![
        (w - 228, 100, "Recargar · F5", Command::Reload),
        (w - 116, 100, "Cerrar · Esc", Command::Close),
    ];
    if !empty {
        // Own row at the bottom: fits the 440px minimum window.
        let bw = ((w - 52 - 16) / 3).clamp(60, 170);
        let short = bw < 150;
        for (i, (long, brief, cmd)) in [
            ("Restaurar · R", "Restaurar", Command::Restore),
            ("Eliminar · Supr", "Eliminar", Command::Purge),
            ("Vaciar · Ctrl+Supr", "Vaciar", Command::Empty),
        ]
        .into_iter()
        .enumerate()
        {
            buttons.push((
                26 + i as i32 * (bw + 8),
                bw,
                if short { brief } else { long },
                cmd,
            ));
        }
    }
    for (i, (x, bw, label, cmd)) in buttons.into_iter().enumerate() {
        let r = Rect::new(x, if i < 2 { 28 } else { h - 54 }, bw, 30);
        p.rounded_rect(r, 4, c.lighter_background);
        p.text(r, label, 11, c.foreground, false, false);
        m.hits.push(Hit {
            generation: m.generation,
            rect: r,
            action: Action::TrashBrowser(s.id, cmd),
        });
    }
    let caption = if let Some(notice) = m.delete.trash_notice.clone().filter(|_| !s.loading) {
        notice
    } else if m.delete.busy() {
        "Procesando…".into()
    } else if s.loading {
        "Leyendo…".into()
    } else if !s.error.is_empty() {
        s.error.clone()
    } else if s.catalog.limited {
        format!(
            "{} elementos mostrados · límite de consulta alcanzado",
            s.catalog.items.len()
        )
    } else {
        format!(
            "{} elementos · consulta de la papelera del usuario",
            s.catalog.items.len()
        )
    };
    p.text(
        Rect::new(26, 66, w - 52, 28),
        &caption,
        12,
        if s.error.is_empty() {
            c.dark_foreground
        } else {
            c.red
        },
        false,
        false,
    );
    let body = Rect::new(26, 100, w - 52, (h - 220).max(44));
    s.page = (body.h / 44).max(1);
    s.scroll = s.scroll.clamp(0, s.max());
    p.clip(body);
    if !s.loading && s.error.is_empty() && s.catalog.items.is_empty() {
        p.text(
            body,
            "La Papelera está vacía",
            14,
            c.dark_foreground,
            false,
            false,
        );
    }
    for (i, item) in s
        .catalog
        .items
        .iter()
        .enumerate()
        .skip((s.scroll / 44) as usize)
        .take(s.page as usize + 2)
    {
        let r = Rect::new(body.x, body.y + i as i32 * 44 - s.scroll, body.w, 42);
        if i == s.selected {
            p.rounded_rect(r, 4, c.lighter_background);
        }
        let name = item
            .original
            .as_ref()
            .and_then(|p| p.file_name())
            .unwrap_or(&item.name);
        p.text(
            Rect::new(r.x + 8, r.y, r.w - 16, 23),
            &files_core::name_label::label(name),
            12,
            c.foreground,
            false,
            false,
        );
        p.text(
            Rect::new(r.x + 8, r.y + 23, r.w - 16, 18),
            item.issue
                .as_deref()
                .or(item.deletion_date.as_deref())
                .unwrap_or("Fecha no disponible"),
            11,
            c.dark_foreground,
            false,
            false,
        );
        let y = r.y.max(body.y);
        let bottom = (r.y + r.h).min(body.y + body.h);
        if bottom > y {
            m.hits.push(Hit {
                generation: m.generation,
                rect: Rect::new(r.x, y, r.w, bottom - y),
                action: Action::TrashBrowser(s.id, Command::Select(i)),
            });
        }
    }
    p.unclip();
    let origin = s
        .catalog
        .items
        .get(s.selected)
        .and_then(|i| i.original.as_ref())
        .map(|path| {
            format!(
                "Origen: {}",
                files_core::name_label::label(path.as_os_str())
            )
        })
        .unwrap_or_else(|| "Origen no disponible".into());
    s.pan_max = (p.text_width(&origin, 12, true).ceil() as i32 - body.w).max(0);
    s.pan = s.pan.clamp(0, s.pan_max);
    let detail = Rect::new(body.x, h - 108, body.w, 24);
    p.clip(detail);
    p.literal_text(
        Rect::new(detail.x - s.pan, detail.y, detail.w + s.pan, detail.h),
        &origin,
        12,
        c.foreground,
    );
    p.unclip();
    p.text(
        Rect::new(26, h - 80, w - 52, 22),
        "↑↓ / PgUp / PgDn · R restaurar · Supr eliminar · Ctrl+Supr vaciar · F5 recargar · Esc cerrar",
        11,
        c.dark_foreground,
        false,
        false,
    );
    if s.catalog.items.is_empty() || s.loading {
        p.text(
            Rect::new(26, h - 56, w - 52, 22),
            "Restaurar nunca reemplaza un archivo existente · eliminar no se puede deshacer",
            11,
            c.dark_foreground,
            false,
            false,
        );
    }
    p.unclip();
    m.trash_browser = Some(s);
}
pub fn event(m: &mut Model, ev: Event) -> bool {
    let Some(s) = m.trash_browser.as_mut() else {
        return false;
    };
    if matches!(ev, Event::Push | Event::KeyDown | Event::Shortcut) {
        s.motion.jump(s.scroll as i64);
    }
    match ev {
        Event::Push => {
            m.pointer_down(app::event_x(), app::event_y(), false, false, false);
        }
        Event::MouseWheel => {
            s.scroll = s.motion.wheel(
                s.scroll as i64,
                app::event_dy_value() as f64 * 38.0,
                s.max() as i64,
            ) as i32;
        }
        Event::KeyDown | Event::Shortcut => match app::event_key() {
            Key::Escape => {
                m.trash_browser = None;
                m.hits.clear();
            }
            Key::F5 => {
                m.reload_trash_browser();
                m.hits.clear();
            }
            Key::Down => s.select(s.selected.saturating_add(1)),
            Key::Up => s.select(s.selected.saturating_sub(1)),
            Key::PageDown => s.select(s.selected.saturating_add(s.page as usize)),
            Key::PageUp => s.select(s.selected.saturating_sub(s.page as usize)),
            Key::Home => s.select(0),
            Key::End => s.select(usize::MAX),
            Key::Delete => {
                let command = if app::event_state().intersects(fltk::enums::Shortcut::Ctrl) {
                    Command::Empty
                } else {
                    Command::Purge
                };
                let id = s.id;
                m.trash_browser_command(id, command);
            }
            k if k == Key::from_char('r') => {
                let id = s.id;
                m.trash_browser_command(id, Command::Restore);
            }
            Key::Left => s.pan = s.pan.saturating_sub(48),
            Key::Right => s.pan = s.pan.saturating_add(48).min(s.pan_max),
            _ => {}
        },
        _ => {}
    }
    true
}
