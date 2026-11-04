//! Private FD transport used by AuxIo; no compositor calls or UI mutation.
use std::{
    fs::File,
    io::{self, Read},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
/// UI may cancel/close a QUEUED pipe without waiting for unrelated AuxIo work.
/// Worker takes it exactly once under a tiny mutex, never holds the mutex in I/O.
pub struct Reader {
    file: Mutex<Option<File>>,
    metadata: Mutex<Option<File>>,
    kde: bool,
    pub cancel: Arc<AtomicBool>,
    deadline: Instant,
}
impl Reader {
    pub fn new(file: File, cancel: Arc<AtomicBool>) -> Self {
        Self {
            file: Mutex::new(Some(file)),
            metadata: Mutex::new(None),
            kde: false,
            cancel,
            deadline: Instant::now() + Duration::from_secs(3),
        }
    }
    pub fn new_kde(file: File, metadata: File, cancel: Arc<AtomicBool>) -> Self {
        Self {
            file: Mutex::new(Some(file)),
            metadata: Mutex::new(Some(metadata)),
            kde: true,
            cancel,
            deadline: Instant::now() + Duration::from_secs(3),
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut file) = self.file.try_lock() {
            file.take();
        }
        if let Ok(mut file) = self.metadata.try_lock() {
            file.take();
        }
    }
    pub fn expire_at(&self, now: Instant) -> bool {
        if now < self.deadline {
            return false;
        }
        let first = !self.cancel.swap(true, Ordering::AcqRel);
        if let Ok(mut file) = self.file.try_lock() {
            file.take();
        }
        if let Ok(mut file) = self.metadata.try_lock() {
            file.take();
        }
        first
    }
    fn take(&self) -> io::Result<File> {
        self.file
            .lock()
            .map_err(|_| io::Error::other("Lector MIME inconsistente"))?
            .take()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Lector MIME cancelado antes de iniciar",
                )
            })
    }
    fn take_metadata(&self) -> io::Result<File> {
        self.metadata
            .lock()
            .map_err(|_| io::Error::other("Lector KDE inconsistente"))?
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Interrupted, "Metadata KDE cancelada"))
    }
}
#[cfg(test)]
fn read(file: File, cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    read_until(file, cancel, Instant::now() + Duration::from_secs(3))
}
#[cfg(unix)]
fn read_until(mut file: File, cancel: &AtomicBool, end: Instant) -> io::Result<Vec<u8>> {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if flags & libc::O_NONBLOCK == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "El lector MIME debe ser no bloqueante",
        ));
    }
    let mut bytes = Vec::with_capacity(16 * 1024);
    let mut chunk = [0u8; 16 * 1024];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Lectura MIME cancelada",
            ));
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Clipboard sin EOF dentro de3s",
            ));
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        let n = unsafe { libc::poll(&mut poll, 1, 20) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if n == 0 {
            continue;
        }
        let remaining = files_core::file_transfer::MAX_WIRE - bytes.len();
        let limit = chunk.len().min(remaining + 1);
        match file.read(&mut chunk[..limit]) {
            Ok(0) => return Ok(bytes),
            Ok(n) => {
                if n > remaining {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "MIME supera1MiB; ninguna operación iniciada",
                    ));
                }
                bytes.extend_from_slice(&chunk[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
    }
}
#[cfg(unix)]
fn read_kde(
    uri: File,
    metadata: File,
    cancel: &AtomicBool,
    end: Instant,
) -> io::Result<(Vec<u8>, files_core::file_transfer::Intent)> {
    read_kde_with(uri, metadata, cancel, end, || {})
}
#[cfg(unix)]
fn read_kde_with(
    mut uri: File,
    mut metadata: File,
    cancel: &AtomicBool,
    end: Instant,
    mut uri_eof: impl FnMut(),
) -> io::Result<(Vec<u8>, files_core::file_transfer::Intent)> {
    use std::os::fd::AsRawFd;
    for f in [&uri, &metadata] {
        let flags = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 || flags & libc::O_NONBLOCK == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Lector KDE debe ser no bloqueante",
            ));
        }
    }
    let mut bytes = Vec::with_capacity(16 * 1024);
    let mut marker = [0u8; 2];
    let mut marker_len = 0;
    let mut done = [false; 2];
    let mut chunk = [0u8; 16 * 1024];
    while !done.iter().all(|v| *v) {
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Lectura KDE cancelada",
            ));
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "KDE sin EOF dentro de3s",
            ));
        }
        let mut fds = [
            libc::pollfd {
                fd: if done[0] { -1 } else { uri.as_raw_fd() },
                events: libc::POLLIN | libc::POLLHUP,
                revents: 0,
            },
            libc::pollfd {
                fd: if done[1] { -1 } else { metadata.as_raw_fd() },
                events: libc::POLLIN | libc::POLLHUP,
                revents: 0,
            },
        ];
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 20) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        for i in 0..2 {
            if done[i] || fds[i].revents == 0 {
                continue;
            }
            if fds[i].revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err(io::Error::other("Error en pipe KDE"));
            }
            let result = if i == 0 {
                let remaining = files_core::file_transfer::MAX_WIRE - bytes.len();
                let len = chunk.len().min(remaining + 1);
                uri.read(&mut chunk[..len]).and_then(|n| {
                    if n > remaining {
                        Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "URI KDE supera1MiB",
                        ))
                    } else {
                        bytes.extend_from_slice(&chunk[..n]);
                        Ok(n)
                    }
                })
            } else {
                metadata.read(&mut marker[marker_len..]).and_then(|n| {
                    marker_len += n;
                    if marker_len > 1 {
                        Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Metadata KDE inválida",
                        ))
                    } else {
                        Ok(n)
                    }
                })
            };
            match result {
                Ok(0) => {
                    done[i] = true;
                    if i == 0 {
                        uri_eof();
                    }
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
    }
    let intent = match &marker[..marker_len] {
        b"0" => files_core::file_transfer::Intent::Copy,
        b"1" => files_core::file_transfer::Intent::Cut,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Metadata KDE debe ser 0 o 1",
            ));
        }
    };
    Ok((bytes, intent))
}
#[cfg(not(unix))]
fn read_until(_: File, _: &AtomicBool, _: Instant) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Transporte nativo de esta plataforma pendiente",
    ))
}

