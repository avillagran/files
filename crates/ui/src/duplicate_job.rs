//! Resultado final de duplicados sólo después de validar protocolo y recolectar el proceso.
use files_core::duplicates::{self, Event, Frame, Progress, Report};
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
pub struct Outcome {
    pub report: Option<Report>,
    pub error: Option<String>,
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    result: Receiver<Outcome>,
}
struct Decoder {
    id: u64,
    line: Vec<u8>,
    updates: usize,
    rows: Vec<duplicates::Row>,
    path_bytes: usize,
    previous: Progress,
    report: Option<Report>,
    error: Option<String>,
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Protocolo de duplicados inválido o incompleto",
    )
}
fn advances(a: Progress, b: Progress) -> bool {
    b.phase >= a.phase
        && b.entries >= a.entries
        && b.files >= a.files
        && b.aliases >= a.aliases
        && b.skipped >= a.skipped
        && b.errors >= a.errors
        && b.candidates >= a.candidates
        && b.hashed >= a.hashed
        && b.bytes_read >= a.bytes_read
        && b.bytes_total >= a.bytes_total
        && b.reasons & a.reasons == a.reasons
}
impl Decoder {
    fn feed(&mut self, bytes: &[u8], published: &Mutex<Progress>) -> io::Result<()> {
        for &byte in bytes {
            if self.report.is_some() || self.error.is_some() {
                return Err(bad());
            }
            if byte != b'\n' {
                if self.line.len() >= 256 * 1024 {
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
                Event::Error { message } => {
                    if message.is_empty() || message.len() > 256 {
                        return Err(bad());
                    }
                    self.error = Some(message);
                }
                Event::Progress { progress } => {
                    duplicates::validate_progress(progress)?;
                    if !self.rows.is_empty()
                        || progress.phase >= 2
                        || self.updates >= 512
                        || !advances(self.previous, progress)
                    {
                        return Err(bad());
                    }
                    self.updates += 1;
                    self.previous = progress;
                    *published.lock().unwrap_or_else(|e| e.into_inner()) = progress;
                }
                Event::Page { offset, rows } => {
                    let bytes: usize = rows.iter().map(|r| r.path.as_os_str().len()).sum();
                    if offset != self.rows.len()
                        || rows.is_empty()
                        || rows.len() > duplicates::MAX_ROWS
                        || bytes > duplicates::MAX_RESULT_BYTES
                        || self.rows.len() + rows.len() > duplicates::MAX_FILES
                        || self.path_bytes + bytes > duplicates::MAX_PATH_BYTES
                    {
                        return Err(bad());
                    }
                    self.path_bytes += bytes;
                    self.rows.extend(rows);
                }
                Event::Done { mut report } => {
                    if !report.rows.is_empty() {
                        return Err(bad());
                    }
                    report.rows = std::mem::take(&mut self.rows);
                    duplicates::validate_all(&report)?;
                    if !advances(self.previous, report.progress) {
                        return Err(bad());
                    }
                    self.report = Some(report);
                }
            }
        }
        Ok(())
    }
}
impl Job {
    pub fn start(id: u64, root: PathBuf) -> Result<Self, String> {
        duplicates::validate_root(&root).map_err(|e| e.to_string())?;
        let mut request =
            serde_json::to_vec(&files_core::protocol::Request::FindDuplicates { id, root })
                .map_err(|e| e.to_string())?;
        request.push(b'\n');
        if request.len() > files_core::protocol::MAX_LINE_BYTES {
            return Err("Carpeta fuera del presupuesto de petición".into());
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
            .name("files-duplicates".into())
            .spawn(move || {
                let mut decoder = Decoder {
                    id,
                    line: Vec::with_capacity(4096),
                    updates: 0,
                    rows: Vec::new(),
                    path_bytes: 0,
                    previous: Progress::default(),
                    report: None,
                    error: None,
                };
                let result = crate::space_map_job::run_helper_stream(
                    &request,
                    &flag,
                    executable,
                    Duration::from_secs(120),
                    "find_duplicates",
                    |chunk| decoder.feed(chunk, &published),
                );
                let error = if flag.load(Ordering::Relaxed) {
                    Some("Búsqueda cancelada; sin informe final".into())
                } else {
                    result
                        .and_then(|_| {
                            if decoder.line.is_empty()
                                && (decoder.report.is_some() || decoder.error.is_some())
                            {
                                Ok(())
                            } else {
                                Err(bad())
                            }
                        })
                        .err()
                        .map(|e| e.to_string())
                };
                let error = error.or(decoder.error);
                let report = if error.is_none() {
                    decoder.report
                } else {
                    None
                };
                let _ = tx.send(Outcome { report, error });
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
                report: None,
                error: Some("El lector de duplicados terminó sin ACK".into()),
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
    #[test]
    fn progress_cannot_reverse_or_clear_partial_reasons() {
        let a = Progress {
            entries: 8,
            reasons: 1,
            ..Progress::default()
        };
        assert!(!advances(
            a,
            Progress {
                entries: 7,
                reasons: 1,
                ..Progress::default()
            }
        ));
        assert!(!advances(
            a,
            Progress {
                entries: 9,
                ..Progress::default()
            }
        ));
    }
    #[test]
    fn oversized_or_stale_frame_is_rejected() {
        let mut d = Decoder {
            id: 1,
            line: Vec::new(),
            updates: 0,
            rows: Vec::new(),
            path_bytes: 0,
            previous: Progress::default(),
            report: None,
            error: None,
        };
        let mut wire = serde_json::to_vec(&Frame {
            id: 2,
            event: Event::Progress {
                progress: Progress::default(),
            },
        })
        .unwrap();
        wire.push(b'\n');
        assert!(d.feed(&wire, &Mutex::new(Progress::default())).is_err());
        d.line.clear();
        assert!(
            d.feed(
                &vec![b'x'; 256 * 1024 + 1],
                &Mutex::new(Progress::default())
            )
            .is_err()
        );
    }
}
