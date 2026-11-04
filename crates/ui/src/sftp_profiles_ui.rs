//! Explicit local drafts only. Catalogue ACKs never navigate, probe or connect.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
    sftp_state::Form,
    sftp_ui::Command,
};
use files_core::sftp_profiles::{Criteria, Op};
use fltk::{
    app,
    enums::{Key, Shortcut},
};
impl Model {
    pub fn sftp_profile_command(&mut self, command: Command) {
        if self.closing {
            return;
        }
        let Some(f) = &mut self.sftp.form else {
            return;
        };
        if matches!(command, Command::Profiles) {
            f.profiles = true;
            f.dragging = false;
            let host = &f.fields[0];
            let mut end = host.len().min(files_core::sftp_profiles::MAX_LABEL);
            while !host.is_char_boundary(end) {
                end -= 1;
            }
            f.profile_label = host[..end].to_owned();
            f.profile_cursor = Default::default();
            f.profile_cursor.all(&f.profile_label);
            f.touch();
            if !self.sftp_profiles.loaded && self.sftp_profiles.can_request() {
                self.sftp_profiles.request(Op::Read);
            }
        } else if f.profiles {
            match command {
                Command::ProfileBack => {
                    f.profiles = false;
                    f.dragging = false;
                    f.touch();
                }
                Command::ProfileFocus => {
                    f.touch();
                }
                Command::ProfileSelect(id) => {
                    if let Some(i) = self
                        .sftp_profiles
                        .document
                        .rows
                        .iter()
                        .position(|e| e.id == id)
                    {
                        f.profile_selected = i;
                        f.touch();
                    }
                }
                Command::ProfileLoad(id, revision) if revision == self.sftp_profiles.revision => {
                    if let Some(e) = self.sftp_profiles.document.rows.iter().find(|e| e.id == id) {
                        // Fresh draft only: previous Review is invalid even if fields happen to match.
                        f.fields = e.criteria.fields.clone();
                        f.cursors = Default::default();
                        f.focus = 0;
                        f.profiles = false;
                        f.dragging = false;
                        f.changed();
                    }
                }
                Command::ProfileSave if self.sftp_profiles.can_request() => {
                    let fields = f.fields.each_ref().map(String::as_str);
                    if files_core::sftp_target::Target::validate(
                        fields[0], fields[1], fields[2], fields[3],
                    )
                    .is_err()
                    {
                        f.error = "Completa un destino válido antes de guardarlo".into();
                    } else if f.profile_label.trim().is_empty()
                        || f.profile_label.len() > files_core::sftp_profiles::MAX_LABEL
                        || f.profile_label.chars().any(char::is_control)
                    {
                        f.error = "Etiqueta requerida, hasta96 bytes y sin controles".into();
                    } else {
                        self.sftp_profiles.request(Op::Add {
                            criteria: Criteria {
                                fields: f.fields.clone(),
                            },
                            label: f.profile_label.clone(),
                        });
                        f.error.clear();
                        f.touch();
                    }
                }
                Command::ProfileRemove(id, revision)
                    if revision == self.sftp_profiles.revision
                        && self.sftp_profiles.can_request() =>
                {
                    if let Some(e) = self.sftp_profiles.document.rows.iter().find(|e| e.id == id) {
                        self.sftp_profiles.request(Op::Remove(e.clone()));
                    }
                }
                Command::ProfileRefresh => self.sftp_profiles.refresh(),
                Command::ProfileDiscard if self.sftp_profiles.can_discard() => {
                    self.sftp_profiles.discard()
                }
                _ => {}
            }
        }
        self.hits.clear();
    }
}
fn button(p: &mut dyn Painter, m: &mut Model, f: &Form, rect: Rect, label: &str, cmd: Command) {
    p.rounded_rect(rect, 4 * m.radius, m.palette.lighter_background);
    p.literal_text(rect, label, 11, m.palette.accent);
    m.hits.push(Hit {
        rect,
        action: Action::SftpCommand(f.stamp, cmd),
        generation: m.generation,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, f: &mut Form, w: i32, h: i32) {
    m.hits.clear();
    let c = m.palette.clone();
    let r = Rect::new(12, 66, w - 24, h - 106);
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.literal_text(
        Rect::new(r.x + 14, r.y + 8, r.w - 110, 28),
        " Perfiles SFTP",
        16,
        c.foreground,
    );
    button(
        p,
        m,
        f,
        Rect::new(r.x + r.w - 94, r.y + 8, 80, 28),
        " Esc",
        Command::ProfileBack,
    );
    p.literal_text(
        Rect::new(r.x + 14, r.y + 40, r.w - 28, 20),
        "Cargar rellena; revisa antes de conectar.",
        11,
        c.dark_foreground,
    );
    let list_top = r.y + 66;
    let list_bottom = r.y + r.h - 164;
    let visible = ((list_bottom - list_top) / 36).max(1) as usize;
    let document = std::sync::Arc::clone(&m.sftp_profiles.document);
    let rows = &document.rows;
    f.profile_selected = f.profile_selected.min(rows.len().saturating_sub(1));
    let start = f.profile_selected.saturating_sub(visible - 1);
    let end = (start + visible).min(rows.len());
    for (offset, e) in rows[start..end].iter().enumerate() {
        let a = Rect::new(r.x + 14, list_top + offset as i32 * 36, r.w - 28, 32);
        p.rect(
            a,
            if start + offset == f.profile_selected {
                c.lighter_background
            } else {
                c.darker_background
            },
        );
        p.literal_text(
            Rect::new(a.x + 6, a.y, a.w - 12, 17),
            &e.label,
            12,
            c.foreground,
        );
        p.literal_text(
            Rect::new(a.x + 6, a.y + 16, a.w - 12, 16),
            &e.criteria.fields[0],
            10,
            c.dark_foreground,
        );
        m.hits.push(Hit {
            rect: a,
            action: Action::SftpCommand(f.stamp, Command::ProfileSelect(e.id)),
            generation: m.generation,
        });
    }
    if rows.is_empty() {
        p.literal_text(
            Rect::new(r.x + 14, list_top, r.w - 28, 28),
            "Sin perfiles guardados",
            12,
            c.dark_foreground,
        );
    }
    let y = r.y + r.h - 158;
    let count = m.sftp_profiles.document.rows.len();
    p.literal_text(
        Rect::new(r.x + 14, y, r.w - 28, 18),
        &format!("{count}/32 · ↑↓ elegir · Enter cargar · Supr quitar"),
        10,
        c.dark_foreground,
    );
    p.literal_text(
        Rect::new(r.x + 14, y + 19, r.w - 28, 18),
        "Nombre para guardar el destino actual · CtrlS",
        10,
        c.dark_foreground,
    );
    let input = Rect::new(r.x + 14, y + 39, r.w - 28, 26);
    p.rect(input, c.lighter_background);
    f.profile_geometry.paint(
        p,
        &f.profile_label,
        &f.profile_cursor,
        input,
        &c,
        (true, f.edit_epoch),
    );
    m.hits.push(Hit {
        rect: input,
        action: Action::SftpCommand(f.stamp, Command::ProfileFocus),
        generation: m.generation,
    });
    let status = if !f.error.is_empty() {
        f.error.as_str()
    } else {
        m.sftp_profiles
            .status()
            .unwrap_or("Sólo datos de conexión; sin contraseñas ni conexión automática")
    };
    p.literal_text(
        Rect::new(r.x + 14, y + 69, r.w - 28, 20),
        status,
        10,
        if !f.error.is_empty() || !m.sftp_profiles.error.is_empty() {
            c.red
        } else {
            c.dark_foreground
        },
    );
    let selected_id = rows.get(f.profile_selected).map_or(0, |e| e.id);
    let width = (r.w - 44) / 3;
    for (i, (label, cmd)) in [
        (
            " Cargar",
            Command::ProfileLoad(selected_id, m.sftp_profiles.revision),
        ),
        (" Guardar", Command::ProfileSave),
        (" F5", Command::ProfileRefresh),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            p,
            m,
            f,
            Rect::new(r.x + 14 + i as i32 * (width + 8), y + 94, width, 26),
            label,
            cmd,
        );
    }
    for (i, (label, cmd)) in [
        (
            " Quitar",
            Command::ProfileRemove(selected_id, m.sftp_profiles.revision),
        ),
        (" Omitir F8", Command::ProfileDiscard),
        (" Volver", Command::ProfileBack),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            p,
            m,
            f,
            Rect::new(r.x + 14 + i as i32 * (width + 8), y + 124, width, 26),
            label,
            cmd,
        );
    }
    p.unclip();
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut, text: &str) {
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let shift = mods.intersects(Shortcut::Shift);
    let selected_id = m
        .sftp
        .form
        .as_ref()
        .and_then(|f| m.sftp_profiles.document.rows.get(f.profile_selected))
        .map_or(0, |e| e.id);
    let command = match key {
        Key::Escape => Some(Command::ProfileBack),
        Key::Enter | Key::KPEnter => {
            Some(Command::ProfileLoad(selected_id, m.sftp_profiles.revision))
        }
        Key::F5 => Some(Command::ProfileRefresh),
        Key::F8 => Some(Command::ProfileDiscard),
        Key::Delete if !ctrl => Some(Command::ProfileRemove(
            selected_id,
            m.sftp_profiles.revision,
        )),
        k if ctrl && k == Key::from_char('s') => Some(Command::ProfileSave),
        _ => None,
    };
    if let Some(cmd) = command {
        m.sftp_profile_command(cmd);
        return;
    }
    let Some(f) = &mut m.sftp.form else {
        return;
    };
    if matches!(key, Key::Up | Key::Down | Key::PageUp | Key::PageDown) {
        let delta = if matches!(key, Key::PageUp | Key::PageDown) {
            5
        } else {
            1
        };
        f.profile_selected = if matches!(key, Key::Up | Key::PageUp) {
            f.profile_selected.saturating_sub(delta)
        } else {
            f.profile_selected
                .saturating_add(delta)
                .min(m.sftp_profiles.document.rows.len().saturating_sub(1))
        };
        f.touch();
    } else if ctrl && (key == Key::from_char('z') || key == Key::from_char('y')) {
        f.profile_cursor
            .undo(&mut f.profile_label, shift || key == Key::from_char('y'));
        f.touch();
    } else if ctrl && key == Key::from_char('a') {
        f.profile_cursor.all(&f.profile_label);
        f.touch();
    } else if matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
        use crate::line_edit::Motion;
        let motion = match key {
            Key::Left => Motion::Left,
            Key::Right => Motion::Right,
            Key::Home => Motion::Start,
            _ => Motion::End,
        };
        f.profile_cursor.motion(&f.profile_label, motion, shift);
        f.touch();
    } else if key == Key::BackSpace {
        f.profile_cursor
            .delete(&mut f.profile_label, true, ctrl, false);
        f.touch();
    } else if ctrl && (key == Key::from_char('c') || key == Key::from_char('x')) {
        let range = f.profile_cursor.range(&f.profile_label);
        if !range.is_empty() {
            app::copy(&f.profile_label[range]);
            if key == Key::from_char('x') {
                f.profile_cursor
                    .delete(&mut f.profile_label, false, false, false);
                f.touch();
            }
        }
    } else if !ctrl
        && !mods.intersects(Shortcut::Alt)
        && !text.is_empty()
        && !text.chars().any(char::is_control)
    {
        f.input(text);
    }
    m.hits.clear();
}
pub fn push(m: &mut Model) -> bool {
    let Some(f) = &mut m.sftp.form else {
        return false;
    };
    if !f.profiles {
        return false;
    }
    if let Some(Action::SftpCommand(stamp, Command::ProfileFocus)) = m
        .hits
        .iter()
        .rev()
        .find(|h| h.rect.contains(app::event_x(), app::event_y()))
        .map(|h| &h.action)
        && *stamp == f.stamp
    {
        if let Some(at) = f.profile_geometry.position(app::event_x(), f.edit_epoch) {
            let anchor = if app::event_state().intersects(Shortcut::Shift) {
                f.profile_cursor.anchor
            } else {
                at
            };
            f.profile_cursor.select(&f.profile_label, anchor, at);
            f.dragging = true;
            f.touch();
            m.hits.clear();
        }
        return true;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    fn model() -> Model {
        let mut m = Model::new("/browser".into());
        m.sftp.open();
        let f = m.sftp.form.as_mut().unwrap();
        f.fields = ["draft".into(), "u".into(), "22".into(), "/different".into()];
        f.review();
        m.sftp_profiles.loaded = true;
        m.sftp_profiles.document = Arc::new(files_core::sftp_profiles::Document {
            next: 3,
            rows: vec![
                files_core::sftp_profiles::Entry {
                    id: 1,
                    label: "A".into(),
                    criteria: Criteria {
                        fields: [
                            "saved".into(),
                            "user".into(),
                            "0022".into(),
                            "/ñ/%41/$()".into(),
                        ],
                    },
                },
                files_core::sftp_profiles::Entry {
                    id: 2,
                    label: "B".into(),
                    criteria: Criteria {
                        fields: ["other".into(), "".into(), "".into(), "".into()],
                    },
                },
            ],
        });
        m
    }
    #[test]
    fn load_is_explicit_invalidates_review_and_never_connects_or_navigates() {
        let mut m = model();
        let path = m.path.clone();
        let fields = m.sftp.form.as_ref().unwrap().fields.clone();
        m.sftp_profile_command(Command::Profiles);
        assert_eq!(m.sftp.form.as_ref().unwrap().fields, fields);
        let old = m.sftp.form.as_ref().unwrap().edit_epoch;
        m.sftp_profile_command(Command::ProfileLoad(1, 0));
        let f = m.sftp.form.as_ref().unwrap();
        assert_eq!(f.fields, ["saved", "user", "0022", "/ñ/%41/$()"]);
        assert!(f.review.is_none() && !f.profiles);
        assert_ne!(f.edit_epoch, old);
        assert_eq!(m.path, path);
        assert!(!m.sftp.transport.busy());
        assert!(!m.sftp.transport.status.open);
    }
    #[test]
    fn stale_profile_id_and_old_modal_cannot_load_a_different_entry() {
        let mut m = model();
        m.sftp_profile_command(Command::Profiles);
        let old = m.sftp.form.as_ref().unwrap().stamp;
        let d = Arc::get_mut(&mut m.sftp_profiles.document).unwrap();
        d.rows.remove(0);
        let fields = m.sftp.form.as_ref().unwrap().fields.clone();
        m.sftp_profile_command(Command::ProfileLoad(1, 0));
        assert_eq!(m.sftp.form.as_ref().unwrap().fields, fields);
        m.sftp.close();
        m.sftp.open();
        m.sftp_profile_command(Command::Profiles);
        m.sftp_command(old, Command::ProfileLoad(2, 0));
        assert_eq!(m.sftp.form.as_ref().unwrap().fields, ["", "", "", ""]);
    }
    #[test]
    fn stale_catalogue_action_cannot_load_or_remove_reused_id_after_reload() {
        let mut m = model();
        m.sftp_profile_command(Command::Profiles);
        let fields = m.sftp.form.as_ref().unwrap().fields.clone();
        let changed = Arc::get_mut(&mut m.sftp_profiles.document).unwrap();
        changed.rows[0].criteria.fields[0] = "recreated-same-id".into();
        m.sftp_profiles.revision = 1;
        m.sftp_profile_command(Command::ProfileLoad(1, 0));
        assert_eq!(m.sftp.form.as_ref().unwrap().fields, fields);
        m.sftp_profile_command(Command::ProfileRemove(1, 0));
        assert!(!m.sftp_profiles.busy());
        m.sftp_profile_command(Command::ProfileLoad(1, 1));
        assert_eq!(m.sftp.form.as_ref().unwrap().fields[0], "recreated-same-id");
        assert!(m.sftp.form.as_ref().unwrap().review.is_none());
    }
    #[test]
    fn captured_save_ack_never_changes_newer_draft_and_close_requires_exact_ack() {
        let mut m = model();
        m.sftp_profile_command(Command::Profiles);
        let fields = m.sftp.form.as_ref().unwrap().fields.clone();
        m.sftp_profile_command(Command::ProfileSave);
        let (tx, rx) = mpsc::sync_channel(1);
        m.sftp_profiles.dispatch(&tx);
        let crate::aux_io::Request::SftpProfiles(id, op) = rx.recv().unwrap() else {
            panic!()
        };
        let Op::Add { criteria, .. } = &*op else {
            panic!()
        };
        assert_eq!(criteria.fields, fields);
        m.sftp_profile_command(Command::ProfileBack);
        m.sftp.form.as_mut().unwrap().fields[0] = "newer".into();
        m.sftp.form.as_mut().unwrap().changed();
        m.sftp_profiles.begin_close(true);
        assert!(!m.sftp_profiles.close_ready());
        m.sftp_profiles.receive(
            id,
            Ok(files_core::sftp_profiles::Updated {
                document: files_core::sftp_profiles::Document::default(),
                unsynced: false,
                detail: None,
                notice: "saved",
            }),
        );
        assert_eq!(m.sftp.form.as_ref().unwrap().fields[0], "newer");
        assert!(m.sftp.form.as_ref().unwrap().review.is_none());
        assert!(m.sftp_profiles.close_ready());
    }
    #[test]
    fn label_budget_keys_and_selection_are_separate_from_connection_fields() {
        let mut m = model();
        m.sftp_profile_command(Command::Profiles);
        let fields = m.sftp.form.as_ref().unwrap().fields.clone();
        key(&mut m, Key::from_char('a'), Shortcut::Ctrl, "");
        m.sftp_input("Servidor ñ");
        assert_eq!(m.sftp.form.as_ref().unwrap().profile_label, "Servidor ñ");
        m.sftp_input("line\nbreak");
        assert_eq!(m.sftp.form.as_ref().unwrap().profile_label, "Servidor ñ");
        m.sftp_input(&"x".repeat(97));
        assert_eq!(m.sftp.form.as_ref().unwrap().profile_label, "Servidor ñ");
        key(&mut m, Key::Down, Shortcut::None, "");
        assert_eq!(m.sftp.form.as_ref().unwrap().profile_selected, 1);
        assert_eq!(m.sftp.form.as_ref().unwrap().fields, fields);
    }
}
