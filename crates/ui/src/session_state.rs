//! One session request on the existing AuxIo channel, immutable buffer until ACK.
use crate::{
    aux_io::Request,
    session_store::{Opened, Published},
};
use std::sync::{
    Arc,
    mpsc::{SyncSender, TrySendError},
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Disabled,
    Off,
    Opening,
    Writable,
    ReadOnly,
}
pub struct State {
    pub mode: Mode,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub restore_allowed: bool,
    pub opening_generation: u64,
    pub save_after_open: bool,
    pub closing_saved: bool,
    pub pending: Option<Arc<Vec<u8>>>,
    pub in_flight: bool,
    revision: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            notice: None,
            error: None,
            restore_allowed: false,
            opening_generation: 0,
            save_after_open: false,
            closing_saved: false,
            pending: None,
            in_flight: false,
            revision: 0,
        }
    }
}
impl State {
    pub fn begin(
        &mut self,
        tx: &SyncSender<Request>,
        generation: u64,
        restore: bool,
        save: bool,
    ) -> Result<(), &'static str> {
        if self.mode != Mode::Off {
            return Err("Sesión ya iniciada");
        }
        tx.try_send(Request::SessionOpen(restore))
            .map_err(|e| match e {
                TrySendError::Full(_) => "Trabajador ocupado; reintenta Guardar sesión",
                TrySendError::Disconnected(_) => "Trabajador de sesión desconectado",
            })?;
        self.mode = Mode::Opening;
        self.opening_generation = generation;
        self.restore_allowed = restore;
        self.save_after_open = save;
        Ok(())
    }
    pub fn opened(&mut self, o: &Opened) {
        self.mode = if o.writable {
            Mode::Writable
        } else {
            Mode::ReadOnly
        };
        self.notice = o.notice.clone();
    }
    pub fn submit(&mut self, bytes: Vec<u8>) -> Result<(), &'static str> {
        if self.mode != Mode::Writable {
            return Err("Sesión no disponible para guardar; revisa el aviso");
        }
        if self.pending.is_some() || self.in_flight {
            return Err("Espera confirmación del guardado de sesión anterior");
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("Revisión de sesión agotada")?;
        self.pending = Some(Arc::new(bytes));
        self.error = None;
        Ok(())
    }
    pub fn dispatch(&mut self, tx: &SyncSender<Request>) {
        if self.in_flight || self.error.is_some() {
            return;
        }
        let Some(bytes) = self.pending.as_ref() else {
            return;
        };
        match tx.try_send(Request::SessionSave(self.revision, Arc::clone(bytes))) {
            Ok(()) => self.in_flight = true,
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                self.error = Some("Trabajador de sesión desconectado; estado sin guardar".into())
            }
        }
    }
    pub fn acknowledge(&mut self, revision: u64, result: Result<Published, String>) -> bool {
        if !self.in_flight || revision != self.revision {
            return false;
        }
        self.in_flight = false;
        match result {
            Ok(Published::Durable) => {
                self.pending = None;
                self.error = None;
                self.notice = Some("Sesión guardada".into());
            }
            Ok(Published::Unsynced(e)) | Err(e) => self.error = Some(e),
        }
        true
    }
    pub fn retry(&mut self) {
        self.error = None;
    }
    pub fn message(&self) -> Option<&str> {
        self.error.as_deref().or(self.notice.as_deref())
    }
    pub fn can_restore(&self, generation: u64, closing: bool) -> bool {
        self.mode == Mode::Opening
            && self.restore_allowed
            && self.opening_generation == generation
            && !closing
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[test]
    fn full_ack_and_retry_keep_exact_buffer() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut s = State {
            mode: Mode::Writable,
            ..State::default()
        };
        s.submit(vec![1, 2, 3]).unwrap();
        tx.send(Request::Load(0, "/a".into())).unwrap();
        s.dispatch(&tx);
        assert!(!s.in_flight && s.pending.is_some());
        drop(rx.recv().unwrap());
        s.dispatch(&tx);
        assert!(s.in_flight);
        drop(rx.recv().unwrap());
        assert!(!s.acknowledge(0, Ok(Published::Durable)));
        assert!(s.acknowledge(1, Ok(Published::Unsynced("published, unsynced".into()))));
        assert!(s.pending.is_some() && s.error.is_some());
        s.retry();
        s.dispatch(&tx);
        drop(rx.recv().unwrap());
        s.acknowledge(1, Ok(Published::Durable));
        assert!(s.pending.is_none());
    }
    #[test]
    fn late_startup_never_overwrites_intent_or_closing() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let mut s = State::default();
        s.begin(&tx, 7, true, false).unwrap();
        assert!(s.can_restore(7, false));
        assert!(!s.can_restore(8, false));
        assert!(!s.can_restore(7, true));
        s.restore_allowed = false;
        assert!(!s.can_restore(7, false));
    }
}
