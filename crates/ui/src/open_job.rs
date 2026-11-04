//! External user-app requests: system `open`, or Linux uwsm-app + xdg-terminal-exec.
//! Direct argv only: no shell, viewer selection or launcher fallback.
//! Terminal sends one bounded ASCII-hex directory to files-helper through UWSM,
//! then helper execs the terminal with raw cwd/--dir (no decoder limits). UWSM,
//! already installed on Omarchy, manages the external terminal independently.
//!
//! The owner admits at most one outstanding Job, with no automatic retries.
//! Opened means only that the launcher exited successfully; it does not prove a
//! viewer displayed the file. This is neither an image helper nor a managed
//! viewer: closing files must NOT kill the external application or wait for it.
//! Drop therefore does nothing. The worker independently waits/reaps the direct
//! launcher, which normally returns promptly (macOS `open` is used without -W).
//! A blocking/custom `open` can retain this worker until files exits; no timeout,
//! daemon, process-group kill, xdg-open/opener fallback or new global config is
//! introduced. Runtime capability is reported as Unavailable when missing.
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

const MAX_PATH_BYTES: usize = 4096;
const MAX_ERROR_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Open,
    Terminal,
}
impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Terminal => "Terminal",
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Opened,
    Unavailable,
    Failed(String),
}
pub struct Job {
    kind: Kind,
    /// Desktop ID chosen in "Abrir con" (None: the default application).
    pub app: Option<String>,
    result: Receiver<Outcome>,
    reported: Cell<bool>,
}
impl Job {
    /// Opens with the default application. Linux resolves it like Nautilus
    /// (MIME type, mimeapps.list, installed desktop entries) and launches it
    /// through UWSM (Omarchy's app launcher); Omarchy's `open` is only a
    /// shell function, not a program. macOS uses the system `open`.
    pub fn start(path: PathBuf) -> Result<Self, String> {
        validate_path(&path)?;
        #[cfg(target_os = "linux")]
        {
            Self::start_with(path, None)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::start_program(path, PathBuf::from("open"))
        }
    }
    /// "Abrir con": this desktop entry (or the default when None).
    pub fn start_with(path: PathBuf, app: Option<String>) -> Result<Self, String> {
        validate_path(&path)?;
        if let Some(id) = &app
            && files_core::apps::launch_id(id).is_none()
        {
            return Err("Aplicación no válida".into());
        }
        let program = std::env::var_os("FILES_TEST_OPEN_LAUNCHER")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("uwsm-app"));
        let mut job = Self::start_resolved(path, program, app.clone())?;
        job.app = app;
        Ok(job)
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn start_terminal(path: PathBuf) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            Self::start_kind(path, "uwsm-app".into(), Kind::Terminal)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err("Terminal en este sistema: integración PENDIENTE".into())
        }
    }
    #[cfg(any(not(target_os = "linux"), test))]
    fn start_program(path: PathBuf, program: PathBuf) -> Result<Self, String> {
        Self::start_kind(path, program, Kind::Open)
    }
    fn start_resolved(
        path: PathBuf,
        program: PathBuf,
        app: Option<String>,
    ) -> Result<Self, String> {
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-open".into())
            .spawn(move || {
                let outcome = match resolve(&path, app) {
                    Ok(id) => run_app(program, path, id),
                    Err(e) => Outcome::Failed(bounded(e)),
                };
                let _ = tx.send(outcome);
            })
            .map_err(|e| bounded(format!("No se pudo iniciar el trabajador de apertura: {e}")))?;
        Ok(Self {
            kind: Kind::Open,
            app: None,
            result,
            reported: Cell::new(false),
        })
    }
    fn start_kind(path: PathBuf, program: PathBuf, kind: Kind) -> Result<Self, String> {
        // Validate before any thread/channel/request copies. The incoming path
        // is moved, never converted through a lossy String or reinterpreted.
        validate_path(&path)?;
        if program.as_os_str().is_empty()
            || program.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES
            || program.as_os_str().as_encoded_bytes().contains(&0)
        {
            return Err("Comando de apertura no válido".into());
        }
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-open".into())
            .spawn(move || {
                let _ = tx.send(run(program, path, kind));
            })
            .map_err(|error| {
                bounded(format!(
                    "No se pudo iniciar el trabajador de apertura: {error}"
                ))
            })?;
        Ok(Self {
            kind,
            app: None,
            result,
            reported: Cell::new(false),
        })
    }
    /// Delivers exactly one launcher outcome; no blocking work runs on the UI.
    pub fn poll(&self) -> Option<Outcome> {
        if self.reported.get() {
            return None;
        }
        let outcome = match self.result.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Outcome::Failed("El trabajador de apertura terminó sin respuesta".into())
            }
        };
        self.reported.set(true);
        Some(outcome)
    }
}
// Intentionally NO Drop implementation: external launchers/viewers are not
// canceled or owned by the window's helper-drain/operation-close lifecycle.
fn validate_path(path: &Path) -> Result<(), String> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if !path.is_absolute() || bytes.len() > MAX_PATH_BYTES || bytes.contains(&0) {
        return Err("La apertura requiere una ruta absoluta de hasta 4096 bytes, sin NUL".into());
    }
    Ok(())
}
fn bounded(mut message: String) -> String {
    if message.len() > MAX_ERROR_BYTES {
        let mut end = MAX_ERROR_BYTES - '…'.len_utf8();
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push('…');
    }
    message
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn command(program: &Path, path: &Path, kind: Kind) -> std::io::Result<std::process::Command> {
    let mut command = std::process::Command::new(program);
    match kind {
        Kind::Open => {
            command.arg(path);
        }
        Kind::Terminal => {
            let encoded =
                files_core::terminal_directory::encode(path).map_err(std::io::Error::other)?;
            let helper = std::env::current_exe()?.with_file_name("files-helper");
            if helper.to_str().is_none() {
                return Err(std::io::Error::other(
                    "Terminal: ubicación de files-helper no UTF8, integración pendiente",
                ));
            }
            command
                .args(["-t", "service", "--"])
                .arg(helper)
                .arg("terminal")
                .arg(encoded)
                .current_dir("/");
        }
    }
    Ok(command)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run(program: PathBuf, path: PathBuf, kind: Kind) -> Outcome {
    use std::{io, process::Stdio};
    let mut command = match command(&program, &path, kind) {
        Ok(command) => command,
        Err(error) => return Outcome::Failed(bounded(format!("{}: {error}", kind.label()))),
    };
    let mut child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Outcome::Unavailable,
        Err(error) => {
            return Outcome::Failed(bounded(format!(
                "No se pudo ejecutar {}: {error}",
                kind.label()
            )));
        }
    };
    loop {
        match child.wait() {
            Ok(status) if status.success() => return Outcome::Opened,
            Ok(status) => {
                return Outcome::Failed(bounded(match status.code() {
                    Some(code) => format!("{} falló · código {code}", kind.label()),
                    None => format!("{} terminó por señal: {status}", kind.label()),
                }));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Outcome::Failed(bounded(format!(
                    "No se pudo recoger {}: {error}",
                    kind.label()
                )));
            }
        }
    }
}
/// The application to use: the chosen one, else the default for the file's
/// MIME type (else the first application that declares it).
fn resolve(path: &Path, app: Option<String>) -> Result<String, String> {
    let catalog = files_core::apps::scan();
    if let Some(id) = app {
        return files_core::apps::find(&catalog, &id)
            .map(|a| a.id.clone())
            .ok_or_else(|| format!("{id} ya no está instalada"));
    }
    let mime = files_core::apps::detect_mime(path);
    files_core::apps::default_for(&mime, &catalog)
        .ok_or_else(|| format!("Sin aplicación para {mime} · usa Abrir con… (Ctrl+Shift+O)"))
}
/// `uwsm-app -t service -- <id> <path>`: the app runs as its own unit and
/// the launcher returns at once (no slot held while the app stays open).
fn run_app(program: PathBuf, path: PathBuf, id: String) -> Outcome {
    use std::{io, process::Stdio};
    let Some(id_arg) = files_core::apps::launch_id(&id) else {
        return Outcome::Failed("Aplicación no válida".into());
    };
    let mut command = std::process::Command::new(&program);
    command
        .args(["-t", "service", "--"])
        .arg(id_arg)
        .arg(&path)
        .current_dir("/");
    let mut child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Outcome::Unavailable,
        Err(e) => return Outcome::Failed(bounded(format!("No se pudo abrir con {id}: {e}"))),
    };
    loop {
        match child.wait() {
            Ok(status) if status.success() => return Outcome::Opened,
            Ok(status) => {
                return Outcome::Failed(bounded(format!("{id} no se pudo iniciar ({status})")));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Outcome::Failed(bounded(format!("No se pudo recoger {id}: {e}"))),
        }
    }
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn run(_: PathBuf, _: PathBuf, _: Kind) -> Outcome {
    Outcome::Unavailable
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    #[test]
    fn terminal_arguments_and_cwd_preserve_raw_paths_without_shell() {
        use std::ffi::OsString;
        let path = PathBuf::from(OsString::from_vec(b"/raw \' $() -- \xff".to_vec()));
        let command = command(Path::new("uwsm-app"), &path, Kind::Terminal).unwrap();
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "-t");
        assert_eq!(args[1], "service");
        assert_eq!(args[2], "--");
        assert_eq!(args[4], "terminal");
        assert_eq!(
            files_core::terminal_directory::decode(args[5].to_str().unwrap()).unwrap(),
            path
        );
        assert_eq!(command.get_current_dir(), Some(Path::new("/")));
        assert_eq!(command.get_program(), "uwsm-app");
        assert!(args.iter().all(|a| a.to_str().is_some()));
    }
    #[test]
    fn terminal_worker_reports_managed_request_once() {
        let f = Fixture::new();
        let path =
            f.0.join(std::ffi::OsString::from_vec(b"raw \' $() \xff".to_vec()));
        fs::create_dir(&path).unwrap();
        let job = Job::start_kind(path, "/usr/bin/true".into(), Kind::Terminal).unwrap();
        assert_eq!(job.kind(), Kind::Terminal);
        assert_eq!(result(&job), Outcome::Opened);
    }
    #[test]
    fn terminal_failure_and_unavailable_do_not_fallback_or_spawn_after_close() {
        let f = Fixture::new();
        assert!(
            matches!(result(&Job::start_kind(f.0.clone(),"/usr/bin/false".into(),Kind::Terminal).unwrap()),Outcome::Failed(e)if e.contains("Terminal")&&e.contains('1'))
        );
        assert_eq!(
            result(&Job::start_kind(f.0.clone(), f.0.join("absent"), Kind::Terminal).unwrap()),
            Outcome::Unavailable
        );
        let mut model = crate::model::Model::new(f.0.clone());
        model.closing = true;
        model.open_terminal();
        assert!(model.open_job.is_none());
    }
    #[test]
    fn terminal_ack_preserves_operation_rescue_and_shares_single_flight_slot() {
        let f = Fixture::new();
        let mut model = crate::model::Model::new(f.0.clone());
        model.operation_message = Some("Rescate conservado".into());
        model.operation_details = vec!["ruta retenida".into()];
        let (tx, rx) = mpsc::sync_channel(1);
        model.open_job = Some(Job {
            kind: Kind::Terminal,
            app: None,
            result: rx,
            reported: Cell::new(false),
        });
        model.open_terminal();
        assert!(model.error.contains("pendiente"));
        assert_eq!(model.open_job.as_ref().unwrap().kind(), Kind::Terminal);
        tx.send(Outcome::Opened).unwrap();
        model.pump();
        assert!(model.open_job.is_none());
        assert_eq!(
            model.operation_message.as_deref(),
            Some("Rescate conservado")
        );
        assert_eq!(model.operation_details, ["ruta retenida"]);
        assert!(model.launch_notice.is_some());
    }
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let parent =
                PathBuf::from(std::env::var_os("HOME").expect("HOME required for fixtures"))
                    .join(".cache/files-test/open-job");
            fs::create_dir_all(&parent).unwrap();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = parent.join(format!(
                "{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn program(&self, name: &str, source: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, format!("#!/usr/bin/python3\n{source}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn result(job: &Job) -> Outcome {
        let started = Instant::now();
        loop {
            if let Some(value) = job.poll() {
                assert!(job.poll().is_none());
                return value;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "launcher did not return"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn absolute_argv_preserves_spaces_unicode_shell_symbols_and_non_utf8() {
        let fixture = Fixture::new();
        // Use an installed immutable interpreter: executing a just-written
        // script can race concurrent fork/exec inheritance of its write FD
        // (ETXTBSY). The target filename is still passed as one raw argv item.
        let program = PathBuf::from("/usr/bin/python3");
        for name in [
            std::ffi::OsString::from("-área con espacios $(printf NO_SHELL);.txt"),
            std::ffi::OsString::from_vec(vec![b'-', 0xff, b'x']),
        ] {
            let path = fixture.0.join(name);
            fs::write(&path, "import os,sys\nassert len(sys.argv)==1\nwith open(os.path.join(os.path.dirname(__file__),'argv'),'w') as out: out.write(os.fsencode(sys.argv[0]).hex())\n").unwrap();
            let expected = path
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let job = Job::start_program(path, program.clone()).unwrap();
            assert_eq!(result(&job), Outcome::Opened);
            assert_eq!(
                fs::read_to_string(fixture.0.join("argv")).unwrap(),
                expected
            );
        }
    }
    #[test]
    fn missing_command_and_failed_status_are_distinct_and_do_not_fallback() {
        let fixture = Fixture::new();
        let file = fixture.0.join("file.txt");
        assert_eq!(
            result(&Job::start_program(file.clone(), fixture.0.join("missing-open")).unwrap()),
            Outcome::Unavailable
        );
        let program = fixture.program("exit-seven", "raise SystemExit(7)");
        assert!(
            matches!(result(&Job::start_program(program,PathBuf::from("/usr/bin/python3")).unwrap()),Outcome::Failed(message) if message.contains('7'))
        );
    }
    #[test]
    fn preflight_rejects_relative_nul_and_excess_bytes_before_spawn() {
        let exact = PathBuf::from(format!("/{}a", "á".repeat((MAX_PATH_BYTES - 2) / 2)));
        assert_eq!(exact.as_os_str().as_encoded_bytes().len(), MAX_PATH_BYTES);
        assert!(validate_path(&exact).is_ok());
        assert!(validate_path(Path::new(&format!("/{}", "a".repeat(MAX_PATH_BYTES)))).is_err());
        assert!(validate_path(Path::new("relative.txt")).is_err());
        let nul = PathBuf::from(std::ffi::OsString::from_vec(b"/a\0b".to_vec()));
        assert!(Job::start(nul).is_err());
        assert!(bounded("漢".repeat(1000)).len() <= MAX_ERROR_BYTES);
    }
    #[test]
    fn dropping_job_does_not_cancel_the_launcher() {
        let fixture = Fixture::new();
        let program=fixture.program("delayed-open","import time\ntime.sleep(0.05)\nwith open(__file__ + '.done', 'w') as out:\n    out.write('completed')");
        let job = Job::start_program(program.clone(), PathBuf::from("/usr/bin/python3")).unwrap();
        drop(job);
        let started = Instant::now();
        while fs::read_to_string(program.with_file_name("delayed-open.done"))
            .ok()
            .as_deref()
            != Some("completed")
        {
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            fs::read_to_string(program.with_file_name("delayed-open.done")).unwrap(),
            "completed"
        );
    }
}
