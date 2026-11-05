//! Private, bounded files clipboard. Never claims OS MIME interoperability.
use files_core::selection_source::Source;
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
pub const LIMIT: usize = 2 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Copy,
    Cut,
}
#[derive(Debug)]
pub struct Lease {
    pub system: bool,
    pub generation: u64,
    pub kind: Kind,
    pub source: Arc<Source>,
    pub names: Arc<[OsString]>,
}
impl Lease {
    pub fn retained_bytes(&self) -> usize {
        self.source.retained_bytes()
            + self
                .names
                .iter()
                .map(|n| n.capacity() + std::mem::size_of::<OsString>())
                .sum::<usize>()
            + std::mem::size_of::<Self>()
            + 512
    }
}
pub struct Input {
    pub id: u64,
    pub kind: Kind,
    pub parent: PathBuf,
    pub names: Arc<[OsString]>,
    pub reserve: usize,
}
struct Pending {
    input_id: u64,
    kind: Kind,
    cancel: Arc<AtomicBool>,
    reserve: usize,
}
struct Buffer {
    lease: Arc<Lease>,
    remaining: Vec<usize>,
}
#[derive(Clone)]
pub struct JobTag {
    pub lease: Arc<Lease>,
    pub destination: PathBuf,
}
pub struct RecoveryTag {
    pub job: [u8; 16],
    pub proof: files_core::move_journal::IntentProof,
    pub tag: JobTag,
}
impl RecoveryTag {
    pub fn matches(&self, context: &crate::move_operation::Context) -> bool {
        self.job == context.job && context.proof.as_ref() == Some(&self.proof)
    }
}
#[derive(Default)]
pub struct State {
    next: u64,
    current: Option<Buffer>,
    desired: Option<Input>,
    pending: Option<Pending>,
    leases: Vec<Weak<Lease>>,
    pub error: String,
    closing: bool,
    external_reserved: usize,
}
impl State {
    pub fn intent_generation(&self) -> u64 {
        self.next
    }
    pub fn busy(&self) -> bool {
        self.desired.is_some() || self.pending.is_some()
    }
    pub fn count(&self) -> usize {
        self.current.as_ref().map_or(0, |b| b.remaining.len())
    }
    /// Absolute paths of the items still in Files' clipboard (Ctrl+M).
    pub fn copied_paths(&self) -> Vec<PathBuf> {
        let Some(b) = &self.current else {
            return Vec::new();
        };
        b.remaining
            .iter()
            .filter_map(|&i| b.lease.names.get(i))
            .map(|n| b.lease.source.parent().join(n))
            .collect()
    }
    pub fn kind(&self) -> Option<Kind> {
        self.current.as_ref().map(|b| b.lease.kind)
    }
    pub fn generation(&self) -> Option<u64> {
        self.current.as_ref().map(|b| b.lease.generation)
    }
    pub fn retained_bytes(&self) -> usize {
        self.leases
            .iter()
            .filter_map(Weak::upgrade)
            .map(|b| b.retained_bytes())
            .sum::<usize>()
            + self.pending.as_ref().map_or(0, |p| p.reserve)
            + self.desired.as_ref().map_or(0, |i| i.reserve)
            + self.external_reserved
            + 4096
    }
    /// Reserve a separate in-flight proof. Unlike clipboard replacement, this
    /// cannot subtract a desired clipboard capture that is still retained.
    pub fn reserve_external(&mut self, reserve: usize) -> bool {
        if self.closing
            || self.external_reserved != 0
            || reserve == 0
            || self
                .retained_bytes()
                .checked_add(reserve)
                .is_none_or(|n| n > LIMIT)
        {
            return false;
        }
        self.external_reserved = reserve;
        true
    }
    pub fn release_external(&mut self) {
        self.external_reserved = 0;
    }
    /// Current/capture(+scratch)/dialog/one external result. Queue proof FDs
    /// have their own request accounting, so duplicate counts are conservative.
    pub fn external_fds(&self) -> usize {
        if self.current.is_some() || self.busy() || self.leases.iter().any(|w| w.strong_count() > 0)
        {
            4
        } else {
            0
        }
    }
    /// Borrowed UI preflight calls this BEFORE cloning selection/path names.
    pub fn can_capture(&mut self, reserve: usize) -> bool {
        self.leases.retain(|w| w.strong_count() > 0);
        !self.closing
            && self.leases.len() < 32
            && self
                .retained_bytes()
                .saturating_sub(self.desired.as_ref().map_or(0, |i| i.reserve))
                .saturating_add(reserve)
                <= LIMIT
    }
    pub fn capture(
        &mut self,
        kind: Kind,
        parent: PathBuf,
        names: Arc<[OsString]>,
        reserve: usize,
    ) -> Result<(), &'static str> {
        if names.is_empty()
            || names.len() > 128
            || parent.as_os_str().len() > 4096
            || !parent.is_absolute()
        {
            return Err("Portapapeles de files: entrada fuera de límites");
        }
        let reserve = reserve.max(names.iter().fold(
            64usize * 1024 + parent.capacity().saturating_mul(2),
            |n, name| {
                n.saturating_add(name.capacity().saturating_mul(2))
                    .saturating_add(128)
            },
        ));
        if !self.can_capture(reserve) {
            return Err(
                "Portapapeles de files: presupuesto conjunto agotado; espera trabajos pendientes",
            );
        }
        self.next = self
            .next
            .checked_add(1)
            .expect("clipboard generation exhausted");
        if let Some(p) = &self.pending {
            p.cancel.store(true, Ordering::Release);
        }
        self.desired = Some(Input {
            id: self.next,
            kind,
            parent,
            names,
            reserve,
        });
        self.error.clear();
        Ok(())
    }
    pub fn take_request(&mut self) -> Option<(Input, Arc<AtomicBool>)> {
        if self.closing || self.pending.is_some() {
            return None;
        }
        let input = self.desired.take()?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending = Some(Pending {
            input_id: input.id,
            kind: input.kind,
            cancel: cancel.clone(),
            reserve: input.reserve,
        });
        Some((input, cancel))
    }
    pub fn unsent(&mut self, input: Input) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.input_id == input.id)
        {
            self.pending = None;
            if !self.closing {
                self.desired = Some(input);
            }
        }
    }
    pub fn failed_send(&mut self, id: u64, error: String) {
        if self.pending.as_ref().is_some_and(|p| p.input_id == id) {
            self.pending = None;
            self.error = error;
        }
    }
    pub fn receive(&mut self, id: u64, result: Result<Source, String>) {
        if self.pending.as_ref().is_none_or(|p| p.input_id != id) {
            return;
        }
        let p = self.pending.take().unwrap();
        if self.closing || self.next != id {
            return;
        }
        match result {
            Ok(source) => {
                let source = Arc::new(source);
                let names = source.names().clone();
                let lease = Arc::new(Lease {
                    system: false,
                    generation: id,
                    kind: p.kind,
                    source,
                    names,
                });
                self.leases.retain(|w| w.strong_count() > 0);
                if self.retained_bytes().saturating_add(lease.retained_bytes()) > LIMIT
                    || self.leases.len() >= 32
                {
                    self.error="Portapapeles de files: captura excede presupuesto; buffer anterior conservado".into();
                    return;
                }
                self.leases.push(Arc::downgrade(&lease));
                self.current = Some(Buffer {
                    remaining: (0..lease.names.len()).collect(),
                    lease,
                });
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }
    pub fn lease(&mut self) -> Result<Arc<Lease>, &'static str> {
        self.leases.retain(|w| w.strong_count() > 0);
        if self.closing || self.busy() {
            return Err("Espera la captura del portapapeles de files");
        }
        let b = self
            .current
            .as_ref()
            .filter(|b| !b.remaining.is_empty())
            .ok_or("Portapapeles de files vacío; selecciona y Copia/Corta primero")?;
        if b.remaining.len() == b.lease.names.len() {
            return Ok(b.lease.clone());
        }
        let additional = b.lease.source.retained_bytes()
            + b.remaining
                .iter()
                .map(|&i| b.lease.names[i].len() + std::mem::size_of::<OsString>())
                .sum::<usize>()
            + 1024;
        if self.retained_bytes().saturating_add(additional) > LIMIT || self.leases.len() >= 32 {
            return Err("Portapapeles de files: presupuesto de lote pendiente agotado");
        }
        let names = b
            .remaining
            .iter()
            .map(|&i| b.lease.names[i].clone())
            .collect::<Vec<_>>()
            .into();
        let lease = Arc::new(Lease {
            system: b.lease.system,
            generation: b.lease.generation,
            kind: b.lease.kind,
            source: b.lease.source.clone(),
            names,
        });
        self.leases.push(Arc::downgrade(&lease));
        Ok(lease)
    }
    /// No filesystem reads or absence inference. Only a matching clipboard
    /// generation and positively confirmed move roots consume cut entries.
    pub fn acknowledge(&mut self, tag: &JobTag, outcome: &crate::copy_job::Outcome) {
        let Some(b) = self.current.as_mut().filter(|b| {
            !tag.lease.system
                && Arc::ptr_eq(&b.lease.source, &tag.lease.source)
                && b.lease.generation == tag.lease.generation
                && b.lease.kind == Kind::Cut
                && outcome.moving
        }) else {
            return;
        };
        let Some(report) = outcome.report() else {
            return;
        };
        let full = outcome.result.is_ok() && report.completed_count == tag.lease.names.len() as u64;
        b.remaining.retain(|&i| {
            let name = &b.lease.names[i];
            !(tag.lease.names.iter().any(|n| n == name)
                && (full
                    || report.completed.iter().any(|p| {
                        p.parent() == Some(tag.destination.as_path())
                            && p.file_name() == Some(name.as_os_str())
                    })))
        });
        if b.remaining.is_empty() {
            self.current = None;
        }
    }
    pub fn begin_close(&mut self) {
        self.closing = true;
        self.desired = None;
        // Keep current until actual window destruction; cancelling Close must
        // not discard the private clipboard or its partial-cut ACK updates.
        if let Some(p) = &self.pending {
            p.cancel.store(true, Ordering::Release);
        }
    }
    pub fn is_drained(&self) -> bool {
        self.pending.is_none()
    }
    pub fn reopen(&mut self) {
        if self.is_drained() {
            self.closing = false;
        }
    }
}
