//! Supervised fresh read-only sync review, retained by the comparison panel.
//! Results are proposals; Apply and recovery are still separate pending stages.
use files_core::{
    directory_compare::Progress,
    directory_sync::Review,
    directory_sync_wire::{self as wire, Collector, Frame},
};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
pub struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    result: Receiver<Result<Review, String>>,
}
struct Decoder {
    ledger: Collector,
    line: Vec<u8>,
    bytes: usize,
    poisoned: bool,
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Stream de revisión de sincronización inválido, incompleto o fuera del límite",
    )
}
impl Decoder {
    fn new(intent: &wire::Intent) -> io::Result<Self> {
        Ok(Self {
            ledger: Collector::new(intent.clone())?,
            line: Vec::with_capacity(1024),
            bytes: 0,
            poisoned: false,
        })
    }
    fn feed(&mut self, bytes: &[u8], published: &Mutex<Progress>) -> io::Result<()> {
        if self.poisoned {
            return Err(bad());
        }
        let result = self.feed_inner(bytes, published);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn feed_inner(&mut self, bytes: &[u8], published: &Mutex<Progress>) -> io::Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|&n| n <= wire::MAX_STREAM)
            .ok_or_else(bad)?;
        for &byte in bytes {
            if byte != b'\n' {
                if self.line.len() >= wire::MAX_FRAME {
                    return Err(bad());
                }
                self.line.push(byte);
                continue;
            }
            let frame: Frame = serde_json::from_slice(&self.line).map_err(|_| bad())?;
            self.line.clear();
            if let Some(p) = self.ledger.accept(frame)? {
                *published.lock().unwrap_or_else(|e| e.into_inner()) = p;
            }
        }
        Ok(())
    }
    fn finish(self) -> Result<Review, String> {
        if self.poisoned || !self.line.is_empty() {
            return Err(bad().to_string());
        }
        self.ledger.finish()
    }
}
fn outcome(
    request: &[u8],
    cancel: &AtomicBool,
    executable: PathBuf,
    timeout: Duration,
    mut decoder: Decoder,
    published: &Mutex<Progress>,
) -> Result<Review, String> {
    let result = crate::space_map_job::run_helper_stream(
        request,
        cancel,
        executable,
        timeout,
        "prepare_directory_sync",
        |bytes| decoder.feed(bytes, published),
    );
    if cancel.load(Ordering::Acquire) {
        return Err("Revisión de sincronización cancelada; sin informe final".into());
    }
    result.map_err(|e| e.to_string())?;
    decoder.finish()
}
impl Job {
    pub fn start(intent: wire::Intent) -> Result<Self, String> {
        intent.validate().map_err(|e| e.to_string())?;
        let decoder = Decoder::new(&intent).map_err(|e| e.to_string())?;
        let mut request =
            serde_json::to_vec(&files_core::protocol::Request::PrepareDirectorySync { intent })
                .map_err(|e| e.to_string())?;
        request.push(b'\n');
        if request.len() > files_core::protocol::MAX_LINE_BYTES {
            return Err("Petición de revisión de sincronización fuera del límite".into());
        }
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("files-helper");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let progress = Arc::new(Mutex::new(Progress::default()));
        let published = progress.clone();
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("files-sync-review".into())
            .spawn(move || {
                let report = outcome(
                    &request,
                    &flag,
                    executable,
                    Duration::from_secs(120),
                    decoder,
                    &published,
                )
                .map_err(|mut e| {
                    if e.len() > 1024 {
                        let mut end = 1024;
                        while !e.is_char_boundary(end) {
                            end -= 1;
                        }
                        e.truncate(end);
                    }
                    e
                });
                let _ = tx.send(report);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cancel,
            progress,
            result,
        })
    }
    #[cfg(test)]
    pub(crate) fn pending() -> (Self, mpsc::SyncSender<Result<Review, String>>) {
        let (tx, result) = mpsc::sync_channel(1);
        (
            Self {
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(Mutex::new(Progress::default())),
                result,
            },
            tx,
        )
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn progress(&self) -> Progress {
        *self.progress.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn poll(&self) -> Option<Result<Review, String>> {
        match self.result.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "El lector de revisión de sincronización terminó sin ACK".into(),
            )),
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod lifecycle_tests {
    use super::*;
    use std::{fs, sync::atomic::AtomicU64};
    struct Program {
        path: PathBuf,
        _registration: crate::space_map_job::TestScriptRegistration,
    }
    impl Program {
        fn new(tail: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let base = files_core::theme::home().join(".cache/files-test");
            fs::create_dir_all(&base).unwrap();
            let path = base.join(format!(
                "sync-job-{}-{}.py",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let bytes = super::tests::frames();
            fs::write(&path,format!("import os,sys,time\nfrom pathlib import Path\nassert sys.argv[1]=='prepare_directory_sync'\nsys.stdin.buffer.readline()\nPath(__file__+'.pid').write_text(str(os.getpid())+' '+Path('/proc/self/stat').read_text().rsplit(')',1)[1].split()[19])\nsys.stdout.buffer.write(bytes({bytes:?}))\nsys.stdout.buffer.flush()\nPath(__file__+'.sent').write_text('sent')\n{tail}\n")).unwrap();
            let registration = crate::space_map_job::TestScriptRegistration::new(path.clone());
            Self {
                path,
                _registration: registration,
            }
        }
        fn assert_reaped(&self) {
            let marker = fs::read_to_string(self.path.with_extension("py.pid")).unwrap();
            let mut fields = marker.split_whitespace();
            let pid: u32 = fields.next().unwrap().parse().unwrap();
            let start = fields.next().unwrap();
            match fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(stat) => assert_ne!(
                    stat.rsplit(')')
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(19)
                        .unwrap(),
                    start,
                    "child identity still exists, including zombie"
                ),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => panic!("Cannot establish child cleanup: {e}"),
            }
        }
        fn run(&self, timeout: Duration) -> Result<Review, String> {
            outcome(
                b"{}\n",
                &AtomicBool::new(false),
                self.path.clone(),
                timeout,
                Decoder::new(&super::tests::intent()).unwrap(),
                &Mutex::new(Progress::default()),
            )
        }
    }
    impl Drop for Program {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("py.pid"));
            let _ = fs::remove_file(self.path.with_extension("py.sent"));
        }
    }
    #[test]
    fn terminal_frame_requires_successful_process_exit_and_reap() {
        let p = Program::new("sys.exit(0)");
        assert!(p.run(Duration::from_secs(15)).is_ok());
        p.assert_reaped();
        let p = Program::new("sys.exit(9)");
        assert!(p.run(Duration::from_secs(15)).is_err());
        p.assert_reaped();
    }
    #[test]
    fn cancellation_after_terminal_still_reaps_and_cannot_publish() {
        let p = Program::new("time.sleep(60)");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let sent = p.path.with_extension("py.sent");
        let waiter = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !sent.exists() {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            flag.store(true, Ordering::Release);
        });
        let r = outcome(
            b"{}\n",
            &cancel,
            p.path.clone(),
            Duration::from_secs(15),
            Decoder::new(&super::tests::intent()).unwrap(),
            &Mutex::new(Progress::default()),
        );
        waiter.join().unwrap();
        assert!(r.is_err());
        p.assert_reaped();
    }
    #[test]
    fn eof_and_valid_terminal_do_not_bypass_exit_ack() {
        let p = Program::new("os.close(1)\ntime.sleep(60)");
        assert!(p.run(Duration::from_secs(5)).is_err());
        assert!(p.path.with_extension("py.sent").exists());
        p.assert_reaped();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::{
        directory_compare::{Difference, Kind, Row},
        directory_compare_wire::{self as comparison, Event},
        directory_sync::{Mode, Proposal, Side},
        relative_source::DirectoryIdentity,
    };
    pub(super) fn intent() -> wire::Intent {
        wire::Intent {
            version: wire::VERSION,
            id: 7,
            left: "/left".into(),
            right: "/right".into(),
            expected: Some([
                DirectoryIdentity {
                    device: 1,
                    inode: 2,
                },
                DirectoryIdentity {
                    device: 1,
                    inode: 3,
                },
            ]),
            mode: Mode::Mirror(Side::Left),
            read_limit: 0,
        }
    }
    pub(super) fn frames() -> Vec<u8> {
        let i = intent();
        let mut bytes = Vec::new();
        for event in [
            Event::Page {
                offset: 0,
                rows: vec![Row {
                    path: "á-file".into(),
                    left: Some(Kind::File),
                    right: Some(Kind::File),
                    difference: Difference::Equal,
                    metadata_differs: false,
                }],
            },
            Event::Done {
                roots: i.expected.unwrap(),
                progress: Progress {
                    phase: 3,
                    entries: [1, 1],
                    compared: 1,
                    equal: 1,
                    ..Progress::default()
                },
            },
        ] {
            serde_json::to_writer(
                &mut bytes,
                &Frame {
                    version: wire::VERSION,
                    mode: i.mode,
                    observation: comparison::Frame { id: i.id, event },
                },
            )
            .unwrap();
            bytes.push(b'\n');
        }
        bytes
    }
    #[test]
    fn fragmented_utf8_frames_produce_only_locally_derived_pending_review() {
        let mut d = Decoder::new(&intent()).unwrap();
        let p = Mutex::new(Progress::default());
        for b in frames() {
            d.feed(&[b], &p).unwrap();
        }
        assert_eq!(*p.lock().unwrap(), Progress::default());
        let r = d.finish().unwrap();
        assert_eq!(r.entries()[0].observation().path, PathBuf::from("á-file"));
        assert_eq!(r.entries()[0].proposal(), Proposal::Unchanged);
        assert_eq!(r.mode(), intent().mode);
    }
    #[test]
    fn malformed_trailing_stale_mode_version_and_unterminated_streams_are_rejected() {
        for suffix in [b" ".as_slice(), b"\n", b"{}\n"] {
            let mut d = Decoder::new(&intent()).unwrap();
            let mut b = frames();
            b.extend_from_slice(suffix);
            let _ = d.feed(&b, &Mutex::new(Progress::default()));
            assert!(d.finish().is_err());
        }
        for case in 0..3 {
            let mut i = intent();
            match case {
                0 => i.id += 1,
                1 => i.mode = Mode::Merge,
                _ => i.expected.as_mut().unwrap()[1].inode += 1,
            };
            let mut d = Decoder::new(&i).unwrap();
            assert!(d.feed(&frames(), &Mutex::new(Progress::default())).is_err());
            assert!(d.finish().is_err());
        }
        let mut d = Decoder::new(&intent()).unwrap();
        let mut b = frames();
        b.pop();
        d.feed(&b, &Mutex::new(Progress::default())).unwrap();
        assert!(d.finish().is_err());
    }
    #[test]
    fn byte_caps_poison_an_already_terminal_stream_before_further_allocation() {
        let mut d = Decoder::new(&intent()).unwrap();
        let p = Mutex::new(Progress::default());
        d.feed(&frames(), &p).unwrap();
        let capacity = d.line.capacity();
        d.bytes = wire::MAX_STREAM;
        assert!(d.feed(b"x", &p).is_err());
        assert_eq!(d.line.capacity(), capacity);
        assert!(d.finish().is_err());
        let mut d = Decoder::new(&intent()).unwrap();
        assert!(d.feed(&vec![b'x'; wire::MAX_FRAME + 1], &p).is_err());
        assert!(d.line.capacity() <= wire::MAX_FRAME);
        assert!(d.finish().is_err());
    }
    #[test]
    fn invalid_intents_reject_before_worker_or_executable_lookup() {
        for case in 0..3 {
            let mut i = intent();
            match case {
                0 => i.left = "relative".into(),
                1 => i.version = 2,
                _ => i.read_limit = files_core::directory_compare::MAX_READ + 1,
            };
            assert!(Job::start(i).is_err());
        }
    }
}
