//! Read-only ZIP catalog over the shared cancel/timeout/reap supervisor.
use crate::text_preview_job::TextPreview;
use files_core::protocol::{Request, Response};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
const MAX_WIRE: usize = 1024 * 1024;
const MAX_TEXT: usize = 128 * 1024;
pub struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<TextPreview, String>>,
}
impl Job {
    pub fn start(path: PathBuf) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(run(path, &flag).map_err(|e| e.to_string()));
        });
        Self { cancel, result }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&self) -> Option<Result<TextPreview, String>> {
        match self.result.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El listado ZIP terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn decode(wire: &[u8]) -> io::Result<TextPreview> {
    if wire.len() > MAX_WIRE {
        return Err(io::Error::other("Respuesta ZIP excesiva"));
    }
    match serde_json::from_slice::<Response>(wire)? {
        Response::Text {
            id: 1,
            text,
            encoding,
            truncated,
            ..
        } if text.len() <= MAX_TEXT
            && text.lines().count() <= 2000
            && encoding == "ZIP · directorio central" =>
        {
            Ok(TextPreview {
                text,
                encoding,
                truncated,
                relative_source: None,
            })
        }
        Response::Error { id: 1, message } if message.len() <= 4096 => {
            Err(io::Error::other(message))
        }
        _ => Err(io::Error::other("Respuesta ZIP fuera del contrato")),
    }
}
#[cfg(target_os = "linux")]
fn run(path: PathBuf, cancel: &AtomicBool) -> io::Result<TextPreview> {
    let wire = crate::text_preview_job::run_request_program(
        Request::PreviewArchive { id: 1, path },
        "preview_archive",
        cancel,
        std::env::current_exe()?.with_file_name("files-helper"),
        Duration::from_secs(5),
        MAX_WIRE,
        None,
    )?;
    decode(&wire)
}
#[cfg(not(target_os = "linux"))]
fn run(_: PathBuf, _: &AtomicBool) -> io::Result<TextPreview> {
    Err(io::Error::other(
        "Vista ZIP segura pendiente en esta plataforma",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn wire(id: u64, text: String, encoding: &str) -> Vec<u8> {
        serde_json::to_vec(&Response::Text {
            id,
            text,
            encoding: encoding.into(),
            truncated: true,
            relative_source: None,
        })
        .unwrap()
    }
    #[test]
    fn exact_catalog_contract_rejects_stale_identity_encoding_and_size() {
        let good = wire(1, "niño.zip\\xFF".into(), "ZIP · directorio central");
        let ready = decode(&good).unwrap();
        assert!(ready.truncated);
        assert_eq!(ready.text, "niño.zip\\xFF");
        assert!(decode(&wire(2, "x".into(), "ZIP · directorio central")).is_err());
        assert!(decode(&wire(1, "x".into(), "UTF-8")).is_err());
        assert!(
            decode(&wire(
                1,
                "x".repeat(MAX_TEXT + 1),
                "ZIP · directorio central"
            ))
            .is_err()
        );
        assert!(decode(&wire(1, "x\n".repeat(2001), "ZIP · directorio central")).is_err());
        assert!(decode(&vec![b' '; MAX_WIRE + 1]).is_err());
    }
}
