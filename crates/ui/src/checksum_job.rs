//! Supervisor único de un lote SHA-256; ningún resultado final libera antes del ACK/reap.
use files_core::checksum::{Event, Frame, MAX_PROGRESS, Observation};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Progress {
    pub completed: usize,
    pub bytes: u64,
    pub total: u64,
}
pub struct Outcome {
    pub rows: Vec<Result<Observation, String>>,
    pub error: Option<String>,
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    result: Receiver<Outcome>,
}
struct Decoder {
    id: u64,
    count: usize,
    line: Vec<u8>,
    rows: Vec<Result<Observation, String>>,
    done: bool,
    updates: usize,
    last: Option<(u64, u64)>,
    root: Option<files_core::relative_source::DirectoryIdentity>,
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Respuesta SHA-256 inválida, incompleta o fuera de presupuesto",
    )
}
impl Decoder {
    fn new(id: u64, count: usize) -> Self {
        Self {
            id,
            count,
            line: Vec::with_capacity(1024),
            rows: Vec::new(),
            done: false,
            updates: 0,
            last: None,
            root: None,
        }
    }
    fn feed(&mut self, bytes: &[u8], published: &Mutex<Progress>) -> io::Result<()> {
        for &byte in bytes {
            if self.done {
                return Err(bad());
            }
            if byte != b'\n' {
                if self.line.len() >= 2048 {
                    return Err(bad());
                }
                self.line.push(byte);
                continue;
            }
            let frame: Frame = serde_json::from_slice(&self.line).map_err(|_| bad())?;
            self.line.clear();
            if frame.id != self.id {
                return Err(bad());
            }
            match frame.event {
                Event::Progress {
                    index,
                    bytes,
                    total,
                } => {
                    if index != self.rows.len()
                        || index >= self.count
                        || bytes > total
                        || self.updates >= MAX_PROGRESS
                        || self
                            .last
                            .is_some_and(|(before, t)| t != total || bytes < before)
                    {
                        return Err(bad());
                    }
                    self.updates += 1;
                    self.last = Some((bytes, total));
                    *published.lock().unwrap_or_else(|e| e.into_inner()) = Progress {
                        completed: index,
                        bytes,
                        total,
                    };
                }
                Event::Item { index, result } => {
                    if index != self.rows.len() || index >= self.count {
                        return Err(bad());
                    }
                    match &result {
                        Ok(row) => {
                            if self.root.is_some_and(|root| root != row.proof.root)
                                || self
                                    .last
                                    .is_none_or(|(_, total)| total != row.proof.file.bytes)
                            {
                                return Err(bad());
                            }
                            self.root = Some(row.proof.root);
                        }
                        Err(e) => {
                            if e.len() > 256 {
                                return Err(bad());
                            }
                        }
                    }
                    self.rows.push(result);
                    self.last = None;
                    *published.lock().unwrap_or_else(|e| e.into_inner()) = Progress {
                        completed: self.rows.len(),
                        bytes: 0,
                        total: 0,
                    };
                }
                Event::Done { count } => {
                    if count != self.count || count != self.rows.len() || self.last.is_some() {
                        return Err(bad());
                    }
                    self.done = true;
                }
            }
        }
        Ok(())
    }
    fn finish(&self) -> io::Result<()> {
        if self.done && self.line.is_empty() && self.rows.len() == self.count {
            Ok(())
        } else {
            Err(bad())
        }
    }
}
impl Job {
    pub fn start(
        id: u64,
        root: PathBuf,
        names: Vec<files_core::checksum::Name>,
    ) -> Result<Self, String> {
        files_core::checksum::validate(&root, &names).map_err(|e| e.to_string())?;
        let count = names.len();
        let mut request =
            serde_json::to_vec(&files_core::protocol::Request::HashFiles { id, root, names })
                .map_err(|e| e.to_string())?;
        request.push(b'\n');
        if request.len() > files_core::protocol::MAX_LINE_BYTES {
            return Err("La selección SHA-256 supera64KiB de petición".into());
        }
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("files-helper");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let progress = Arc::new(Mutex::new(Progress::default()));
        let published = progress.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-checksum".into())
            .spawn(move || {
                let mut decoder = Decoder::new(id, count);
                let result = crate::space_map_job::run_helper_stream(
                    &request,
                    &flag,
                    executable,
                    Duration::from_secs(120),
                    "hash_files",
                    |chunk| decoder.feed(chunk, &published),
                )
                .and_then(|_| decoder.finish());
                let retain = result.is_ok()
                    || result
                        .as_ref()
                        .err()
                        .is_some_and(|e| e.kind() == io::ErrorKind::Interrupted);
                let error = if flag.load(Ordering::Relaxed) {
                    Some("Cálculo cancelado; se conservan sólo observaciones completas".into())
                } else {
                    result.err().map(|e| e.to_string())
                };
                // Tras un protocolo inválido no se publican observaciones del proceso.
                let rows = if !retain { Vec::new() } else { decoder.rows };
                let _ = tx.send(Outcome { rows, error });
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cancel,
            progress,
            result,
        })
    }
    #[cfg(test)]
    pub fn pending() -> (Self, std::sync::mpsc::SyncSender<Outcome>) {
        let (tx, result) = mpsc::sync_channel(1);
        (
            Self {
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(Mutex::new(Progress::default())),
                result,
            },
            tx,
        )
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn progress(&self) -> Progress {
        *self.progress.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn poll(&self) -> Option<Outcome> {
        match self.result.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Outcome {
                rows: Vec::new(),
                error: Some("El trabajador SHA-256 terminó sin respuesta".into()),
            }),
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn send(d: &mut Decoder, event: Event) -> io::Result<()> {
        let mut line = serde_json::to_vec(&Frame { id: 7, event }).unwrap();
        line.push(b'\n');
        d.feed(&line, &Mutex::new(Progress::default()))
    }
    #[test]
    fn duplicate_out_of_order_and_missing_ack_are_rejected() {
        let mut d = Decoder::new(7, 1);
        assert!(d.finish().is_err());
        assert!(
            send(
                &mut d,
                Event::Item {
                    index: 1,
                    result: Err("x".into())
                }
            )
            .is_err()
        );
        send(
            &mut d,
            Event::Item {
                index: 0,
                result: Err("x".into()),
            },
        )
        .unwrap();
        assert!(d.finish().is_err());
        send(&mut d, Event::Done { count: 1 }).unwrap();
        d.finish().unwrap();
        assert!(send(&mut d, Event::Done { count: 1 }).is_err());
    }
    #[test]
    fn progress_reversal_changed_total_and_overflow_are_rejected() {
        for (bytes, total) in [(2, 10), (4, 11), (12, 10)] {
            let mut d = Decoder::new(7, 1);
            send(
                &mut d,
                Event::Progress {
                    index: 0,
                    bytes: 3,
                    total: 10,
                },
            )
            .unwrap();
            assert!(
                send(
                    &mut d,
                    Event::Progress {
                        index: 0,
                        bytes,
                        total
                    }
                )
                .is_err()
            );
        }
        let mut d = Decoder::new(7, 1);
        assert!(
            d.feed(&vec![b'x'; 2049], &Mutex::new(Progress::default()))
                .is_err()
        );
    }
}
