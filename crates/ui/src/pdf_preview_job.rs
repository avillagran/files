//! Optional PDF raster worker: two direct, sequential Poppler tools, one pinned
//! source FD, bounded stdout and a bitmap moved out of that same allocation.
//!
//! Multipage UI is shipped with scoped functional evidence; resource gate pending.
//! pdfinfo/pdftoppm are discovered through PATH at runtime; they are not linked or
//! distributed by this crate. Password-protected/corrupt/complex PDFs can fail.
//! No JavaScript interpreter or external URI action is invoked. The
//! host does not request external resources. This is NOT a filesystem/network
//! sandbox or a guarantee about the PDF parser. RLIMIT_AS is containment, never a
//! PSS metric; a parser vulnerability or external-file reference needs separate
//! containment before treating arbitrary hostile documents as safely isolated.
//!
//! The common supervisor's optional header callback expects one newline-delimited
//! JSON line, incompatible with multi-line PPM. Consequently the *bounded* PPM
//! wire (at most 960²*3 + 4096 B) is collected before its dimensions are checked.
//! No allocation based on unvalidated dimensions occurs; exposing RGB requires
//! an exact P6/header/dimension/maxval/payload/EOF check. A future incremental PPM
//! validator in the common supervisor can reject bad headers before collection.
use crate::image_preview_job::Bitmap;
use crate::readonly_gate::{Gate, Phase, admit_helper};
use files_core::media_source::Revision;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
const MAX_SOURCE: u64 = 256 * 1024 * 1024;
const MAX_SIDE: usize = 960;
const MAX_HEADER: usize = 4096;
const MAX_RGB: usize = MAX_SIDE * MAX_SIDE * 3;
const MAX_PAGES: u32 = 100_000;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct Ready {
    pub bitmap: Bitmap,
    pub page: u32,
    pub pages: u32,
}
pub struct TextReady {
    pub text: String,
    pub source: Revision,
    pub page: u32,
    pub pages: u32,
}
pub enum Output {
    Raster(Ready),
    Text(TextReady),
}
const MAX_TEXT: usize = 256 * 1024;
pub struct Job {
    gate: Arc<Mutex<Gate>>,
    source_invalid: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Output, String>>,
}
impl Job {
    pub fn start(path: PathBuf, page: u32, expected: Revision, text: bool) -> Self {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let state = gate.clone();
        let source_invalid = Arc::new(AtomicBool::new(false));
        let source_flag = source_invalid.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = run(path, page, expected, text, &flag, &state, &source_flag)
                .map_err(|e| e.to_string());
            if let Ok(mut gate) = state.lock() {
                gate.phase = Phase::Stopped;
            }
            let _ = tx.send(result);
        });
        Self {
            gate,
            source_invalid,
            cancel,
            result,
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut gate) = self.gate.lock() {
            gate.closing = true;
        }
    }
    pub fn source_invalid(&self) -> bool {
        self.source_invalid.load(Ordering::Acquire)
    }
    pub fn exit_allowed(&self) -> bool {
        self.gate
            .lock()
            .is_ok_and(|g| g.closing && g.phase != Phase::Helper)
    }
    pub fn readonly_pending(&self) -> bool {
        self.gate.lock().is_ok_and(|g| g.phase == Phase::ReadOnly)
    }
    pub fn poll(&self) -> Option<Result<Output, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("La vista PDF terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn source_path(path: &Path, page: u32) -> io::Result<()> {
    if !path.is_absolute()
        || path.as_os_str().as_encoded_bytes().len() > 4096
        || path.as_os_str().as_encoded_bytes().contains(&0)
        || !(1..=MAX_PAGES).contains(&page)
    {
        return Err(io::Error::other("Ruta o página PDF no válida"));
    }
    Ok(())
}
fn parse_pages(bytes: &[u8]) -> io::Result<u32> {
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::other("Metadatos PDF demasiado grandes"));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| io::Error::other("Metadatos PDF no válidos"))?;
    let mut found = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Pages:") {
            let value = value.trim();
            if found.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(io::Error::other("Número de páginas PDF no válido"));
            }
            let count: u32 = value
                .parse()
                .map_err(|_| io::Error::other("Número de páginas PDF no válido"))?;
            if !(1..=MAX_PAGES).contains(&count) {
                return Err(io::Error::other("PDF fuera del límite de páginas"));
            }
            found = Some(count);
        }
    }
    found.ok_or_else(|| io::Error::other("No se pudo obtener el número de páginas PDF"))
}
#[derive(Debug, PartialEq, Eq)]
struct Header {
    width: usize,
    height: usize,
    payload: usize,
    length: usize,
}
/// P6 whitespace/comment tokenizer, restricted to a bounded ASCII header. The
/// one separator after maxval is consumed exactly; binary RGB whitespace/# is
/// never skipped. CRLF is accepted as a single separator, not two RGB bytes.
fn ppm_header(bytes: &[u8]) -> io::Result<Header> {
    let mut at = 0;
    let mut token = || -> io::Result<&[u8]> {
        loop {
            while at < bytes.len() && at < MAX_HEADER && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            if at >= bytes.len() || at >= MAX_HEADER {
                return Err(io::Error::other("Cabecera PPM incompleta o excesiva"));
            }
            if bytes[at] != b'#' {
                break;
            }
            while at < bytes.len() && at < MAX_HEADER && bytes[at] != b'\n' {
                at += 1;
            }
        }
        let start = at;
        while at < bytes.len()
            && at < MAX_HEADER
            && !bytes[at].is_ascii_whitespace()
            && bytes[at] != b'#'
        {
            at += 1;
        }
        if at == start || at >= bytes.len() || at >= MAX_HEADER {
            return Err(io::Error::other("Cabecera PPM incompleta o excesiva"));
        }
        Ok(&bytes[start..at])
    };
    if token()? != b"P6" {
        return Err(io::Error::other("El render PDF no produjo PPM RGB"));
    }
    let decimal = |value: &[u8]| -> io::Result<usize> {
        if value.is_empty() || value.len() > 6 || !value.iter().all(u8::is_ascii_digit) {
            return Err(io::Error::other("Dimensión PPM no válida"));
        }
        value.iter().try_fold(0usize, |n, b| {
            n.checked_mul(10)
                .and_then(|n| n.checked_add((b - b'0') as usize))
                .ok_or_else(|| io::Error::other("Dimensión PPM excesiva"))
        })
    };
    let width = decimal(token()?)?;
    let height = decimal(token()?)?;
    let maxval = decimal(token()?)?;
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) || maxval != 255 {
        return Err(io::Error::other("PPM fuera del límite RGB de 960×960"));
    }
    // End the closure's mutable borrow before reading the exact separator.
    let payload = if bytes.get(at) == Some(&b'\r') && bytes.get(at + 1) == Some(&b'\n') {
        at + 2
    } else if bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at + 1
    } else {
        return Err(io::Error::other("Falta separador de píxeles PPM"));
    };
    if payload > MAX_HEADER {
        return Err(io::Error::other("Cabecera PPM demasiado grande"));
    }
    Ok(Header {
        width,
        height,
        payload,
        length: width * height * 3,
    })
}
fn decode(mut bytes: Vec<u8>) -> io::Result<Bitmap> {
    if bytes.len() > MAX_RGB + MAX_HEADER {
        return Err(io::Error::other("Render PDF fuera del límite de salida"));
    }
    let h = ppm_header(&bytes)?;
    if bytes.len() != h.payload + h.length {
        return Err(io::Error::other(
            "Píxeles PDF incompletos o bytes adicionales",
        ));
    }
    bytes.drain(..h.payload);
    Ok(Bitmap {
        source: None,
        width: h.width as u32,
        height: h.height as u32,
        // These are raster dimensions, NOT original PDF page points/pixels.
        source_width: h.width as u32,
        source_height: h.height as u32,
        format: "pdf".into(),
        rgb: bytes,
    })
}