pub struct Imported {
    pub source: files_core::selection_source::Source,
    pub kind: crate::file_clipboard::Kind,
}
/// Capture and encode the original drag selection on AuxIo. The retained
/// source pins the exact parent and revisions until the UI consumes the ACK.
/// This never changes the window's private clipboard or its Cut state.
pub struct DragPayload {
    pub lease: std::sync::Arc<crate::file_clipboard::Lease>,
    pub bytes: Vec<u8>,
}
pub fn capture_drag(
    parent: std::path::PathBuf,
    names: std::sync::Arc<[std::ffi::OsString]>,
    cancel: &AtomicBool,
) -> Result<DragPayload, String> {
    if names.is_empty() || names.len() > 128 {
        return Err("Selecciona entre 1 y 128 elementos para arrastrar".into());
    }
    let options = files_core::copy_engine::CopyOptions::default();
    crate::copy_request_builder::preflight(
        &parent,
        &parent,
        names.iter().map(|n| n.as_os_str()),
        &options,
    )
    .map_err(str::to_string)?;
    let source = files_core::selection_source::Source::capture(parent, names.clone(), cancel)
        .map_err(|e| e.to_string())?;
    let lease = std::sync::Arc::new(crate::file_clipboard::Lease {
        system: false,
        generation: 0,
        kind: crate::file_clipboard::Kind::Copy,
        source: std::sync::Arc::new(source),
        names,
    });
    if lease.retained_bytes() > crate::file_clipboard::LIMIT {
        return Err("Selección de arrastre supera el presupuesto de metadata".into());
    }
    let bytes = encode(&lease, cancel)?;
    Ok(DragPayload { lease, bytes })
}
pub fn import(
    reader: &Reader,
    format: files_core::file_transfer::Format,
) -> Result<Imported, String> {
    if reader.kde && format != files_core::file_transfer::Format::UriList {
        return Err("KDE requiere URI y metadata de la misma oferta".into());
    }
    let file = reader.take().map_err(|e| e.to_string())?;
    let cancel = &reader.cancel;
    #[cfg(unix)]
    let (bytes, kde_intent) = if reader.kde {
        let metadata = reader.take_metadata().map_err(|e| e.to_string())?;
        let (bytes, intent) =
            read_kde(file, metadata, cancel, reader.deadline).map_err(|e| e.to_string())?;
        (bytes, Some(intent))
    } else {
        (
            read_until(file, cancel, reader.deadline).map_err(|e| e.to_string())?,
            None,
        )
    };
    #[cfg(not(unix))]
    let (bytes, kde_intent) = (
        read_until(file, cancel, reader.deadline).map_err(|e| e.to_string())?,
        None,
    );
    let mut files = files_core::file_transfer::decode(format, &bytes).map_err(str::to_string)?;
    if let Some(intent) = kde_intent {
        files.intent = intent;
    }
    drop(bytes);
    let parent = files.paths[0]
        .parent()
        .ok_or("Falta padre origen")?
        .to_path_buf();
    if files.paths.iter().any(|p| p.parent() != Some(&parent)) {
        return Err("Esta revisión admite un padre origen; lote multi-origen pendiente. No se importó ningún grupo".into());
    }
    let names = files
        .paths
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_os_string())
                .ok_or("Falta nombre origen")
        })
        .collect::<Result<Vec<_>, _>>()?;
    drop(files.paths);
    let source = files_core::selection_source::Source::capture(parent, names.into(), cancel)
        .map_err(|e| e.to_string())?;
    if source.retained_bytes() > 2 * 1024 * 1024 {
        return Err("Origen MIME supera su presupuesto de metadata".into());
    }
    Ok(Imported {
        source,
        kind: if files.intent == files_core::file_transfer::Intent::Cut {
            crate::file_clipboard::Kind::Cut
        } else {
            crate::file_clipboard::Kind::Copy
        },
    })
}
pub fn encode(
    lease: &crate::file_clipboard::Lease,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    lease
        .source
        .verify_subset(lease.names.iter().map(|n| n.as_os_str()), cancel)
        .map_err(|e| e.to_string())?;
    let options = files_core::copy_engine::CopyOptions::default();
    crate::copy_request_builder::preflight(
        lease.source.parent(),
        lease.source.parent(),
        lease.names.iter().map(|n| n.as_os_str()),
        &options,
    )
    .map_err(str::to_string)?;
    let mut paths = Vec::with_capacity(lease.names.len());
    for name in lease.names.iter() {
        if cancel.load(Ordering::Acquire) {
            return Err("Publicación MIME cancelada".into());
        }
        paths.push(lease.source.parent().join(name));
    }
    files_core::file_transfer::encode(
        files_core::file_transfer::Format::GnomeCopiedFiles,
        if lease.kind == crate::file_clipboard::Kind::Cut {
            files_core::file_transfer::Intent::Cut
        } else {
            files_core::file_transfer::Intent::Copy
        },
        &paths,
    )
    .map_err(str::to_string)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{io::Write, os::fd::FromRawFd, thread};
    fn pipe() -> (File, File) {
        let mut f = [0; 2];
        assert_eq!(unsafe { libc::pipe2(f.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        assert_eq!(
            unsafe { libc::fcntl(f[0], libc::F_SETFL, libc::O_NONBLOCK) },
            0
        );
        unsafe { (File::from_raw_fd(f[0]), File::from_raw_fd(f[1])) }
    }
    #[test]
    fn chunks_eof_cap_and_blocking_writer() {
        for length in [
            0,
            81921,
            files_core::file_transfer::MAX_WIRE,
            files_core::file_transfer::MAX_WIRE + 1,
        ] {
            let (r, mut w) = pipe();
            use std::os::fd::AsRawFd;
            assert_eq!(
                unsafe { libc::fcntl(w.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
                0
            );
            let writer = thread::spawn(move || {
                let b = [b'x'; 4096];
                let mut left = length;
                while left > 0 {
                    let n = left.min(b.len());
                    if w.write_all(&b[..n]).is_err() {
                        break;
                    }
                    left -= n;
                }
            });
            let result = read(r, &AtomicBool::new(false));
            writer.join().unwrap();
            if length <= files_core::file_transfer::MAX_WIRE {
                assert_eq!(result.unwrap().len(), length);
            } else {
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
            }
        }
    }
    #[test]
    fn cancel_never_waits_for_writer_or_accepts_blocking_fd() {
        let (r, _w) = pipe();
        assert_eq!(
            read(r, &AtomicBool::new(true)).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        let (r, _w) = pipe();
        use std::os::fd::AsRawFd;
        unsafe {
            libc::fcntl(r.as_raw_fd(), libc::F_SETFL, 0);
        };
        assert_eq!(
            read(r, &AtomicBool::new(false)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn expired_queued_reader_closes_pipe_before_auxio_ack() {
        let (r, mut w) = pipe();
        let cancel = Arc::new(AtomicBool::new(false));
        let reader = Arc::new(Reader::new(r, cancel));
        let queued = reader.clone();
        assert!(!reader.expire_at(Instant::now()));
        assert!(reader.expire_at(reader.deadline));
        assert!(!reader.expire_at(reader.deadline));
        assert_eq!(
            w.write_all(b"no reader").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            queued.take().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn kde_drains_both_pipes_and_requires_exact_marker() {
        for (marker, expected) in [
            (&b"0"[..], Some(files_core::file_transfer::Intent::Copy)),
            (&b"1"[..], Some(files_core::file_transfer::Intent::Cut)),
            (&b""[..], None),
            (&b"1\n"[..], None),
            (&b"x"[..], None),
        ] {
            let (uri, mut uri_write) = pipe();
            let (meta, mut meta_write) = pipe();
            let marker = marker.to_vec();
            let writer = thread::spawn(move || {
                uri_write.write_all(&vec![b'a'; 128 * 1024]).unwrap();
                drop(uri_write);
                meta_write.write_all(&marker).unwrap();
            });
            let got = read_kde(
                uri,
                meta,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(3),
            );
            writer.join().unwrap();
            match expected {
                Some(intent) => {
                    let (data, result) = got.unwrap();
                    assert_eq!(data.len(), 128 * 1024);
                    assert_eq!(result, intent);
                }
                None => assert!(got.is_err()),
            }
        }
    }
    #[test]
    fn kde_cancel_closes_both_queued_fds() {
        let (a, mut aw) = pipe();
        let (b, mut bw) = pipe();
        let r = Reader::new_kde(a, b, Arc::new(AtomicBool::new(false)));
        r.cancel();
        assert_eq!(
            aw.write_all(b"x").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            bw.write_all(b"x").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert!(r.take().is_err());
        assert!(r.take_metadata().is_err());
    }
    #[test]
    fn kde_metadata_first_oversize_and_deadline_fail_closed() {
        let (uri, mut uw) = pipe();
        let (meta, mut mw) = pipe();
        let writer = thread::spawn(move || {
            mw.write_all(b"1").unwrap();
            drop(mw);
            uw.write_all(&vec![b'x'; 128 * 1024]).unwrap();
        });
        let (bytes, intent) = read_kde(
            uri,
            meta,
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
        writer.join().unwrap();
        assert_eq!(bytes.len(), 128 * 1024);
        assert_eq!(intent, files_core::file_transfer::Intent::Cut);
        let (uri, mut uw) = pipe();
        let (meta, mut mw) = pipe();
        let writer = thread::spawn(move || {
            let _ = uw.write_all(&vec![b'x'; files_core::file_transfer::MAX_WIRE + 1]);
            let _ = mw.write_all(b"0");
        });
        assert_eq!(
            read_kde(
                uri,
                meta,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(3)
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidData
        );
        writer.join().unwrap();
        let (uri, _uw) = pipe();
        let (meta, _mw) = pipe();
        assert_eq!(
            read_kde(uri, meta, &AtomicBool::new(false), Instant::now())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn kde_active_cancel_after_uri_eof_closes_both_readers() {
        let (uri, uw) = pipe();
        let (meta, _mw) = pipe();
        use std::os::unix::fs::MetadataExt;
        let readers =
            [uri.metadata().unwrap(), meta.metadata().unwrap()].map(|m| (m.dev(), m.ino()));
        drop(uw); // URI EOF, while metadata writer stays open.
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            read_kde_with(
                uri,
                meta,
                &flag,
                Instant::now() + Duration::from_secs(3),
                || {
                    tx.send(()).unwrap();
                },
            )
        });
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        cancel.store(true, Ordering::Release);
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        // Pipe writability counts every reader, including a descriptor inherited
        // by a concurrent fork before exec closes O_CLOEXEC. It is not proof of
        // which owned reader remains in this process. Verify both identities,
        // including accidental parent-side duplicates, without reducing concurrency.
        use std::os::fd::RawFd;
        let directory = if cfg!(target_os = "linux") {
            "/proc/self/fd"
        } else {
            "/dev/fd"
        };
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let Ok(fd) = entry.file_name().to_string_lossy().parse::<RawFd>() else {
                continue;
            };
            let metadata = match std::fs::metadata(entry.path()) {
                Ok(m) => m,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => panic!("Cannot establish FD identity: {e}"),
            };
            if readers.contains(&(metadata.dev(), metadata.ino())) {
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
                if flags < 0 {
                    let e = io::Error::last_os_error();
                    if e.raw_os_error() == Some(libc::EBADF) {
                        continue;
                    }
                    panic!("Cannot establish reader access mode: {e}");
                }
                assert_eq!(
                    flags & libc::O_ACCMODE,
                    libc::O_WRONLY,
                    "Cancelled KDE reader identity still open in the owning process"
                );
            }
        }
    }
}
