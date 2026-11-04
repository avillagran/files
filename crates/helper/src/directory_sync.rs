//! One fresh read-only synchronization review per helper; parent owns kill/reap.
use files_core::{
    directory_compare::Limits,
    directory_compare_wire::{self as observation, Event},
    directory_sync as sync, directory_sync_wire as wire,
};
use std::{
    io::{self, Write},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
struct Output<W> {
    writer: W,
    bytes: usize,
    mode: sync::Mode,
}
impl<W: Write> Output<W> {
    fn send(&mut self, id: u64, event: Event) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(&wire::Frame {
            version: wire::VERSION,
            mode: self.mode,
            observation: observation::Frame { id, event },
        })?;
        if bytes.len() > wire::MAX_FRAME {
            return Err(io::Error::other(
                "Frame de revisión de sincronización fuera del límite",
            ));
        }
        bytes.push(b'\n');
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|&n| n <= wire::MAX_STREAM)
            .ok_or_else(|| {
                io::Error::other("Stream de revisión de sincronización fuera del límite")
            })?;
        self.writer.write_all(&bytes)?;
        self.writer.flush()
    }
}
pub fn run(intent: &wire::Intent, writer: impl Write) -> io::Result<()> {
    intent.validate()?;
    let id = intent.id;
    let mut out = Output {
        writer,
        bytes: 0,
        mode: intent.mode,
    };
    let mut updates = 0;
    let mut phase = u8::MAX;
    let mut last = Instant::now();
    let mut sink_failed = false;
    let report = sync::prepare(
        [&intent.left, &intent.right],
        intent.expected,
        intent.mode,
        Limits {
            read: intent.read_limit,
            ..Limits::default()
        },
        &AtomicBool::new(false),
        |p| {
            if p.phase < 3
                && updates < observation::MAX_UPDATES
                && (p.phase != phase || last.elapsed() >= Duration::from_millis(250))
            {
                updates += 1;
                phase = p.phase;
                last = Instant::now();
                if let Err(e) = out.send(id, Event::Progress { progress: p }) {
                    sink_failed = true;
                    return Err(e);
                }
            }
            Ok(())
        },
    );
    match report {
        Ok(review) => {
            let report = review.into_observation();
            observation::validate(&report)?;
            let mut offset = 0;
            while offset < report.rows.len() {
                let end = observation::page_end(&report.rows, offset);
                if end == offset {
                    return Err(io::Error::other(
                        "Página de revisión de sincronización sin avance",
                    ));
                }
                out.send(
                    id,
                    Event::Page {
                        offset,
                        rows: report.rows[offset..end].to_vec(),
                    },
                )?;
                offset = end;
            }
            out.send(
                id,
                Event::Done {
                    roots: report.roots,
                    progress: report.progress,
                },
            )
        }
        Err(error) => {
            if sink_failed {
                return Err(error);
            }
            let mut message = String::new();
            for c in error
                .to_string()
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
            {
                if message.len() + c.len_utf8() > 256 {
                    break;
                }
                message.push(c);
            }
            if message.is_empty() {
                message.push_str("Error de revisión de sincronización");
            }
            out.send(id, Event::Error { message })
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::directory_sync::{Mode, Proposal, Side};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "files-sync-helper-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(p.join("a")).unwrap();
            fs::create_dir(p.join("b")).unwrap();
            Self(p)
        }
        fn intent(&self, mode: Mode) -> wire::Intent {
            wire::Intent {
                version: wire::VERSION,
                id: 7,
                left: self.0.join("a"),
                right: self.0.join("b"),
                expected: None,
                mode,
                read_limit: 1024 * 1024,
            }
        }
        fn read(&self, intent: wire::Intent) -> sync::Review {
            let mut bytes = Vec::new();
            run(&intent, &mut bytes).unwrap();
            let mut c = wire::Collector::new(intent).unwrap();
            assert!(bytes.len() <= wire::MAX_STREAM);
            for line in bytes.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
                assert!(line.len() <= wire::MAX_FRAME);
                c.accept(serde_json::from_slice(line).unwrap()).unwrap();
            }
            c.finish().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn fresh_helper_modes_and_raw_paged_namespace_roundtrip_without_modification() {
        use std::os::unix::ffi::OsStringExt;
        let f = Fixture::new();
        let raw = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        fs::write(f.0.join("a").join(&raw), b"raw").unwrap();
        fs::write(f.0.join("b/extra"), b"extra").unwrap();
        for n in 0..140 {
            fs::write(f.0.join(format!("a/file-{n:03}")), b"x").unwrap();
        }
        for mode in [
            Mode::Merge,
            Mode::Update(Side::Left),
            Mode::Update(Side::Right),
            Mode::Mirror(Side::Left),
            Mode::Mirror(Side::Right),
        ] {
            let r = f.read(f.intent(mode));
            assert_eq!(r.mode(), mode);
            assert_eq!(r.entries().len(), 142);
            assert!(
                r.entries()
                    .iter()
                    .any(|e| e.observation().path.as_os_str() == raw)
            );
            assert!(!r.blocked());
        }
        assert_eq!(fs::read(f.0.join("a").join(&raw)).unwrap(), b"raw");
        assert_eq!(fs::read_dir(f.0.join("b")).unwrap().count(), 1);
    }
    #[test]
    fn read_limit_and_foreign_root_error_never_create_actionable_review() {
        let f = Fixture::new();
        for side in ["a", "b"] {
            fs::write(f.0.join(side).join("file"), b"same").unwrap();
        }
        let mut i = f.intent(Mode::Mirror(Side::Left));
        i.read_limit = 0;
        let r = f.read(i.clone());
        assert!(r.blocked());
        assert!(
            r.entries()
                .iter()
                .all(|e| e.proposal() == Proposal::Blocked)
        );
        i.left = f.0.join("missing");
        let mut bytes = Vec::new();
        run(&i, &mut bytes).unwrap();
        let mut c = wire::Collector::new(i).unwrap();
        for line in bytes.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
            c.accept(serde_json::from_slice(line).unwrap()).unwrap();
        }
        assert!(c.finish().is_err());
    }
    #[test]
    fn broken_output_stops_without_retrying_a_terminal_error_on_failed_sink() {
        struct Failed(usize);
        impl Write for Failed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.0 += 1;
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "sink"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let f = Fixture::new();
        let mut out = Failed(0);
        assert_eq!(
            run(&f.intent(Mode::Merge), &mut out).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(out.0, 1);
    }
}
