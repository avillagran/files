//! Mutation dialogs share one bounded worker; closing always drains its response.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
    rename_dialog::{Dialog, Mode},
    rename_job::{Request, Response},
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
use std::{ffi::OsStr, sync::Arc, time::Duration};

fn refresh_matches(
    id: u64,
    expected: u64,
    affected: Option<&std::path::Path>,
    current: &std::path::Path,
    canonical: Option<&std::path::Path>,
) -> bool {
    id == 0 || id != expected || affected.is_some_and(|p| p == current || Some(p) == canonical)
}

fn preflight_selection<'a>(names: impl Iterator<Item = &'a OsStr>) -> Result<(), String> {
    let mut bytes = 0usize;
    for (count, name) in names.enumerate() {
        if count >= 100_000 {
            return Err("Reduce el lote: máximo 100000 seleccionados".into());
        }
        let size = name.as_encoded_bytes().len();
        bytes = bytes
            .checked_add(size)
            .ok_or("Presupuesto de nombres excedido")?;
        if size > 4096 || bytes > 8 * 1024 * 1024 {
            return Err("Reduce el lote: máximo 8MiB de nombres y 4096 bytes por nombre".into());
        }
    }
    Ok(())
}

impl Model {
    pub fn send_trash(&mut self) {
        if self.mutation_applied || self.copy_busy() || self.mutations.busy {
            self.operation_message =
                Some("Espera la tarea actual antes de enviar a Papelera".into());
            return;
        }
        let selected = || {
            self.entries.iter().enumerate().filter(|(i, _)| {
                self.selection.contains(i) || self.selection.is_empty() && self.selected == Some(*i)
            })
        };
        if let Err(error) =
            crate::trash_history::preflight(&self.path, selected().map(|(_, e)| e.name.as_os_str()))
        {
            self.operation_message = Some(error);
            self.operation_details.clear();
            self.copy_notice_id = None;
            self.notice_open = true;
            return;
        }
        let root = match crate::trash_history::root() {
            Ok(root) => root,
            Err(error) => {
                self.operation_message = Some(error);
                self.operation_details.clear();
                self.copy_notice_id = None;
                self.notice_open = true;
                return;
            }
        };
        let names = selected()
            .map(|(_, e)| e.name.clone())
            .collect::<Vec<_>>()
            .into();
        let id = self.next_mutation_id();
        let request = Request::Trash {
            id,
            directory: self.path.clone(),
            names,
            root,
        };
        if self.mutations.submit(request).is_ok() {
            self.preview.close();
            self.rename_dialog = None;
            self.settings = false;
            self.actions_menu = false;
            self.notice_open = false;
            self.mutation_applied = true;
            self.hits.clear();
        } else {
            self.operation_message =
                Some("No se pudo iniciar la tarea de Papelera; vuelve a intentarlo".into());
        }
    }
    pub(crate) fn next_mutation_id(&mut self) -> u64 {
        self.mutation_id = self
            .mutation_id
            .checked_add(1)
            .expect("mutation ID exhausted");
        self.mutation_id
    }
    pub fn open_rename(&mut self, mode: Mode) {
        if self.mutation_applied || self.copy_busy() {
            return;
        }
        // Any pending preview is cancelled; its ACK must be drained before the next
        // request. IDs are process-global, never restarted with a new dialog.
        self.mutations.cancel();
        let selected: Arc<[std::ffi::OsString]> = if mode == Mode::Rename {
            let selected_indices = || {
                self.entries.iter().enumerate().filter(|(i, _)| {
                    self.selection.contains(i)
                        || self.selection.is_empty() && self.selected == Some(*i)
                })
            };
            if let Err(error) =
                preflight_selection(selected_indices().map(|(_, e)| e.name.as_os_str()))
            {
                self.operation_message = Some(error);
                self.operation_details.clear();
                self.copy_notice_id = None;
                self.notice_open = true;
                return;
            }
            let mut indices = selected_indices().map(|(i, _)| i).collect::<Vec<_>>();
            indices.sort_by_cached_key(|i| {
                files_core::natural_sort::NaturalKeyRef::new(&self.entries[*i].name)
            });
            indices
                .into_iter()
                .map(|i| self.entries[i].name.clone())
                .collect::<Vec<_>>()
                .into()
        } else {
            Arc::from([])
        };
        if mode == Mode::Rename && selected.is_empty() {
            return;
        }
        let id = self.next_mutation_id();
        self.rename_dialog = Some(Dialog::new(id, self.path.clone(), selected, mode));
        self.notice_open = false;
        self.settings = false;
        self.hits.clear();
    }
    pub fn cancel_rename(&mut self) {
        self.mutations.cancel();
        if !self.mutation_applied {
            self.rename_dialog = None;
        }
    }
    pub fn apply_rename(&mut self) {
        if self.mutations.busy || self.mutation_applied {
            return;
        }
        let Some(mut dialog) = self.rename_dialog.take() else {
            return;
        };
        let id = self.next_mutation_id();
        let request = if dialog.mode == Mode::Rename {
            let Some(prepared) = dialog.prepared.take().filter(|p| !p.is_empty()) else {
                self.rename_dialog = Some(dialog);
                return;
            };
            Request::Apply { id, prepared }
        } else {
            Request::Create {
                id,
                directory: dialog.directory.clone(),
                name: dialog.fields[0].clone(),
                folder: dialog.mode == Mode::Folder,
            }
        };
        dialog.id = id;
        match self.mutations.submit(request) {
            Ok(()) => {
                self.mutation_applied = true;
                self.rename_dialog = Some(dialog);
            }
            Err(_) => {
                dialog.error =
                    "El trabajador no aceptó la operación; vuelve a calcular la vista previa"
                        .into();
                dialog.changed = dialog.mode == Mode::Rename;
                self.rename_dialog = Some(dialog);
            }
        }
    }
    pub fn undo_rename(&mut self, redo: bool) {
        if self.mutations.busy
            || self.mutation_applied
            || if redo { !self.can_redo } else { !self.can_undo }
        {
            return;
        }
        let id = self.next_mutation_id();
        if self.mutations.submit(Request::History { id, redo }).is_ok() {
            self.mutation_applied = true;
            self.rename_dialog = None;
            self.notice_open = false;
        }
    }
    pub fn pump_mutations(&mut self) -> bool {
        let mut dirty = false;
        if let Some(response) = self.mutations.poll() {
            dirty = true;
            match response {
                Response::Properties(id, reply) => self.properties_reply(id, *reply),
                Response::Planned(id, result) => {
                    if let Some(dialog) = &mut self.rename_dialog
                        && dialog.id == id
                        && !dialog.changed
                    {
                        match result {
                            Ok(prepared) => dialog.prepared = Some(prepared),
                            Err(error) => dialog.error = error,
                        }
                    }
                }
                Response::Finished(id, mut outcome, refresh_directory) => {
                    self.invalidate_sizes();
                    if std::env::var_os("FILES_TEST_MUTATIONS").as_deref() == Some(OsStr::new("1"))
                    {
                        eprintln!(
                            "FILES_MUTATION_FINISHED id={id} success={} undo={} redo={}",
                            outcome.success, outcome.undo, outcome.redo
                        );
                    }
                    if id != 0 && id != self.mutation_id {
                        outcome.success = false;
                        outcome.details.push(format!("Respuesta inesperada del trabajador: solicitud {id}, actual {}. Revisa los nombres y rutas antes de continuar.",self.mutation_id));
                    }
                    self.mutation_applied = false;
                    self.can_undo = outcome.undo;
                    self.can_redo = outcome.redo;
                    self.operation_message = Some(outcome.message);
                    self.operation_details = outcome.details;
                    self.notice_scroll = 0;
                    self.notice_pan = 0;
                    self.notice_width = 0;
                    self.copy_notice_id = None;
                    self.notice_open = !outcome.success || !self.operation_details.is_empty();
                    self.rename_dialog = None;
                    self.properties_dialog = None;
                    // Apply blocks tab changes. Global Undo can nevertheless belong
                    // to another tab: refresh only its captured affected directory.
                    if let Some(path) = refresh_directory.as_ref() {
                        self.dual.refresh_other(std::iter::once(path.clone()));
                    } else if (id == 0 || id != self.mutation_id)
                        && let Some(other) = self.dual.other.as_ref()
                    {
                        self.dual.refresh_other(std::iter::once(other.path.clone()));
                    }
                    if refresh_matches(
                        id,
                        self.mutation_id,
                        refresh_directory.as_deref(),
                        &self.path,
                        self.layout_path.as_deref(),
                    ) {
                        self.refresh_current();
                    }
                    if self.notice_open {
                        self.closing = false;
                    }
                }
            }
        }
        if self.closing {
            if self.mutations.busy {
                self.mutations.cancel();
            }
            return dirty;
        }
        let ready = !self.mutations.busy
            && !self.mutation_applied
            && self
                .rename_dialog
                .as_ref()
                .is_some_and(|d| d.changed && d.edited_at.elapsed() >= Duration::from_millis(150));
        if ready {
            let id = self.next_mutation_id();
            let dialog = self.rename_dialog.as_mut().expect("ready dialog");
            dialog.id = id;
            match dialog.options() {
                Ok(options) => {
                    let request = Request::Plan {
                        id,
                        directory: dialog.directory.clone(),
                        selected: Arc::clone(&dialog.selected),
                        options,
                    };
                    if self.mutations.submit(request).is_ok() {
                        dialog.changed = false;
                    } else {
                        dialog.error = "No se pudo solicitar la vista previa".into();
                        dialog.changed = false;
                    }
                }
                Err(error) => {
                    dialog.error = error;
                    dialog.changed = false;
                }
            }
            dirty = true;
        }
        dirty
    }
    /// This is independent of preferences/discard: staging and rollback must finish.
    pub fn mutations_drained(&self) -> bool {
        !self.mutations.busy && !self.mutation_applied
    }
}

