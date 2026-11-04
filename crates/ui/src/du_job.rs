//! One cancellable helper at a time; no filesystem traversal on the UI thread.
use files_core::folder_usage::{Measurement, Mode};
use files_core::protocol::{Request, Response};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

pub struct Job<T = (u64, u64)> {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<T, String>>,
    progress: Option<Arc<Mutex<(u64, u64)>>>,
}
impl<T> Job<T> {
    #[cfg(test)]
    pub fn completed(value: Result<T, String>) -> Self {
        let (tx, result) = mpsc::sync_channel(1);
        tx.send(value).unwrap();
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            result,
            progress: None,
        }
    }
    pub fn progress(&self) -> Option<(u64, u64)> {
        self.progress
            .as_ref()
            .map(|p| *p.lock().unwrap_or_else(|e| e.into_inner()))
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El cálculo terminó sin respuesta".into()))
            }
        }
    }
}
impl Job {
    pub fn start_directory(path: PathBuf) -> Self {
        Self::start_mode(path, true)
    }
    fn start_mode(path: PathBuf, directory: bool) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let value = run(path, &flag, directory).map_err(|e| e.to_string());
            let _ = tx.send(value);
        });
        Self {
            cancel,
            result,
            progress: None,
        }
    }
}
impl Job<Measurement> {
    pub fn start_tracked(path: PathBuf, mode: Mode) -> Result<Self, String> {
        let request = tracked_request(path, mode)?;
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("files-helper");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let progress = Arc::new(Mutex::new((0u64, 0u64)));
        let published = progress.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-folder-size".into())
            .spawn(move || {
                let value = run_tracked(&request, &flag, executable, mode, &published)
                    .map_err(|e| e.to_string());
                let _ = tx.send(value);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cancel,
            result,
            progress: Some(progress),
        })
    }
}
fn tracked_request(path: PathBuf, mode: Mode) -> Result<Vec<u8>, String> {
    let command = match mode {
        Mode::Apparent => Request::MeasureDirectory { id: 1, path },
        Mode::Allocated => Request::MeasureDirectoryAllocated {
            id: 1,
            version: files_core::folder_usage::VERSION,
            path,
        },
    };
    let mut request = serde_json::to_vec(&command).map_err(|e| e.to_string())?;
    request.push(b'\n');
    if request.len() > 64 * 1024 {
        return Err("Petición de tamaño demasiado grande".into());
    }
    Ok(request)
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn run_tracked(
    request: &[u8],
    cancel: &AtomicBool,
    executable: PathBuf,
    mode: Mode,
    published: &Mutex<(u64, u64)>,
) -> io::Result<Measurement> {
    match mode {
        Mode::Apparent => {
            let mut decoder = UsageStream::default();
            crate::space_map_job::run_helper_stream(
                request,
                cancel,
                executable,
                Duration::from_secs(20),
                "measure_directory",
                |data| decoder.feed(data, published),
            )?;
            let (bytes, entries) = decoder.finish()?;
            Ok(Measurement::apparent(bytes, entries))
        }
        Mode::Allocated => {
            let mut decoder = files_core::folder_usage::Reader::new(1).map_err(io::Error::other)?;
            crate::space_map_job::run_helper_stream(
                request,
                cancel,
                executable,
                Duration::from_secs(20),
                "measure_directory_allocated",
                |data| {
                    decoder
                        .feed(data, |line| {
                            serde_json::from_slice(line).map_err(|e| e.to_string())
                        })
                        .map_err(io::Error::other)?;
                    if let Some(value) = decoder.progress() {
                        *published.lock().unwrap_or_else(|e| e.into_inner()) = value;
                    }
                    Ok(())
                },
            )?;
            let report = decoder.finish().map_err(io::Error::other)?;
            Measurement::try_from(report).map_err(io::Error::other)
        }
    }
}

#[cfg(target_os = "linux")]
fn run(path: PathBuf, cancel: &AtomicBool, directory: bool) -> io::Result<(u64, u64)> {
    let executable = std::env::current_exe()?.with_file_name("files-helper");
    if directory {
        run_program_mode(path, cancel, executable, true)
    } else {
        run_program(path, cancel, executable)
    }
}
#[cfg(target_os = "linux")]
fn run_program(path: PathBuf, cancel: &AtomicBool, executable: PathBuf) -> io::Result<(u64, u64)> {
    run_program_mode(path, cancel, executable, false)
}
#[cfg(target_os = "linux")]
fn run_program_mode(
    path: PathBuf,
    cancel: &AtomicBool,
    executable: PathBuf,
    directory: bool,
) -> io::Result<(u64, u64)> {
    let mut command = Command::new(executable);
    command.arg(if directory { "preview_directory" } else { "du" });
    run_command_mode(path, cancel, command, directory)
}
#[cfg(target_os = "linux")]
fn run_command_mode(
    path: PathBuf,
    cancel: &AtomicBool,
    mut command: Command,
    directory: bool,
) -> io::Result<(u64, u64)> {
    use std::os::fd::AsRawFd;
    let request = if directory {
        Request::PreviewDirectory { id: 1, path }
    } else {
        Request::Du { id: 1, path }
    };
    let mut request = serde_json::to_vec(&request)?;
    request.push(b'\n');
    // One atomic pipe-sized write, even if the helper fails before reading.
    if request.len() > 4096 {
        return Err(io::Error::other(
            "Ruta demasiado larga para calcular tamaño",
        ));
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("Cálculo cancelado"));
    }
    crate::owned_child::arm(&mut command);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", child.id())).ok();
        let start = stat
            .as_deref()
            .and_then(|s| s.rsplit_once(')'))
            .and_then(|(_, s)| s.split_whitespace().nth(19))
            .unwrap_or("unknown");
        eprintln!(
            "FILES_PREVIEW_HELPER pid={} start={} command=preview_directory_du",
            child.id(),
            start
        );
    }
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Sin salida del helper"))?;
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("Sin entrada del helper"))?;
        stdin.write_all(&request)?;
        drop(stdin);
        let start = Instant::now();
        let mut output = Vec::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::other("Cálculo cancelado"));
            }
            if start.elapsed() > Duration::from_secs(20) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Cálculo de tamaño agotó su tiempo",
                ));
            }
            let mut chunk = [0u8; 1024];
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output.len() + n > 8192 {
                            return Err(io::Error::other("Respuesta del helper demasiado grande"));
                        }
                        output.extend_from_slice(&chunk[..n]);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    return Err(io::Error::other("El helper de tamaño terminó con error"));
                }
                // The pipe may have received its final bytes after the read above.
                loop {
                    match stdout.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if output.len() + n > 8192 {
                                return Err(io::Error::other(
                                    "Respuesta del helper demasiado grande",
                                ));
                            }
                            output.extend_from_slice(&chunk[..n]);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e),
                    }
                }
                return match serde_json::from_slice::<Response>(&output)? {
                    Response::Usage {
                        id: 1,
                        bytes,
                        entries,
                    } => Ok((bytes, entries)),
                    Response::Error { id: 1, message } => Err(io::Error::other(message)),
                    _ => Err(io::Error::other("Respuesta de tamaño inesperada")),
                };
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    // Reap on the background worker, never block the window on waitpid.
    let _ = child.kill();
    let _ = child.wait();
    result
}
#[cfg(not(target_os = "linux"))]
fn run(_: PathBuf, _: &AtomicBool, _: bool) -> io::Result<(u64, u64)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Cálculo seguro aún no disponible en esta plataforma",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicU64;
    struct Script(PathBuf);
    impl Script {
        fn new(body: &str) -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "files-du-job-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            use std::fs::OpenOptions;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            file.write_all(format!("#!/bin/sh\nread request\n{body}\n").as_bytes())
                .unwrap();
            file.set_permissions(std::fs::Permissions::from_mode(0o700))
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Script {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    // Installed interpreter avoids executing a file still inherited writable by
    // concurrent fork/exec. All pipe/cancel/reap logic remains production code.
    fn run_fixture(path: PathBuf, cancel: &AtomicBool, script: PathBuf) -> io::Result<(u64, u64)> {
        let mut command = Command::new("/bin/sh");
        command.arg(script).arg("du");
        run_command_mode(path, cancel, command, false)
    }
    #[test]
    fn supervisor_accepts_matching_response_and_rejects_wrong_id() {
        let cancel = AtomicBool::new(false);
        let valid = Script::new(
            "printf '%s\\n' '{\"status\":\"usage\",\"id\":1,\"bytes\":8,\"entries\":4}'",
        );
        assert_eq!(
            run_fixture("/tmp".into(), &cancel, valid.0.clone()).unwrap(),
            (8, 4)
        );
        let stale = Script::new(
            "printf '%s\\n' '{\"status\":\"usage\",\"id\":2,\"bytes\":8,\"entries\":4}'",
        );
        assert!(run_fixture("/tmp".into(), &cancel, stale.0.clone()).is_err());
    }
    #[test]
    fn supervisor_cancels_and_reaps_without_waiting_for_helper_deadline() {
        let slow = Script::new("exec sleep 60");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let start = Instant::now();
        let worker = std::thread::spawn(move || run_fixture("/tmp".into(), &flag, slow.0.clone()));
        std::thread::sleep(Duration::from_millis(100));
        cancel.store(true, Ordering::Relaxed);
        assert!(worker.join().unwrap().is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod allocated_worker_tests {
    use super::*;
    use crate::space_map_job::TestScriptRegistration;
    use files_core::folder_usage::{Observation, Report, VERSION, Value};
    use std::{fs, sync::atomic::AtomicU64};

    struct Script {
        path: PathBuf,
        _registration: TestScriptRegistration,
    }
    impl Script {
        fn new(frames: &[serde_json::Value], tail: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = files_core::theme::home().join(".cache/files-test");
            fs::create_dir_all(&base).unwrap();
            let path = base.join(format!(
                "allocation-worker-{}-{}.py",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let payload = serde_json::to_string(&serde_json::to_string(frames).unwrap()).unwrap();
            let body = format!(
                "import os,sys,json,time\nfrom pathlib import Path\nrequest=json.loads(sys.stdin.buffer.readline())\nassert request['command']==sys.argv[1]\nassert request['id']==1\nif sys.argv[1]=='measure_directory_allocated': assert request['version']==1\nfor frame in json.loads({payload}): print(json.dumps(frame),flush=True)\n{tail}\n"
            );
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            file.write_all(body.as_bytes()).unwrap();
            drop(file);
            let registration = TestScriptRegistration::new(path.clone());
            Self {
                path,
                _registration: registration,
            }
        }
        fn assert_reaped(pid: Option<u32>) {
            let pid = pid.expect("transport must record the child PID");
            match fs::read(format!("/proc/{pid}/stat")) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                value => panic!("Child must be reaped, including zombies: {value:?}"),
            }
        }
    }
    impl Drop for Script {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("ready"));
        }
    }
    fn report() -> Report {
        Report {
            version: VERSION,
            bytes: 7,
            entries: 2,
            allocation: Observation {
                by_name: Value::Bytes(8192),
                by_identity: Value::Bytes(4096),
            },
        }
    }
    fn frames() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"status":"usage_progress","id":1,"bytes":0,"entries":0}),
            serde_json::json!({"status":"usage_progress","id":1,"bytes":7,"entries":2}),
            serde_json::json!({"status":"allocated_usage","id":1,"report":report()}),
        ]
    }
    fn request(mode: Mode) -> Vec<u8> {
        tracked_request("/fixture".into(), mode).unwrap()
    }
    fn invoke(script: &Script, mode: Mode) -> io::Result<Measurement> {
        let value = run_tracked(
            &request(mode),
            &AtomicBool::new(false),
            script.path.clone(),
            mode,
            &Mutex::new((0, 0)),
        );
        Script::assert_reaped(TestScriptRegistration::spawned_pid());
        value
    }
    #[test]
    fn tracked_allocated_worker_accepts_only_its_mode_and_preserves_unavailable() {
        let script = Script::new(&frames(), "");
        assert_eq!(
            invoke(&script, Mode::Allocated).unwrap(),
            Measurement::try_from(report()).unwrap()
        );
        assert!(invoke(&script, Mode::Apparent).is_err());
        let legacy = Script::new(
            &[serde_json::json!({"status":"usage","id":1,"bytes":7,"entries":2})],
            "",
        );
        assert_eq!(
            invoke(&legacy, Mode::Apparent).unwrap(),
            Measurement::apparent(7, 2)
        );
        assert!(invoke(&legacy, Mode::Allocated).is_err());
        let mut unavailable = frames();
        unavailable[1]["entries"] = serde_json::json!(65537);
        unavailable[2]["report"]["entries"] = serde_json::json!(65537);
        unavailable[2]["report"]["allocation"]["by_identity"] =
            serde_json::json!({"state":"unavailable","value":"identity_limit"});
        let script = Script::new(&unavailable, "");
        let result = invoke(&script, Mode::Allocated).unwrap();
        assert_eq!(
            result.allocation.unwrap().by_identity,
            Value::Unavailable(files_core::folder_usage::Reason::IdentityLimit)
        );
    }
    #[test]
    fn tracked_worker_rejects_nonzero_exit_wrong_id_invalid_final_and_trailing_data() {
        let script = Script::new(&frames(), "sys.exit(3)");
        assert!(invoke(&script, Mode::Allocated).is_err());
        let script = Script::new(&frames(), "print('extra',flush=True)");
        assert!(invoke(&script, Mode::Allocated).is_err());
        let mut wrong = frames();
        wrong[2]["id"] = serde_json::json!(2);
        assert!(invoke(&Script::new(&wrong, ""), Mode::Allocated).is_err());
        let mut mismatch = frames();
        mismatch[2]["report"]["bytes"] = serde_json::json!(8);
        assert!(invoke(&Script::new(&mismatch, ""), Mode::Allocated).is_err());
        assert!(invoke(&Script::new(&frames()[..2], ""), Mode::Allocated).is_err());
    }
    #[test]
    fn cancellation_after_terminal_bytes_still_requires_child_exit_and_reap() {
        let script = Script::new(
            &frames(),
            "Path(__file__).with_suffix('.ready').write_text(str(os.getpid()))\ntime.sleep(60)",
        );
        let ready = script.path.with_extension("ready");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let worker = std::thread::spawn(move || {
            let value = run_tracked(
                &request(Mode::Allocated),
                &flag,
                script.path.clone(),
                Mode::Allocated,
                &Mutex::new((0, 0)),
            );
            let pid = TestScriptRegistration::spawned_pid();
            (value, pid, script)
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && !worker.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let terminal_sent = ready.exists();
        let completed_early = worker.is_finished();
        cancel.store(true, Ordering::Release);
        let (value, pid, _script) = worker.join().unwrap();
        Script::assert_reaped(pid);
        assert!(
            terminal_sent,
            "fixture must have sent its terminal before cancellation"
        );
        assert!(
            !completed_early,
            "terminal bytes do not complete a still-live child"
        );
        assert_eq!(value.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
}

#[derive(Default)]
struct UsageStream {
    line: Vec<u8>,
    last: (u64, u64),
    final_result: Option<io::Result<(u64, u64)>>,
}
impl UsageStream {
    fn feed(&mut self, data: &[u8], progress: &Mutex<(u64, u64)>) -> io::Result<()> {
        for (i, part) in data.split(|&b| b == b'\n').enumerate() {
            if i > 0 {
                self.decode(progress)?;
                self.line.clear();
            }
            if self.line.len() + part.len() > 4096 {
                return Err(io::Error::other("Línea de progreso superior a4096B"));
            }
            self.line.extend_from_slice(part);
        }
        Ok(())
    }
    fn decode(&mut self, progress: &Mutex<(u64, u64)>) -> io::Result<()> {
        if self.final_result.is_some() {
            return Err(io::Error::other("Datos después del resultado final"));
        }
        let message: Response = serde_json::from_slice(&self.line)?;
        let (value, done) = match message {
            Response::UsageProgress {
                id: 1,
                bytes,
                entries,
            } => ((bytes, entries), false),
            Response::Usage {
                id: 1,
                bytes,
                entries,
            } => ((bytes, entries), true),
            Response::Error { id: 1, mut message } => {
                if message.len() > 512 {
                    let mut end = 512;
                    while !message.is_char_boundary(end) {
                        end -= 1;
                    }
                    message.truncate(end);
                }
                self.final_result = Some(Err(io::Error::other(message)));
                return Ok(());
            }
            _ => return Err(io::Error::other("Mensaje de tamaño inesperado")),
        };
        if value.0 < self.last.0 || value.1 < self.last.1 || value.1 > 1_000_000 {
            return Err(io::Error::other("Progreso de tamaño inválido"));
        }
        self.last = value;
        *progress.lock().unwrap_or_else(|e| e.into_inner()) = value;
        if done {
            self.final_result = Some(Ok(value));
        }
        Ok(())
    }
    fn finish(self) -> io::Result<(u64, u64)> {
        if !self.line.is_empty() {
            return Err(io::Error::other("Respuesta de tamaño truncada"));
        }
        self.final_result
            .unwrap_or_else(|| Err(io::Error::other("El helper terminó sin resumen completo")))
    }
}
#[cfg(test)]
mod stream_tests {
    use super::*;
    #[test]
    fn split_progress_is_coalesced_and_only_terminal_summary_is_complete() {
        let progress = Mutex::new((0, 0));
        let mut s = UsageStream::default();
        let p = serde_json::to_vec(&Response::UsageProgress {
            id: 1,
            bytes: 3,
            entries: 1,
        })
        .unwrap();
        s.feed(&p[..5], &progress).unwrap();
        assert_eq!(*progress.lock().unwrap(), (0, 0));
        s.feed(&p[5..], &progress).unwrap();
        s.feed(b"\n", &progress).unwrap();
        assert_eq!(*progress.lock().unwrap(), (3, 1));
        let mut final_bytes = serde_json::to_vec(&Response::Usage {
            id: 1,
            bytes: 8,
            entries: 5,
        })
        .unwrap();
        final_bytes.push(b'\n');
        s.feed(&final_bytes, &progress).unwrap();
        assert_eq!(s.finish().unwrap(), (8, 5));
    }
    #[test]
    fn rejects_regressions_duplicate_final_oversize_and_unfinished_stream() {
        let p = Mutex::new((0, 0));
        let mut s = UsageStream::default();
        s.feed(
            b"{\"status\":\"usage_progress\",\"id\":1,\"bytes\":9,\"entries\":2}\n",
            &p,
        )
        .unwrap();
        assert!(s.finish().is_err());
        let mut s = UsageStream::default();
        assert!(s.feed(&vec![b'x'; 4097], &p).is_err());
        let final_line = b"{\"status\":\"usage\",\"id\":1,\"bytes\":9,\"entries\":2}\n";
        let mut s = UsageStream::default();
        s.feed(final_line, &p).unwrap();
        assert!(s.feed(final_line, &p).is_err());
        let mut s = UsageStream::default();
        s.feed(
            b"{\"status\":\"usage_progress\",\"id\":1,\"bytes\":9,\"entries\":2}\n",
            &p,
        )
        .unwrap();
        assert!(
            s.feed(
                b"{\"status\":\"usage\",\"id\":1,\"bytes\":8,\"entries\":2}\n",
                &p
            )
            .is_err()
        );
    }
}
