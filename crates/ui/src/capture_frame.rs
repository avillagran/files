//! Fixture-only capture protocol. Publication is atomic; redraw is explicit.
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Default)]
pub struct Request {
    pub id: u64,
    pub redraw: bool,
    /// Observe the current buffer without flushing pending paints first.
    pub passive: bool,
}
pub fn request(path: &Path) -> io::Result<Request> {
    let mut data = Vec::new();
    std::fs::File::open(path)?
        .take(129)
        .read_to_end(&mut data)?;
    if data.is_empty() {
        return Ok(Request::default());
    }
    if data.len() > 128 {
        return Err(io::Error::other("Petición de captura superior a128B"));
    }
    let value: serde_json::Value = serde_json::from_slice(&data)?;
    let invalid = || io::Error::other("Petición de captura inválida");
    let object = value.as_object().ok_or_else(invalid)?;
    if object
        .keys()
        .any(|key| key != "id" && key != "redraw" && key != "passive")
    {
        return Err(invalid());
    }
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(invalid)?;
    let redraw = match object.get("redraw") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(invalid)?,
    };
    let passive = match object.get("passive") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(invalid)?,
    };
    if passive && redraw {
        return Err(invalid());
    }
    let request = Request {
        id,
        redraw,
        passive,
    };
    Ok(request)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn publish(path: &Path, width: i32, height: i32, rgb: &[u8]) -> io::Result<()> {
    let size = usize::try_from(width)
        .ok()
        .filter(|w| *w > 0)
        .and_then(|w| {
            usize::try_from(height)
                .ok()
                .filter(|h| *h > 0)
                .and_then(|h| w.checked_mul(h))
        })
        .and_then(|n| n.checked_mul(3));
    if size != Some(rgb.len()) {
        return Err(io::Error::other("Dimensiones/píxeles de captura inválidos"));
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("capture-{}-{id}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&tmp)?;
    let tmp = Temporary(tmp);
    let written = (|| -> io::Result<()> {
        write!(out, "P6\n{width} {height}\n255\n")?;
        out.write_all(rgb)?;
        out.flush()
    })();
    drop(out);
    written?;
    fs::rename(&tmp.0, path)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "capture-frame-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn invalid_frame_preserves_prior_publication_and_valid_frame_replaces_whole_file() {
        let root = fixture();
        let target = root.join("frame.ppm");
        fs::write(&target, b"prior").unwrap();
        assert!(publish(&target, 2, 1, &[1, 2, 3]).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"prior");
        publish(&target, 1, 1, &[1, 2, 3]).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"P6\n1 1\n255\n\x01\x02\x03");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn concurrent_reader_sees_only_complete_old_or_new_frames() {
        use std::sync::{Arc, Barrier, atomic::AtomicBool};
        let root = fixture();
        let path = root.join("frame.ppm");
        let a = vec![17; 256 * 256 * 3];
        let b = vec![34; 256 * 256 * 3];
        publish(&path, 256, 256, &a).unwrap();
        let old = fs::read(&path).unwrap();
        publish(&path, 256, 256, &b).unwrap();
        let new = fs::read(&path).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let barrier = Arc::new(Barrier::new(2));
        let ready = barrier.clone();
        let reader_path = path.clone();
        let (observed_tx, observed_rx) = std::sync::mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            assert_eq!(fs::read(&reader_path).unwrap(), new);
            ready.wait();
            let mut notify = Some(observed_tx);
            while !flag.load(Ordering::Relaxed) {
                let data = fs::read(&reader_path).unwrap();
                assert!(data == old || data == new);
                if let Some(tx) = notify.take() {
                    tx.send(()).unwrap();
                }
            }
        });
        barrier.wait();
        let result =
            (0..16).try_for_each(|i| publish(&path, 256, 256, if i % 2 == 0 { &a } else { &b }));
        let observed = observed_rx.recv_timeout(std::time::Duration::from_secs(2));
        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
        observed.unwrap();
        result.unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_and_explicit_request_are_bounded_and_redraw_is_opt_in() {
        let root = fixture();
        let path = root.join("frame.request");
        fs::write(&path, b"").unwrap();
        assert!(!request(&path).unwrap().redraw);
        fs::write(&path, br#"{"id":2,"redraw":true}"#).unwrap();
        assert!(request(&path).unwrap().redraw);
        fs::write(&path, br#"{"id":3,"passive":true}"#).unwrap();
        let observed = request(&path).unwrap();
        assert!(observed.passive && !observed.redraw);
        for bytes in [
            br#"{"id":0}"#.as_slice(),
            br#"{"id":1,"other":2}"#,
            br#"{"id":1,"redraw":true,"passive":true}"#,
            br#"{"id":1,"passive":4}"#,
            &[b'x'; 129],
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(request(&path).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