fn source_result<T>(flag: &AtomicBool, result: io::Result<T>) -> io::Result<T> {
    if result.is_err() {
        flag.store(true, Ordering::Release);
    }
    result
}
fn decode_text(output: Vec<u8>) -> io::Result<String> {
    if output.len() > MAX_TEXT
        || output.iter().filter(|c| **c == b'\n').count() >= crate::text_document::MAX_LINES
        || output.contains(&0)
    {
        return Err(io::Error::other(
            "Texto PDF fuera del límite256KiB/2000saltos o contiene NUL",
        ));
    }
    String::from_utf8(output).map_err(|_| io::Error::other("Texto PDF no es UTF-8 válido"))
}
#[cfg(target_os = "linux")]
fn checked_source(path: &Path, expected: Revision) -> io::Result<(std::fs::File, Revision)> {
    let (source, actual) = files_core::media_source::open(path)?;
    if actual != expected {
        return Err(io::Error::other("El PDF cambió antes de renderizar"));
    }
    if actual.bytes == 0 || actual.bytes > MAX_SOURCE {
        return Err(io::Error::other(
            "PDF debe ser archivo regular de hasta256MiB",
        ));
    }
    Ok((source, actual))
}
#[cfg(target_os = "linux")]
fn finish_render(
    source: &std::fs::File,
    path: &Path,
    revision: Revision,
    output: Vec<u8>,
    page: u32,
    pages: u32,
    source_invalid: &AtomicBool,
) -> io::Result<Ready> {
    let mut bitmap = decode(output)?;
    source_result(source_invalid, revision.revalidate(source, path))?;
    bitmap.source = Some(revision);
    Ok(Ready {
        bitmap,
        page,
        pages,
    })
}
#[cfg(target_os = "linux")]
fn run(
    path: PathBuf,
    page: u32,
    expected: Revision,
    text: bool,
    cancel: &AtomicBool,
    gate: &Mutex<Gate>,
    source_invalid: &AtomicBool,
) -> io::Result<Output> {
    use std::os::fd::AsRawFd;
    source_path(&path, page)?;
    // A supported-source limit is terminal for this stable revision, not a
    // source race requiring a new watcher token and repeated worker retries.
    if expected.bytes == 0 || expected.bytes > MAX_SOURCE {
        return Err(io::Error::other(
            "PDF debe ser archivo no vacío de hasta256MiB",
        ));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "PDF cancelado"));
    }
    // Ordinary parent symlinks/mountpoints follow OS navigation, not confinement.
    // BOTH subprocesses receive this SAME checked FD, never reopen the name.
    let (source, revision) = source_result(source_invalid, checked_source(&path, expected))?;
    let fd = source.as_raw_fd();
    let pinned = format!("/proc/self/fd/{fd}");
    let mut info = command("pdfinfo", fd)?;
    info.args(["-enc", "UTF-8", &pinned]);
    admit_helper(gate)?;
    let output = supervised(info, "preview_pdf_info", cancel, 64 * 1024)?;
    if let Ok(mut g) = gate.lock() {
        g.phase = Phase::ReadOnly;
    }
    source_result(source_invalid, revision.revalidate(&source, &path))?;
    let pages = parse_pages(&output)?;
    drop(output);
    if page > pages {
        return Err(io::Error::other("La página solicitada no existe"));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "PDF cancelado"));
    }
    if text {
        let mut extract = command("pdftotext", fd)?;
        extract.args([
            "-f",
            &page.to_string(),
            "-l",
            &page.to_string(),
            "-enc",
            "UTF-8",
            "-nopgbrk",
            "-layout",
            &pinned,
            "-",
        ]);
        admit_helper(gate)?;
        let output = supervised(extract, "preview_pdf_text", cancel, MAX_TEXT)?;
        if let Ok(mut g) = gate.lock() {
            g.phase = Phase::ReadOnly;
        }
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "PDF cancelado"));
        }
        let text = decode_text(output)?;
        source_result(source_invalid, revision.revalidate(&source, &path))?;
        return Ok(Output::Text(TextReady {
            text,
            source: revision,
            page,
            pages,
        }));
    }
    let mut render = command("pdftoppm", fd)?;
    render.args([
        "-f",
        &page.to_string(),
        "-l",
        &page.to_string(),
        "-singlefile",
        "-scale-to",
        "960",
        &pinned,
    ]);
    admit_helper(gate)?;
    let output = supervised(render, "preview_pdf_page", cancel, MAX_RGB + MAX_HEADER)?;
    if let Ok(mut g) = gate.lock() {
        g.phase = Phase::ReadOnly;
    }
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "PDF cancelado"));
    }
    finish_render(
        &source,
        &path,
        revision,
        output,
        page,
        pages,
        source_invalid,
    )
    .map(Output::Raster)
}
#[cfg(target_os = "linux")]
fn supervised(
    command: std::process::Command,
    phase: &str,
    cancel: &AtomicBool,
    limit: usize,
) -> io::Result<Vec<u8>> {
    crate::text_preview_job::run_command_program(command, &[], phase, cancel, TIMEOUT, limit, None)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Vista PDF no disponible: se necesitan pdfinfo y pdftoppm",
                )
            } else {
                io::Error::new(
                    e.kind(),
                    format!("No se pudo leer PDF (protegido, dañado o fuera de límites): {e}"),
                )
            }
        })
}
#[cfg(target_os = "linux")]
fn command(program: &str, fd: std::os::fd::RawFd) -> io::Result<std::process::Command> {
    use std::{os::unix::process::CommandExt, process::Command};
    if !(3..256).contains(&fd) {
        return Err(io::Error::other(
            "Demasiados descriptores para la vista PDF",
        ));
    }
    let mut command = Command::new(program);
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("OMP_NUM_THREADS", "1")
        .env("OPENBLAS_NUM_THREADS", "1");
    // Preserve the already occupied source FD: a new fixed dup target could
    // collide with Rust's exec-error pipe. Clear CLOEXEC only inside the child.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_AS, 96 * 1024 * 1024),
                (libc::RLIMIT_CPU, 3),
                (libc::RLIMIT_NOFILE, u64::from(fd as u32 + 8).max(64)),
                (libc::RLIMIT_CORE, 0),
            ] {
                let limit = libc::rlimit {
                    rlim_cur: value,
                    rlim_max: value,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(command)
}
#[cfg(not(target_os = "linux"))]
fn run(
    _: PathBuf,
    _: u32,
    _: Revision,
    _: bool,
    _: &AtomicBool,
    _: &Mutex<Gate>,
    _: &AtomicBool,
) -> io::Result<Output> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Vista PDF segura pendiente en esta plataforma",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_metadata_requires_unique_bounded_decimal_count() {
        assert_eq!(
            parse_pages(b"Title: abc\nPages: 42\nEncrypted: no\n").unwrap(),
            42
        );
        assert_eq!(parse_pages(b"Pages: 100000\n").unwrap(), MAX_PAGES);
        for value in [
            b"Pages: 0".as_slice(),
            b"Pages: 100001",
            b"Pages: -1",
            b"Pages: +1",
            b"Pages: 2 other",
            b"Pages: 1\nPages: 2",
            b"XPages: 3",
            b"Pages: \xff",
        ] {
            assert!(parse_pages(value).is_err(), "{value:?}");
        }
        assert!(parse_pages(&vec![b'x'; 64 * 1024 + 1]).is_err());
    }
    #[test]
    fn ppm_preserves_binary_whitespace_and_moves_same_allocation() {
        for header in [
            b"P6\n1 1\n255\n".as_slice(),
            b"P6\r\n# comment\r\n1\t1\r\n255\r\n",
        ] {
            let mut wire = header.to_vec();
            wire.extend_from_slice(b"\n #");
            let ptr = wire.as_ptr();
            let capacity = wire.capacity();
            let bitmap = decode(wire).unwrap();
            assert_eq!(bitmap.rgb, b"\n #");
            assert_eq!(bitmap.rgb.as_ptr(), ptr);
            assert_eq!(bitmap.rgb.capacity(), capacity);
        }
    }
    #[test]
    fn split_header_prefixes_and_exact_payload() {
        let header = b"P6\n2 1\n255\n";
        for end in 0..header.len() {
            assert!(ppm_header(&header[..end]).is_err());
        }
        let h = ppm_header(header).unwrap();
        assert_eq!(
            (h.width, h.height, h.length, h.payload),
            (2, 1, 6, header.len())
        );
        let mut wire = header.to_vec();
        wire.extend_from_slice(&[1; 6]);
        assert!(decode(wire.clone()).is_ok());
        wire.pop();
        assert!(decode(wire.clone()).is_err());
        wire.extend_from_slice(&[1, 2]);
        assert!(decode(wire).is_err());
    }
    #[test]
    fn rejects_wrong_magic_dimensions_maxval_and_excessive_header() {
        for header in [
            b"P3\n1 1\n255\n".as_slice(),
            b"P6\n0 1\n255\n",
            b"P6\n961 1\n255\n",
            b"P6\n1 961\n255\n",
            b"P6\n1 1\n65535\n",
            b"P6\n99999999999999999 1\n255\n",
            b"P6\n-1 1\n255\n",
        ] {
            assert!(ppm_header(header).is_err(), "{header:?}");
        }
        let mut header = b"P6\n#".to_vec();
        header.extend_from_slice(&vec![b'x'; MAX_HEADER]);
        header.extend_from_slice(b"\n1 1\n255\n");
        assert!(ppm_header(&header).is_err());
    }
    #[test]
    fn source_path_and_page_preflight() {
        assert!(source_path(Path::new("/own/fixture.pdf"), 1).is_ok());
        assert!(source_path(Path::new("relative.pdf"), 1).is_err());
        assert!(source_path(Path::new("/own/fixture.pdf"), 0).is_err());
        assert!(source_path(Path::new("/own/fixture.pdf"), MAX_PAGES + 1).is_err());
        assert!(source_path(Path::new(&format!("/{}", "a".repeat(4096))), 1).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert!(source_path(Path::new(std::ffi::OsStr::from_bytes(b"/x\0.pdf")), 1).is_err());
            assert!(source_path(Path::new(std::ffi::OsStr::from_bytes(b"/x\xff.pdf")), 1).is_ok());
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn verified_a_cannot_open_b_and_finished_a_cannot_publish_after_replace() {
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!("pdf-source-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.pdf");
        std::fs::write(&path, b"AAAA").unwrap();
        let (file, a) = files_core::media_source::open(&path).unwrap();
        let next = root.join("next.pdf");
        std::fs::write(&next, b"BBBB").unwrap();
        let old_modified = file.metadata().unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&next)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old_modified))
            .unwrap();
        std::fs::rename(&next, &path).unwrap();
        assert!(checked_source(&path, a).is_err());
        let (_, b) = files_core::media_source::open(&path).unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(
            (a.modified_seconds, a.modified_nanoseconds),
            (b.modified_seconds, b.modified_nanoseconds)
        );
        assert_ne!(a.inode, b.inode);
        let invalid = AtomicBool::new(false);
        let wire = b"P6\n1 1\n255\nRGB".to_vec();
        assert!(finish_render(&file, &path, a, wire.clone(), 1, 1, &invalid).is_err());
        assert!(invalid.load(Ordering::Acquire));
        let (file, b) = checked_source(&path, b).unwrap();
        let invalid = AtomicBool::new(false);
        let ready = finish_render(&file, &path, b, wire.clone(), 1, 1, &invalid).unwrap();
        assert_eq!(ready.bitmap.source, Some(b));
        assert_eq!(ready.bitmap.rgb, b"RGB");
        // Same inode/size/mtime changed in-place still differs by ctime.
        std::fs::write(&path, b"CCCC").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old_modified))
            .unwrap();
        assert!(finish_render(&file, &path, b, wire, 1, 1, &invalid).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn pdf_gate_close_forbids_later_helper_and_stdio_cannot_be_source() {
        let gate = Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: true,
        });
        assert!(admit_helper(&gate).is_err());
        for fd in [0, 1, 2, 256] {
            assert!(command("pdfinfo", fd).is_err());
        }
        assert!(command("pdfinfo", 3).is_ok());
        let invalid = AtomicBool::new(false);
        assert!(
            source_result(&invalid, Err::<(), _>(io::Error::other("source replaced"))).is_err()
        );
        assert!(invalid.load(Ordering::Acquire));
    }
    #[test]
    fn extracted_text_is_utf8_bounded_and_moves_its_allocation() {
        let bytes = "Texto á\t漢\r\n".as_bytes().to_vec();
        let ptr = bytes.as_ptr();
        let text = decode_text(bytes).unwrap();
        assert_eq!(text.as_ptr(), ptr);
        assert_eq!(text, "Texto á\t漢\r\n");
        assert_eq!(decode_text(Vec::new()).unwrap(), "");
        assert!(decode_text(vec![b'x'; MAX_TEXT]).is_ok());
        assert!(decode_text(vec![b'x'; MAX_TEXT + 1]).is_err());
        assert!(decode_text(vec![b'\n'; 2000]).is_ok());
        assert!(decode_text(vec![b'\n'; 2001]).is_err());
        assert!(decode_text(vec![0xff]).is_err());
        assert!(decode_text(vec![0]).is_err());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn stable_unsupported_pdf_does_not_rearm_source_retry_loop() {
        let mut expected = Revision {
            device: 1,
            inode: 1,
            bytes: MAX_SOURCE + 1,
            modified_seconds: 0,
            modified_nanoseconds: 0,
            changed_seconds: 0,
            changed_nanoseconds: 0,
        };
        let flag = AtomicBool::new(false);
        let cancel = AtomicBool::new(false);
        let gate = Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        });
        for bytes in [0, MAX_SOURCE + 1] {
            expected.bytes = bytes;
            assert!(
                run(
                    "/not-opened/large.pdf".into(),
                    1,
                    expected,
                    true,
                    &cancel,
                    &gate,
                    &flag
                )
                .is_err()
            );
            assert!(!flag.load(Ordering::Acquire));
            assert!(gate.lock().unwrap().phase == Phase::ReadOnly);
        }
    }
}
