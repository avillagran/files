//! Explicit report capture/navigation/export. Does not dispatch a transport command.
use crate::notice_navigation::Command as Nav;
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
    sftp_state::Form,
    sftp_ui::Command,
};
use fltk::{
    app,
    enums::{Key, Shortcut},
};
impl Model {
    pub fn sftp_report_command(&mut self, cmd: Command) {
        let Some(f) = &mut self.sftp.form else {
            return;
        };
        let current = f.report.as_ref().map(|r| r.id);
        match cmd {
            Command::Report if !f.profiles && current.is_none() => {
                f.dragging = false;
                f.touch();
                f.report = Some(crate::sftp_report::Report::capture(
                    f.edit_epoch,
                    f,
                    &self.sftp.transport.status,
                    self.sftp.transport.busy(),
                ));
            }
            Command::ReportBack(id) if current == Some(id) => {
                f.report = None;
                f.touch();
            }
            Command::ReportRefresh(id) if current == Some(id) => {
                f.touch();
                f.report = Some(crate::sftp_report::Report::capture(
                    f.edit_epoch,
                    f,
                    &self.sftp.transport.status,
                    self.sftp.transport.busy(),
                ));
            }
            Command::ReportNav(id, nav) if current == Some(id) => {
                f.report.as_mut().unwrap().navigate(nav);
            }
            Command::ReportCopy(id) if current == Some(id) => {
                let r = f.report.as_mut().unwrap();
                app::copy(r.text());
                r.copy_requests = r
                    .copy_requests
                    .checked_add(1)
                    .expect("SFTP copy request count exhausted");
            }
            _ => {}
        }
        self.hits.clear();
    }
}
fn button(
    p: &mut dyn Painter,
    m: &mut Model,
    f: &Form,
    r: Rect,
    label: &str,
    cmd: Command,
    active: bool,
) {
    p.rounded_rect(r, 4 * m.radius, m.palette.lighter_background);
    p.literal_text(
        r,
        label,
        11,
        if active {
            m.palette.accent
        } else {
            m.palette.dark_foreground
        },
    );
    if active {
        m.hits.push(Hit {
            rect: r,
            action: Action::SftpCommand(f.stamp, cmd),
            generation: m.generation,
        });
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, f: &mut Form, w: i32, h: i32) {
    m.hits.clear();
    let c = m.palette.clone();
    let area = Rect::new(12, 66, w - 24, h - 106);
    p.rounded_rect(area, 8 * m.radius, c.darker_background);
    p.clip(area);
    let Some(report) = &mut f.report else {
        p.unclip();
        return;
    };
    let body = Rect::new(area.x + 14, area.y + 80, area.w - 28, area.h - 158);
    report.layout(p, body.w - 12, (body.h / 18).max(1) as usize);
    let id = report.id;
    let top = report.scroll;
    let rows = report.visible_rows;
    let total = report.total();
    let copies = report.copy_requests;
    let truncated = report.truncated;
    p.literal_text(
        Rect::new(area.x + 14, area.y + 8, area.w - 110, 28),
        " Informe SFTP",
        16,
        c.foreground,
    );
    p.rect(body, c.lighter_background);
    p.clip(body);
    for (row, line) in report.ranges_for_paint().enumerate() {
        p.literal_text(
            Rect::new(body.x + 6, body.y + row as i32 * 18, body.w - 12, 18),
            line,
            12,
            c.foreground,
        );
    }
    p.unclip();
    button(
        p,
        m,
        f,
        Rect::new(area.x + area.w - 94, area.y + 8, 80, 28),
        " Esc",
        Command::ReportBack(id),
        true,
    );
    let bw = (area.w - 36) / 2;
    button(
        p,
        m,
        f,
        Rect::new(area.x + 14, area.y + 42, bw, 28),
        " Copiar · CtrlC",
        Command::ReportCopy(id),
        true,
    );
    button(
        p,
        m,
        f,
        Rect::new(area.x + 22 + bw, area.y + 42, bw, 28),
        " Actualizar · F5",
        Command::ReportRefresh(id),
        true,
    );
    let end = top.saturating_add(rows).min(total);
    let note = match (truncated, copies > 0) {
        (true, true) => " · parcial · copia solicitada",
        (true, false) => " · parcial",
        (false, true) => " · copia solicitada",
        (false, false) => " · captura fija",
    };
    p.literal_text(
        Rect::new(area.x + 14, area.y + area.h - 74, area.w - 28, 20),
        &format!("{}–{end}/{total}{note}", top + 1),
        11,
        c.dark_foreground,
    );
    let width = (area.w - 52) / 4;
    for (i, (label, nav, active)) in [
        ("", Nav::Start, top > 0),
        ("", Nav::Pages(-1), top > 0),
        ("", Nav::Pages(1), end < total),
        ("", Nav::End, end < total),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            p,
            m,
            f,
            Rect::new(
                area.x + 14 + i as i32 * (width + 8),
                area.y + area.h - 50,
                width,
                26,
            ),
            label,
            Command::ReportNav(id, nav),
            active,
        );
    }
    p.literal_text(
        Rect::new(area.x + 14, area.y + area.h - 23, area.w - 28, 18),
        "↑↓ · PgUp/PgDn · Home/End · rueda",
        10,
        c.dark_foreground,
    );
    p.unclip();
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut) {
    let Some(id) = m
        .sftp
        .form
        .as_ref()
        .and_then(|f| f.report.as_ref())
        .map(|r| r.id)
    else {
        return;
    };
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let cmd = match key {
        Key::Escape => Some(Command::ReportBack(id)),
        k if ctrl && k == Key::from_char('w') => Some(Command::Close),
        Key::F5 => Some(Command::ReportRefresh(id)),
        Key::Up => Some(Command::ReportNav(id, Nav::Rows(-1))),
        Key::Down => Some(Command::ReportNav(id, Nav::Rows(1))),
        Key::PageUp => Some(Command::ReportNav(id, Nav::Pages(-1))),
        Key::PageDown => Some(Command::ReportNav(id, Nav::Pages(1))),
        Key::Home => Some(Command::ReportNav(id, Nav::Start)),
        Key::End => Some(Command::ReportNav(id, Nav::End)),
        k if ctrl && k == Key::from_char('c') => Some(Command::ReportCopy(id)),
        _ => None,
    };
    if let Some(cmd) = cmd {
        let stamp = m.sftp.form.as_ref().unwrap().stamp;
        m.sftp_command(stamp, cmd);
    }
}
// Hidden-scene metadata is only requested by the existing owned fixture protocol.
pub fn fixture(m: &Model) -> serde_json::Value {
    let Some((f, r)) = m
        .sftp
        .form
        .as_ref()
        .and_then(|f| f.report.as_ref().map(|r| (f, r)))
    else {
        return serde_json::Value::Null;
    };
    serde_json::json!({"id":r.id,"text":r.text(),"bytes":r.text().len(),"truncated":r.truncated,"scroll":r.scroll,"rows":r.visible_rows,"total":r.total(),"visible":r.ranges_for_paint().collect::<Vec<_>>(),"copy_requests":r.copy_requests,"painted":r.total()>0&&m.hits.iter().any(|h|matches!(h.action,Action::SftpCommand(stamp,Command::ReportCopy(id))if stamp==f.stamp&&id==r.id)),"buttons":m.hits.iter().filter_map(|h|if let Action::SftpCommand(_,ref cmd)=h.action{Some(serde_json::json!({"command":format!("{cmd:?}"),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}else{None}).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Model {
        let mut m = Model::new("/browser".into());
        m.sftp.open();
        let f = m.sftp.form.as_mut().unwrap();
        f.fields = [
            "draft".into(),
            "user".into(),
            "0022".into(),
            "/ñ/%41/$()".into(),
        ];
        f.review();
        m
    }
    #[test]
    fn capture_refresh_return_never_dispatches_or_edits_draft_and_blocks_old_submit() {
        let mut m = model();
        let fields = m.sftp.form.as_ref().unwrap().fields.clone();
        let stamp = m.sftp.form.as_ref().unwrap().stamp;
        m.sftp.transport.status.diagnostic = "Captured A".into();
        m.sftp_command(stamp, Command::Report);
        let report = m.sftp.form.as_ref().unwrap().report.as_ref().unwrap();
        let id = report.id;
        assert!(report.text().contains("Captured A"));
        m.sftp.transport.status.diagnostic = "Later B".into();
        assert!(
            !m.sftp
                .form
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .text()
                .contains("Later B")
        );
        assert!(!Command::Connect.allowed_for(m.sftp.form.as_ref().unwrap()));
        assert!(!Command::Disconnect.allowed_for(m.sftp.form.as_ref().unwrap()));
        m.sftp_command(stamp, Command::Check);
        assert!(!m.sftp.transport.busy());
        assert!(m.sftp.active.is_none());
        assert!(m.text_context().is_none());
        m.sftp_input("MUST-NOT-EDIT");
        assert_eq!(m.sftp.form.as_ref().unwrap().fields, fields);
        m.sftp_command(stamp, Command::ReportRefresh(id));
        let current = m.sftp.form.as_ref().unwrap().report.as_ref().unwrap();
        assert_ne!(current.id, id);
        assert!(current.text().contains("Later B"));
        m.sftp_command(stamp, Command::ReportBack(id));
        assert!(m.sftp.form.as_ref().unwrap().report.is_some());
        let id = m.sftp.form.as_ref().unwrap().report.as_ref().unwrap().id;
        m.sftp_command(stamp, Command::ReportBack(id));
        let f = m.sftp.form.as_ref().unwrap();
        assert!(f.report.is_none() && f.review.is_some());
        assert_eq!(f.fields, fields);
        assert!(!m.sftp.transport.busy());
    }
    #[test]
    fn stale_report_copy_nav_and_parent_stamp_never_touch_new_capture() {
        let mut m = model();
        let stamp = m.sftp.form.as_ref().unwrap().stamp;
        m.sftp_command(stamp, Command::Report);
        let old = m.sftp.form.as_ref().unwrap().report.as_ref().unwrap().id;
        m.sftp_command(stamp, Command::ReportRefresh(old));
        m.sftp_command(stamp, Command::ReportCopy(old));
        m.sftp_command(stamp, Command::ReportNav(old, Nav::End));
        let current = m.sftp.form.as_ref().unwrap().report.as_ref().unwrap();
        assert_eq!(current.copy_requests, 0);
        assert_eq!(current.scroll, 0);
        let current = current.id;
        m.sftp.close();
        m.sftp.open();
        let new = m.sftp.form.as_ref().unwrap().stamp;
        m.sftp_command(new, Command::Report);
        m.sftp_command(stamp, Command::ReportCopy(current));
        assert_eq!(
            m.sftp
                .form
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .copy_requests,
            0
        );
        key(&mut m, Key::from_char('w'), Shortcut::Ctrl);
        assert!(m.sftp.form.is_none());
    }
    #[test]
    fn profiles_reject_stale_background_transport_and_report_commands() {
        let mut m = model();
        m.sftp_profiles.loaded = true;
        let stamp = m.sftp.form.as_ref().unwrap().stamp;
        m.sftp_command(stamp, Command::Profiles);
        assert!(!Command::Connect.allowed_for(m.sftp.form.as_ref().unwrap()));
        assert!(!Command::Disconnect.allowed_for(m.sftp.form.as_ref().unwrap()));
        m.sftp_command(stamp, Command::Report);
        assert!(!m.sftp.transport.busy());
        assert!(m.sftp.form.as_ref().unwrap().profiles);
        assert!(m.sftp.form.as_ref().unwrap().report.is_none());
    }
}
