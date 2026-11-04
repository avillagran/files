//! Experimental native HTML display-list worker. No browser, network or script
//! engine. The optional local adapter is supervised/reaped by the common loop.
use crate::html_display_list::Document;
use crate::readonly_gate::{Gate, Phase, admit_helper};
use files_core::media_source::Revision;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
pub struct Ready {
    pub source: Revision,
    pub rendered: Document,
    pub width: i32,
    pub height: i32,
}
pub struct Job {
    gate: Arc<Mutex<Gate>>,
    source_invalid: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Ready, String>>,
}
impl Job {
    pub fn start(path: PathBuf, width: i32, height: i32, expected: Revision) -> Self {
        let gate = Arc::new(Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: false,
        }));
        let state = gate.clone();
        let source_invalid = Arc::new(AtomicBool::new(false));
        let source_flag = source_invalid.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = run(path, width, height, expected, &flag, &state, &source_flag);
            if let Ok(mut g) = state.lock() {
                g.phase = Phase::Stopped;
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
        if let Ok(mut g) = self.gate.lock() {
            g.closing = true;
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
    pub fn poll(&self) -> Option<Result<Ready, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El motor HTML terminó sin respuesta".into()))
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
fn checked_source(
    path: &std::path::Path,
    expected: Revision,
) -> std::io::Result<(std::fs::File, Revision)> {
    let (file, actual) = files_core::media_source::open(path)?;
    if actual != expected {
        return Err(std::io::Error::other("El HTML cambió antes de renderizar"));
    }
    if actual.bytes > 32768 {
        return Err(std::io::Error::other("HTML fuera del límite32KiB"));
    }
    Ok((file, actual))
}
fn source_result<T>(flag: &AtomicBool, result: std::io::Result<T>) -> Result<T, String> {
    result.map_err(|error| {
        flag.store(true, Ordering::Release);
        error.to_string()
    })
}
#[cfg(target_os = "linux")]
fn finish_render(
    file: &std::fs::File,
    path: &std::path::Path,
    revision: Revision,
    output: &[u8],
    viewport: (i32, i32),
    source_invalid: &AtomicBool,
) -> Result<Ready, String> {
    let rendered = Document::decode(output)?;
    if rendered.viewport_dimensions() != viewport {
        return Err("El motor devolvió otro tamaño de viewport".into());
    }
    source_result(source_invalid, revision.revalidate(file, path))?;
    Ok(Ready {
        rendered,
        source: revision,
        width: viewport.0,
        height: viewport.1,
    })
}
#[cfg(target_os = "linux")]
fn run(
    path: PathBuf,
    width: i32,
    height: i32,
    expected: Revision,
    cancel: &AtomicBool,
    gate: &Mutex<Gate>,
    source_invalid: &AtomicBool,
) -> Result<Ready, String> {
    use std::{
        os::{fd::AsRawFd, unix::process::CommandExt},
        process::Command,
    };
    if cancel.load(Ordering::Acquire) {
        return Err("Vista HTML cancelada".into());
    }
    // Unsupported size is terminal for this revision, not a source-change
    // event: do not spin a fresh watcher token/worker for the same stable file.
    if expected.bytes > 32768 {
        return Err("HTML fuera del límite32KiB".into());
    }
    let (file, revision) = source_result(source_invalid, checked_source(&path, expected))?;
    let fd = file.as_raw_fd();
    if !(3..256).contains(&fd) {
        return Err("Demasiados descriptores para HTML".into());
    }
    let width = width.clamp(128, 1600);
    let height = height.clamp(128, 1200);
    let executable = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("files-html-helper");
    let mut command = Command::new(executable);
    command
        .arg("--source-fd")
        .arg(fd.to_string())
        .arg(width.to_string())
        .arg(height.to_string())
        .arg(crate::fonts::family());
    // Process containment, NOT a PSS gate. No child helpers are spawned by the
    // adapter; file/network callbacks are disabled in this restricted mode.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_AS, 64 * 1024 * 1024),
                (libc::RLIMIT_CPU, 3),
                (libc::RLIMIT_NOFILE, 32),
                (libc::RLIMIT_CORE, 0),
            ] {
                let limit = libc::rlimit {
                    rlim_cur: value,
                    rlim_max: value,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    admit_helper(gate).map_err(|e| e.to_string())?;
    let output = crate::text_preview_job::run_command_program(
        command,
        &[],
        "preview_html",
        cancel,
        Duration::from_secs(5),
        2 * 1024 * 1024 + 16384,
        None,
    )
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Vista HTML no disponible en esta compilación".into()
        } else {
            format!("No se pudo renderizar HTML (máximo32KiB): {e}")
        }
    })?;
    if let Ok(mut g) = gate.lock() {
        g.phase = Phase::ReadOnly;
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Vista HTML cancelada".into());
    }
    finish_render(
        &file,
        &path,
        revision,
        &output,
        (width, height),
        source_invalid,
    )
}

#[cfg(not(target_os = "linux"))]
fn run(
    _: PathBuf,
    _: i32,
    _: i32,
    _: Revision,
    _: &AtomicBool,
    _: &Mutex<Gate>,
    _: &AtomicBool,
) -> Result<Ready, String> {
    Err("Motor HTML nativo pendiente en esta plataforma".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    fn wire() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"schema":1,"status":"PROBE_ONLY","viewport":[640,480],"document":[640,480],"command_count":0,"commands":[]})).unwrap()
    }
    #[test]
    fn source_expected_a_rejects_open_b_and_late_a_render_after_replacement() {
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!("html-source-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.html");
        std::fs::write(&path, b"<p>A</p>").unwrap();
        let (file, a) = files_core::media_source::open(&path).unwrap();
        let modified = file.metadata().unwrap().modified().unwrap();
        let next = root.join("next.html");
        std::fs::write(&next, b"<p>B</p>").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&next)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        std::fs::rename(next, &path).unwrap();
        assert!(checked_source(&path, a).is_err());
        let invalid = AtomicBool::new(false);
        assert!(finish_render(&file, &path, a, &wire(), (640, 480), &invalid).is_err());
        assert!(invalid.load(Ordering::Acquire));
        let (file, b) = files_core::media_source::open(&path).unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(
            (a.modified_seconds, a.modified_nanoseconds),
            (b.modified_seconds, b.modified_nanoseconds)
        );
        assert_ne!(a.inode, b.inode);
        let invalid = AtomicBool::new(false);
        let ready = finish_render(&file, &path, b, &wire(), (640, 480), &invalid).unwrap();
        assert_eq!(ready.source, b);
        assert!(!invalid.load(Ordering::Acquire));
        assert!(finish_render(&file, &path, b, &wire(), (641, 480), &invalid).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(finish_render(&file, &path, b, &wire(), (640, 480), &invalid).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn size_cap_precedes_child_and_readonly_close_denies_admission() {
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!("html-source-cap-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.htm");
        std::fs::write(&path, vec![b' '; 32769]).unwrap();
        let (_, revision) = files_core::media_source::open(&path).unwrap();
        assert!(checked_source(&path, revision).is_err());
        let gate = Mutex::new(Gate {
            phase: Phase::ReadOnly,
            closing: true,
        });
        assert!(admit_helper(&gate).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stable_unsupported_html_size_is_terminal_without_reprobe_flag() {
        let expected = Revision {
            device: 1,
            inode: 1,
            bytes: 32769,
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
        assert!(
            run(
                "/not-opened/large.html".into(),
                640,
                480,
                expected,
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
