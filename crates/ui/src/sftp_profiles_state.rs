//! One bounded SFTP profile operation on the existing AuxIo worker.
use files_core::sftp_profiles::{Document, Op, Updated};
use std::{
    sync::{
        Arc,
        mpsc::{SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct State {
    pub document: Arc<Document>,
    pub loaded: bool,
    pub revision: u64,
    pub error: String,
    pub notice: String,
    pub unsynced: bool,
    next: u64,
    pending: Option<(u64, Arc<Op>)>,
    active: Option<(u64, Arc<Op>)>,
    retry: Option<Arc<Op>>,
    until: Option<Instant>,
}
fn write(op: &Op) -> bool {
    !matches!(op, Op::Read)
}
fn short(mut s: String) -> String {
    if s.len() > 512 {
        let mut n = 509;
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        s.truncate(n);
        s.push('…');
    }
    s
}
impl State {
    pub fn request_id(&self) -> Option<u64> {
        self.pending
            .as_ref()
            .or(self.active.as_ref())
            .map(|(id, _)| *id)
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.active.is_some()
    }
    pub fn needs_resolution(&self) -> bool {
        self.unsynced || self.retry.as_deref().is_some_and(write)
    }
    pub fn can_request(&self) -> bool {
        !self.busy() && self.retry.is_none() && !self.unsynced
    }
    pub fn request(&mut self, op: Op) -> bool {
        if !self.can_request() {
            return false;
        }
        self.next = self
            .next
            .checked_add(1)
            .expect("SFTP profile request IDs exhausted");
        self.pending = Some((self.next, Arc::new(op)));
        self.error.clear();
        self.notice = "Esperando respuesta de perfiles SFTP".into();
        self.until = None;
        true
    }
    pub fn refresh(&mut self) {
        if self.busy() {
            return;
        }
        if let Some(op) = self.retry.take() {
            self.next = self.next.checked_add(1).unwrap();
            self.pending = Some((self.next, op));
            self.error.clear();
        } else {
            self.request(if self.unsynced { Op::Sync } else { Op::Read });
        }
    }
    pub fn discard(&mut self) {
        if self.active.is_some() {
            return;
        }
        self.pending = None;
        self.retry = None;
        self.unsynced = false;
        self.error.clear();
        self.notice = "Intento omitido; no revierte lo ya publicado".into();
        self.until = Some(Instant::now() + Duration::from_secs(5));
    }
    pub fn dispatch(&mut self, tx: &SyncSender<crate::aux_io::Request>) -> bool {
        let Some((id, op)) = self.pending.take() else {
            return false;
        };
        let held = Arc::clone(&op);
        match tx.try_send(crate::aux_io::Request::SftpProfiles(id, op)) {
            Ok(()) => {
                self.active = Some((id, held));
                true
            }
            Err(TrySendError::Full(crate::aux_io::Request::SftpProfiles(id, op))) => {
                self.pending = Some((id, op));
                false
            }
            Err(TrySendError::Disconnected(crate::aux_io::Request::SftpProfiles(_, op))) => {
                self.error = "Trabajador de perfiles SFTP no disponible".into();
                self.notice.clear();
                self.retry = Some(op);
                true
            }
            _ => unreachable!(),
        }
    }
    pub fn receive(&mut self, id: u64, result: Result<Updated, String>) -> bool {
        if self.active.as_ref().map(|a| a.0) != Some(id) {
            return false;
        }
        let (_, op) = self.active.take().unwrap();
        match result {
            Ok(result) => {
                self.revision = self
                    .revision
                    .checked_add(1)
                    .expect("SFTP catalogue revisions exhausted");
                self.document = Arc::new(result.document);
                self.loaded = true;
                self.notice = result.notice.into();
                self.until = Some(Instant::now() + Duration::from_secs(5));
                if write(&op) {
                    self.unsynced = result.unsynced;
                }
                if result.unsynced {
                    self.error = short(format!(
                        "{}: {}",
                        result.notice,
                        result.detail.unwrap_or_default()
                    ));
                    self.retry = Some(Arc::new(Op::Sync));
                } else {
                    self.error.clear();
                }
            }
            Err(error) => {
                self.notice.clear();
                self.error = short(error);
                self.retry = Some(op);
                self.until = None;
            }
        }
        true
    }
    pub fn worker_lost(&mut self) -> bool {
        let Some((_, op)) = self.active.take() else {
            return false;
        };
        self.retry = Some(op);
        self.error="Trabajador terminó sin ACK; resultado de guardado sin confirmar. Reintenta u omite explícitamente".into();
        self.notice.clear();
        true
    }
    pub fn tick(&mut self) -> bool {
        if self.until.is_some_and(|t| Instant::now() >= t) {
            self.until = None;
            self.notice.clear();
            true
        } else {
            false
        }
    }
    pub fn status(&self) -> Option<&str> {
        if !self.error.is_empty() {
            Some(&self.error)
        } else if !self.notice.is_empty() {
            Some(&self.notice)
        } else {
            None
        }
    }
    pub fn begin_close(&mut self, discard: bool) {
        if self.pending.as_ref().is_some_and(|(_, op)| !write(op)) {
            self.pending = None;
        }
        if self.retry.as_deref().is_some_and(|op| !write(op)) {
            self.retry = None;
        }
        if discard && self.active.is_none() {
            self.discard();
        }
    }
    pub fn close_ready(&self) -> bool {
        !self.busy() && !self.needs_resolution()
    }
    pub fn can_discard(&self) -> bool {
        self.active.is_none() && (self.pending.is_some() || self.retry.is_some() || self.unsynced)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    fn add() -> Op {
        Op::Add {
            criteria: files_core::sftp_profiles::Criteria {
                fields: [
                    "alias".into(),
                    "".into(),
                    "".into(),
                    "/exact/%41/$()".into(),
                ],
            },
            label: "Servidor".into(),
        }
    }
    fn updated(unsynced: bool) -> Updated {
        let Op::Add { criteria, label } = add() else {
            unreachable!()
        };
        Updated {
            document: Document {
                next: 2,
                rows: vec![files_core::sftp_profiles::Entry {
                    id: 1,
                    label,
                    criteria,
                }],
            },
            unsynced,
            detail: unsynced.then(|| "synthetic sync error after publication".into()),
            notice: "Guardado",
        }
    }
    #[test]
    fn stale_ack_cannot_release_write_close_and_retry_keeps_captured_intent() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut s = State::default();
        assert!(s.request(add()));
        assert!(s.dispatch(&tx));
        let crate::aux_io::Request::SftpProfiles(id, op) = rx.recv().unwrap() else {
            panic!()
        };
        s.begin_close(true);
        assert!(!s.close_ready());
        assert!(!s.can_discard());
        assert!(!s.receive(id + 1, Ok(updated(false))));
        assert!(!s.close_ready());
        assert!(s.receive(id, Err("readonly".into())));
        assert!(s.needs_resolution());
        assert!(!s.request(Op::Read));
        s.refresh();
        s.dispatch(&tx);
        let crate::aux_io::Request::SftpProfiles(next, retry) = rx.recv().unwrap() else {
            panic!()
        };
        assert!(next > id);
        assert!(Arc::ptr_eq(&op, &retry));
        s.receive(next, Ok(updated(false)));
        assert!(s.close_ready());
        assert_eq!(s.document.rows[0].criteria.fields[3], "/exact/%41/$()");
    }
    #[test]
    fn unsynced_publication_retries_only_sync_and_discard_does_not_undo() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut s = State::default();
        s.request(add());
        s.dispatch(&tx);
        let crate::aux_io::Request::SftpProfiles(id, _) = rx.recv().unwrap() else {
            panic!()
        };
        s.receive(id, Ok(updated(true)));
        assert!(s.unsynced && s.needs_resolution());
        assert_eq!(s.document.rows.len(), 1);
        s.refresh();
        s.dispatch(&tx);
        let crate::aux_io::Request::SftpProfiles(next, op) = rx.recv().unwrap() else {
            panic!()
        };
        assert!(matches!(&*op, Op::Sync));
        s.receive(next, Err("sync still fails".into()));
        s.discard();
        assert!(s.close_ready());
        assert_eq!(s.document.rows.len(), 1);
    }
    #[test]
    fn full_channel_worker_loss_and_read_failure_do_not_spin_or_fake_ack() {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.try_send(crate::aux_io::Request::SftpProfiles(
            999,
            Arc::new(Op::Read),
        ))
        .unwrap();
        let mut s = State::default();
        s.request(add());
        assert!(!s.dispatch(&tx));
        assert!(s.busy());
        rx.recv().unwrap();
        assert!(s.dispatch(&tx));
        rx.recv().unwrap();
        assert!(s.worker_lost());
        assert!(!s.close_ready());
        assert!(!s.dispatch(&tx));
        s.begin_close(true);
        assert!(s.close_ready());
        assert!(s.request(Op::Read));
        s.dispatch(&tx);
        let crate::aux_io::Request::SftpProfiles(id, _) = rx.recv().unwrap() else {
            panic!()
        };
        s.receive(id, Err("future version".into()));
        assert!(!s.dispatch(&tx));
        s.begin_close(false);
        assert!(s.close_ready());
    }
}
