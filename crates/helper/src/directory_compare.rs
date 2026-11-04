//! One read-only comparison per helper; parent cancellation kills/reaps this process.
use files_core::{
    directory_compare::{self as compare, Limits},
    directory_compare_wire::{self as wire, Event, Frame},
    relative_source::DirectoryIdentity,
};
use std::{
    io::{self, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
struct Output<W> {
    writer: W,
    bytes: usize,
}
impl<W: Write> Output<W> {
    fn send(&mut self, id: u64, event: Event) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(&Frame { id, event })?;
        if bytes.len() > wire::MAX_FRAME {
            return Err(io::Error::other("Frame de comparación fuera del límite"));
        }
        bytes.push(b'\n');
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|&n| n <= wire::MAX_STREAM)
            .ok_or_else(|| io::Error::other("Stream de comparación fuera del límite"))?;
        self.writer.write_all(&bytes)?;
        self.writer.flush()
    }
}
pub fn run(
    id: u64,
    left: &Path,
    right: &Path,
    expected: Option<[DirectoryIdentity; 2]>,
    read_limit: u64,
    writer: impl Write,
) -> io::Result<()> {
    let mut out = Output { writer, bytes: 0 };
    let mut updates = 0;
    let mut phase = u8::MAX;
    let mut last = Instant::now();
    let mut sink_failed = false;
    let report = compare::compare(
        left,
        right,
        expected,
        Limits {
            read: read_limit,
            ..Limits::default()
        },
        &AtomicBool::new(false),
        |p| {
            if p.phase < 3
                && updates < wire::MAX_UPDATES
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
        Ok(report) => {
            wire::validate(&report)?;
            let mut offset = 0;
            while offset < report.rows.len() {
                let end = wire::page_end(&report.rows, offset);
                if end == offset {
                    return Err(io::Error::other("Página de comparación sin avance"));
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
                message.push_str("Error de comparación");
            }
            out.send(id, Event::Error { message })
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "compare-helper-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(p.join("left")).unwrap();
            fs::create_dir(p.join("right")).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn real_helper_output_reassembles_exact_raw_namespace_with_partial_read_limit() {
        use std::os::unix::ffi::OsStringExt;
        let f = Fixture::new();
        let name = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        for side in ["left", "right"] {
            fs::create_dir(f.0.join(side).join("dir")).unwrap();
            fs::write(f.0.join(side).join("dir").join(&name), b"same").unwrap();
        }
        let mut bytes = Vec::new();
        run(
            7,
            &f.0.join("left"),
            &f.0.join("right"),
            None,
            0,
            &mut bytes,
        )
        .unwrap();
        let mut rows = Vec::new();
        let mut terminal = None;
        for line in bytes.split(|&b| b == b'\n').filter(|b| !b.is_empty()) {
            assert!(line.len() <= wire::MAX_FRAME);
            let frame: Frame = serde_json::from_slice(line).unwrap();
            assert_eq!(frame.id, 7);
            match frame.event {
                Event::Progress { progress } => {
                    wire::validate_progress(progress).unwrap();
                    assert!(terminal.is_none());
                }
                Event::Page { offset, rows: page } => {
                    assert_eq!(offset, rows.len());
                    rows.extend(page);
                }
                Event::Done { roots, progress } => terminal = Some((roots, progress)),
                Event::Error { message } => panic!("{message}"),
            }
        }
        let (roots, progress) = terminal.unwrap();
        let report = compare::Report {
            roots,
            progress,
            rows,
        };
        wire::validate(&report).unwrap();
        assert!(!progress.complete());
        assert_ne!(progress.reasons & compare::READ_LIMIT, 0);
        assert_eq!(report.rows[1].path, PathBuf::from("dir").join(name));
        assert_eq!(report.rows[0].difference, compare::Difference::Unknown);
    }
    #[test]
    fn invalid_roots_or_budget_have_error_without_false_done() {
        let f = Fixture::new();
        for (left, budget) in [
            (Path::new("relative"), 1),
            (f.0.as_path(), compare::MAX_READ + 1),
        ] {
            let mut bytes = Vec::new();
            run(9, left, &f.0.join("right"), None, budget, &mut bytes).unwrap();
            let frames = bytes
                .split(|&b| b == b'\n')
                .filter(|b| !b.is_empty())
                .map(|b| serde_json::from_slice::<Frame>(b).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(frames.len(), 1);
            assert!(matches!(frames[0].event, Event::Error { .. }));
        }
    }
    #[test]
    fn broken_consumer_is_returned_without_attempting_an_error_frame() {
        struct Broken {
            writes: usize,
        }
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let f = Fixture::new();
        let mut out = Broken { writes: 0 };
        let err = run(1, &f.0.join("left"), &f.0.join("right"), None, 0, &mut out).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(out.writes, 1);
    }
    #[test]
    fn multiple_pages_pass_the_shared_ledger_without_omitting_rows() {
        let f = Fixture::new();
        for side in ["left", "right"] {
            for n in 0..130 {
                fs::write(f.0.join(side).join(format!("file-{n:03}")), b"x").unwrap();
            }
        }
        let mut bytes = Vec::new();
        run(
            4,
            &f.0.join("left"),
            &f.0.join("right"),
            None,
            1024,
            &mut bytes,
        )
        .unwrap();
        let mut ledger = wire::Collector::new(4, 1024, None).unwrap();
        let mut pages = 0;
        for line in bytes.split(|&b| b == b'\n').filter(|b| !b.is_empty()) {
            let frame: Frame = serde_json::from_slice(line).unwrap();
            if matches!(frame.event, Event::Page { .. }) {
                pages += 1;
            }
            ledger.accept(frame).unwrap();
        }
        let report = ledger.finish().unwrap();
        assert_eq!(pages, 2);
        assert_eq!(report.rows.len(), 130);
        assert!(report.progress.complete());
        assert_eq!(report.progress.bytes_read, 260);
        for (n, row) in report.rows.iter().enumerate() {
            assert_eq!(row.path, PathBuf::from(format!("file-{n:03}")));
        }
    }
}
