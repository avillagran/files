//! Read/reap through the common helper, then parse/prepare Markdown on that same
//! supervisor thread. No parser, disk access or resource fetch occurs in draw().
use crate::{markdown_document::Document, markdown_render::Rendered, text_preview_job};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};
#[derive(Clone)]
pub enum Source {
    Direct(PathBuf),
    Relative { root: PathBuf, relative: PathBuf },
}
pub struct Ready {
    pub rendered: Rendered,
    pub encoding: String,
    pub truncated: bool,
    pub notice: Option<String>,
    pub source: Option<files_core::relative_source::Proof>,
    /// At most eight normalized paths (<=4096B each) and effective descriptor
    /// proofs. No RGB clone; this manifest is separate from the RGB budget.
    pub resources: Arc<Vec<(PathBuf, files_core::relative_source::Proof)>>,
    pub plan: Option<Arc<crate::document_source::Plan>>,
    pub resource_intents: Arc<Vec<PathBuf>>,
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Ready, String>>,
}
impl Job {
    pub fn start(source: Source) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let context = match &source {
                Source::Direct(path) => path
                    .parent()
                    .zip(path.file_name())
                    .map(|(root, name)| (root.to_path_buf(), PathBuf::from(name), true)),
                Source::Relative { root, relative } => {
                    Some((root.clone(), relative.clone(), false))
                }
            };
            let value = read(source, &flag)
                .and_then(|text| prepare(text, &flag))
                .and_then(|mut ready| {
                    if let Some((root, current, direct)) = context {
                        load_images(&mut ready, &root, &current, &flag, started)?;
                        let proof = ready
                            .source
                            .ok_or_else(|| "Documento sin firma efectiva".to_owned())?;
                        ready.plan = Some(Arc::new(
                            crate::document_source::Plan::with_intents(
                                root,
                                current,
                                proof,
                                ready.resources.clone(),
                                ready.resource_intents.clone(),
                                direct,
                            )
                            .map_err(|e| e.to_string())?,
                        ));
                    }
                    Ok(ready)
                });
            let _ = tx.send(value);
        });
        Self { cancel, result }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&self) -> Option<Result<Ready, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El documento terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn prepare(text: text_preview_job::TextPreview, cancel: &AtomicBool) -> Result<Ready, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Documento cancelado".into());
    }
    let ir = Document::parse(&text.text)
        .map_err(|e| format!("Markdown supera un límite del documento: {e:?}"))?;
    // Source is no longer needed: the renderer copies visible content only.
    drop(text.text);
    if cancel.load(Ordering::Acquire) {
        return Err("Documento cancelado".into());
    }
    let rendered = Rendered::new(ir)?;
    if cancel.load(Ordering::Acquire) {
        return Err("Documento cancelado".into());
    }
    Ok(Ready {
        rendered,
        encoding: text.encoding,
        truncated: text.truncated,
        notice: None,
        source: text.relative_source,
        resources: Arc::new(Vec::new()),
        plan: None,
        resource_intents: Arc::new(Vec::new()),
    })
}
/// Local resources are decoded sequentially on this same worker, each helper
/// reaped before the next. The renderer charges actual retained Vec capacities.
fn load_images(
    ready: &mut Ready,
    root: &std::path::Path,
    current: &std::path::Path,
    cancel: &AtomicBool,
    started: std::time::Instant,
) -> Result<(), String> {
    let expected_root = ready
        .source
        .ok_or_else(|| "Documento sin firma de raíz efectiva".to_owned())?
        .root;
    const RETAINED: usize = 4 * 1024 * 1024;
    const SCRATCH: usize = 4 * 1024 * 1024;
    // Per-document dedupe only. No cross-document LRU without safe FS invalidation.
    let mut decoded =
        std::collections::HashMap::<PathBuf, Arc<crate::image_preview_job::Bitmap>>::new();
    for (index, target) in ready.rendered.image_requests() {
        if cancel.load(Ordering::Acquire) {
            return Err("Documento cancelado".into());
        }
        let relative = match target {
            crate::preview_link::Target::Relative { path, .. } => {
                crate::preview_link::resolve_relative(current, &path)
                    .map_err(|_| "Imagen bloqueada: fuera de la raíz o ruta no válida".to_owned())
            }
            _ => Err("Una imagen requiere un archivo local, no un fragmento".into()),
        };
        let remaining = std::time::Duration::from_secs(10).saturating_sub(started.elapsed());
        if let Ok(path) = &relative {
            let intents = Arc::get_mut(&mut ready.resource_intents).expect("unpublished intents");
            if !intents.contains(path) {
                intents.push(path.clone());
            }
        }
        let mut loaded_proof = None;
        let result = match &relative {
            Err(error) => Err(error.clone()),
            Ok(path) if decoded.contains_key(path) => Ok(Arc::clone(&decoded[path])),
            Ok(_) if remaining.is_zero() => {
                Err("Imagen omitida: límite10s de recursos del documento".into())
            }
            Ok(_)
                if !ready.rendered.can_load_image()
                    || ready
                        .rendered
                        .image_bytes
                        .checked_add(SCRATCH)
                        .is_none_or(|n| n > RETAINED + SCRATCH) =>
            {
                Err("Imagen omitida: presupuesto retenido4MiB + reserva transiente4MiB".into())
            }
            Ok(path) => crate::image_preview_job::run_relative(
                root.to_path_buf(),
                path.clone(),
                expected_root,
                cancel,
                remaining,
            )
            .map(|(bitmap, proof)| {
                loaded_proof = Some(proof);
                Arc::new(bitmap)
            })
            .map_err(|e| e.to_string()),
        };
        // Keep at most8 normalized paths and share each allocation by Arc. This
        // temporary map adds no RGB copies and is dropped before publishing Ready.
        let retained = result.as_ref().ok().map(Arc::clone);
        if let Err(error) = ready.rendered.set_image_shared(index, result) {
            ready.notice = Some(error);
        } else if let (Ok(path), Some(bitmap)) = (relative, retained) {
            if let Some(proof) = loaded_proof {
                Arc::get_mut(&mut ready.resources)
                    .expect("unpublished resource manifest")
                    .push((path.clone(), proof));
            }
            decoded.insert(path, bitmap);
        }
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Documento cancelado".into());
    }
    Ok(())
}
fn read(source: Source, cancel: &AtomicBool) -> Result<text_preview_job::TextPreview, String> {
    match source {
        Source::Direct(path) => text_preview_job::run(path, cancel).map_err(|e| e.to_string()),
        Source::Relative { root, relative } => {
            read_relative(root, relative, cancel).map_err(|e| e.to_string())
        }
    }
}
#[cfg(target_os = "linux")]
fn read_relative(
    root: PathBuf,
    relative: PathBuf,
    cancel: &AtomicBool,
) -> std::io::Result<text_preview_job::TextPreview> {
    let output = text_preview_job::run_request_program(
        files_core::protocol::Request::PreviewRelativeText {
            id: 1,
            root,
            relative,
        },
        "preview_relative_text",
        cancel,
        std::env::current_exe()?.with_file_name("files-helper"),
        std::time::Duration::from_secs(5),
        2 * 1024 * 1024,
        None,
    )?;
    text_preview_job::decode(&output)
}
#[cfg(not(target_os = "linux"))]
fn read_relative(
    _: PathBuf,
    _: PathBuf,
    _: &AtomicBool,
) -> std::io::Result<text_preview_job::TextPreview> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Recursos relativos pendientes en esta plataforma",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn text(value: &str) -> text_preview_job::TextPreview {
        text_preview_job::TextPreview {
            text: value.into(),
            encoding: "UTF-8".into(),
            truncated: false,
            relative_source: None,
        }
    }
    #[test]
    fn preparation_retains_visible_unicode_and_excludes_source_metadata() {
        let ready = prepare(
            text("# Hola\n\n**café** &amp; 漢字\n\n```rust\nlet a = 1;\n```"),
            &AtomicBool::new(false),
        )
        .unwrap();
        let visible = ready.rendered.document.text();
        assert!(visible.contains("café & 漢字"));
        assert!(!visible.contains("**"));
        assert!(!visible.contains("rust"));
        assert!(!visible.contains("&amp;"));
    }
    #[test]
    fn cancelled_and_oversized_source_return_no_document() {
        assert!(prepare(text("# Hola"), &AtomicBool::new(true)).is_err());
        assert!(prepare(text(&"a".repeat(256 * 1024 + 1)), &AtomicBool::new(false)).is_err());
    }
}
