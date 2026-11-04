use crate::{
    model::{Action, Model},
    properties_dialog::Dialog,
    properties_job::Reply,
    rename_job::Request,
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
impl Model {
    pub fn open_properties(&mut self) {
        if self.mutations.busy || self.mutation_applied || self.copy_busy() {
            return;
        }
        if self.selection.len() > 1 {
            self.operation_message =
                Some("Propiedades: selecciona sólo un objeto en este bloque".into());
            return;
        }
        let index = self.selection.iter().next().copied().or(self.selected);
        let Some(entry) = index.and_then(|i| self.entries.get(i)) else {
            return;
        };
        if self
            .path
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .saturating_add(entry.name.as_encoded_bytes().len())
            .saturating_add(1)
            > 4096
        {
            self.operation_message =
                Some("Ruta de propiedades demasiado larga (máximo 4096 bytes)".into());
            return;
        }
        let path = self.path.join(&entry.name);
        self.operation_message = None;
        self.preview.close();
        self.rename_dialog = None;
        self.settings = false;
        self.actions_menu = false;
        self.notice_open = false;
        let id = self.next_mutation_id();
        let mut d = Dialog::new(id, path.clone());
        if self
            .mutations
            .submit(Request::Inspect { id, path })
            .is_err()
        {
            d.pending = false;
            d.stale = true;
            d.message = "El worker no aceptó la lectura; vuelve a intentar Recargar".into();
        }
        self.properties_dialog = Some(d);
        self.hits.clear();
    }
    pub fn reload_properties(&mut self) {
        if self.mutations.busy || self.mutation_applied {
            return;
        }
        let Some(path) = self.properties_dialog.as_ref().map(|d| d.path.clone()) else {
            return;
        };
        let id = self.next_mutation_id();
        let mut d = Dialog::new(id, path.clone());
        if self
            .mutations
            .submit(Request::Inspect { id, path })
            .is_err()
        {
            d.pending = false;
            d.stale = true;
            d.message = "No se pudo recargar; el worker no está disponible".into();
        }
        self.properties_dialog = Some(d);
        self.hits.clear();
    }
    pub fn apply_properties(&mut self) {
        if self.mutations.busy
            || self.mutation_applied
            || self.properties_dialog.as_ref().is_none_or(|d| !d.changed())
        {
            return;
        }
        self.operation_message = None;
        let id = self.next_mutation_id();
        let d = self.properties_dialog.as_mut().expect("checked dialog");
        let snapshot = d.snapshot.take().expect("editable snapshot");
        let mode = d.requested;
        match self
            .mutations
            .submit(Request::Permissions { id, snapshot, mode })
        {
            Ok(()) => {
                d.id = id;
                d.pending = true;
                d.applying = true;
                d.message = "Aplicando permisos por descriptor…".into();
                d.warnings.clear();
                self.mutation_applied = true;
            }
            Err(request) => {
                if let Request::Permissions { snapshot, .. } = *request {
                    d.snapshot = Some(snapshot);
                }
                d.stale = true;
                d.message = "No se aceptó el cambio; Recargar antes de confirmar".into();
            }
        }
        self.hits.clear();
    }
    pub fn close_properties(&mut self) {
        if self.properties_dialog.as_ref().is_some_and(|d| d.applying) {
            self.mutations.cancel();
            if let Some(d) = &mut self.properties_dialog {
                d.close_after = true;
                d.message = "Cancelación solicitada; esperando confirmación del worker…".into();
            }
        } else {
            if self.properties_dialog.as_ref().is_some_and(|d| d.pending) {
                self.mutations.cancel();
            }
            self.properties_dialog = None;
        }
        self.hits.clear();
    }
    pub fn properties_reply(&mut self, id: u64, reply: Reply) {
        if std::env::var_os("FILES_TEST_PROPERTIES").as_deref() == Some(std::ffi::OsStr::new("1")) {
            eprintln!(
                "FILES_PROPERTIES_ACK id={id} success={} changed={} snapshot={} message={:?}",
                reply.success,
                reply.changed,
                reply.snapshot.is_some(),
                reply.message
            );
        }
        if reply.changed {
            self.can_undo = false;
            self.can_redo = false;
            self.operation_message = Some(reply.message.clone());
        }
        if id == self.mutation_id {
            self.mutation_applied = false;
        }
        let Some(mut d) = self.properties_dialog.take() else {
            return;
        };
        if d.id != id {
            self.properties_dialog = Some(d);
            return;
        }
        if reply.changed {
            let paths = d
                .path
                .parent()
                .into_iter()
                .chain(reply.current_path.as_deref().and_then(|p| p.parent()))
                .map(std::path::Path::to_path_buf)
                .collect::<Vec<_>>();
            let refresh = paths.iter().any(|p| self.path.starts_with(p));
            self.dual.refresh_other(paths.into_iter());
            if refresh {
                self.refresh_current();
            }
        }
        let close = d.close_after;
        d.pending = false;
        d.applying = false;
        d.stale = !reply.success || reply.snapshot.is_none();
        if let Some(snapshot) = &reply.snapshot {
            d.observed = Some(snapshot.properties);
            if reply.success {
                d.requested = snapshot.properties.mode & 0o777;
            }
        }
        d.snapshot = reply.snapshot;
        d.current_path = reply.current_path;
        d.message = reply.message;
        d.warnings = reply.warnings;
        if reply.changed && !d.warnings.is_empty() {
            self.closing = false;
            d.close_after = false;
        } else if close {
            return;
        }
        self.properties_dialog = Some(d);
    }
}
pub fn paint(p: &mut dyn crate::painter::Painter, m: &mut Model, w: i32, h: i32) {
    if let Some(mut d) = m.properties_dialog.take() {
        crate::properties_dialog::paint(p, m, &mut d, w, h);
        m.properties_dialog = Some(d);
    }
}
pub fn event(m: &mut Model, ev: Event, h: i32) -> bool {
    match ev {
        Event::Push => m.pointer_down(app::event_x(), app::event_y(), false, false, false),
        Event::MouseWheel => {
            if let Some(d) = &mut m.properties_dialog {
                d.scroll_by(
                    app::event_dy_value().saturating_mul(38),
                    d.body_height.max(1),
                );
            }
        }
        Event::KeyDown | Event::Shortcut => key(m, app::event_key(), app::event_state(), h),
        _ => {}
    }
    true
}

pub fn key(m: &mut Model, key: Key, mods: Shortcut, h: i32) {
    if key == Key::Escape {
        m.action(Action::PropertyClose);
    } else if key == Key::Enter {
        m.action(Action::PropertyApply);
    } else if key == Key::Tab {
        if let Some(d) = &mut m.properties_dialog {
            let height = if d.body_height > 0 {
                d.body_height
            } else {
                ((h - 68).min(640) - 176).max(1)
            };
            d.focus_next(mods.intersects(Shortcut::Shift), height);
        }
    } else if matches!(
        key,
        Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End
    ) {
        if let Some(d) = &mut m.properties_dialog {
            let height = d.body_height.max(1);
            let delta = match key {
                Key::Up => -18,
                Key::Down => 18,
                Key::PageUp => -(height - 18).max(18),
                Key::PageDown => (height - 18).max(18),
                Key::Home => i32::MIN,
                _ => i32::MAX,
            };
            d.scroll_by(delta, height);
        }
    } else if key == Key::from_char(' ')
        && let Some(index) = m.properties_dialog.as_ref().map(|d| d.focus)
    {
        m.action(Action::PropertyBit(index));
    }
}