pub fn event(m: &mut Model, ev: Event) -> bool {
    if m.recovery_catalog.open {
        match ev {
            Event::KeyDown | Event::Shortcut if app::event_key() == Key::Escape => {
                m.recovery_catalog.close()
            }
            Event::MouseWheel => {
                m.recovery_catalog.scroll = (m.recovery_catalog.scroll as i64
                    + app::event_dy_value() as i64 * 3)
                    .max(0) as usize;
            }
            Event::Push if app::event_button() == 1 => {
                let action = m
                    .hits
                    .iter()
                    .rev()
                    .find(|h| {
                        h.generation == m.generation
                            && h.rect.contains(app::event_x(), app::event_y())
                    })
                    .map(|h| h.action.clone());
                if let Some(a) = action {
                    m.action(a);
                }
            }
            _ => (),
        }
        return true;
    }
    match ev {
        Event::Push if app::event_button() == 1 => {
            let action = m
                .hits
                .iter()
                .rev()
                .find(|h| {
                    h.generation == m.generation && h.rect.contains(app::event_x(), app::event_y())
                })
                .map(|h| h.action.clone());
            if let Some(action) = action {
                m.action(action);
            }
        }
        Event::MouseWheel => {
            let delta = app::event_dy_value();
            if m.notice_open {
                if app::event_state().intersects(Shortcut::Shift) {
                    pan_notice(m, delta.saturating_mul(60));
                } else {
                    navigate_notice(
                        m,
                        crate::notice_navigation::Command::Rows(delta.saturating_mul(3)),
                    );
                }
            } else if let Some(d) = &mut m.rename_dialog {
                let len = d.prepared.as_ref().map_or(0, |p| p.len());
                d.scroll = (d.scroll as i64 + delta as i64 * 3)
                    .clamp(0, len.saturating_sub(1) as i64) as usize;
            }
        }
        Event::Paste if !m.mutations.busy => {
            if let Some(d) = &mut m.rename_dialog {
                d.input(&app::event_text());
            }
        }
        Event::KeyDown | Event::Shortcut => {
            let key = app::event_key();
            let ctrl = app::event_state().intersects(Shortcut::Ctrl | Shortcut::Meta);
            if archive_notice_shortcut(m, key, ctrl) {
                return true;
            }
            if key == Key::Escape {
                if m.notice_open {
                    m.notice_open = false;
                } else {
                    m.cancel_rename();
                }
            } else if m.notice_open {
                if notice_shortcut(m, key, ctrl) {
                    return true;
                }
                if ctrl && key == Key::from_char('c') {
                    m.action(Action::CopyNotice);
                }
                if key == Key::Enter {
                    m.notice_open = false;
                }
            } else if !m.mutations.busy {
                if key == Key::Enter {
                    m.apply_rename();
                } else if let Some(d) = &mut m.rename_dialog {
                    if key == Key::Tab {
                        d.next_field(app::event_state().intersects(Shortcut::Shift));
                    } else if key == Key::BackSpace {
                        d.backspace();
                    } else if ctrl && key == Key::from_char('a') {
                        d.replace = true;
                    } else if ctrl && key == Key::from_char('v') {
                        // Canvas routes paste before borrowing Model. Never invoke
                        // the synchronous backend while holding this mutable borrow.
                    } else if !ctrl {
                        d.input(&app::event_text());
                    }
                }
            }
        }
        _ => {}
    }
    true
}
pub(crate) fn archive_notice_shortcut(m: &mut Model, key: Key, ctrl: bool) -> bool {
    if !m.notice_open {
        return false;
    }
    if let Some(c) = m.archive_confirmation {
        if key == Key::Escape {
            m.action(Action::ArchiveBack(c.stamp));
            return true;
        }
        if matches!(key, Key::Enter | Key::KPEnter) {
            if ctrl {
                m.action(Action::ArchiveCommit(c.stamp));
            }
            return true;
        }
        if ctrl && key == Key::from_char('p') {
            m.action(Action::ArchivePause(c.stamp));
            return true;
        }
    }
    if ctrl && matches!(key,k if k==Key::from_char('k') || k==Key::from_char('s')) {
        if let Some(id) = m.copy_notice_id {
            m.action(Action::ArchiveReview(
                id,
                if key == Key::from_char('k') {
                    crate::archive_operation::RecoveryAction::Cleanup
                } else {
                    crate::archive_operation::RecoveryAction::Sync
                },
            ));
        }
        return true;
    }
    false
}
pub(crate) fn notice_shortcut(m: &mut Model, key: Key, ctrl: bool) -> bool {
    if m.notice_open && ctrl && key == Key::from_char('r') {
        m.action(Action::RetryPendingMove);
        return true;
    }
    if m.notice_open {
        if key == Key::Left || key == Key::Right {
            pan_notice(m, if key == Key::Left { -80 } else { 80 });
            return true;
        }
        use crate::notice_navigation::Command;
        let command = match key {
            Key::Up => Some(Command::Rows(-1)),
            Key::Down => Some(Command::Rows(1)),
            Key::PageUp => Some(Command::Pages(-1)),
            Key::PageDown => Some(Command::Pages(1)),
            Key::Home => Some(Command::Start),
            Key::End => Some(Command::End),
            _ => None,
        };
        if let Some(command) = command {
            m.action(Action::NoticeNavigate(command));
            return true;
        }
    }
    false
}
pub fn navigate_notice(m: &mut Model, command: crate::notice_navigation::Command) {
    if m.notice_open {
        m.notice_scroll = crate::notice_navigation::navigate(
            m.notice_scroll,
            m.operation_details.len(),
            m.notice_rows,
            command,
        );
    }
}
pub fn pan_notice(m: &mut Model, delta: i32) {
    m.notice_pan = m
        .notice_pan
        .saturating_add(delta)
        .clamp(0, m.notice_pan_limit);
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 6 * m.radius, m.palette.selection);
    p.text(
        Rect::new(r.x + 10, r.y, r.w - 20, r.h),
        label,
        12,
        m.palette.accent,
        true,
        false,
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: r,
        action,
    });
}
/// Compact notices occupy their own browser space, above the optional MC bar.
pub fn compact_notice_visible(m: &Model) -> bool {
    !m.closing
        && !m.recovery_catalog.open
        && !m.notice_open
        && !m.actions_menu
        && !m.settings
        && m.context_menu.is_none()
        && !m.copy_busy()
        && !m.preview.visible()
        && m.copy_form.is_none()
        && m.rename_dialog.is_none()
        && m.properties_dialog.is_none()
        && !m.mutation_applied
        && m.operation_message.is_some()
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    if m.recovery_catalog.open {
        crate::move_catalog::paint(p, m, w, h);
        return;
    }
    if compact_notice_visible(m)
        && let Some(message) = m.operation_message.clone()
    {
        let x = if m.rail { 186 } else { 68 };
        let inspector_shown = !m.dual.enabled && m.inspector && w >= 800;
        let width = (w - x - 24 - if inspector_shown { 280 } else { 0 }).clamp(160, 540);
        let c = m.palette.clone();
        p.rounded_rect(
            Rect::new(x, h - 80 - crate::operation_bar::reserve(m), width, 38),
            7 * m.radius,
            c.selection,
        );
        let reserve = if m.can_undo || m.can_redo { 120 } else { 0 };
        p.text(
            Rect::new(
                x + 12,
                h - 80 - crate::operation_bar::reserve(m),
                width - reserve - 20,
                38,
            ),
            &message,
            12,
            c.bright_foreground,
            true,
            false,
        );
        if m.can_undo || m.can_redo {
            button(
                p,
                m,
                Rect::new(
                    x + width - 116,
                    h - 76 - crate::operation_bar::reserve(m),
                    108,
                    30,
                ),
                if m.can_redo { "Rehacer" } else { "Deshacer" },
                Action::Undo(m.can_redo),
            );
        }
    }
    if m.notice_open {
        m.hits.clear();
        let width = (w - 36).min(940);
        let height = (h - 50).min(580);
        let x = (w - width) / 2;
        let y = (h - height) / 2;
        let c = m.palette.clone();
        p.rounded_rect(Rect::new(x, y, width, height), 10 * m.radius, c.muted);
        p.rounded_rect(
            Rect::new(x + 1, y + 1, width - 2, height - 2),
            10 * m.radius,
            c.darker_background,
        );
        let confirmation = m.archive_confirmation;
        let title = confirmation.map(|c| {
            format!(
                "{} #{}\nCtrl+Enter confirma · Esc vuelve",
                c.action.title(),
                c.outcome
            )
        });
        p.text(
            Rect::new(x + 20, y + 12, width - 40, 64),
            title
                .as_deref()
                .unwrap_or_else(|| m.operation_message.as_deref().unwrap_or("Resultado")),
            14,
            c.bright_foreground,
            true,
            false,
        );
        let top = y + 84 + if confirmation.is_some() { 54 } else { 0 };
        if let Some(c) = confirmation {
            let explanation = match c.action {
                crate::archive_operation::RecoveryAction::Cleanup => {
                    "Revalida el temporal antes de limpiar.\nSe detiene ante cambios detectados."
                }
                crate::archive_operation::RecoveryAction::Sync => {
                    "Confirma guardado del directorio.\nNo vuelve a extraer ni a copiar."
                }
            };
            p.text(
                Rect::new(x + 20, y + 76, width - 40, 54),
                &format!(
                    "{explanation}\nAntes de {} trabajos pendientes.",
                    m.copy.queued_count()
                ),
                11,
                m.palette.dark_foreground,
                true,
                false,
            );
        }
        let actions = m.recovery_actions();
        let retry = m.pending_move_retry_count().is_some();
        let archive = m.archive_recovery_actions();
        let recovery_count = if confirmation.is_some() {
            2 + usize::from(m.copy.queued_count() > 0)
        } else {
            usize::from(actions.0)
                + usize::from(actions.1)
                + usize::from(retry)
                + usize::from(archive.0)
                + usize::from(archive.1)
        };
        let recovery = recovery_count > 0;
        let recovery_rows = if width >= 560 {
            usize::from(recovery)
        } else {
            recovery_count
        };
        let controls_top = y + height - 48 - (recovery_rows as i32 * 40);
        let bottom = controls_top - 42;
        m.notice_rows = ((bottom - top).max(0) / 30).max(1) as usize;
        m.notice_scroll = crate::notice_navigation::navigate(
            m.notice_scroll,
            m.operation_details.len(),
            m.notice_rows,
            crate::notice_navigation::Command::Rows(0),
        );
        m.notice_width = m
            .operation_details
            .iter()
            .skip(m.notice_scroll)
            .take(m.notice_rows)
            .map(|text| p.text_width(text, 11, true).ceil() as i32)
            .max()
            .unwrap_or(0);
        let maximum_pan = (m.notice_width - (width - 40)).max(0);
        m.notice_pan_limit = maximum_pan;
        m.notice_pan = m.notice_pan.clamp(0, maximum_pan);
        p.clip(Rect::new(x + 20, top, width - 40, (bottom - top).max(0)));
        for (row, text) in m
            .operation_details
            .iter()
            .skip(m.notice_scroll)
            .take(m.notice_rows)
            .enumerate()
        {
            p.text(
                Rect::new(
                    x + 20 - m.notice_pan,
                    top + row as i32 * 30,
                    m.notice_width.max(width - 40),
                    28,
                ),
                text,
                11,
                c.foreground,
                true,
                false,
            );
        }
        p.unclip();
        use crate::notice_navigation::Command;
        let last = crate::notice_navigation::last_start(m.operation_details.len(), m.notice_rows);
        let nav_y = controls_top - 36;
        for (index, (label, fallback, command, enabled)) in [
            ("", "|<", Command::Start, m.notice_scroll > 0),
            ("", "^", Command::Pages(-1), m.notice_scroll > 0),
            ("", "v", Command::Pages(1), m.notice_scroll < last),
            ("", ">|", Command::End, m.notice_scroll < last),
        ]
        .into_iter()
        .enumerate()
        {
            let r = Rect::new(x + 20 + index as i32 * 34, nav_y, 30, 28);
            p.rounded_rect(r, 4 * m.radius, c.selection);
            p.text(
                Rect::new(r.x + 7, r.y, r.w - 10, r.h),
                crate::fonts::icon(label, fallback),
                12,
                if enabled { c.accent } else { c.dark_foreground },
                true,
                false,
            );
            if enabled {
                m.hits.push(Hit {
                    generation: m.generation,
                    rect: r,
                    action: Action::NoticeNavigate(command),
                });
            }
        }
        let total = m.operation_details.len();
        let first = if total == 0 { 0 } else { m.notice_scroll + 1 };
        let end = m.notice_scroll.saturating_add(m.notice_rows).min(total);
        p.text(
            Rect::new(x + 160, nav_y - 1, width - 180, 16),
            &format!("{first}–{end} / {total} · ↑↓ PgUp/PgDn"),
            10,
            c.foreground,
            true,
            false,
        );
        p.text(
            Rect::new(x + 160, nav_y + 14, width - 180, 16),
            if maximum_pan > 0 {
                "Home/End · ←→ Shift+rueda"
            } else {
                "Home/End · rueda"
            },
            9,
            c.dark_foreground,
            true,
            false,
        );
        let mut recovery_buttons = Vec::new();
        if let Some(c) = confirmation {
            recovery_buttons.push((
                if c.paused {
                    " Iniciar en pausa · Ctrl+P"
                } else {
                    " Iniciar en pausa · Ctrl+P"
                },
                Action::ArchivePause(c.stamp),
            ));
            if m.copy.queued_count() > 0 {
                recovery_buttons
                    .push((" Vaciar pendientes", Action::ArchiveClearPending(c.stamp)));
            }
            recovery_buttons.push(("󰄬 Confirmar · Ctrl+Enter", Action::ArchiveCommit(c.stamp)));
        } else if let Some(id) = m.copy_notice_id {
            if archive.0 {
                recovery_buttons.push((
                    "󰆴 Limpiar temporal · Ctrl+K",
                    Action::ArchiveReview(id, crate::archive_operation::RecoveryAction::Cleanup),
                ));
            }
            if archive.1 {
                recovery_buttons.push((
                    "󰄬 Confirmar guardado · Ctrl+S",
                    Action::ArchiveReview(id, crate::archive_operation::RecoveryAction::Sync),
                ));
            }
        }
        if retry && confirmation.is_none() {
            recovery_buttons.push((
                crate::fonts::icon(
                    "  Reintentar pendientes · Ctrl+R",
                    "Reintentar pendientes · Ctrl+R",
                ),
                Action::RetryPendingMove,
            ));
        }
        if actions.0 && confirmation.is_none() {
            recovery_buttons.push((
                crate::fonts::icon("  Finalizar", "Finalizar"),
                Action::RecoverMove(crate::move_operation::RecoveryAction::Finish),
            ));
        }
        if actions.1 && confirmation.is_none() {
            recovery_buttons.push((
                crate::fonts::icon("  Restaurar origen", "Restaurar origen"),
                Action::RecoverMove(crate::move_operation::RecoveryAction::Restore),
            ));
        }
        for (index, (label, action)) in recovery_buttons.into_iter().enumerate() {
            let count = if width >= 560 {
                recovery_count as i32
            } else {
                1
            };
            let bw = (width - 40 - (count - 1) * 12) / count;
            let bx = x
                + 20
                + if width >= 560 {
                    index as i32 * (bw + 12)
                } else {
                    0
                };
            let by = controls_top + if width >= 560 { 0 } else { index as i32 * 40 };
            button(p, m, Rect::new(bx, by, bw, 32), label, action);
        }
        let mut controls = vec![("Copiar informe", Action::CopyNotice)];
        if m.notice_copy_outcome().is_some() {
            controls.push(("Detalle", Action::CopyFullReport));
        }
        controls.push(if let Some(c) = confirmation {
            ("Volver", Action::ArchiveBack(c.stamp))
        } else {
            ("Continuar", Action::DismissNotice)
        });
        let count = controls.len() as i32;
        let bw = (width - 40 - (count - 1) * 8) / count;
        for (index, (label, action)) in controls.into_iter().enumerate() {
            button(
                p,
                m,
                Rect::new(x + 20 + index as i32 * (bw + 8), y + height - 48, bw, 32),
                label,
                action,
            );
        }
    } else if let Some(dialog) = m.rename_dialog.take() {
        crate::rename_dialog::paint(p, m, &dialog, w, h);
        m.rename_dialog = Some(dialog);
    } else if m.mutation_applied {
        m.hits.clear();
        let x = (w - 560) / 2;
        let y = (h - 150) / 2;
        p.rounded_rect(
            Rect::new(x, y, 560, 150),
            10 * m.radius,
            m.palette.darker_background,
        );
        p.text(
            Rect::new(x + 20, y + 12, 520, 64),
            "Aplicando cambios…\nCancelar se solicita entre pasos; espera el resultado.",
            13,
            m.palette.foreground,
            true,
            false,
        );
        button(
            p,
            m,
            Rect::new(x + 20, y + 96, 150, 32),
            "Cancelar tarea",
            Action::RenameCancel,
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::{
        listing::{Entry, Kind},
        rename_transaction::PreparedRename,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "mutation-ui-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn model(dir: &Fixture) -> Model {
        let mut m = Model::new(dir.0.clone());
        m.entries = vec![Entry {
            name: "before".into(),
            kind: Kind::File,
            metadata: None,
            stat_attempted: false,
        }];
        m.order = vec![0];
        m.select_entry(0, false, false);
        m
    }
    fn drain(m: &mut Model) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while m.mutations.busy {
            m.pump_mutations();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn selection_preflight_is_bounded_before_cloning_and_preserves_raw_names() {
        use std::os::unix::ffi::OsStrExt;
        let n = OsStr::new("a");
        assert!(preflight_selection(std::iter::repeat_n(n, 100_000)).is_ok());
        assert!(preflight_selection(std::iter::repeat_n(n, 100_001)).is_err());
        let long = "a".repeat(4096);
        assert!(preflight_selection(std::iter::repeat_n(OsStr::new(&long), 2048)).is_ok());
        assert!(preflight_selection(std::iter::repeat_n(OsStr::new(&long), 2049)).is_err());
        assert!(preflight_selection([OsStr::from_bytes(b"invalid-\xff")].into_iter()).is_ok());
    }
    #[test]
    fn mutation_refresh_routes_known_directory_but_keeps_unknown_failure_fallback() {
        let a = std::path::Path::new("/A");
        let b = std::path::Path::new("/B");
        assert!(!refresh_matches(1, 1, Some(a), b, None));
        assert!(refresh_matches(1, 1, Some(a), a, None));
        assert!(refresh_matches(1, 1, Some(a), b, Some(a)));
        assert!(refresh_matches(0, 1, None, b, None));
        assert!(refresh_matches(2, 1, None, b, None));
        assert!(!refresh_matches(1, 1, None, b, None));
    }
    #[test]
    fn stale_dialog_plan_cannot_be_accepted_after_edit_or_reopen() {
        let dir = Fixture::new();
        fs::write(dir.0.join("before"), b"payload").unwrap();
        let mut m = model(&dir);
        m.open_rename(Mode::Rename);
        m.rename_dialog.as_mut().unwrap().edited_at = Instant::now() - Duration::from_secs(1);
        m.pump_mutations();
        let old = m.rename_dialog.as_ref().unwrap().id;
        assert!(m.mutations.busy);
        m.rename_dialog.as_mut().unwrap().input("changed-{n}");
        assert_eq!(m.rename_dialog.as_ref().unwrap().id, 0);
        drain(&mut m);
        assert!(m.rename_dialog.as_ref().unwrap().prepared.is_none());
        m.cancel_rename();
        m.open_rename(Mode::Rename);
        assert!(m.rename_dialog.as_ref().unwrap().id > old);
        assert!(m.rename_dialog.as_ref().unwrap().prepared.is_none());
    }
    #[test]
    fn close_even_with_discard_waits_for_mutation_ack_and_exposes_errors() {
        let dir = Fixture::new();
        fs::write(dir.0.join("before"), b"payload").unwrap();
        let mut m = model(&dir);
        let prepared =
            PreparedRename::capture(dir.0.clone(), vec![("before".into(), "after".into())])
                .unwrap();
        assert!(
            m.mutations
                .submit(Request::Apply { id: 1, prepared })
                .is_ok()
        );
        m.mutation_id = 1;
        m.mutation_applied = true;
        m.closing = true;
        m.discard_close = true;
        // Even if the worker has completed, its unread ACK still prevents exit.
        assert!(!m.close_ready());
        assert!(!m.mutations_drained());
        drain(&mut m);
        assert!(m.mutations_drained());
        assert!(!m.mutation_applied);
        assert_eq!(
            fs::read(dir.0.join(if dir.0.join("before").exists() {
                "before"
            } else {
                "after"
            }))
            .unwrap(),
            b"payload"
        );
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        // A cancellation/error cannot be discarded by the close preference.
        if m.notice_open {
            assert!(!m.closing);
            assert!(m.operation_message.is_some());
        }
    }
    #[test]
    fn file_creation_does_not_copy_selection_and_ui_history_survives_collision() {
        let dir = Fixture::new();
        fs::write(dir.0.join("before"), b"payload").unwrap();
        let mut m = model(&dir);
        let prepared =
            PreparedRename::capture(dir.0.clone(), vec![("before".into(), "after".into())])
                .unwrap();
        assert!(
            m.mutations
                .submit(Request::Apply { id: 1, prepared })
                .is_ok()
        );
        m.mutation_id = 1;
        m.mutation_applied = true;
        drain(&mut m);
        assert!(m.can_undo);
        m.open_rename(Mode::File);
        assert!(m.rename_dialog.as_ref().unwrap().selected.is_empty());
        m.rename_dialog.as_mut().unwrap().fields[0] = "after".into();
        m.apply_rename();
        drain(&mut m);
        assert!(m.notice_open && m.can_undo && !m.can_redo);
        m.notice_open = false;
        m.undo_rename(false);
        drain(&mut m);
        assert!(!m.can_undo && m.can_redo);
        assert_eq!(fs::read(dir.0.join("before")).unwrap(), b"payload");
    }
    #[derive(Default)]
    struct ReportPainter {
        lines: Vec<String>,
    }
    impl Painter for ReportPainter {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            text: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
            self.lines.push(text.into());
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn report_keyboard_and_mouse_pages_share_viewport_and_resize_clamps_full_last_page() {
        use crate::notice_navigation::Command;
        let f = Fixture::new();
        let mut m = model(&f);
        m.notice_open = true;
        m.operation_details = (0..31).map(|n| format!("LINE {n}")).collect();
        let original = m.operation_details.clone();
        let mut p = ReportPainter::default();
        paint(&mut p, &mut m, 440, 440);
        let rows = m.notice_rows;
        assert!(rows > 1 && rows < 31);
        assert_eq!(
            p.lines.iter().filter(|l| l.starts_with("LINE ")).count(),
            rows
        );
        assert!(
            !m.hits
                .iter()
                .any(|h| matches!(h.action, Action::NoticeNavigate(Command::Start)))
        );
        assert!(notice_shortcut(&mut m, Key::PageDown, false));
        assert_eq!(m.notice_scroll, rows - 1);
        assert!(notice_shortcut(&mut m, Key::End, true));
        assert_eq!(m.notice_scroll, 31 - rows);
        notice_shortcut(&mut m, Key::Down, false);
        assert_eq!(m.notice_scroll, 31 - rows);
        p.lines.clear();
        paint(&mut p, &mut m, 1000, 650);
        assert!(m.notice_rows > rows);
        assert_eq!(m.notice_scroll, 31 - m.notice_rows);
        assert_eq!(
            p.lines.iter().rfind(|l| l.starts_with("LINE ")).unwrap(),
            "LINE 30"
        );
        let up = m
            .hits
            .iter()
            .find(|h| matches!(h.action, Action::NoticeNavigate(Command::Pages(-1))))
            .unwrap()
            .action
            .clone();
        let old = m.notice_scroll;
        m.action(up);
        assert_eq!(m.notice_scroll, old.saturating_sub(m.notice_rows - 1));
        notice_shortcut(&mut m, Key::Home, false);
        assert_eq!(m.notice_scroll, 0);
        assert_eq!(m.operation_details, original);
        m.notice_open = false;
        assert!(!notice_shortcut(&mut m, Key::End, false));
        assert_eq!(m.notice_scroll, 0);
    }
    #[test]
    fn report_empty_controls_are_disabled_and_pan_is_limited_to_actual_visible_width() {
        let f = Fixture::new();
        let mut m = model(&f);
        m.notice_open = true;
        let mut p = ReportPainter::default();
        paint(&mut p, &mut m, 440, 440);
        assert!(p.lines.iter().any(|s| s.starts_with("0–0 / 0")));
        assert!(
            !m.hits
                .iter()
                .any(|h| matches!(h.action, Action::NoticeNavigate(_)))
        );
        m.operation_details = vec!["LONG".repeat(150)];
        paint(&mut p, &mut m, 440, 440);
        assert!(m.notice_pan_limit > 0);
        pan_notice(&mut m, i32::MAX);
        assert_eq!(m.notice_pan, m.notice_pan_limit);
        pan_notice(&mut m, i32::MIN);
        assert_eq!(m.notice_pan, 0);
        m.operation_details = vec!["short".into()];
        paint(&mut p, &mut m, 1000, 650);
        assert_eq!(m.notice_pan_limit, 0);
        pan_notice(&mut m, 80);
        assert_eq!(m.notice_pan, 0);
    }
    #[test]
    fn full_copy_detail_belongs_only_to_its_open_notice_and_preserves_previous_result() {
        let f = Fixture::new();
        let mut m = model(&f);
        m.copy_last = Some(Box::new(crate::copy_job::Outcome {
            exdev: None,
            moving: false,
            extracting: false,
            creating: false,
            archive_action: None,
            id: 77,
            result: Ok(Default::default()),
            progress: Default::default(),
        }));
        let mut p = ReportPainter::default();
        m.notice_open = true;
        m.copy_notice_id = None;
        m.operation_message = Some("Unrelated tool".into());
        m.operation_details = vec!["Current diagnostic".into()];
        paint(&mut p, &mut m, 440, 440);
        assert!(m.notice_copy_outcome().is_none());
        assert!(
            !m.hits
                .iter()
                .any(|h| matches!(h.action, Action::CopyFullReport))
        );
        assert_eq!(m.copy_last.as_ref().unwrap().id, 77);
        assert_eq!(m.operation_details, ["Current diagnostic"]);
        m.copy_notice_id = Some(76);
        assert!(m.notice_copy_outcome().is_none());
        m.show_copy_details();
        paint(&mut p, &mut m, 440, 440);
        assert_eq!(m.notice_copy_outcome().unwrap().id, 77);
        assert!(
            m.hits
                .iter()
                .any(|h| matches!(h.action, Action::CopyFullReport))
        );
        m.notice_open = false;
        assert!(m.notice_copy_outcome().is_none());
        assert_eq!(m.copy_last.as_ref().unwrap().id, 77);
    }
}
