//! Current-browser measured observations, not filesystem snapshots or mutation proofs.
use files_core::folder_usage::{Measurement, Mode, Observation};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::SystemTime,
};
pub const MAX_ROWS: usize = 128;
const MAX_NAMES: usize = 64 * 1024;
#[derive(Debug)]
pub enum Status {
    Queued,
    Running {
        bytes: u64,
        entries: u64,
    },
    Measured {
        bytes: u64,
        entries: u64,
        at: SystemTime,
    },
    Failed(String),
    Cancelled,
}
pub struct Record {
    pub id: u64,
    pub name: OsString,
    pub status: Status,
    pub mode: Mode,
    pub allocation: Option<Observation>,
}
pub struct Work {
    pub path: PathBuf,
    pub mode: Mode,
}
struct Active {
    id: u64,
    epoch: u64,
    valid: bool,
    mode: Mode,
}
pub struct State {
    root: PathBuf,
    epoch: u64,
    records: Vec<Record>,
    active: Option<Active>,
    paused: bool,
    next_id: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            epoch: 0,
            records: Vec::with_capacity(MAX_ROWS),
            active: None,
            paused: false,
            next_id: 1,
        }
    }
}
fn allocate(next: &mut u64) -> u64 {
    let id = *next;
    *next = next.checked_add(1).expect("size record id exhausted");
    id
}
impl State {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn rows(&self, root: &Path) -> &[Record] {
        if self.root == root {
            &self.records
        } else {
            &[]
        }
    }
    pub fn can_forget(&self, epoch: u64, id: u64) -> bool {
        epoch == self.epoch
            && !self.active.as_ref().is_some_and(|a| a.id == id)
            && self.records.iter().any(|r| {
                r.id == id
                    && matches!(
                        r.status,
                        Status::Measured { .. } | Status::Failed(_) | Status::Cancelled
                    )
            })
    }
    pub fn forget_id(&mut self, epoch: u64, id: u64) -> bool {
        if !self.can_forget(epoch, id) {
            return false;
        }
        self.records.retain(|r| r.id != id);
        true
    }
    pub fn forget_measured(&mut self, epoch: u64, ids: &[u64]) -> Result<usize, &'static str> {
        if epoch != self.epoch || ids.is_empty() || ids.len() > MAX_ROWS {
            return Err("Informe antiguo o sin mediciones");
        }
        for (i, id) in ids.iter().enumerate() {
            if ids[..i].contains(id)
                || !self.can_forget(epoch, *id)
                || !self
                    .records
                    .iter()
                    .any(|r| r.id == *id && matches!(r.status, Status::Measured { .. }))
            {
                return Err("Las mediciones cambiaron; actualiza el informe");
            }
        }
        self.records.retain(|r| !ids.contains(&r.id));
        Ok(ids.len())
    }
    pub fn retry_id(&mut self, epoch: u64, id: u64) -> Option<u64> {
        if epoch != self.epoch {
            return None;
        }
        let record = self
            .records
            .iter_mut()
            .find(|r| r.id == id && matches!(r.status, Status::Failed(_) | Status::Cancelled))?;
        if let Some(a) = &mut self.active
            && a.id == id
        {
            a.valid = false;
        }
        record.id = allocate(&mut self.next_id);
        record.allocation = None;
        record.status = Status::Queued;
        Some(record.id)
    }
    /// Explicitly replace one completed observation; other records and admission order stay intact.
    pub fn recalculate_id(&mut self, epoch: u64, id: u64) -> Option<u64> {
        if epoch != self.epoch {
            return None;
        }
        let record = self
            .records
            .iter_mut()
            .find(|r| r.id == id && matches!(r.status, Status::Measured { .. }))?;
        record.id = allocate(&mut self.next_id);
        record.allocation = None;
        record.status = Status::Queued;
        Some(record.id)
    }
    pub fn enqueue<'a>(
        &mut self,
        root: &Path,
        names: impl Iterator<Item = &'a OsStr> + Clone,
    ) -> Result<(), &'static str> {
        self.enqueue_mode(root, names, Mode::Apparent)
    }
    pub fn enqueue_mode<'a>(
        &mut self,
        root: &Path,
        names: impl Iterator<Item = &'a OsStr> + Clone,
        mode: Mode,
    ) -> Result<(), &'static str> {
        let raw = root.as_os_str().as_encoded_bytes();
        if !root.is_absolute() || raw.len() > 4096 || raw.contains(&0) {
            return Err("Raíz de tamaños inválida");
        }
        let same = self.root == root;
        let mut added = 0;
        let mut count = 0;
        let mut bytes = if same {
            self.records
                .iter()
                .map(|r| r.name.capacity())
                .sum::<usize>()
        } else {
            0
        };
        let mut seen: [Option<&OsStr>; MAX_ROWS] = [None; MAX_ROWS];
        for name in names.clone() {
            let n = name.as_encoded_bytes();
            if n.is_empty()
                || n.contains(&0)
                || n.contains(&b'/')
                || name == "."
                || name == ".."
                || raw.len() + 1 + n.len() > 4096
            {
                return Err("Ruta de carpeta fuera del límite");
            }
            if count == MAX_ROWS {
                return Err("Tamaños admite 128 carpetas; reduce el filtro");
            }
            if seen[..count].contains(&Some(name)) {
                return Err("La cola contiene carpetas duplicadas");
            }
            seen[count] = Some(name);
            count += 1;
            if !same || !self.records.iter().any(|r| r.name == name) {
                added += 1;
                bytes = bytes.saturating_add(n.len());
            }
        }
        if count == 0 {
            return Err("No hay carpetas para calcular");
        }
        if added + if same { self.records.len() } else { 0 } > MAX_ROWS || bytes > MAX_NAMES {
            return Err("Presupuesto de tamaños agotado; actualiza o reduce el filtro");
        }
        if !same {
            self.invalidate();
            self.root = root.to_path_buf();
        }
        for name in names {
            let id = allocate(&mut self.next_id);
            if let Some(record) = self.records.iter_mut().find(|r| r.name == name) {
                if let Some(a) = &mut self.active
                    && a.id == record.id
                {
                    a.valid = false;
                }
                record.id = id;
                record.mode = mode;
                record.allocation = None;
                record.status = Status::Queued;
            } else {
                self.records.push(Record {
                    id,
                    name: name.to_os_string(),
                    status: Status::Queued,
                    mode,
                    allocation: None,
                });
            }
        }
        Ok(())
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    /// Pause admission only: the active helper keeps running to its normal ACK.
    pub fn pause(&mut self, paused: bool) {
        self.paused = paused && self.has_work();
    }
    /// Retry only explicit failures; measured and manually cancelled rows are untouched.
    pub fn retry_errors(&mut self) -> usize {
        let mut count = 0;
        for record in &mut self.records {
            if matches!(record.status, Status::Failed(_)) {
                record.id = allocate(&mut self.next_id);
                record.allocation = None;
                record.status = Status::Queued;
                count += 1;
            }
        }
        count
    }
    pub fn invalidate(&mut self) {
        self.paused = false;
        self.epoch = self.epoch.checked_add(1).expect("size epoch exhausted");
        self.records.clear();
        if let Some(a) = &mut self.active {
            a.valid = false;
        }
    }
    pub fn has_work(&self) -> bool {
        self.active.is_some() || self.pending()
    }
    pub fn pending(&self) -> bool {
        self.records
            .iter()
            .any(|r| matches!(r.status, Status::Queued))
    }
    pub fn pending_matches(&self, path: Option<&Path>) -> bool {
        match (
            self.records
                .iter()
                .find(|r| matches!(r.status, Status::Queued)),
            path,
        ) {
            (Some(record), Some(path)) => {
                path.parent() == Some(self.root.as_path())
                    && path.file_name() == Some(record.name.as_os_str())
            }
            (None, None) => true,
            _ => false,
        }
    }
    pub fn pending_path(&self) -> Option<PathBuf> {
        self.records
            .iter()
            .find(|r| matches!(r.status, Status::Queued))
            .map(|r| self.root.join(&r.name))
    }
    pub fn status(&self, root: &Path, name: &OsStr) -> Option<&Status> {
        (self.root == root)
            .then(|| {
                self.records
                    .iter()
                    .find(|r| r.name == name)
                    .map(|r| &r.status)
            })
            .flatten()
    }
    pub fn measured_map<'a>(&'a self, root: &Path) -> std::collections::HashMap<&'a OsStr, u64> {
        let mut result = std::collections::HashMap::new();
        if self.root == root {
            for r in &self.records {
                if let Status::Measured { bytes, .. } = r.status {
                    result.insert(r.name.as_os_str(), bytes);
                }
            }
        }
        result
    }
    #[cfg(test)]
    pub fn measured(&self, root: &Path, name: &OsStr) -> Option<u64> {
        match self.status(root, name) {
            Some(Status::Measured { bytes, .. }) => Some(*bytes),
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn dispatch(&mut self) -> Option<PathBuf> {
        self.dispatch_work().map(|work| work.path)
    }
    pub fn dispatch_work(&mut self) -> Option<Work> {
        if self.paused || self.active.is_some() {
            return None;
        }
        let record = self
            .records
            .iter_mut()
            .find(|r| matches!(r.status, Status::Queued))?;
        record.status = Status::Running {
            bytes: 0,
            entries: 0,
        };
        record.allocation = None;
        self.active = Some(Active {
            id: record.id,
            epoch: self.epoch,
            valid: true,
            mode: record.mode,
        });
        Some(Work {
            path: self.root.join(&record.name),
            mode: record.mode,
        })
    }
    pub fn progress(&mut self, value: (u64, u64)) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        if !active.valid || active.epoch != self.epoch {
            return false;
        }
        let Some(record) = self.records.iter_mut().find(|r| r.id == active.id) else {
            return false;
        };
        if let Status::Running { bytes, entries } = &mut record.status {
            if (*bytes, *entries) == value {
                return false;
            }
            *bytes = value.0;
            *entries = value.1;
            return true;
        }
        false
    }
    /// True only when a valid completed total was published (including zero).
    /// Reap still releases the active slot on stale/error results; errors have no size sort key.
    #[cfg(test)]
    pub fn finish(&mut self, result: Result<(u64, u64), String>) -> bool {
        self.finish_measurement(
            result.map(|(bytes, entries)| Measurement::apparent(bytes, entries)),
        )
    }
    pub fn finish_measurement(&mut self, result: Result<Measurement, String>) -> bool {
        let Some(active) = self.active.take() else {
            return false;
        };
        if !active.valid || active.epoch != self.epoch {
            return false;
        }
        let Some(record) = self.records.iter_mut().find(|r| r.id == active.id) else {
            return false;
        };
        if record.mode != active.mode {
            return false;
        }
        let result = result.and_then(|value| {
            value.validate(active.mode).map_err(str::to_owned)?;
            Ok(value)
        });
        let measured = result.is_ok();
        record.allocation = None;
        record.status = match result {
            Ok(value) => {
                record.allocation = value.allocation;
                Status::Measured {
                    bytes: value.bytes,
                    entries: value.entries,
                    at: SystemTime::now(),
                }
            }
            Err(error) => {
                let mut end = error.len().min(if error.len() > 512 { 509 } else { 512 });
                while !error.is_char_boundary(end) {
                    end -= 1;
                }
                let mut message = error[..end].to_string();
                if end < error.len() {
                    message.push('…');
                }
                Status::Failed(message)
            }
        };
        measured
    }
    pub fn cancel(&mut self, name: &OsStr) -> bool {
        let Some(id) = self.records.iter().find(|r| r.name == name).map(|r| r.id) else {
            return false;
        };
        self.cancel_id(self.epoch, id)
    }
    pub fn cancel_id(&mut self, epoch: u64, id: u64) -> bool {
        if epoch != self.epoch {
            return false;
        }
        let Some(record) = self
            .records
            .iter_mut()
            .find(|r| r.id == id && matches!(r.status, Status::Queued | Status::Running { .. }))
        else {
            return false;
        };
        let active = self.active.as_mut().filter(|a| a.id == id);
        let cancel = active.is_some();
        if let Some(a) = active {
            a.valid = false;
        }
        record.status = Status::Cancelled;
        record.allocation = None;
        cancel
    }
    pub fn cancel_all(&mut self) {
        self.paused = false;
        if let Some(a) = &mut self.active {
            a.valid = false;
        }
        for r in &mut self.records {
            if matches!(r.status, Status::Queued | Status::Running { .. }) {
                r.status = Status::Cancelled;
                r.allocation = None;
            }
        }
    }
    pub fn label(&self, root: &Path, name: &OsStr) -> Option<String> {
        let record = self.rows(root).iter().find(|r| r.name == name)?;
        let mut label = match &record.status {
            Status::Queued if self.paused => "En pausa".into(),
            Status::Queued => "En cola".into(),
            Status::Running { bytes, .. } => format!("≥{}", files_core::format_size(*bytes)),
            Status::Measured { bytes, .. } => format!("∑ {}", files_core::format_size(*bytes)),
            Status::Failed(_) => "Error".into(),
            Status::Cancelled => "Cancelado".into(),
        };
        if record.mode == Mode::Allocated {
            label.push_str(" · B");
        }
        Some(label)
    }
    pub fn detail(&self, root: &Path, name: &OsStr) -> Option<String> {
        let record = self.rows(root).iter().find(|r| r.name == name)?;
        let mut text = match &record.status {
            Status::Queued if self.paused => {
                "En pausa · reanuda desde Acciones → Reanudar tamaños".into()
            }
            Status::Queued => "En cola · esperando helper disponible".into(),
            Status::Running { bytes, entries } => format!(
                "Parcial: ≥{} · {entries} entradas recorridas",
                files_core::format_size(*bytes)
            ),
            Status::Measured { bytes, entries, at } => format!(
                "Total: {} · {entries} entradas · {}",
                files_core::format_size(*bytes),
                files_core::relative_date(*at, SystemTime::now())
            ),
            Status::Failed(e) => format!("Error: {e}"),
            Status::Cancelled => "Cancelado · sin total confirmado".into(),
        };
        if record.mode == Mode::Allocated {
            text.push_str("\nModo: aparente + bloques de archivos regulares");
            if matches!(record.status, Status::Measured { .. }) {
                if let Some(observation) = record.allocation {
                    text.push_str(&format!("\nBloques por nombre: {}\nPor identidad en esta carpeta: {}\nExcluye directorios/enlaces; no es espacio liberable ni unión entre carpetas", observation.by_name.label(), observation.by_identity.label()));
                } else {
                    text.push_str("\nBloques: SIN OBSERVACIÓN; no equivale a cero");
                }
            }
        }
        Some(text)
    }
    pub fn errors(&self) -> usize {
        self.records
            .iter()
            .filter(|r| matches!(r.status, Status::Failed(_)))
            .count()
    }
    pub fn compact_summary(&self) -> Option<String> {
        if !self.has_work() {
            return (self.errors() > 0).then(|| format!("Tamaños: {} errores", self.errors()));
        }
        if self.active.as_ref().is_some_and(|a| !a.valid) && !self.pending() {
            return Some("Tamaños: cerrando".into());
        }
        let queued = self
            .records
            .iter()
            .filter(|r| matches!(r.status, Status::Queued))
            .count();
        Some(format!(
            "∑ {} · cola {queued} · err {}",
            if self.paused { "pausa" } else { "activo" },
            self.errors()
        ))
    }
    pub fn summary(&self) -> Option<String> {
        if !self.has_work() {
            let errors = self.errors();
            return (errors > 0).then(|| format!("Tamaños: {errors} errores"));
        }
        if self.active.as_ref().is_some_and(|a| !a.valid) && !self.pending() {
            return Some("Tamaños: drenando cancelación".into());
        }
        if self.paused {
            let queued = self
                .records
                .iter()
                .filter(|r| matches!(r.status, Status::Queued))
                .count();
            let active = self.active.as_ref().is_some_and(|a| a.valid);
            return Some(format!(
                "Tamaños en pausa: {queued} pendientes{} · {} errores",
                if active { " · 1 activo" } else { "" },
                self.errors()
            ));
        }
        let done = self
            .records
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    Status::Measured { .. } | Status::Failed(_) | Status::Cancelled
                )
            })
            .count();
        Some(if self.records.is_empty() {
            "Tamaños: drenando cancelación".into()
        } else {
            format!(
                "Tamaños: {done}/{} carpetas · {} errores",
                self.records.len(),
                self.errors()
            )
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn allocated() -> Measurement {
        Measurement {
            bytes: 7,
            entries: 2,
            allocation: Some(Observation {
                by_name: files_core::folder_usage::Value::Bytes(8192),
                by_identity: files_core::folder_usage::Value::Bytes(4096),
            }),
        }
    }
    #[test]
    fn mixed_modes_preserve_allocation_through_finish_and_revoke_it_for_fresh_attempts() {
        let root = Path::new("/r");
        let mut s = State::default();
        s.enqueue_mode(root, [OsStr::new("a")].into_iter(), Mode::Allocated)
            .unwrap();
        s.enqueue(root, [OsStr::new("b")].into_iter()).unwrap();
        let work = s.dispatch_work().unwrap();
        assert_eq!((work.path, work.mode), (root.join("a"), Mode::Allocated));
        assert!(s.progress((3, 1)));
        assert_eq!(s.rows(root)[0].allocation, None);
        assert!(s.finish_measurement(Ok(allocated())));
        assert_eq!(s.measured(root, OsStr::new("a")), Some(7));
        assert_eq!(s.rows(root)[0].allocation, allocated().allocation);
        let epoch = s.epoch();
        let previous = s.rows(root)[0].id;
        let next = s.recalculate_id(epoch, previous).unwrap();
        assert_ne!(previous, next);
        assert_eq!(s.rows(root)[0].allocation, None);
        assert_eq!(s.dispatch_work().unwrap().mode, Mode::Allocated);
        assert!(s.cancel_id(epoch, next));
        let retry = s.retry_id(epoch, next).unwrap();
        assert_ne!(next, retry);
        assert!(s.dispatch_work().is_none());
        assert!(!s.finish_measurement(Ok(allocated())));
        assert_eq!(s.rows(root)[0].id, retry);
        assert_eq!(s.rows(root)[0].allocation, None);
        assert_eq!(s.dispatch_work().unwrap().mode, Mode::Allocated);
        let mut unavailable = allocated();
        unavailable.entries = files_core::folder_usage::MAX_IDENTITIES as u64 + 1;
        unavailable.allocation.as_mut().unwrap().by_identity =
            files_core::folder_usage::Value::Unavailable(
                files_core::folder_usage::Reason::IdentityLimit,
            );
        assert!(s.finish_measurement(Ok(unavailable)));
        assert_eq!(s.rows(root)[0].allocation, unavailable.allocation);
        assert_eq!(s.dispatch_work().unwrap().mode, Mode::Apparent);
        assert!(s.finish_measurement(Ok(Measurement::apparent(9, 1))));
        assert_eq!(s.rows(root)[1].allocation, None);
        assert!(s.forget_measured(epoch, &[previous]).is_err());
        assert_eq!(s.rows(root).len(), 2);
    }
    #[test]
    fn wrong_mode_and_invalid_allocation_fail_without_measured_sort_key_or_cached_allocation() {
        let root = Path::new("/r");
        for (mode, result) in [
            (Mode::Allocated, Measurement::apparent(7, 2)),
            (Mode::Apparent, allocated()),
        ] {
            let mut s = State::default();
            s.enqueue_mode(root, [OsStr::new("a")].into_iter(), mode)
                .unwrap();
            s.dispatch_work().unwrap();
            assert!(!s.finish_measurement(Ok(result)));
            assert!(matches!(s.rows(root)[0].status, Status::Failed(_)));
            assert_eq!(s.rows(root)[0].allocation, None);
            assert_eq!(s.measured(root, OsStr::new("a")), None);
            assert_eq!(s.retry_errors(), 1);
            assert_eq!(s.dispatch_work().unwrap().mode, mode);
            s.finish_measurement(Err("stop".into()));
        }
        let mut s = State::default();
        s.enqueue_mode(root, [OsStr::new("a")].into_iter(), Mode::Allocated)
            .unwrap();
        s.dispatch_work();
        let mut bad = allocated();
        bad.allocation.as_mut().unwrap().by_name = files_core::folder_usage::Value::Bytes(1);
        assert!(!s.finish_measurement(Ok(bad)));
        assert_eq!(s.rows(root)[0].allocation, None);
    }
    #[test]
    fn mode_replacement_waits_for_old_ack_and_rejected_batch_preserves_observations_and_pause() {
        let root = Path::new("/r");
        let mut s = State::default();
        s.enqueue_mode(root, [OsStr::new("a")].into_iter(), Mode::Allocated)
            .unwrap();
        s.dispatch_work();
        s.enqueue(root, [OsStr::new("a")].into_iter()).unwrap();
        assert!(s.dispatch_work().is_none());
        assert!(!s.finish_measurement(Ok(allocated())));
        assert_eq!(s.dispatch_work().unwrap().mode, Mode::Apparent);
        assert!(s.finish_measurement(Ok(Measurement::apparent(8, 1))));
        s.enqueue_mode(root, [OsStr::new("a")].into_iter(), Mode::Allocated)
            .unwrap();
        s.dispatch_work();
        assert!(s.finish_measurement(Ok(allocated())));
        s.enqueue(root, [OsStr::new("b")].into_iter()).unwrap();
        s.pause(true);
        let before: Vec<_> = s
            .rows(root)
            .iter()
            .map(|r| (r.id, r.mode, r.allocation))
            .collect();
        assert!(
            s.enqueue_mode(
                root,
                [OsStr::new("a"), OsStr::new("../bad")].into_iter(),
                Mode::Apparent
            )
            .is_err()
        );
        assert_eq!(
            before,
            s.rows(root)
                .iter()
                .map(|r| (r.id, r.mode, r.allocation))
                .collect::<Vec<_>>()
        );
        assert!(s.paused());
        assert!(s.dispatch_work().is_none());
    }
    #[test]
    fn oversized_error_keeps_utf8_budget_and_signals_omitted_suffix() {
        let mut s = State::default();
        let root = Path::new("/r");
        s.enqueue(root, [OsStr::new("bad")].into_iter()).unwrap();
        s.dispatch();
        s.finish(Err("é".repeat(600)));
        let Status::Failed(message) = &s.rows(root)[0].status else {
            panic!("expected failure");
        };
        assert!(message.len() <= 512 && message.ends_with('…'));
    }
    #[test]
    fn record_tickets_survive_cancel_retry_ack_without_reusing_attempt_identity() {
        let mut s = State::default();
        let root = Path::new("/r");
        s.enqueue(root, [OsStr::new("same")].into_iter()).unwrap();
        let epoch = s.epoch();
        let old = s.rows(root)[0].id;
        s.dispatch().unwrap();
        assert!(s.cancel_id(epoch, old));
        let new = s.retry_id(epoch, old).unwrap();
        assert_ne!(old, new);
        assert!(!s.cancel_id(epoch, old));
        assert!(s.dispatch().is_none());
        s.finish(Ok((99, 9)));
        assert!(matches!(s.rows(root)[0].status, Status::Queued));
        s.dispatch().unwrap();
        s.finish(Ok((3, 1)));
        assert_eq!(s.measured(root, OsStr::new("same")), Some(3));
        s.invalidate();
        s.enqueue(root, [OsStr::new("same")].into_iter()).unwrap();
        assert_ne!(new, s.rows(root)[0].id);
        assert!(!s.cancel_id(epoch, new));
        assert!(s.retry_id(epoch, new).is_none());
    }
    #[test]
    fn recalculate_one_observation_preserves_other_results_pause_and_active_ack() {
        let mut s = State::default();
        let root = Path::new("/r");
        s.enqueue(
            root,
            [OsStr::new("a"), OsStr::new("b"), OsStr::new("c")].into_iter(),
        )
        .unwrap();
        let epoch = s.epoch();
        s.dispatch().unwrap();
        s.finish(Ok((8, 2)));
        let old = s.rows(root)[0].id;
        let others: Vec<_> = s.rows(root)[1..].iter().map(|r| r.id).collect();
        assert_eq!(s.dispatch().unwrap(), root.join("b"));
        assert!(s.recalculate_id(epoch, others[0]).is_none()); // running is never replaced
        s.pause(true);
        let fresh = s.recalculate_id(epoch, old).unwrap();
        assert_ne!(fresh, old);
        assert!(s.paused());
        assert_eq!(s.measured(root, OsStr::new("a")), None);
        assert_eq!(
            s.rows(root)[1..].iter().map(|r| r.id).collect::<Vec<_>>(),
            others
        );
        assert!(s.recalculate_id(epoch, old).is_none());
        assert!(s.recalculate_id(epoch, fresh).is_none()); // double submission
        assert!(!s.cancel_id(epoch, old));
        assert!(s.dispatch().is_none());
        s.finish(Ok((9, 3))); // B's existing ACK still belongs to B
        assert_eq!(s.measured(root, OsStr::new("b")), Some(9));
        assert!(s.dispatch().is_none());
        s.pause(false);
        assert_eq!(s.dispatch().unwrap(), root.join("a"));
        s.finish(Ok((16, 4)));
        assert_eq!(s.measured(root, OsStr::new("a")), Some(16));
        assert_eq!(s.measured(root, OsStr::new("b")), Some(9));
        assert_eq!(s.dispatch().unwrap(), root.join("c"));
    }
    #[test]
    fn recalculate_rejects_old_context_and_non_measurements_without_mutation() {
        let mut s = State::default();
        let root = Path::new("/r");
        s.enqueue(root, [OsStr::new("a")].into_iter()).unwrap();
        let id = s.rows(root)[0].id;
        let epoch = s.epoch();
        assert!(s.recalculate_id(epoch, id).is_none());
        s.dispatch().unwrap();
        s.finish(Err("failure".into()));
        assert!(s.recalculate_id(epoch, id).is_none());
        let id = s.retry_id(epoch, id).unwrap();
        s.dispatch().unwrap();
        s.finish(Ok((0, 0)));
        assert!(s.recalculate_id(epoch + 1, id).is_none());
        assert_eq!(s.measured(root, OsStr::new("a")), Some(0));
        s.invalidate();
        assert!(s.recalculate_id(epoch, id).is_none());
    }
    #[test]
    fn pause_finishes_active_but_never_admits_next_until_resume() {
        let mut s = State::default();
        let root = Path::new("/r");
        s.enqueue(root, [OsStr::new("one"), OsStr::new("two")].into_iter())
            .unwrap();
        s.dispatch().unwrap();
        s.pause(true);
        assert!(s.progress((7, 2)));
        assert!(s.summary().unwrap().contains("1 activo"));
        s.finish(Ok((8, 3)));
        assert_eq!(s.measured(root, OsStr::new("one")), Some(8));
        assert!(s.has_work());
        assert!(s.dispatch().is_none());
        assert_eq!(
            s.label(root, OsStr::new("two")).as_deref(),
            Some("En pausa")
        );
        s.pause(false);
        assert_eq!(s.dispatch().unwrap(), root.join("two"));
    }
    #[test]
    fn retry_only_errors_preserves_pause_measured_cancelled_and_ack_barrier() {
        let mut s = State::default();
        let root = Path::new("/r");
        s.enqueue(
            root,
            [OsStr::new("good"), OsStr::new("bad"), OsStr::new("cancel")].into_iter(),
        )
        .unwrap();
        s.dispatch().unwrap();
        s.finish(Ok((3, 1)));
        s.dispatch().unwrap();
        s.finish(Err("denied".into()));
        s.dispatch().unwrap();
        s.pause(true);
        assert!(s.cancel(OsStr::new("cancel")));
        assert_eq!(s.retry_errors(), 1);
        assert_eq!(s.retry_errors(), 0);
        assert!(s.paused());
        s.pause(false);
        assert!(s.dispatch().is_none(), "cancelled child still owes ACK");
        s.finish(Ok((999, 999)));
        assert_eq!(s.measured(root, OsStr::new("good")), Some(3));
        assert!(matches!(
            s.status(root, OsStr::new("cancel")),
            Some(Status::Cancelled)
        ));
        assert_eq!(s.dispatch().unwrap(), root.join("bad"));
        s.finish(Ok((9, 2)));
        assert_eq!(s.measured(root, OsStr::new("bad")), Some(9));
        assert!(!s.has_work());
    }
    #[test]
    fn rejected_enqueue_preserves_pause_while_navigation_and_stop_clear_it() {
        let mut s = State::default();
        s.enqueue(Path::new("/r"), [OsStr::new("one")].into_iter())
            .unwrap();
        s.pause(true);
        assert!(
            s.enqueue(Path::new("/r"), [OsStr::new("../bad")].into_iter())
                .is_err()
        );
        assert!(s.paused());
        assert!(s.dispatch().is_none());
        s.cancel_all();
        assert!(!s.paused());
        assert!(!s.has_work());
        s.enqueue(Path::new("/r"), [OsStr::new("one")].into_iter())
            .unwrap();
        s.pause(true);
        s.enqueue(Path::new("/next"), [OsStr::new("one")].into_iter())
            .unwrap();
        assert!(!s.paused());
        assert_eq!(s.dispatch().unwrap(), Path::new("/next/one"));
    }
    #[test]
    fn navigation_and_same_name_do_not_publish_old_ack_or_start_before_it() {
        let mut s = State::default();
        s.enqueue(Path::new("/a"), [OsStr::new("same")].into_iter())
            .unwrap();
        assert_eq!(s.dispatch().unwrap(), Path::new("/a/same"));
        s.progress((3, 1));
        s.enqueue(Path::new("/b"), [OsStr::new("same")].into_iter())
            .unwrap();
        assert!(s.dispatch().is_none());
        s.finish(Ok((99, 9)));
        assert!(s.measured(Path::new("/b"), OsStr::new("same")).is_none());
        assert_eq!(s.dispatch().unwrap(), Path::new("/b/same"));
        s.finish(Ok((8, 5)));
        assert_eq!(s.measured(Path::new("/b"), OsStr::new("same")), Some(8));
    }
    #[test]
    fn over_budget_batch_rejects_atomically_and_cancelled_results_stay_unconfirmed() {
        let mut s = State::default();
        s.enqueue(Path::new("/root"), [OsStr::new("keep")].into_iter())
            .unwrap();
        let names = (0..129)
            .map(|i| OsString::from(format!("d{i}")))
            .collect::<Vec<_>>();
        assert!(
            s.enqueue(Path::new("/root"), names.iter().map(|n| n.as_os_str()))
                .is_err()
        );
        assert_eq!(s.dispatch().unwrap(), Path::new("/root/keep"));
        assert!(s.cancel(OsStr::new("keep")));
        s.finish(Ok((20, 3)));
        assert!(s.measured(Path::new("/root"), OsStr::new("keep")).is_none());
        assert!(!s.has_work());
    }
    #[test]
    fn progress_is_not_a_final_total_and_errors_do_not_become_zero() {
        let mut s = State::default();
        s.enqueue(Path::new("/r"), [OsStr::new("folder")].into_iter())
            .unwrap();
        s.dispatch();
        assert!(s.progress((3, 1)));
        assert!(s.measured(Path::new("/r"), OsStr::new("folder")).is_none());
        s.finish(Err("denied".into()));
        assert!(matches!(
            s.status(Path::new("/r"), OsStr::new("folder")),
            Some(Status::Failed(_))
        ));
    }
    #[test]
    fn forgetting_finished_row_releases_capacity_but_never_forgets_queued_or_active_ack() {
        let root = Path::new("/forget");
        let mut s = State::default();
        let names: Vec<std::ffi::OsString> = (0..128).map(|i| format!("n{i}").into()).collect();
        s.enqueue(root, names.iter().map(|n| n.as_os_str()))
            .unwrap();
        let epoch = s.epoch();
        let queued = s.rows(root)[1].id;
        assert!(!s.forget_id(epoch, queued));
        s.dispatch().unwrap();
        let active = s.rows(root)[0].id;
        s.cancel_id(epoch, active);
        assert!(!s.forget_id(epoch, active));
        s.finish(Ok((123, 1)));
        assert!(s.forget_id(epoch, active));
        assert_eq!(s.rows(root).len(), 127);
        s.enqueue(root, [OsStr::new("replacement")].into_iter())
            .unwrap();
        assert_eq!(s.rows(root).len(), 128);
        assert!(s.rows(root).last().unwrap().id > active);
        assert!(!s.forget_id(epoch, active));
    }
    #[test]
    fn snapshot_forget_preserves_new_measurements_errors_pause_and_zero_semantics() {
        let root = Path::new("/forget");
        let mut s = State::default();
        s.enqueue(
            root,
            [
                OsStr::new("old"),
                OsStr::new("new"),
                OsStr::new("bad"),
                OsStr::new("pending"),
            ]
            .into_iter(),
        )
        .unwrap();
        s.dispatch().unwrap();
        s.finish(Ok((0, 0)));
        let old = s.rows(root)[0].id;
        assert_eq!(s.measured(root, OsStr::new("old")), Some(0));
        s.dispatch().unwrap();
        s.finish(Ok((9, 2)));
        s.dispatch().unwrap();
        s.finish(Err("denied".into()));
        let bad = s.rows(root)[2].id;
        s.pause(true);
        assert!(s.forget_measured(s.epoch(), &[old, bad]).is_err());
        assert!(s.forget_measured(s.epoch(), &[old, old]).is_err());
        assert_eq!(s.rows(root).len(), 4);
        assert_eq!(s.forget_measured(s.epoch(), &[old]), Ok(1));
        assert_eq!(s.measured(root, OsStr::new("old")), None);
        assert_eq!(s.measured(root, OsStr::new("new")), Some(9));
        assert_eq!(s.errors(), 1);
        assert!(s.paused());
    }
    #[test]
    fn forgetting_other_observation_does_not_reset_active_epoch_or_progress() {
        let root = Path::new("/forget");
        let mut s = State::default();
        s.enqueue(root, [OsStr::new("done"), OsStr::new("active")].into_iter())
            .unwrap();
        s.dispatch().unwrap();
        s.finish(Ok((3, 1)));
        let id = s.rows(root)[0].id;
        s.dispatch().unwrap();
        s.progress((7, 2));
        let epoch = s.epoch();
        assert!(s.forget_id(epoch, id));
        assert_eq!(s.epoch(), epoch);
        assert!(s.dispatch().is_none());
        s.progress((9, 3));
        s.finish(Ok((11, 4)));
        assert_eq!(s.measured(root, OsStr::new("active")), Some(11));
    }
    #[test]
    fn stale_snapshot_after_recalculation_is_atomic_and_old_epoch_is_rejected() {
        let root = Path::new("/forget");
        let mut s = State::default();
        s.enqueue(root, [OsStr::new("a"), OsStr::new("b")].into_iter())
            .unwrap();
        s.dispatch().unwrap();
        s.finish(Ok((2, 1)));
        s.dispatch().unwrap();
        s.finish(Ok((3, 1)));
        let ids: Vec<_> = s.rows(root).iter().map(|r| r.id).collect();
        s.recalculate_id(s.epoch(), ids[1]).unwrap();
        assert!(s.forget_measured(s.epoch(), &ids).is_err());
        assert_eq!(s.measured(root, OsStr::new("a")), Some(2));
        assert!(!s.forget_id(s.epoch() + 1, ids[0]));
    }
    #[test]
    fn finish_signals_only_fresh_measured_values_not_errors_or_cancelled_old_acks() {
        let mut s = State::default();
        let root = Path::new("/signal");
        s.enqueue(
            root,
            [OsStr::new("zero"), OsStr::new("fail"), OsStr::new("old")].into_iter(),
        )
        .unwrap();
        s.dispatch();
        assert!(s.finish(Ok((0, 0))));
        s.dispatch();
        assert!(!s.finish(Err("denied".into())));
        s.dispatch();
        let id = s.rows(root)[2].id;
        s.cancel_id(s.epoch(), id);
        assert!(!s.finish(Ok((99, 2))));
        assert!(!s.finish(Ok((88, 1))));
        assert_eq!(s.measured(root, OsStr::new("zero")), Some(0));
        assert_eq!(s.measured(root, OsStr::new("old")), None);
    }
}
