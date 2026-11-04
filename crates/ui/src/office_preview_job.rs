//! Checked DOCX text over the existing single-flight cancel/timeout/reap path.
use crate::text_preview_job::TextPreview;
use files_core::{
    media_source::Revision,
    protocol::{Request, Response},
};
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
const MAX_WIRE: usize = 2 * 1024 * 1024;
const MAX_TEXT: usize = 256 * 1024;
pub struct Ready {
    pub preview: TextPreview,
    pub source: Revision,
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Ready, String>>,
}
impl Job {
    pub fn start(path: PathBuf, expected: Revision) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(run(path, expected, &flag).map_err(|e| e.to_string()));
        });
        Self { cancel, result }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&self) -> Option<Result<Ready, String>> {
        match self.result.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El lector DOCX terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn decode(wire: &[u8], expected: Revision) -> io::Result<Ready> {
    if wire.len() > MAX_WIRE {
        return Err(io::Error::other("Respuesta DOCX excesiva"));
    }
    match serde_json::from_slice::<Response>(wire)? {
        Response::OfficeText {
            id: 1,
            text,
            truncated,
            source,
        } if source == expected
            && text.len() <= MAX_TEXT
            && text.bytes().filter(|b| *b == b'\n').count() <= 2000
            && !text.contains('\0') =>
        {
            Ok(Ready {
                source,
                preview: TextPreview {
                    text,
                    encoding: "DOCX · texto extraído".into(),
                    truncated,
                    relative_source: None,
                },
            })
        }
        Response::Error { id: 1, message } if message.len() <= 4096 => {
            Err(io::Error::other(message))
        }
        _ => Err(io::Error::other(
            "Respuesta DOCX fuera del contrato/origen esperado",
        )),
    }
}
#[cfg(target_os = "linux")]
fn run(path: PathBuf, expected: Revision, cancel: &AtomicBool) -> io::Result<Ready> {
    let wire = crate::text_preview_job::run_request_program(
        Request::PreviewOffice {
            id: 1,
            path,
            revision: expected,
        },
        "preview_office",
        cancel,
        std::env::current_exe()?.with_file_name("files-helper"),
        Duration::from_secs(5),
        MAX_WIRE,
        None,
    )?;
    decode(&wire, expected)
}
#[cfg(not(target_os = "linux"))]
fn run(_: PathBuf, _: Revision, _: &AtomicBool) -> io::Result<Ready> {
    Err(io::Error::other(
        "Texto DOCX supervisado pendiente en esta plataforma",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(n: u64) -> Revision {
        Revision {
            device: 1,
            inode: n,
            bytes: 50,
            modified_seconds: 0,
            modified_nanoseconds: 0,
            changed_seconds: 0,
            changed_nanoseconds: 0,
        }
    }
    fn wire(id: u64, text: &str, s: Revision) -> Vec<u8> {
        serde_json::to_vec(&Response::OfficeText {
            id,
            text: text.into(),
            truncated: true,
            source: s,
        })
        .unwrap()
    }
    #[test]
    fn checked_revision_id_bounds_and_unicode_are_required() {
        let a = source(1);
        let r = decode(&wire(1, "niño\t文档\r\n", a), a).unwrap();
        assert_eq!(r.preview.text, "niño\t文档\r\n");
        assert!(r.preview.truncated);
        assert_eq!(r.source, a);
        assert!(decode(&wire(2, "x", a), a).is_err());
        assert!(decode(&wire(1, "old", a), source(2)).is_err());
        assert!(decode(&wire(1, &"x".repeat(MAX_TEXT + 1), a), a).is_err());
        assert!(decode(&wire(1, &"\n".repeat(2001), a), a).is_err());
        assert!(decode(&wire(1, "\0", a), a).is_err());
        assert!(decode(&vec![b' '; MAX_WIRE + 1], a).is_err());
    }
}
