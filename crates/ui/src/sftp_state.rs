//! One latest capability probe on shared AuxIo. Read-only ACK is not a mount ACK.
use crate::{aux_io, line_edit::Cursor, line_field::Geometry};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
};
fn epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut value = NEXT.load(Ordering::Relaxed);
    loop {
        let next = value.checked_add(1).expect("SFTP epochs exhausted");
        match NEXT.compare_exchange_weak(value, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return value,
            Err(actual) => value = actual,
        }
    }
}
pub const LIMITS: [usize; 4] = [253, 64, 5, 4096];
pub struct Review {
    pub source: String,
    pub target: files_core::sftp_target::Target,
    pub probe: u64,
}
pub struct Form {
    pub stamp: u64,
    pub edit_epoch: u64,
    pub fields: [String; 4],
    pub cursors: [Cursor; 4],
    pub geometry: [Geometry; 4],
    pub focus: usize,
    pub review: Option<Review>,
    pub error: String,
    pub capability: Option<Result<Capability, String>>,
    pub probe: u64,
    pub dragging: bool,
    pub profiles: bool,
    pub report: Option<crate::sftp_report::Report>,
    pub profile_label: String,
    pub profile_cursor: Cursor,
    pub profile_geometry: Geometry,
    pub profile_selected: usize,
}
impl Form {
    fn new() -> Self {
        Self {
            stamp: epoch(),
            edit_epoch: epoch(),
            fields: Default::default(),
            cursors: Default::default(),
            geometry: Default::default(),
            focus: 0,
            review: None,
            error: String::new(),
            capability: None,
            probe: 0,
            dragging: false,
            profiles: false,
            report: None,
            profile_label: String::new(),
            profile_cursor: Default::default(),
            profile_geometry: Default::default(),
            profile_selected: 0,
        }
    }
    pub fn touch(&mut self) {
        self.edit_epoch = epoch();
    }
    pub fn focus(&mut self, i: usize) {
        if i < 4 {
            self.focus = i;
            self.dragging = false;
            self.touch();
        }
    }
    pub fn changed(&mut self) {
        self.review = None;
        self.error.clear();
        self.touch();
    }
    pub fn input(&mut self, text: &str) {
        if self.report.is_some() {
            return;
        }
        if self.profiles {
            if text.len() > files_core::sftp_profiles::MAX_LABEL
                || text.chars().any(char::is_control)
            {
                self.error = "Etiqueta rechazada: máximo96 bytes y sin controles".into();
                return;
            }
            match self.profile_cursor.insert_bounded(
                &mut self.profile_label,
                text,
                false,
                files_core::sftp_profiles::MAX_LABEL,
            ) {
                Ok(()) => {
                    self.error.clear();
                    self.touch();
                }
                Err(e) => self.error = e.into(),
            }
            return;
        }
        let i = self.focus;
        if text.len() > LIMITS[i] || text.chars().any(char::is_control) {
            self.error = "Texto rechazado: exceso de bytes o carácter de control".into();
            return;
        }
        match self.cursors[i].insert_bounded(&mut self.fields[i], text, false, LIMITS[i]) {
            Ok(()) => self.changed(),
            Err(e) => self.error = e.into(),
        }
    }
    pub fn review(&mut self) {
        match files_core::sftp_target::Target::parse(
            &self.fields[0],
            &self.fields[1],
            &self.fields[2],
            &self.fields[3],
        ) {
            Ok(target) => {
                let source = target.source();
                self.review = Some(Review {
                    source,
                    target,
                    probe: self.probe,
                });
                self.error.clear();
            }
            Err(e) => {
                self.review = None;
                self.error = e.into();
            }
        }
        self.touch();
    }
}
#[derive(Clone, Debug)]
pub struct Capability {
    pub supported: bool,
    pub sshfs: Option<PathBuf>,
    pub ssh: Option<PathBuf>,
    pub unmount: Option<PathBuf>,
    pub fuse_access: bool,
}
impl Capability {
    pub fn tools_ready(&self) -> bool {
        self.supported
            && self.sshfs.is_some()
            && self.ssh.is_some()
            && self.unmount.is_some()
            && self.fuse_access
    }
    pub fn summary(&self) -> String {
        if !self.supported {
            return "Montaje SFTP: sólo Linux en esta entrega".into();
        }
        let mut missing = Vec::new();
        if self.sshfs.is_none() {
            missing.push("sshfs");
        }
        if self.ssh.is_none() {
            missing.push("ssh");
        }
        if self.unmount.is_none() {
            missing.push("fusermount3");
        }
        if !self.fuse_access {
            missing.push("acceso /dev/fuse");
        }
        if missing.is_empty() {
            "Herramientas presentes · claves/agente SSH".into()
        } else {
            format!("Falta: {}", missing.join(", "))
        }
    }
}
#[cfg(target_os = "linux")]
pub fn probe() -> Result<Capability, String> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = std::env::var_os("PATH").unwrap_or_default();
    if path.len() > 4096 {
        return Err("PATH supera4096 bytes; comprobación no realizada".into());
    }
    let mut dirs = Vec::new();
    for dir in std::env::split_paths(&path) {
        if dirs.len() == 32 {
            return Err("PATH supera32 entradas; comprobación no realizada".into());
        }
        dirs.push(dir);
    }
    let find = |name: &str| -> Result<Option<PathBuf>, String> {
        for dir in &dirs {
            if !dir.is_absolute() {
                continue;
            }
            if dir.as_os_str().len() + 1 + name.len() > 4096 {
                return Err("Ruta de herramienta fuera del límite".into());
            }
            let p = dir.join(name);
            let c = CString::new(p.as_os_str().as_bytes()).map_err(|_| "NUL en PATH")?;
            if p.metadata().is_ok_and(|m| m.is_file())
                && unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0
            {
                return Ok(Some(p));
            }
        }
        Ok(None)
    };
    Ok(Capability {
        supported: true,
        sshfs: find("sshfs")?,
        ssh: find("ssh")?,
        unmount: find("fusermount3")?,
        fuse_access: unsafe { libc::access(c"/dev/fuse".as_ptr(), libc::R_OK | libc::W_OK) } == 0,
    })
}
#[cfg(not(target_os = "linux"))]
pub fn probe() -> Result<Capability, String> {
    Ok(Capability {
        supported: false,
        sshfs: None,
        ssh: None,
        unmount: None,
        fuse_access: false,
    })
}
#[derive(Default)]
pub struct State {
    pub form: Option<Form>,
    pub transport: crate::sftp_executor::Client,
    pub inspected: bool,
    pub open_requested: Option<u64>,
    pending: Option<u64>,
    pub active: Option<u64>,
}
impl State {
    pub fn open(&mut self) {
        self.form = Some(Form::new());
        self.recheck();
    }
    pub fn close(&mut self) {
        self.form = None;
        self.pending = None;
    }
    pub fn recheck(&mut self) {
        if let Some(f) = &mut self.form {
            let id = epoch();
            f.review = None;
            f.probe = id;
            f.capability = None;
            self.pending = Some(id);
        }
    }
    pub fn dispatch(&mut self, tx: &SyncSender<aux_io::Request>) -> bool {
        if self.active.is_some() || self.form.is_none() {
            return false;
        }
        let Some(id) = self.pending.take() else {
            return false;
        };
        match tx.try_send(aux_io::Request::SftpCapability(id)) {
            Ok(()) => {
                self.active = Some(id);
                true
            }
            Err(TrySendError::Full(_)) => {
                self.pending = Some(id);
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                if let Some(f) = &mut self.form {
                    f.capability = Some(Err("Worker I/O no disponible".into()));
                }
                true
            }
        }
    }
    pub fn receive(&mut self, id: u64, value: Result<Capability, String>) -> bool {
        if self.active != Some(id) {
            return false;
        }
        self.active = None;
        if let Some(f) = &mut self.form
            && f.probe == id
        {
            f.capability = Some(value);
            return true;
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cap() -> Result<Capability, String> {
        Ok(Capability {
            supported: true,
            sshfs: None,
            ssh: None,
            unmount: None,
            fuse_access: false,
        })
    }
    #[test]
    fn late_probe_after_close_reopen_does_not_replace_new_form_and_admission_waits_ack() {
        let (mut s, (tx, rx)) = (State::default(), std::sync::mpsc::sync_channel(8));
        s.open();
        assert!(s.dispatch(&tx));
        let old = s.active.unwrap();
        rx.try_recv().unwrap();
        s.close();
        s.open();
        let stamp = s.form.as_ref().unwrap().stamp;
        assert!(!s.dispatch(&tx));
        assert!(!s.receive(old, cap()));
        assert!(s.form.as_ref().unwrap().capability.is_none());
        assert!(s.dispatch(&tx));
        let new = s.active.unwrap();
        assert_ne!(old, new);
        assert!(s.receive(new, cap()));
        assert_eq!(s.form.as_ref().unwrap().stamp, stamp);
    }
    #[test]
    fn recheck_is_latest_and_close_removes_future_admission_without_faking_active_ack() {
        let (mut s, (tx, rx)) = (State::default(), std::sync::mpsc::sync_channel(8));
        s.open();
        s.dispatch(&tx);
        rx.try_recv().unwrap();
        let old = s.active.unwrap();
        for _ in 0..100 {
            s.recheck();
        }
        assert!(!s.dispatch(&tx));
        assert!(rx.try_recv().is_err());
        assert!(!s.receive(old, cap()));
        assert!(s.dispatch(&tx));
        rx.try_recv().unwrap();
        let active = s.active;
        s.close();
        assert_eq!(s.active, active);
        assert!(!s.dispatch(&tx));
        assert!(!s.receive(active.unwrap(), cap()));
        assert!(s.active.is_none());
    }
    #[test]
    fn rejected_text_keeps_draft_cursor_review_and_history_while_edit_invalidates_review() {
        let mut f = Form::new();
        f.input("Work");
        f.focus(3);
        f.input("/Fotos ñ/%20");
        f.review();
        assert_eq!(f.review.as_ref().unwrap().source, "Work:/Fotos ñ/%20");
        let epoch = f.edit_epoch;
        let cursor = f.cursors[3].caret;
        f.input(&"x".repeat(1024 * 1024));
        assert_eq!(f.edit_epoch, epoch);
        assert_eq!(f.cursors[3].caret, cursor);
        assert!(f.review.is_some());
        f.input("\n");
        assert_eq!(f.fields[3], "/Fotos ñ/%20");
        f.input(" ok");
        assert!(f.review.is_none());
        f.cursors[3].undo(&mut f.fields[3], false);
        assert_eq!(f.fields[3], "/Fotos ñ/%20");
    }
    #[test]
    fn reviewed_intent_retains_port_and_probe_and_recheck_revokes_it() {
        let mut s = State::default();
        s.open();
        let f = s.form.as_mut().unwrap();
        f.fields = ["host".into(), "user".into(), "2222".into(), "/path".into()];
        f.review();
        let r = f.review.as_ref().unwrap();
        assert_eq!(r.probe, f.probe);
        let args = r
            .target
            .foreground_args(std::path::Path::new("/owned/mount"))
            .unwrap();
        assert!(args.windows(2).any(|a| a[0] == "-p" && a[1] == "2222"));
        s.recheck();
        assert!(s.form.as_ref().unwrap().review.is_none());
    }
}
