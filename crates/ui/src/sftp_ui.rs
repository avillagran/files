//! Explicit SFTP controls. Transport and filesystem work stay off the graphical thread.
use crate::{
    line_edit::Motion,
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
#[derive(Clone, Debug)]
pub enum Command {
    Close,
    Profiles,
    Report,
    ReportBack(u64),
    ReportRefresh(u64),
    ReportCopy(u64),
    ReportNav(u64, crate::notice_navigation::Command),
    ProfileBack,
    ProfileFocus,
    ProfileSelect(u64),
    ProfileLoad(u64, u64),
    ProfileSave,
    ProfileRemove(u64, u64),
    ProfileRefresh,
    ProfileDiscard,
    Focus(usize),
    Check,
    Review,
    Connect,
    Open,
    Disconnect,
    Reconcile,
    Sync,
}
impl Command {
    pub fn allowed_for(&self, form: &crate::sftp_state::Form) -> bool {
        if form.report.is_some() {
            matches!(
                self,
                Self::Close
                    | Self::ReportBack(_)
                    | Self::ReportRefresh(_)
                    | Self::ReportCopy(_)
                    | Self::ReportNav(..)
            )
        } else if form.profiles {
            matches!(
                self,
                Self::Close
                    | Self::Profiles
                    | Self::ProfileBack
                    | Self::ProfileFocus
                    | Self::ProfileSelect(_)
                    | Self::ProfileLoad(..)
                    | Self::ProfileSave
                    | Self::ProfileRemove(..)
                    | Self::ProfileRefresh
                    | Self::ProfileDiscard
            )
        } else {
            true
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Connect => "Conectar destino revisado, CtrlEnter",
            Self::Open => "Abrir montaje verificado, AltO",
            Self::Disconnect => "Desconectar mediante desmontaje normal, AltD",
            Self::Reconcile => "Reconciliar registro existente, AltR",
            Self::Sync => "Reintentar sólo sincronización del registro, AltS",
            Self::Report => "Informe SFTP legible, F4",
            Self::ReportBack(_) => "Volver al formulario SFTP, Esc",
            Self::ReportRefresh(_) => "Actualizar captura sin ejecutar acciones de red, F5",
            Self::ReportCopy(_) => "Solicitar copia del informe capturado, CtrlC",
            Self::ReportNav(..) => "Recorrer informe SFTP, flechas/PgUp/PgDn/Home/End",
            Self::Profiles => "Perfiles SFTP guardados, F2",
            Self::ProfileBack => "Volver al formulario SFTP, Esc",
            Self::ProfileFocus => "Nombre del perfil SFTP",
            Self::ProfileSelect(_) => "Elegir perfil SFTP",
            Self::ProfileLoad(..) => "Cargar borrador sin conectar, Enter",
            Self::ProfileSave => "Guardar destino actual, CtrlS",
            Self::ProfileRemove(..) => "Quitar perfil local, Supr",
            Self::ProfileRefresh => "Recargar o reintentar guardado, F5",
            Self::ProfileDiscard => "Omitir intento sin revertir publicación, F8",
            Self::Close => "Cerrar preparación SFTP, Esc",
            Self::Focus(_) => "Editar campo SFTP; Ctrl1–4 o Alt↑↓",
            Self::Check => "Comprobar herramientas SFTP, F5",
            Self::Review => "Validar destino SFTP sin conectar, Enter",
        }
    }
}
impl Model {
    pub fn open_sftp(&mut self) {
        if self.sftp.form.is_some() {
            self.sftp.close();
            self.hits.clear();
            return;
        }
        if self.closing
            || self.bookmarks.panel.is_some()
            || self.commands.is_some()
            || self.search.visible
            || self.size_panel.is_some()
            || self.preview.visible()
            || self.rename_dialog.is_some()
            || self.copy_form.is_some()
            || self.properties_dialog.is_some()
            || self.notice_open
            || self.mutation_applied
            || self.recovery_catalog.open
        {
            return;
        }
        self.end_path_edit();
        self.end_filter_edit();
        self.help = None;
        self.context_menu = None;
        self.settings = false;
        self.actions_menu = false;
        self.thumbnails.suspend();
        self.text_paste.invalidate();
        self.sftp.open();
        self.hits.clear();
    }
    pub fn sftp_command(&mut self, stamp: u64, cmd: Command) {
        if self.closing {
            self.sftp.close();
            return;
        }
        if self
            .sftp
            .form
            .as_ref()
            .is_none_or(|f| f.stamp != stamp || !cmd.allowed_for(f))
        {
            return;
        }
        match cmd {
            Command::Report
            | Command::ReportBack(_)
            | Command::ReportRefresh(_)
            | Command::ReportCopy(_)
            | Command::ReportNav(..) => self.sftp_report_command(cmd),
            Command::Profiles
            | Command::ProfileBack
            | Command::ProfileFocus
            | Command::ProfileSelect(_)
            | Command::ProfileLoad(..)
            | Command::ProfileSave
            | Command::ProfileRemove(..)
            | Command::ProfileRefresh
            | Command::ProfileDiscard => {
                self.sftp_profile_command(cmd);
            }
            Command::Close => self.sftp.close(),
            Command::Focus(i) => self.sftp.form.as_mut().unwrap().focus(i),
            Command::Check => self.sftp.recheck(),
            Command::Review => self.sftp.form.as_mut().unwrap().review(),
            command => {
                use crate::sftp_executor::Command as Work;
                let f = self.sftp.form.as_mut().unwrap();
                let request = match command {
                    Command::Connect => {
                        let Some(review) = f.review.as_ref().filter(|r| r.probe == f.probe) else {
                            f.error = "Revisar destino antes de conectar".into();
                            return;
                        };
                        let Some(Ok(cap)) = &f.capability else {
                            f.error = "Comprobar herramientas antes de conectar".into();
                            return;
                        };
                        Work::Connect(review.target.clone(), cap.clone())
                    }
                    Command::Open => {
                        self.sftp.open_requested = Some(stamp);
                        Work::Open
                    }
                    Command::Disconnect => Work::Disconnect,
                    Command::Reconcile => Work::Reconcile,
                    Command::Sync => Work::Sync,
                    _ => unreachable!(),
                };
                f.error.clear();
                if let Err(error) = self.sftp.transport.send(request) {
                    self.sftp.open_requested = None;
                    self.sftp.form.as_mut().unwrap().error = error;
                }
            }
        }
        self.hits.clear();
    }
    pub fn pump_sftp_transport(&mut self) -> bool {
        if !self.closing {
            self.sftp.transport.resume();
        }
        let dirty = self.sftp.transport.pump();
        if self.closing {
            self.sftp.open_requested = None;
            self.sftp.transport.close_ready();
            return dirty;
        }
        if dirty
            && let Some(stamp) = self.sftp.open_requested.take()
            && self.sftp.form.as_ref().is_some_and(|f| f.stamp == stamp)
        {
            if self.sftp.transport.status.open {
                if let Some(path) = self.sftp.transport.status.point.clone() {
                    self.sftp.close();
                    self.navigate(path);
                }
            } else if let Some(f) = &mut self.sftp.form {
                f.error = "Montaje sin prueba de servicio vigente; no se abre".into();
            }
        }
        if !self.sftp.inspected
            && !self.sftp.transport.busy()
            && let Some(Ok(cap)) = self.sftp.form.as_ref().and_then(|f| f.capability.as_ref())
            && self
                .sftp
                .transport
                .send(crate::sftp_executor::Command::Inspect(cap.clone()))
                .is_ok()
        {
            self.sftp.inspected = true;
        }
        self.sftp.transport.tick();
        dirty
    }
    pub fn sftp_input(&mut self, text: &str) {
        if let Some(f) = &mut self.sftp.form {
            f.input(text);
        }
        self.hits.clear();
    }
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, stamp: u64, cmd: Command) {
    p.rounded_rect(r, 4 * m.radius, m.palette.lighter_background);
    p.literal_text(r, label, 11, m.palette.accent);
    m.hits.push(Hit {
        rect: r,
        action: Action::SftpCommand(stamp, cmd),
        generation: m.generation,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut f) = m.sftp.form.take() else {
        return;
    };
    if f.report.is_some() {
        crate::sftp_report_ui::paint(p, m, &mut f, w, h);
        m.sftp.form = Some(f);
        return;
    }
    if f.profiles {
        crate::sftp_profiles_ui::paint(p, m, &mut f, w, h);
        m.sftp.form = Some(f);
        return;
    }
    m.hits.clear();
    let c = m.palette.clone();
    let r = Rect::new(12, 66, w - 24, h - 106);
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.literal_text(
        Rect::new(r.x + 14, r.y + 8, r.w - 242, 28),
        " SFTP · Montaje",
        16,
        c.foreground,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 94, r.y + 8, 80, 28),
        " Esc",
        f.stamp,
        Command::Close,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 182, r.y + 8, 80, 28),
        " Sync",
        f.stamp,
        Command::Sync,
    );
    button(
        p,
        m,
        Rect::new(r.x + 14, r.y + 38, 120, 24),
        " Perfiles · F2",
        f.stamp,
        Command::Profiles,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 230, r.y + 8, 40, 28),
        "",
        f.stamp,
        Command::Report,
    );
    let cap = match &f.capability {
        None => "Comprobando herramientas…".into(),
        Some(Ok(cap)) => cap.summary(),
        Some(Err(e)) => e.clone(),
    };
    let ready = f
        .capability
        .as_ref()
        .is_some_and(|v| v.as_ref().is_ok_and(|c| c.tools_ready()));
    p.literal_text(
        Rect::new(r.x + 142, r.y + 42, r.w - 156, 22),
        &cap,
        11,
        if ready { c.foreground } else { c.accent },
    );
    let half = (r.w - 36) / 2;
    let areas = [
        Rect::new(r.x + 14, r.y + 84, r.w - 28, 28),
        Rect::new(r.x + 14, r.y + 136, half, 28),
        Rect::new(r.x + 22 + half, r.y + 136, half, 28),
        Rect::new(r.x + 14, r.y + 188, r.w - 28, 28),
    ];
    for (i, label) in [
        "Host o alias SSH · Ctrl1",
        "Usuario · Ctrl2 (opcional)",
        "Puerto · Ctrl3 (opcional)",
        "Carpeta remota · Ctrl4 (vacío: inicio)",
    ]
    .into_iter()
    .enumerate()
    {
        let a = areas[i];
        p.literal_text(
            Rect::new(a.x, a.y - 18, a.w, 17),
            label,
            10,
            c.dark_foreground,
        );
        p.rect(a, c.lighter_background);
        f.geometry[i].paint(
            p,
            &f.fields[i],
            &f.cursors[i],
            a,
            &c,
            (f.focus == i, f.edit_epoch),
        );
        m.hits.push(Hit {
            rect: a,
            action: Action::SftpCommand(f.stamp, Command::Focus(i)),
            generation: m.generation,
        });
    }
    let prepared = if m.sftp.transport.status.point.is_some() && !m.sftp.transport.status.writable {
        "Registro sólo lectura o pendiente de resolver"
    } else if f.review.as_ref().is_some_and(|v| v.probe == f.probe) {
        "Destino revisado · CtrlEnter conecta"
    } else {
        "Claves/agente SSH · no se guardan credenciales"
    };
    p.literal_text(
        Rect::new(r.x + 14, r.y + 222, r.w - 28, 20),
        if m.sftp.transport.busy() {
            "SFTP · Operación en curso…"
        } else if m.sftp.transport.status.phase.is_empty() {
            prepared
        } else {
            m.sftp.transport.status.label()
        },
        11,
        c.foreground,
    );
    p.literal_text(
        Rect::new(r.x + 14, r.y + 244, r.w - 28, 20),
        if f.error.is_empty() {
            &m.sftp.transport.status.diagnostic
        } else {
            &f.error
        },
        11,
        c.red,
    );
    if r.h >= 500 {
        p.literal_text(
            Rect::new(r.x + 14, r.y + 274, r.w - 28, 20),
            "Cerrar Files conserva el montaje. Desconectar requiere acción explícita.",
            11,
            c.dark_foreground,
        );
        if let Some(v) = &f.review {
            p.literal_text(
                Rect::new(r.x + 14, r.y + 300, r.w - 28, 20),
                &match v.target.port() {
                    Some(port) => format!("{} · puerto {port}", v.source),
                    None => format!("{} · puerto de SSH config", v.source),
                },
                11,
                c.dark_foreground,
            );
        }
    }
    let bw = (r.w - 44) / 3;
    let rows = [
        [
            (" Conectar", Command::Connect),
            (" Abrir", Command::Open),
            (" Desconectar", Command::Disconnect),
        ],
        [
            (" F5", Command::Check),
            (" Enter", Command::Review),
            (" Reconciliar", Command::Reconcile),
        ],
    ];
    for (row, items) in rows.into_iter().enumerate() {
        for (column, (label, command)) in items.into_iter().enumerate() {
            button(
                p,
                m,
                Rect::new(
                    r.x + 14 + column as i32 * (bw + 8),
                    r.y + r.h - 68 + row as i32 * 32,
                    bw,
                    28,
                ),
                label,
                f.stamp,
                command,
            );
        }
    }
    p.unclip();
    m.sftp.form = Some(f);
}
pub fn key(m: &mut Model, key: Key, mods: Shortcut, text: &str) {
    let Some(f) = &m.sftp.form else {
        return;
    };
    let stamp = f.stamp;
    if f.report.is_some() {
        crate::sftp_report_ui::key(m, key, mods);
        return;
    }
    if key == Key::F4 {
        m.sftp_command(stamp, Command::Report);
        return;
    }
    if f.profiles {
        crate::sftp_profiles_ui::key(m, key, mods, text);
        return;
    }
    if key == Key::F2 {
        m.sftp_command(stamp, Command::Profiles);
        return;
    }
    let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
    let alt = mods.intersects(Shortcut::Alt);
    let shift = mods.intersects(Shortcut::Shift);
    if key == Key::Escape || ctrl && key == Key::from_char('w') {
        m.sftp_command(stamp, Command::Close);
        return;
    }
    if key == Key::F5 {
        m.sftp_command(stamp, Command::Check);
        return;
    }
    if key == Key::Enter || key == Key::KPEnter {
        m.sftp_command(
            stamp,
            if ctrl {
                Command::Connect
            } else {
                Command::Review
            },
        );
        return;
    }
    if alt {
        let command = match key {
            k if k == Key::from_char('o') => Some(Command::Open),
            k if k == Key::from_char('d') => Some(Command::Disconnect),
            k if k == Key::from_char('r') => Some(Command::Reconcile),
            k if k == Key::from_char('s') => Some(Command::Sync),
            _ => None,
        };
        if let Some(command) = command {
            m.sftp_command(stamp, command);
            return;
        }
    }
    if ctrl {
        for i in 0..4 {
            if key == Key::from_char((b'1' + i as u8) as char) {
                m.sftp_command(stamp, Command::Focus(i));
                return;
            }
        }
    }
    if alt && matches!(key, Key::Up | Key::Down) {
        let delta = if key == Key::Up { 3 } else { 1 };
        m.sftp_command(stamp, Command::Focus((f.focus + delta) % 4));
        return;
    }
    let Some(f) = &mut m.sftp.form else {
        return;
    };
    let i = f.focus;
    if ctrl && key == Key::from_char('a') {
        f.cursors[i].all(&f.fields[i]);
        f.touch();
    } else if ctrl && (key == Key::from_char('z') || key == Key::from_char('y')) {
        f.cursors[i].undo(&mut f.fields[i], shift || key == Key::from_char('y'));
        f.changed();
    } else if ctrl && (key == Key::from_char('c') || key == Key::from_char('x')) {
        let range = f.cursors[i].range(&f.fields[i]);
        if !range.is_empty() {
            app::copy(&f.fields[i][range]);
            if key == Key::from_char('x') {
                f.cursors[i].delete(&mut f.fields[i], false, false, false);
                f.changed();
            }
        }
    } else if matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
        let motion = match key {
            Key::Home => Motion::Start,
            Key::End => Motion::End,
            Key::Left if ctrl => Motion::WordLeft,
            Key::Right if ctrl => Motion::WordRight,
            Key::Left => Motion::Left,
            _ => Motion::Right,
        };
        f.cursors[i].motion(&f.fields[i], motion, shift);
        f.touch();
    } else if matches!(key, Key::BackSpace | Key::Delete) {
        f.cursors[i].delete(&mut f.fields[i], key == Key::BackSpace, ctrl, false);
        f.changed();
    } else if !ctrl && !alt && !text.is_empty() && !text.chars().any(char::is_control) {
        f.input(text);
    }
    m.hits.clear();
}
pub fn event(m: &mut Model, e: Event) -> bool {
    match e {
        Event::KeyDown | Event::Shortcut => {
            key(m, app::event_key(), app::event_state(), &app::event_text())
        }
        Event::Push if app::event_button() == 1 => {
            if crate::sftp_profiles_ui::push(m) {
                return true;
            }
            let hit = m
                .hits
                .iter()
                .rev()
                .find(|h| h.rect.contains(app::event_x(), app::event_y()))
                .map(|h| h.action.clone());
            if let Some(Action::SftpCommand(stamp, cmd)) = hit {
                if let Command::Focus(i) = cmd {
                    if let Some(f) = &mut m.sftp.form
                        && f.stamp == stamp
                    {
                        let at = f.geometry[i].position(app::event_x(), f.edit_epoch);
                        if let Some(at) = at {
                            let anchor = if app::event_state().intersects(Shortcut::Shift) {
                                f.cursors[i].anchor
                            } else {
                                at
                            };
                            f.focus(i);
                            f.cursors[i].select(&f.fields[i], anchor, at);
                            f.dragging = true;
                        }
                    }
                    m.hits.clear();
                } else {
                    m.sftp_command(stamp, cmd);
                }
            }
        }
        Event::Drag => {
            if let Some(f) = &mut m.sftp.form
                && f.dragging
            {
                if f.profiles {
                    if let Some(at) = f.profile_geometry.position(app::event_x(), f.edit_epoch) {
                        let anchor = f.profile_cursor.anchor;
                        f.profile_cursor.select(&f.profile_label, anchor, at);
                        f.touch();
                        m.hits.clear();
                    }
                    return true;
                }
                let i = f.focus;
                if let Some(at) = f.geometry[i].position(app::event_x(), f.edit_epoch) {
                    let anchor = f.cursors[i].anchor;
                    f.cursors[i].select(&f.fields[i], anchor, at);
                    f.touch();
                    m.hits.clear();
                }
            }
        }
        Event::MouseWheel if m.sftp.form.as_ref().is_some_and(|f| f.report.is_some()) => {
            use fltk::app::MouseWheel;
            let key = match app::event_dy() {
                MouseWheel::Up => Some(Key::Up),
                MouseWheel::Down => Some(Key::Down),
                _ => None,
            };
            if let Some(key) = key {
                crate::sftp_report_ui::key(m, key, Shortcut::None);
            }
        }
        Event::MouseWheel if m.sftp.form.as_ref().is_some_and(|f| f.profiles) => {
            use fltk::app::MouseWheel;
            let key = match app::event_dy() {
                MouseWheel::Up => Some(Key::Up),
                MouseWheel::Down => Some(Key::Down),
                _ => None,
            };
            if let Some(key) = key {
                crate::sftp_profiles_ui::key(m, key, Shortcut::None, "");
            }
        }
        Event::Released => {
            if let Some(f) = &mut m.sftp.form {
                f.dragging = false;
            }
        }
        _ => {}
    }
    true
}
