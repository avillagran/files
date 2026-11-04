//! Bounded text preview process supervisor. The owning UI keeps at most one Job,
//! cancels it on navigation/close, and drains its result before starting another.
//! A result is sent only after the direct helper process has been reaped.
use files_core::protocol::{Request, Response};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
// UTF-16 input can expand to three UTF-8 bytes for each two source bytes.
const MAX_TEXT_BYTES: usize = 384 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) type HeaderValidator = fn(&[u8]) -> io::Result<usize>;

#[derive(Debug, PartialEq, Eq)]
pub struct TextPreview {
    pub text: String,
    pub encoding: String,
    pub truncated: bool,
    pub relative_source: Option<files_core::relative_source::Proof>,
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<TextPreview, String>>,
}
impl Job {
    pub fn start(path: PathBuf) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let value = run(path, &flag).map_err(|e| e.to_string());
            let _ = tx.send(value);
        });
        Self { cancel, result }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&self) -> Option<Result<TextPreview, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("La vista previa terminó sin respuesta".into()))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn check(cancel: &AtomicBool, started: Instant, timeout: Duration) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Vista previa cancelada",
        ))
    } else if started.elapsed() >= timeout {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "La vista previa agotó su tiempo",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn decode(output: &[u8]) -> io::Result<TextPreview> {
    match serde_json::from_slice::<Response>(output)? {
        Response::Text {
            id: 1,
            text,
            encoding,
            truncated,
            relative_source,
        } => {
            if text.len() > MAX_TEXT_BYTES
                || !matches!(encoding.as_str(), "UTF-8" | "UTF-16LE" | "UTF-16BE")
            {
                return Err(io::Error::other(
                    "Vista previa fuera de los límites del protocolo",
                ));
            }
            Ok(TextPreview {
                text,
                encoding,
                truncated,
                relative_source,
            })
        }
        Response::Error { id: 1, message } => Err(io::Error::other(message)),
        _ => Err(io::Error::other("Respuesta de vista previa inesperada")),
    }
}
#[cfg(target_os = "linux")]
pub(crate) fn run(path: PathBuf, cancel: &AtomicBool) -> io::Result<TextPreview> {
    run_program(
        path,
        cancel,
        std::env::current_exe()?.with_file_name("files-helper"),
        TIMEOUT,
    )
}
#[cfg(target_os = "linux")]
fn run_program(
    path: PathBuf,
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
) -> io::Result<TextPreview> {
    let output = run_request_program(
        Request::PreviewText { id: 1, path },
        "preview_text",
        cancel,
        executable,
        timeout,
        MAX_RESPONSE_BYTES,
        None,
    )?;
    decode(&output)
}
/// Shared supervisor: one bounded response, timeout/cancel checks per chunk, and
/// direct-child reap before returning. Text and bitmap jobs share this contract.
#[cfg(target_os = "linux")]
pub(crate) fn run_request_program(
    request: Request,
    command: &str,
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
    response_limit: usize,
    header: Option<HeaderValidator>,
) -> io::Result<Vec<u8>> {
    let mut request = serde_json::to_vec(&request)?;
    request.push(b'\n');
    if request.len() > 4096 {
        return Err(io::Error::other(
            "Ruta demasiado larga para la vista previa",
        ));
    }
    let mut program = Command::new(executable);
    program.arg(command);
    run_command_program(
        program,
        &request,
        command,
        cancel,
        timeout,
        response_limit,
        header,
    )
}
#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
pub(crate) struct ProgramObservation<'a> {
    pub started: &'a std::sync::Mutex<Option<Instant>>,
    /// For players whose exit0 can accompany a device/decoder failure. Use an
    /// error-only loglevel; any stderr is conservatively treated as failure.
    pub strict_errors: bool,
}
#[cfg(target_os = "linux")]
fn bounded_diagnostic(bytes: &[u8], truncated: bool) -> String {
    let mut text = String::new();
    for ch in String::from_utf8_lossy(bytes).chars() {
        let ch = if ch.is_control() || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        {
            ' '
        } else {
            ch
        };
        if text.len() + ch.len_utf8() > 4096 {
            break;
        }
        text.push(ch);
    }
    if truncated || bytes.len() > 4096 {
        text.push_str(" [diagnóstico truncado]");
    }
    text
}
/// The native HTML adapter uses an argv-only request. Keep its pipe ownership,
/// monotonic timeout/cancel and reap contract identical to text/bitmap helpers.
#[cfg(target_os = "linux")]
pub(crate) fn run_command_program(
    program: Command,
    request: &[u8],
    command: &str,
    cancel: &AtomicBool,
    timeout: Duration,
    response_limit: usize,
    header: Option<HeaderValidator>,
) -> io::Result<Vec<u8>> {
    run_command_program_observed(
        program,
        request,
        command,
        cancel,
        timeout,
        response_limit,
        header,
        None,
    )
}
#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)] // Existing bounded supervisor plus optional child-start clock.
pub(crate) fn run_command_program_observed(
    mut program: Command,
    request: &[u8],
    command: &str,
    cancel: &AtomicBool,
    timeout: Duration,
    response_limit: usize,
    header: Option<HeaderValidator>,
    observation: Option<ProgramObservation<'_>>,
) -> io::Result<Vec<u8>> {
    use std::os::fd::AsRawFd;
    if request.len() > 4096 {
        return Err(io::Error::other("Petición de preview demasiado grande"));
    }
    let started = Instant::now();
    check(cancel, started, timeout)?;
    let capture_errors = observation.is_some_and(|o| o.strict_errors);
    crate::owned_child::arm(&mut program);
    let mut child = program
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(if capture_errors {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()?;
    if let Some(observation) = observation
        && let Ok(mut started) = observation.started.lock()
    {
        *started = Some(Instant::now());
    }
    if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", child.id())).ok();
        let start = stat
            .as_deref()
            .and_then(|s| s.rsplit_once(')'))
            .and_then(|(_, s)| s.split_whitespace().nth(19))
            .unwrap_or("unknown");
        eprintln!(
            "FILES_PREVIEW_HELPER pid={} start={} command={command}",
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
        let mut diagnostic = Vec::new();
        if capture_errors {
            diagnostic
                .try_reserve_exact(8192)
                .map_err(|_| io::Error::other("Sin memoria para diagnóstico de audio"))?;
        }
        let mut diagnostic_truncated = false;
        let mut stderr = child.stderr.take();
        if let Some(stderr) = &stderr {
            let fd = stderr.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let mut diagnostic_eof = stderr.is_none();
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("Sin entrada del helper"))?;
        // Fresh empty pipe, one request <= PIPE_BUF: no dependency on helper reads.
        stdin.write_all(request)?;
        drop(stdin);
        let mut output = WireBuffer::new(response_limit, header);
        let mut exited = None;
        loop {
            check(cancel, started, timeout)?;
            let mut chunk = [0u8; 8192];
            let eof = loop {
                // Continuous output must not starve cancellation or timeout checks.
                check(cancel, started, timeout)?;
                match stdout.read(&mut chunk) {
                    Ok(0) => break true,
                    Ok(n) => {
                        output.push(&chunk[..n])?;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break false,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            };
            if let Some(stderr) = &mut stderr {
                for _ in 0..16 {
                    if diagnostic_eof {
                        break;
                    }
                    check(cancel, started, timeout)?;
                    match stderr.read(&mut chunk) {
                        Ok(0) => {
                            diagnostic_eof = true;
                            break;
                        }
                        Ok(n) => {
                            let keep = n.min(8192 - diagnostic.len());
                            diagnostic.extend_from_slice(&chunk[..keep]);
                            diagnostic_truncated |= keep < n;
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e),
                    }
                }
            }
            if exited.is_none() {
                exited = child.try_wait()?;
            }
            if let Some(status) = exited
                && eof
                && diagnostic_eof
            {
                if !status.success() || (capture_errors && !diagnostic.is_empty()) {
                    return Err(io::Error::other(if diagnostic.is_empty() {
                        "El helper de vista previa terminó con error".into()
                    } else {
                        bounded_diagnostic(&diagnostic, diagnostic_truncated)
                    }));
                }
                return output.finish();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // These adapters never launch shells or nested helpers. The owned direct
    // process is always collected; the caller never waits for it on the UI thread.
    let _ = child.kill();
    child.wait()?;
    result
}
/// Binary responses validate their small JSON header BEFORE retaining RGB.
/// A malicious helper can never use dimensions to request an unbounded reserve.
struct WireBuffer {
    bytes: Vec<u8>,
    limit: usize,
    header: Option<HeaderValidator>,
    expected: Option<usize>,
}
impl WireBuffer {
    fn new(limit: usize, header: Option<HeaderValidator>) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            header,
            expected: None,
        }
    }
    fn append(&mut self, bytes: &[u8], cap: usize) -> io::Result<()> {
        let needed = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= cap)
            .ok_or_else(|| io::Error::other("Respuesta del helper demasiado grande"))?;
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().max(256).saturating_mul(2))
                .min(cap);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn push(&mut self, bytes: &[u8]) -> io::Result<()> {
        if let Some(validate) = self.header
            && self.expected.is_none()
        {
            let split = bytes.iter().position(|b| *b == b'\n');
            let prefix = split.map_or(bytes.len(), |n| n + 1);
            self.append(&bytes[..prefix], 4096.min(self.limit))?;
            if split.is_some() {
                let body = validate(&self.bytes[..self.bytes.len() - 1])?;
                let total = self
                    .bytes
                    .len()
                    .checked_add(body)
                    .filter(|n| *n <= self.limit)
                    .ok_or_else(|| io::Error::other("Payload de imagen fuera del presupuesto"))?;
                self.expected = Some(total);
                self.append(&bytes[prefix..], total)?;
            }
            return Ok(());
        }
        self.append(bytes, self.expected.unwrap_or(self.limit))
    }
    fn finish(self) -> io::Result<Vec<u8>> {
        if self.header.is_some() && self.expected != Some(self.bytes.len()) {
            return Err(io::Error::other("Respuesta binaria incompleta"));
        }
        Ok(self.bytes)
    }
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn run(_: PathBuf, _: &AtomicBool) -> io::Result<TextPreview> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Vista previa segura aún no disponible en esta plataforma",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, sync::atomic::AtomicU64};
    // Serializing script creation/spawn avoids another test forking while a
    // fixture executable is briefly open for writing (ETXTBSY).
    static PROCESS_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct Fixture {
        root: PathBuf,
        script: PathBuf,
    }
    impl Fixture {
        fn new(body: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = files_core::theme::home().join(".cache/files-test");
            fs::create_dir_all(&base).unwrap();
            let root = base.join(format!(
                "files-text-job-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let script = root.join("helper");
            fs::write(&script, format!("#!/usr/bin/python3\nimport sys, os, json, time\nfrom pathlib import Path\nPath(__file__).with_name('pid').write_text(str(os.getpid()))\nrequest=json.loads(sys.stdin.readline())\nassert sys.argv[1] == 'preview_text'\nassert request['command'] == 'preview_text'\n{body}\n")).unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
            Self { root, script }
        }
        fn reaped(&self) {
            let pid = fs::read_to_string(self.root.join("pid")).unwrap();
            assert!(
                !PathBuf::from(format!("/proc/{pid}")).exists(),
                "helper not reaped"
            );
        }
        fn run(&self, timeout: Duration) -> io::Result<TextPreview> {
            run_program(
                "/fixture.txt".into(),
                &AtomicBool::new(false),
                self.script.clone(),
                timeout,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn observed_player_rejects_exit_zero_stderr_and_bounds_flood_before_reap() {
        let _guard = PROCESS_TEST.lock().unwrap();
        for body in [
            "os.write(2,b'audio open failed'); os._exit(0)",
            "os.write(2,b'\\xff\\0'*65536);os._exit(0)",
        ] {
            let fixture = Fixture::new(body);
            let mut cmd = Command::new(&fixture.script);
            cmd.arg("preview_text");
            let signal = std::sync::Mutex::new(None);
            let error = run_command_program_observed(
                cmd,
                b"{\"id\":1,\"command\":\"preview_text\"}\n",
                "fixture_player",
                &AtomicBool::new(false),
                Duration::from_secs(3),
                128,
                None,
                Some(ProgramObservation {
                    started: &signal,
                    strict_errors: true,
                }),
            )
            .unwrap_err();
            assert!(signal.lock().unwrap().is_some());
            assert!(error.to_string().len() < 4200);
            fixture.reaped();
        }
        let signal = std::sync::Mutex::new(None);
        assert!(
            run_command_program_observed(
                Command::new("/nonexistent/files-player"),
                b"",
                "fixture_player",
                &AtomicBool::new(false),
                Duration::from_secs(1),
                128,
                None,
                Some(ProgramObservation {
                    started: &signal,
                    strict_errors: true
                })
            )
            .is_err()
        );
        assert!(
            signal.lock().unwrap().is_none(),
            "failed spawn is not playback start"
        );
    }
    #[test]
    fn framed_wire_validates_header_before_body_and_requires_exact_eof() {
        fn header(bytes: &[u8]) -> io::Result<usize> {
            if bytes == b"rgb" {
                Ok(3)
            } else {
                Err(io::Error::other("bad header"))
            }
        }
        let mut wire = WireBuffer::new(8192, Some(header));
        assert!(wire.push(b"wrong\n123456789").is_err());
        assert_eq!(wire.bytes, b"wrong\n"); // body never retained
        let mut wire = WireBuffer::new(8192, Some(header));
        wire.push(b"r").unwrap();
        wire.push(b"gb\n12").unwrap();
        assert!(wire.finish().is_err());
        let mut wire = WireBuffer::new(8192, Some(header));
        wire.push(b"rgb\n123").unwrap();
        assert!(wire.push(b"4").is_err());
        assert_eq!(wire.finish().unwrap(), b"rgb\n123");
        let mut wire = WireBuffer::new(8192, Some(header));
        assert!(wire.push(&vec![b'x'; 4097]).is_err());
        assert!(wire.bytes.is_empty());
    }
    #[test]
    fn split_header_every_byte_and_newline_at_boundary() {
        fn header(bytes: &[u8]) -> io::Result<usize> {
            if bytes == vec![b'x'; 4095] {
                Ok(3)
            } else {
                Err(io::Error::other("bad"))
            }
        }
        let mut wire = WireBuffer::new(8192, Some(header));
        for _ in 0..4095 {
            wire.push(b"x").unwrap();
        }
        wire.push(b"\n").unwrap();
        assert_eq!(wire.expected, Some(4099));
        for byte in [1, 2, 3] {
            wire.push(&[byte]).unwrap();
        }
        assert_eq!(wire.finish().unwrap().len(), 4099);
    }
    #[test]
    fn cancellation_during_partial_binary_payload_reaps_direct_helper() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let fixture = Fixture::new(
            "os.write(1,b'rgb\\n123'); Path(__file__).with_name('rgb-started').touch(); time.sleep(10)",
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let script = fixture.script.clone();
        let handle = std::thread::spawn(move || {
            run_request_program(
                Request::PreviewText {
                    id: 1,
                    path: "/fixture".into(),
                },
                "preview_text",
                &flag,
                script,
                Duration::from_secs(3),
                1024 * 1024,
                Some(|header| {
                    if header == b"rgb" {
                        Ok(100000)
                    } else {
                        Err(io::Error::other("bad header"))
                    }
                }),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !fixture.root.join("rgb-started").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        cancel.store(true, Ordering::Release);
        assert_eq!(
            handle.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        fixture.reaped();
    }
    #[test]
    fn reads_pipe_sized_text_and_checks_protocol_identity() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let script = Fixture::new(
            "print(json.dumps(dict(status='text', id=request['id'], text='á'*40000, encoding='UTF-8', truncated=True)))",
        );
        let result = script.run(Duration::from_secs(3)).unwrap();
        assert_eq!(result.text, "á".repeat(40000));
        assert_eq!(result.encoding, "UTF-8");
        assert!(result.truncated);
        script.reaped();
        let stale = Fixture::new(
            "print(json.dumps(dict(status='text', id=2, text='stale', encoding='UTF-8', truncated=False)))",
        );
        assert!(stale.run(Duration::from_secs(3)).is_err());
        stale.reaped();
    }
    #[test]
    fn drains_output_after_child_exits() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let script = Fixture::new(
            "os.write(1, b'{\"status\":\"text\",\"id\":1,\"text\":\"last bytes\",\"encoding\":\"UTF-8\",\"truncated\":false}'); os._exit(0)",
        );
        assert_eq!(
            script.run(Duration::from_secs(3)).unwrap().text,
            "last bytes"
        );
        script.reaped();
    }
    #[test]
    fn oversized_stream_is_rejected_and_reaped() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let script = Fixture::new("while True: os.write(1, b'x'*8192)");
        let error = script.run(Duration::from_secs(3)).unwrap_err();
        assert!(error.to_string().contains("demasiado grande"));
        script.reaped();
    }
    #[test]
    fn timeout_and_cancellation_reap_the_helper() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let slow = Fixture::new("time.sleep(60)");
        assert_eq!(
            slow.run(Duration::from_secs(1)).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        slow.reaped();
        let flag = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&flag);
        let script = slow.script.clone();
        let handle = std::thread::spawn(move || {
            run_program("/fixture".into(), &cancel, script, Duration::from_secs(3))
        });
        std::thread::sleep(Duration::from_millis(100));
        flag.store(true, Ordering::Release);
        assert_eq!(
            handle.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        slow.reaped();
    }
    #[test]
    fn rejects_wrong_type_encoding_and_decoded_size() {
        for output in [
            r#"{"status":"usage","id":1,"bytes":4,"entries":1}"#,
            r#"{"status":"text","id":1,"text":"x","encoding":"guess","truncated":false}"#,
            r#"{"status":"text","id":1,"text":"x","encoding":"UTF-8","truncated":false} garbage"#,
        ] {
            assert!(decode(output.as_bytes()).is_err());
        }
        let output = serde_json::to_vec(&Response::Text {
            id: 1,
            text: "x".repeat(MAX_TEXT_BYTES + 1),
            encoding: "UTF-8".into(),
            truncated: false,
            relative_source: None,
        })
        .unwrap();
        assert!(decode(&output).is_err());
    }
    #[test]
    fn pre_cancelled_job_does_not_spawn() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let script = Fixture::new("raise Exception('must not start')");
        assert_eq!(
            run_program(
                "/fixture".into(),
                &AtomicBool::new(true),
                script.script.clone(),
                TIMEOUT
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!script.root.join("pid").exists());
    }
}
