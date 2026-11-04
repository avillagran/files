//! Bounded binary bitmap protocol over the common cancel/timeout/reap supervisor.
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
const MAX_RGB: usize = 1024 * 1024 * 3;
#[derive(Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub format: String,
    pub rgb: Vec<u8>,
    pub source: Option<files_core::media_source::Revision>,
}
type Metadata = (
    u32,
    u32,
    u32,
    u32,
    String,
    usize,
    Option<files_core::media_source::Revision>,
);
fn metadata(header: &[u8]) -> io::Result<Metadata> {
    match serde_json::from_slice::<Response>(header)? {
        Response::Image {
            id: 1,
            width,
            height,
            source_width,
            source_height,
            format,
            byte_len,
            source,
            ..
        } if width > 0
            && height > 0
            && width <= 1024
            && height <= 1024
            && source_width > 0
            && source_height > 0
            && source_width <= 8192
            && source_height <= 8192
            && u64::from(source_width) * u64::from(source_height)
                <= if matches!(format.as_str(), "jpeg" | "png") {
                    40 * 1024 * 1024
                } else {
                    4 * 1024 * 1024
                }
            && matches!(format.as_str(), "png" | "jpeg" | "webp") =>
        {
            let expected = width as usize * height as usize * 3;
            if byte_len != expected || expected > MAX_RGB {
                return Err(io::Error::other("Longitud RGB inválida"));
            }
            Ok((
                width,
                height,
                source_width,
                source_height,
                format,
                expected,
                source,
            ))
        }
        Response::Error { id: 1, message } => Err(io::Error::other(message)),
        _ => Err(io::Error::other("Cabecera de imagen inválida")),
    }
}
fn validate_header(header: &[u8]) -> io::Result<usize> {
    Ok(metadata(header)?.5)
}
fn decode(mut wire: Vec<u8>) -> io::Result<Bitmap> {
    let header = wire
        .iter()
        .position(|b| *b == b'\n')
        .filter(|n| *n < 4096)
        .ok_or_else(|| io::Error::other("Falta cabecera de imagen"))?;
    let (width, height, source_width, source_height, format, len, source) =
        metadata(&wire[..header])?;
    if wire.len() != header + 1 + len {
        return Err(io::Error::other("Imagen incompleta o bytes adicionales"));
    }
    // Shift in the SAME allocation, avoiding a second RGB Vec.
    wire.drain(..header + 1);
    Ok(Bitmap {
        width,
        height,
        source_width,
        source_height,
        format,
        rgb: wire,
        source,
    })
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Bitmap, String>>,
}
impl Job {
    pub fn start(path: PathBuf, expected: Option<files_core::media_source::Revision>) -> Self {
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
    pub fn poll(&self) -> Option<Result<Bitmap, String>> {
        match self.result.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El helper de imagen terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[cfg(target_os = "linux")]
fn run(
    path: PathBuf,
    expected: Option<files_core::media_source::Revision>,
    cancel: &AtomicBool,
) -> io::Result<Bitmap> {
    let output = crate::text_preview_job::run_request_program(
        match expected {
            Some(revision) => Request::PreviewImageChecked {
                id: 1,
                path,
                revision,
            },
            None => Request::PreviewImage { id: 1, path },
        },
        if expected.is_some() {
            "preview_image_checked"
        } else {
            "preview_image"
        },
        cancel,
        std::env::current_exe()?.with_file_name("files-helper"),
        Duration::from_secs(5),
        MAX_RGB + 4096,
        Some(validate_header),
    )?;
    let bitmap = decode(output)?;
    if expected.is_some() && bitmap.source != expected {
        return Err(io::Error::other(
            "Revisión de imagen no coincide con el origen autorizado",
        ));
    }
    Ok(bitmap)
}
pub const THUMBNAIL_DIMENSION: u32 = 160;
pub const THUMBNAIL_WIRE_BYTES: usize = 160 * 160 * 3 + 4096;
#[cfg(target_os = "linux")]
pub(crate) fn thumbnail(
    path: PathBuf,
    expected: files_core::media_source::Revision,
    cancel: &AtomicBool,
) -> io::Result<Bitmap> {
    let output = crate::text_preview_job::run_request_program(
        Request::PreviewThumbnail {
            id: 1,
            raw_path: path.as_os_str().as_encoded_bytes().to_vec(),
            revision: expected,
            max_dimension: THUMBNAIL_DIMENSION,
        },
        "preview_thumbnail",
        cancel,
        std::env::current_exe()?.with_file_name("files-helper"),
        Duration::from_secs(5),
        THUMBNAIL_WIRE_BYTES,
        Some(validate_header),
    )?;
    let bitmap = decode(output)?;
    if bitmap.width > THUMBNAIL_DIMENSION
        || bitmap.height > THUMBNAIL_DIMENSION
        || bitmap.source != Some(expected)
        || bitmap.rgb.capacity() > THUMBNAIL_WIRE_BYTES
    {
        return Err(io::Error::other("Miniatura/revisión fuera del presupuesto"));
    }
    Ok(bitmap)
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn thumbnail(
    _: PathBuf,
    _: files_core::media_source::Revision,
    _: &AtomicBool,
) -> io::Result<Bitmap> {
    Err(io::Error::other("Miniaturas pendientes en esta plataforma"))
}
#[cfg(target_os = "linux")]
pub(crate) fn run_relative(
    root: PathBuf,
    relative: PathBuf,
    expected_root: files_core::relative_source::DirectoryIdentity,
    cancel: &AtomicBool,
    timeout: Duration,
) -> io::Result<(Bitmap, files_core::relative_source::Proof)> {
    let started = std::time::Instant::now();
    let program = std::env::current_exe()?.with_file_name("files-helper");
    let proof_wire = crate::text_preview_job::run_request_program(
        Request::ProbeRelativeFile {
            id: 1,
            root: root.clone(),
            relative: relative.clone(),
            expected_root,
        },
        "probe_relative_file",
        cancel,
        program.clone(),
        timeout.min(Duration::from_secs(5)),
        4096,
        None,
    )?;
    let expected = match serde_json::from_slice::<Response>(&proof_wire)? {
        Response::RelativeSource { id: 1, proof } if proof.root == expected_root => proof,
        Response::Error { id: 1, message } => return Err(io::Error::other(message)),
        _ => return Err(io::Error::other("Firma de recurso relativa no válida")),
    };
    let remaining = timeout
        .saturating_sub(started.elapsed())
        .min(Duration::from_secs(5));
    if remaining.is_zero() {
        return Err(io::Error::other(
            "Tiempo de recurso agotado antes de decodificar",
        ));
    }
    let output = crate::text_preview_job::run_request_program(
        Request::PreviewRelativeImageChecked {
            id: 1,
            root,
            relative,
            expected,
        },
        "preview_relative_image_checked",
        cancel,
        program,
        remaining,
        MAX_RGB + 4096,
        Some(validate_header),
    )?;
    validate_relative_source(&output, expected)?;
    Ok((decode(output)?, expected))
}
fn validate_relative_source(
    output: &[u8],
    expected: files_core::relative_source::Proof,
) -> io::Result<()> {
    let end = output
        .iter()
        .position(|b| *b == b'\n')
        .filter(|n| *n < 4096)
        .ok_or_else(|| io::Error::other("Falta firma de recurso"))?;
    match serde_json::from_slice::<Response>(&output[..end])? {
        Response::Image {
            id: 1,
            source: Some(source),
            relative_source: Some(actual),
            ..
        } if actual == expected && source == expected.file => Ok(()),
        _ => Err(io::Error::other(
            "La firma efectiva del decoder no coincide con el recurso autorizado",
        )),
    }
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn run_relative(
    _: PathBuf,
    _: PathBuf,
    _: files_core::relative_source::DirectoryIdentity,
    _: &AtomicBool,
    _: Duration,
) -> io::Result<(Bitmap, files_core::relative_source::Proof)> {
    Err(io::Error::other(
        "Recursos de imagen relativos pendientes en esta plataforma",
    ))
}
#[cfg(not(target_os = "linux"))]
fn run(
    _: PathBuf,
    _: Option<files_core::media_source::Revision>,
    _: &AtomicBool,
) -> io::Result<Bitmap> {
    Err(io::Error::other(
        "Preview de imagen seguro pendiente en esta plataforma",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relative_publication_requires_both_root_and_effective_file_revision() {
        use files_core::{
            media_source::Revision,
            relative_source::{DirectoryIdentity, Proof},
        };
        let proof = Proof {
            root: DirectoryIdentity {
                device: 1,
                inode: 2,
            },
            file: Revision {
                device: 1,
                inode: 3,
                bytes: 12,
                modified_seconds: 1,
                modified_nanoseconds: 2,
                changed_seconds: 3,
                changed_nanoseconds: 4,
            },
        };
        let make = |actual: Option<Proof>, source| {
            let mut data = serde_json::to_vec(&Response::Image {
                id: 1,
                width: 1,
                height: 1,
                byte_len: 3,
                source_width: 1,
                source_height: 1,
                format: "png".into(),
                source,
                relative_source: actual,
            })
            .unwrap();
            data.extend_from_slice(b"\nRGB");
            data
        };
        assert!(validate_relative_source(&make(Some(proof), Some(proof.file)), proof).is_ok());
        assert!(validate_relative_source(&make(None, Some(proof.file)), proof).is_err());
        let mut changed = proof;
        changed.root.inode += 1;
        assert!(validate_relative_source(&make(Some(changed), Some(proof.file)), proof).is_err());
        changed = proof;
        changed.file.inode += 1;
        assert!(validate_relative_source(&make(Some(changed), Some(changed.file)), proof).is_err());
        assert!(validate_relative_source(&make(Some(proof), Some(changed.file)), proof).is_err());
    }
    fn wire() -> Vec<u8> {
        let mut b = serde_json::to_vec(&Response::Image {
            id: 1,
            width: 1,
            height: 1,
            source_width: 1,
            source_height: 1,
            format: "png".into(),
            byte_len: 3,
            source: None,
            relative_source: None,
        })
        .unwrap();
        b.extend_from_slice(b"\n\x01\x02\x03");
        b
    }
    #[test]
    fn exact_binary_without_second_rgb_allocation() {
        let bytes = wire();
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();
        let b = decode(bytes).unwrap();
        assert_eq!(b.rgb.as_ptr(), pointer);
        assert_eq!(b.rgb.capacity(), capacity);
        assert_eq!(b.rgb, [1, 2, 3]);
    }
    #[test]
    fn large_source_is_accepted_for_bounded_jpeg_or_streamed_png_payload() {
        for (format, valid) in [("jpeg", true), ("png", true), ("webp", false)] {
            let header = serde_json::to_vec(&Response::Image {
                id: 1,
                width: 750,
                height: 500,
                source_width: 6000,
                source_height: 4000,
                format: format.into(),
                byte_len: 750 * 500 * 3,
                source: None,
                relative_source: None,
            })
            .unwrap();
            assert_eq!(validate_header(&header).is_ok(), valid);
        }
    }
    #[test]
    fn rejects_wrong_id_dimensions_format_length_and_trailing_data() {
        for (id, w, h, f, len) in [
            (2, 1, 1, "png", 3),
            (1, 1025, 1, "png", 3075),
            (1, 1, 1, "raw", 3),
            (1, 1, 1, "png", usize::MAX),
            (1, 0, 1, "png", 0),
        ] {
            let h = serde_json::to_vec(&Response::Image {
                id,
                width: w,
                height: h,
                source_width: 1,
                source_height: 1,
                format: f.into(),
                byte_len: len,
                source: None,
                relative_source: None,
            })
            .unwrap();
            assert!(validate_header(&h).is_err());
        }
        let mut b = wire();
        b.push(0);
        assert!(decode(b).is_err());
        let mut b = wire();
        b.pop();
        assert!(decode(b).is_err());
    }
}
