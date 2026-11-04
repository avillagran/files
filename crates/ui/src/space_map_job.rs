//! One bounded, cancellable space-map helper. The owner MUST drain this job's
//! result before dispatching another; only the background worker waits/reaps.
//! This is process containment/lifecycle, not a measured PSS or latency gate.
use files_core::{
    protocol::{Request, Response},
    space_map::{self, MAX_NODES, SpaceReport},
};
use std::{
    fmt::{self, Write as FmtWrite},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
const MAX_REQUEST: usize = 64 * 1024;
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_DIAGNOSTIC: usize = 1024;
#[cfg(all(test, target_os = "linux"))]
thread_local! {
    static TEST_SPAWN_PID: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}
#[cfg(all(test, target_os = "linux"))]
static TEST_SCRIPT_PATHS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<PathBuf>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));
#[cfg(all(test, target_os = "linux"))]
fn test_script_registered(path: &std::path::Path) -> bool {
    TEST_SCRIPT_PATHS.lock().unwrap().contains(path)
}
#[cfg(all(test, target_os = "linux"))]
pub(crate) struct TestScriptRegistration(PathBuf);
#[cfg(all(test, target_os = "linux"))]
impl TestScriptRegistration {
    pub(crate) fn new(path: PathBuf) -> Self {
        assert!(TEST_SCRIPT_PATHS.lock().unwrap().insert(path.clone()));
        Self(path)
    }
    pub(crate) fn spawned_pid() -> Option<u32> {
        TEST_SPAWN_PID.with(|slot| slot.get())
    }
}
#[cfg(all(test, target_os = "linux"))]
impl Drop for TestScriptRegistration {
    fn drop(&mut self) {
        TEST_SCRIPT_PATHS.lock().unwrap().remove(&self.0);
    }
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<SpaceReport, String>>,
}
impl Job {
    pub fn start(path: PathBuf) -> Result<Self, String> {
        let request = request(path)
            .map_err(|error| diagnostic(format_args!("No se pudo preparar Espacio: {error}")))?;
        let executable = std::env::current_exe()
            .map_err(|error| {
                diagnostic(format_args!("No se encontró el helper de Espacio: {error}"))
            })?
            .with_file_name("files-helper");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-space-map".into())
            .spawn(move || {
                let value = run_program(&request, &flag, executable, TIMEOUT)
                    .and_then(decode)
                    .map_err(|error| {
                        diagnostic(format_args!("No se pudo calcular Espacio: {error}"))
                    });
                let _ = tx.send(value);
            })
            .map_err(|error| {
                diagnostic(format_args!(
                    "No se pudo iniciar el cálculo de Espacio: {error}"
                ))
            })?;
        Ok(Self { cancel, result })
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&self) -> Option<Result<SpaceReport, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("El helper de Espacio terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
struct Diagnostic {
    text: String,
}
impl fmt::Write for Diagnostic {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = MAX_DIAGNOSTIC.saturating_sub(self.text.len());
        if value.len() <= remaining {
            self.text.push_str(value);
            return Ok(());
        }
        let mut end = remaining;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&value[..end]);
        Err(fmt::Error)
    }
}
fn diagnostic(value: fmt::Arguments<'_>) -> String {
    let mut output = Diagnostic {
        text: String::with_capacity(MAX_DIAGNOSTIC),
    };
    let _ = output.write_fmt(value);
    output.text
}
struct Buffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Buffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
    fn append(&mut self, value: &[u8]) -> io::Result<()> {
        let needed = self
            .bytes
            .len()
            .checked_add(value.len())
            .filter(|length| *length <= self.limit)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Respuesta/petición de Espacio fuera del límite",
                )
            })?;
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().max(256).saturating_mul(2))
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(value);
        Ok(())
    }
}
impl io::Write for Buffer {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.append(value)?;
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn valid_path(path: &Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    path.is_absolute() && bytes.len() <= 4096 && !bytes.contains(&0)
}
fn request(path: PathBuf) -> io::Result<Vec<u8>> {
    if !valid_path(&path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Espacio requiere una ruta absoluta de hasta4096bytes, sin NUL",
        ));
    }
    // Path serde rejects non-UTF8 explicitly; never replace bytes with U+FFFD.
    let mut output = Buffer::new(MAX_REQUEST);
    serde_json::to_writer(&mut output, &Request::SpaceMap { id: 1, path })?;
    output.write_all(b"\n")?;
    Ok(output.bytes)
}
fn validate(report: &SpaceReport) -> io::Result<()> {
    space_map::validate_report(report).map_err(io::Error::other)
}
fn decode(output: Vec<u8>) -> io::Result<SpaceReport> {
    if output.len() > MAX_RESPONSE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Respuesta de Espacio superior a4MiB",
        ));
    }
    // Early bound for the helper's canonical serde wire. This is a byte-token
    // preflight, NOT a general JSON parser/security sandbox: alternative key
    // spelling/whitespace still requires the post-deserialization core validator.
    if output
        .windows(b"\"name\":".len())
        .filter(|window| *window == b"\"name\":")
        .take(MAX_NODES + 1)
        .count()
        > MAX_NODES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "La respuesta de Espacio supera2048nodos antes de decodificar",
        ));
    }
    match serde_json::from_slice::<Response>(&output)? {
        Response::SpaceMap { id: 1, report } => {
            validate(&report)?;
            Ok(report)
        }
        Response::Error { id: 1, message } | Response::Unsupported { id: 1, message } => {
            Err(io::Error::other(diagnostic(format_args!("{message}"))))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Respuesta de Espacio inesperada o id incorrecto",
        )),
    }
}
fn check(cancel: &AtomicBool, started: Instant, timeout: Duration) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Operación cancelada",
        ))
    } else if started.elapsed() >= timeout {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "El helper agotó su tiempo",
        ))
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
fn run_program(
    request: &[u8],
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    run_helper(request, cancel, executable, timeout, "space_map")
}
#[cfg(target_os = "linux")]
pub(crate) fn run_helper(
    request: &[u8],
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
    command: &str,
) -> io::Result<Vec<u8>> {
    let mut output = Buffer::new(MAX_RESPONSE);
    run_helper_stream(request, cancel, executable, timeout, command, |chunk| {
        output.append(chunk)
    })?;
    Ok(output.bytes)
}
pub(crate) struct Dialogue {
    pub message: Option<Vec<u8>>,
    pub idle: bool,
}
#[cfg(target_os = "linux")]
pub(crate) fn run_helper_stream(
    request: &[u8],
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
    command: &str,
    consume: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    run_helper_dialogue(request, cancel, executable, timeout, command, consume, None)
}
#[cfg(target_os = "linux")]
pub(crate) fn run_helper_dialogue(
    request: &[u8],
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
    command: &str,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
    mut control: Option<&mut dyn FnMut() -> io::Result<Dialogue>>,
) -> io::Result<()> {
    let response_limit = match command {
        "find_duplicates" => 32 * 1024 * 1024,
        "compare_directories" => files_core::directory_compare_wire::MAX_STREAM,
        "prepare_directory_sync" => files_core::directory_sync_wire::MAX_STREAM,
        _ => MAX_RESPONSE,
    };
    use std::{
        os::fd::{AsRawFd, RawFd},
        process::{Command, Stdio},
    };
    fn nonblocking(fd: RawFd) -> io::Result<()> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    if request.len() > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Petición del helper superior a64KiB",
        ));
    }
    #[cfg(test)]
    TEST_SPAWN_PID.with(|slot| slot.set(None));
    let mut started = Instant::now();
    check(cancel, started, timeout)?;
    // Test scripts are data read by a stable interpreter, never freshly written executables.
    #[cfg(test)]
    let mut program = if test_script_registered(&executable) {
        let mut program = Command::new("/usr/bin/python3");
        program.arg(&executable);
        program
    } else {
        Command::new(&executable)
    };
    #[cfg(not(test))]
    let mut program = Command::new(executable);
    program.arg(command);
    crate::owned_child::arm(&mut program);
    let mut child = program
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(if control.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()?;
    #[cfg(test)]
    TEST_SPAWN_PID.with(|slot| slot.set(Some(child.id())));
    if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", child.id())).ok();
        let start = stat
            .as_deref()
            .and_then(|s| s.rsplit_once(')'))
            .and_then(|(_, s)| s.split_whitespace().nth(19))
            .unwrap_or("unknown");
        eprintln!(
            "FILES_PREVIEW_HELPER pid={} start={} command={}",
            child.id(),
            start,
            command
        );
    }
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Sin salida del helper"))?;
        let mut stdin = Some(
            child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("Sin entrada del helper"))?,
        );
        let mut stderr = child.stderr.take();
        if let Some(error) = &stderr {
            nonblocking(error.as_raw_fd())?;
        }
        let mut diagnostic = Vec::new();
        nonblocking(stdout.as_raw_fd())?;
        nonblocking(stdin.as_ref().expect("stdin present").as_raw_fd())?;
        let (mut offset, mut exited) = (0usize, None);
        let mut output_bytes = 0usize;
        let mut message = std::borrow::Cow::Borrowed(request);
        let mut writing = true;
        let mut idle = false;
        let mut tick = Instant::now();
        loop {
            let now = Instant::now();
            if idle {
                started = started
                    .checked_add(now.duration_since(tick))
                    .ok_or_else(|| io::Error::other("Espera fuera del rango"))?;
            }
            tick = now;
            if !writing && let Some(control) = control.as_mut() {
                let next = control()?;
                idle = next.idle;
                if let Some(bytes) = next.message {
                    if bytes.is_empty() || bytes.len() > 256 {
                        return Err(io::Error::other("Control del helper fuera del límite256B"));
                    }
                    message = std::borrow::Cow::Owned(bytes);
                    offset = 0;
                    writing = true;
                    idle = false;
                    output_bytes = 0;
                }
            }
            check(cancel, started, timeout)?;
            if writing && let Some(input) = stdin.as_mut() {
                // Both directions are nonblocking: request64KiB need not fit
                // PIPE_BUF, and an early helper response cannot deadlock writes.
                match input.write(&message[offset..]) {
                    Ok(0) if offset < message.len() => {
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "No se pudo enviar petición al helper",
                        ));
                    }
                    Ok(count) => offset += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => return Err(error),
                }
                if offset == message.len() {
                    writing = false;
                    if control.is_none() {
                        stdin.take();
                    }
                }
            }
            let mut chunk = [0u8; 8192];
            let eof = loop {
                check(cancel, started, timeout)?;
                let read_limit = chunk.len().min(
                    response_limit
                        .saturating_sub(output_bytes)
                        .saturating_add(1),
                );
                match stdout.read(&mut chunk[..read_limit]) {
                    Ok(0) => break true,
                    Ok(count) => {
                        output_bytes += count;
                        if output_bytes > response_limit {
                            return Err(io::Error::other(format!(
                                "Respuesta del helper superior a{}MiB",
                                response_limit / (1024 * 1024)
                            )));
                        }
                        consume(&chunk[..count])?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break false,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            };
            if let Some(error) = stderr.as_mut() {
                let mut chunk = [0u8; 4096];
                for _ in 0..16 {
                    check(cancel, started, timeout)?;
                    match error.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(count) => {
                            let keep = count.min(8192usize.saturating_sub(diagnostic.len()));
                            diagnostic.extend_from_slice(&chunk[..keep]);
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) => return Err(e),
                    }
                }
            }
            if exited.is_none() {
                exited = child.try_wait()?;
            }
            if let Some(status) = exited {
                if !status.success() {
                    if diagnostic.is_empty() {
                        return Err(io::Error::other("El helper terminó con error"));
                    }
                    let text = String::from_utf8_lossy(&diagnostic);
                    let mut end = text.len().min(512);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    return Err(io::Error::other(format!(
                        "El helper terminó con error: {}",
                        &text[..end]
                    )));
                }
                if eof {
                    if writing {
                        return Err(io::Error::other(
                            "El helper terminó sin recibir toda la petición",
                        ));
                    }
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // All exits pass here. Transport completion reaches State only after reap.
    // Earlier provisional/page callbacks never mean child-completion ACK.
    let _ = child.kill();
    child.wait()?;
    result
}
#[cfg(not(target_os = "linux"))]
fn run_program(_: &[u8], _: &AtomicBool, _: PathBuf, _: Duration) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "El helper seguro de Espacio está pendiente en esta plataforma",
    ))
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn run_helper(
    _: &[u8],
    _: &AtomicBool,
    _: PathBuf,
    _: Duration,
    _: &str,
) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Helper seguro pendiente en esta plataforma",
    ))
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn run_helper_dialogue(
    _: &[u8],
    _: &AtomicBool,
    _: PathBuf,
    _: Duration,
    _: &str,
    _: impl FnMut(&[u8]) -> io::Result<()>,
    _: Option<&mut dyn FnMut() -> io::Result<Dialogue>>,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Helper seguro pendiente en esta plataforma",
    ))
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn run_helper_stream(
    _: &[u8],
    _: &AtomicBool,
    _: PathBuf,
    _: Duration,
    _: &str,
    _: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Helper seguro pendiente en esta plataforma",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use files_core::space_map::NodeStatus;
    fn empty() -> SpaceReport {
        SpaceReport {
            nodes: Vec::new(),
            total_bytes: 0,
            observed_bytes: 0,
            visited: 0,
            complete: true,
            reason: None,
            omitted_children: Some(0),
        }
    }
    #[test]
    fn buffer_limits_before_reserve_and_preserves_bytes_on_rejection() {
        let mut buffer = Buffer::new(8);
        buffer.append(b"1234").unwrap();
        let capacity = buffer.bytes.capacity();
        assert!(buffer.append(b"56789").is_err());
        assert_eq!(buffer.bytes, b"1234");
        assert_eq!(buffer.bytes.capacity(), capacity);
        buffer.append(b"5678").unwrap();
        assert_eq!(buffer.bytes, b"12345678");
        assert!(buffer.bytes.capacity() <= 8);
    }
    #[test]
    fn report_ledger_requires_completion_and_reason_consistency() {
        assert!(validate(&empty()).is_ok());
        let mut report = empty();
        report.total_bytes = 1;
        assert!(validate(&report).is_err());
        let mut report = empty();
        report.complete = false;
        assert!(validate(&report).is_err());
        report.reason = Some("Cancelado".into());
        assert!(validate(&report).is_ok());
        report.nodes.push(space_map::SpaceNode {
            name: b"partial".to_vec(),
            kind: space_map::NodeKind::Directory,
            bytes: None,
            observed_bytes: 3,
            files: 1,
            directories: 1,
            status: NodeStatus::Partial,
            error: Some("Sin permiso".into()),
        });
        report.visited = 2;
        report.observed_bytes = 3;
        assert!(validate(&report).is_ok());
        report.complete = true;
        assert!(validate(&report).is_err());
    }
    #[test]
    fn canonical_node_count_is_rejected_before_json_deserialization() {
        // Deliberately invalid JSON to demonstrate that the count preflight
        // wins before serde reconstructs any SpaceNode Vec.
        let wire = b"{\"name\":".repeat(MAX_NODES + 1);
        let error = decode(wire).unwrap_err();
        assert!(error.to_string().contains("antes de decodificar"));
    }
    #[test]
    fn protocol_id_error_caps_and_request_identity_are_explicit() {
        let report = empty();
        let valid = serde_json::to_vec(&Response::SpaceMap { id: 1, report }).unwrap();
        assert!(decode(valid).is_ok());
        assert!(
            decode(
                serde_json::to_vec(&Response::SpaceMap {
                    id: 2,
                    report: empty()
                })
                .unwrap()
            )
            .is_err()
        );
        let error = decode(
            serde_json::to_vec(&Response::Error {
                id: 1,
                message: "漢".repeat(3000),
            })
            .unwrap(),
        )
        .unwrap_err();
        assert!(error.to_string().len() <= MAX_DIAGNOSTIC);
        assert!(request("relative".into()).is_err());
        assert!(request(PathBuf::from(format!("/{}", "a".repeat(4096)))).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let raw = PathBuf::from(std::ffi::OsString::from_vec(b"/raw\xff".to_vec()));
            let wire = request(raw.clone()).unwrap();
            let Request::SpaceMap { id, path } = serde_json::from_slice(&wire).unwrap() else {
                panic!("wrong command")
            };
            assert_eq!(id, 1);
            assert_eq!(path, raw);
        }
    }
    #[cfg(target_os = "linux")]
    mod linux {
        use super::*;
        use std::{fs, os::unix::fs::PermissionsExt, sync::atomic::AtomicU64};
        static PROCESS_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
        struct Script(PathBuf);
        impl Script {
            fn new(body: &str) -> Self {
                Self::with_prelude(body, "")
            }
            fn with_prelude(body: &str, prelude: &str) -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let base = files_core::theme::home().join(".cache/files-test");
                fs::create_dir_all(&base).unwrap();
                let path = base.join(format!(
                    "space-job-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::write(&path, format!("#!/usr/bin/python3\nimport os,sys,time\nfrom pathlib import Path\n{prelude}\nPath(__file__+'.pid').write_text(str(os.getpid()))\nassert sys.argv[1]=='space_map'\nsys.stdin.buffer.readline()\n{body}\n")).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                TEST_SCRIPT_PATHS.lock().unwrap().insert(path.clone());
                Self(path)
            }
            fn pid_path(&self) -> PathBuf {
                let mut path = self.0.as_os_str().to_os_string();
                path.push(".pid");
                path.into()
            }
            fn reaped(&self) {
                let pid = fs::read_to_string(self.pid_path()).unwrap();
                Self::assert_reaped(pid.trim().parse().unwrap());
            }
            fn assert_reaped(pid: u32) {
                match fs::read(format!("/proc/{pid}/stat")) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => panic!("Cannot establish child cleanup: {error}"),
                    Ok(_) => panic!("Child PID {pid} still exists, including zombie"),
                }
            }
            fn spawned_reaped(&self) {
                let pid = TEST_SPAWN_PID
                    .with(|slot| slot.get())
                    .expect("Rust must record the spawned child even before its script starts");
                match fs::read_to_string(self.pid_path()) {
                    Ok(marker) => assert_eq!(marker.trim(), pid.to_string()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => panic!("Cannot read child marker: {error}"),
                }
                Self::assert_reaped(pid);
            }
        }
        impl Drop for Script {
            fn drop(&mut self) {
                TEST_SCRIPT_PATHS.lock().unwrap().remove(&self.0);
                let _ = fs::remove_file(&self.0);
                let _ = fs::remove_file(self.pid_path());
            }
        }
        #[test]
        fn comparison_stream_budget_is_scoped_and_children_are_reaped() {
            let child = Script::new(
                "sys.stdout.buffer.write(b'x'*(4*1024*1024+1))\nsys.stdout.buffer.flush()",
            );
            let source = fs::read_to_string(&child.0).unwrap().replace(
                "sys.argv[1]=='space_map'",
                "sys.argv[1] in ('space_map','compare_directories','prepare_directory_sync')",
            );
            fs::write(&child.0, source).unwrap();
            for command in ["compare_directories", "prepare_directory_sync", "space_map"] {
                let mut bytes = 0;
                let result = run_helper_stream(
                    b"{}\n",
                    &AtomicBool::new(false),
                    child.0.clone(),
                    Duration::from_secs(15),
                    command,
                    |chunk| {
                        bytes += chunk.len();
                        Ok(())
                    },
                );
                if command != "space_map" {
                    assert!(result.is_ok(), "{result:?}");
                    assert_eq!(bytes, 4 * 1024 * 1024 + 1);
                } else {
                    assert!(result.is_err());
                    assert!(bytes <= MAX_RESPONSE);
                }
                child.spawned_reaped();
            }
        }
        #[test]
        fn comparison_cancel_ack_follows_actual_child_reap() {
            let child = Script::new("print('progress',flush=True)\ntime.sleep(60)");
            let source = fs::read_to_string(&child.0).unwrap().replace(
                "sys.argv[1]=='space_map'",
                "sys.argv[1]=='compare_directories'",
            );
            fs::write(&child.0, source).unwrap();
            let cancel = AtomicBool::new(false);
            let result = run_helper_stream(
                b"{}\n",
                &cancel,
                child.0.clone(),
                Duration::from_secs(15),
                "compare_directories",
                |_| {
                    cancel.store(true, Ordering::Release);
                    Ok(())
                },
            );
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
            child.spawned_reaped();
        }
        #[test]
        fn dialogue_keeps_stdin_and_excludes_consumer_idle_before_exact_control() {
            use std::cell::Cell;
            let child = Script::new(
                "print('first',flush=True)\nassert sys.stdin.buffer.readline()==b'next\\n'\nprint('last',flush=True)",
            );
            let seen = Cell::new(false);
            let sent = Cell::new(false);
            let waiting = Cell::new(None::<Instant>);
            let mut output = Vec::new();
            let mut control = || {
                if seen.get() && !sent.get() {
                    let start = waiting.get().unwrap_or_else(|| {
                        let now = Instant::now();
                        waiting.set(Some(now));
                        now
                    });
                    if start.elapsed() >= Duration::from_millis(1100) {
                        sent.set(true);
                        return Ok(Dialogue {
                            message: Some(b"next\n".to_vec()),
                            idle: false,
                        });
                    }
                    return Ok(Dialogue {
                        message: None,
                        idle: true,
                    });
                }
                Ok(Dialogue {
                    message: None,
                    idle: false,
                })
            };
            run_helper_dialogue(
                b"{}\n",
                &AtomicBool::new(false),
                child.0.clone(),
                Duration::from_secs(1),
                "space_map",
                |chunk| {
                    output.extend_from_slice(chunk);
                    if output.ends_with(b"first\n") {
                        seen.set(true);
                    }
                    Ok(())
                },
                Some(&mut control),
            )
            .unwrap();
            assert!(sent.get());
            assert_eq!(output, b"first\nlast\n");
            child.reaped();
        }
        #[test]
        fn dialogue_rejects_oversized_control_and_cancels_idle_before_reap_ack() {
            let child = Script::new("time.sleep(60)");
            let mut control = || {
                Ok(Dialogue {
                    message: Some(vec![b'x'; 257]),
                    idle: false,
                })
            };
            let error = run_helper_dialogue(
                b"{}\n",
                &AtomicBool::new(false),
                child.0.clone(),
                Duration::from_secs(5),
                "space_map",
                |_| Ok(()),
                Some(&mut control),
            )
            .expect_err("oversized control must fail after admission");
            assert_eq!(error.to_string(), "Control del helper fuera del límite256B");
            child.spawned_reaped();
            let child = Script::new("print('ready',flush=True)\nsys.stdin.buffer.readline()");
            let ready = std::cell::Cell::new(false);
            let cancel = AtomicBool::new(false);
            let mut control = || {
                if ready.get() {
                    cancel.store(true, Ordering::Release);
                }
                Ok(Dialogue {
                    message: None,
                    idle: ready.get(),
                })
            };
            let result = run_helper_dialogue(
                b"{}\n",
                &cancel,
                child.0.clone(),
                Duration::from_secs(5),
                "space_map",
                |_| {
                    ready.set(true);
                    Ok(())
                },
                Some(&mut control),
            );
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
            child.reaped();
        }
        #[test]
        fn dialogue_response_budget_is_per_admitted_turn_not_unbounded_per_read() {
            let child = Script::new(
                "sys.stdout.buffer.write(b'x'*(3*1024*1024));sys.stdout.flush()\nassert sys.stdin.buffer.readline()==b'next\\n'\nsys.stdout.buffer.write(b'y'*(3*1024*1024));sys.stdout.flush()",
            );
            let bytes = std::cell::Cell::new(0usize);
            let sent = std::cell::Cell::new(false);
            let mut control = || {
                let message = if bytes.get() >= 3 * 1024 * 1024
                    && !sent.replace(bytes.get() >= 3 * 1024 * 1024)
                {
                    Some(b"next\n".to_vec())
                } else {
                    None
                };
                Ok(Dialogue {
                    message,
                    idle: false,
                })
            };
            run_helper_dialogue(
                b"{}\n",
                &AtomicBool::new(false),
                child.0.clone(),
                Duration::from_secs(5),
                "space_map",
                |chunk| {
                    bytes.set(bytes.get() + chunk.len());
                    Ok(())
                },
                Some(&mut control),
            )
            .unwrap();
            assert_eq!(bytes.get(), 6 * 1024 * 1024);
            child.reaped();
        }
        #[test]
        fn dialogue_drains_bounded_error_output_without_a_second_reader() {
            let child = Script::new(
                "sys.stderr.write('source changed: '+('x'*20000));sys.stderr.flush();sys.exit(7)",
            );
            let mut control = || {
                Ok(Dialogue {
                    message: None,
                    idle: false,
                })
            };
            let error = run_helper_dialogue(
                b"{}\n",
                &AtomicBool::new(false),
                child.0.clone(),
                Duration::from_secs(5),
                "space_map",
                |_| Ok(()),
                Some(&mut control),
            )
            .unwrap_err();
            assert!(error.to_string().contains("source changed") && error.to_string().len() < 1024);
            child.reaped();
        }
        #[test]
        fn response_overflow_and_timeout_are_reaped_before_return() {
            let _serial = PROCESS_TEST.lock().unwrap();
            let large =
                Script::new("sys.stdout.buffer.write(b'x'*(4*1024*1024+1));sys.stdout.flush()");
            let error = run_program(
                b"{}\n",
                &AtomicBool::new(false),
                large.0.clone(),
                Duration::from_secs(5),
            )
            .unwrap_err();
            assert!(error.to_string().contains("superior a4MiB"));
            large.reaped();
            let slow = Script::new("time.sleep(60)");
            let error = run_program(
                b"{}\n",
                &AtomicBool::new(false),
                slow.0.clone(),
                Duration::from_millis(200),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            slow.spawned_reaped();
        }
        #[test]
        fn timeout_reaps_a_child_that_never_reaches_its_pid_marker() {
            let _serial = PROCESS_TEST.lock().unwrap();
            let slow = Script::with_prelude("", "time.sleep(60)");
            let error = run_program(
                b"{}\n",
                &AtomicBool::new(false),
                slow.0.clone(),
                Duration::from_millis(200),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(!slow.pid_path().exists());
            slow.spawned_reaped();
        }
        #[test]
        fn script_interpreter_registration_survives_worker_thread_handoff() {
            let script = Script::new("pass");
            let path = script.0.clone();
            assert!(
                std::thread::spawn(move || test_script_registered(&path))
                    .join()
                    .unwrap()
            );
            let path = script.0.clone();
            drop(script);
            assert!(!test_script_registered(&path));
        }
        #[test]
        fn cancellation_reaps_one_direct_helper() {
            let _serial = PROCESS_TEST.lock().unwrap();
            let slow = Script::new("time.sleep(60)");
            let pid = slow.pid_path();
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&cancel);
            let executable = slow.0.clone();
            let worker = std::thread::spawn(move || {
                run_program(b"{}\n", &flag, executable, Duration::from_secs(5))
            });
            let deadline = Instant::now() + Duration::from_secs(2);
            while !pid.exists() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Release);
            assert!(worker.join().unwrap().is_err());
            slow.reaped();
        }
    }
}
