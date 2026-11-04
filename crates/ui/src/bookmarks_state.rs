//! One bounded favourites operation on the existing AuxIo worker.
use files_core::bookmarks::{Document, Op, Updated};
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
    pub error: String,
    pub notice: String,
    pub panel: Option<crate::bookmarks_ui::Panel>,
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
            .expect("favourites request IDs exhausted");
        self.pending = Some((self.next, Arc::new(op)));
        self.error.clear();
        self.notice = "Esperando respuesta de favoritos".into();
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
        match tx.try_send(crate::aux_io::Request::Bookmarks(id, op)) {
            Ok(()) => {
                self.active = Some((id, held));
                true
            }
            Err(TrySendError::Full(crate::aux_io::Request::Bookmarks(id, op))) => {
                self.pending = Some((id, op));
                false
            }
            Err(TrySendError::Disconnected(crate::aux_io::Request::Bookmarks(_, op))) => {
                self.error = "Trabajador de favoritos no disponible".into();
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
        if let Some(panel) = &mut self.panel {
            panel.invalidate();
        }
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
        self.panel = None;
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
    use crate::aux_io::Request;
    use std::sync::mpsc;
    fn add() -> Op {
        Op::Add {
            path: "/captured".into(),
            label: "Captured".into(),
        }
    }
    fn updated(unsynced: bool) -> Updated {
        Updated {
            document: Document {
                next: 2,
                rows: vec![files_core::bookmarks::Entry {
                    id: 1,
                    path: "/captured".into(),
                    label: "Captured".into(),
                }],
            },
            unsynced,
            detail: unsynced.then(|| "synthetic sync failure".into()),
            notice: if unsynced {
                "Publicado sin sync"
            } else {
                "Guardado"
            },
        }
    }
    #[test]
    fn exact_intent_retries_and_stale_ack_does_not_release_active_close_gate() {
        let (tx, input) = mpsc::sync_channel(1);
        let mut s = State::default();
        assert!(s.request(add()));
        assert!(s.dispatch(&tx));
        let Request::Bookmarks(id, op) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(!s.request(Op::Read));
        s.begin_close(true);
        assert!(!s.close_ready());
        assert!(!s.receive(id + 1, Ok(updated(false))));
        assert!(!s.close_ready());
        assert!(s.receive(id, Err("read only".into())));
        s.begin_close(false);
        assert!(!s.close_ready());
        s.refresh();
        s.dispatch(&tx);
        let Request::Bookmarks(next, retry) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(next > id && Arc::ptr_eq(&op, &retry));
        s.receive(next, Ok(updated(false)));
        assert!(s.close_ready());
        assert_eq!(s.document.rows[0].path, std::path::Path::new("/captured"));
    }
    #[test]
    fn unsynced_publication_retries_only_sync_and_discard_never_undoes_rows() {
        let (tx, input) = mpsc::sync_channel(1);
        let mut s = State::default();
        s.request(add());
        s.dispatch(&tx);
        let Request::Bookmarks(id, _) = input.try_recv().unwrap() else {
            panic!()
        };
        s.receive(id, Ok(updated(true)));
        assert!(s.unsynced && s.needs_resolution());
        assert!(s.error.contains("synthetic"));
        s.refresh();
        s.dispatch(&tx);
        let Request::Bookmarks(next, op) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(matches!(*op, Op::Sync));
        s.begin_close(true);
        assert!(!s.close_ready());
        s.receive(next, Err("sync still fails".into()));
        s.begin_close(true);
        assert!(s.close_ready());
        assert_eq!(s.document.rows.len(), 1);
    }
    #[test]
    fn full_queue_preserves_arc_and_unadmitted_intent_can_be_discarded() {
        let (tx, input) = mpsc::sync_channel(1);
        tx.send(Request::Load(9, "/fixture".into())).unwrap();
        let mut s = State::default();
        s.request(add());
        let held = Arc::clone(&s.pending.as_ref().unwrap().1);
        assert!(!s.dispatch(&tx));
        assert!(Arc::ptr_eq(&held, &s.pending.as_ref().unwrap().1));
        s.begin_close(true);
        assert!(s.close_ready());
        input.try_recv().unwrap();
        assert!(input.try_recv().is_err());
    }
    #[test]
    fn close_drops_unadmitted_reads_but_waits_for_admitted_read_ack() {
        let (tx, input) = mpsc::sync_channel(1);
        let mut s = State::default();
        s.request(Op::Read);
        s.begin_close(false);
        assert!(!s.dispatch(&tx));
        assert!(s.close_ready());
        s.request(Op::Read);
        s.dispatch(&tx);
        let Request::Bookmarks(id, _) = input.try_recv().unwrap() else {
            panic!()
        };
        s.begin_close(false);
        assert!(!s.close_ready());
        s.receive(id, Err("invalid future data".into()));
        s.begin_close(false);
        assert!(s.close_ready());
        assert!(!s.loaded);
    }
}
