//! Bounded local favourites; data is not authority for filesystem mutations.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};
pub const MAX_BYTES: usize = 65536;
pub const MAX_ROWS: usize = 64;
pub const MAX_LABEL: usize = 96;
const MAGIC: &[u8; 8] = b"FILESFAV";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub label: String,
    pub path: PathBuf,
}
#[derive(Debug)]
pub struct Document {
    pub next: u64,
    pub rows: Vec<Entry>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            next: 1,
            rows: Vec::new(),
        }
    }
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Favoritos inválidos, incompatibles o fuera de presupuesto; se conservan sin modificar",
    )
}
fn path_valid(path: &Path) -> bool {
    let b = path.as_os_str().as_encoded_bytes();
    path.is_absolute()
        && b.len() <= 4096
        && !b.contains(&0)
        && (cfg!(unix) || path.to_str().is_some())
}
fn label_valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= MAX_LABEL && !s.chars().any(char::is_control)
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&v| v <= self.bytes.len())
            .ok_or_else(invalid)?;
        let value = &self.bytes[self.at..end];
        self.at = end;
        Ok(value)
    }
    fn u16(&mut self) -> io::Result<usize> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize)
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}
fn platform() -> u8 {
    if cfg!(unix) { 1 } else { 2 }
}
#[cfg(unix)]
fn path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}
#[cfg(not(unix))]
fn path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).as_ref())
}
fn walk(
    bytes: &[u8],
    mut row: impl FnMut(u64, &str, &[u8]) -> io::Result<()>,
) -> io::Result<(u64, usize)> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut r = Reader { bytes, at: 0 };
    if r.take(8)? != MAGIC || r.u16()? != 1 || r.take(1)?[0] != platform() {
        return Err(invalid());
    }
    let next = r.u64()?;
    let count = r.u16()?;
    if next == 0 || count > MAX_ROWS {
        return Err(invalid());
    }
    let mut ids = [0; MAX_ROWS];
    let mut paths: [&[u8]; MAX_ROWS] = [&[]; MAX_ROWS];
    for i in 0..count {
        let id = r.u64()?;
        let label_n = r.u16()?;
        let path_n = r.u16()?;
        if id == 0 || id >= next || ids[..i].contains(&id) || label_n > MAX_LABEL || path_n > 4096 {
            return Err(invalid());
        }
        ids[i] = id;
        let label = std::str::from_utf8(r.take(label_n)?).map_err(|_| invalid())?;
        let bytes = r.take(path_n)?;
        #[cfg(unix)]
        let valid = {
            use std::os::unix::ffi::OsStrExt;
            path_valid(Path::new(std::ffi::OsStr::from_bytes(bytes)))
        };
        #[cfg(not(unix))]
        let valid = std::str::from_utf8(bytes)
            .ok()
            .is_some_and(|s| path_valid(Path::new(s)));
        if !label_valid(label) || !valid || paths[..i].contains(&bytes) {
            return Err(invalid());
        }
        paths[i] = bytes;
        row(id, label, bytes)?;
    }
    if r.at != bytes.len() {
        return Err(invalid());
    }
    Ok((next, count))
}
pub fn decode(bytes: &[u8]) -> io::Result<Document> {
    let (next, count) = walk(bytes, |_, _, _| Ok(()))?;
    let mut rows = Vec::with_capacity(count);
    walk(bytes, |id, label, bytes| {
        rows.push(Entry {
            id,
            label: label.into(),
            path: path(bytes),
        });
        Ok(())
    })?;
    Ok(Document { next, rows })
}
pub fn encode(doc: &Document) -> io::Result<Vec<u8>> {
    if doc.next == 0 || doc.rows.len() > MAX_ROWS {
        return Err(invalid());
    }
    let mut length = 21usize;
    for (i, e) in doc.rows.iter().enumerate() {
        if e.id == 0
            || e.id >= doc.next
            || !path_valid(&e.path)
            || !label_valid(&e.label)
            || doc.rows[..i]
                .iter()
                .any(|v| v.id == e.id || v.path == e.path)
        {
            return Err(invalid());
        }
        length = length
            .checked_add(12 + e.label.len() + e.path.as_os_str().len())
            .filter(|&v| v <= MAX_BYTES)
            .ok_or_else(invalid)?;
    }
    let mut out = Vec::with_capacity(length);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.push(platform());
    out.extend_from_slice(&doc.next.to_le_bytes());
    out.extend_from_slice(&(doc.rows.len() as u16).to_le_bytes());
    for e in &doc.rows {
        out.extend_from_slice(&e.id.to_le_bytes());
        out.extend_from_slice(&(e.label.len() as u16).to_le_bytes());
        out.extend_from_slice(&(e.path.as_os_str().len() as u16).to_le_bytes());
        out.extend_from_slice(e.label.as_bytes());
        out.extend_from_slice(e.path.as_os_str().as_encoded_bytes());
    }
    Ok(out)
}
#[derive(Clone, Debug)]
pub enum Op {
    Read,
    Add { path: PathBuf, label: String },
    Remove(Entry),
    Rename { expected: Entry, label: String },
    Sync,
}
#[derive(Debug)]
pub struct Updated {
    pub document: Document,
    pub unsynced: bool,
    pub detail: Option<String>,
    pub notice: &'static str,
}
struct Snapshot {
    document: Document,
    file: Option<File>,
    revision: Option<crate::media_source::Revision>,
}
fn read(file_path: &Path) -> io::Result<Snapshot> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let mut file = match opts.open(file_path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Snapshot {
                document: Document::default(),
                file: None,
                revision: None,
            });
        }
        Err(e) => return Err(e),
    };
    if !file.metadata()?.is_file() {
        return Err(invalid());
    }
    let revision = crate::media_source::Revision::from_file(&file)?;
    if revision.bytes > MAX_BYTES as u64 {
        return Err(invalid());
    }
    let mut data = Vec::with_capacity(MAX_BYTES + 1);
    let mut chunk = [0; 8192];
    loop {
        let remain = (MAX_BYTES + 1 - data.len()).min(chunk.len());
        let n = file.read(&mut chunk[..remain])?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
        if data.len() > MAX_BYTES {
            return Err(invalid());
        }
    }
    revision
        .revalidate(&file, file_path)
        .map_err(|_| io::Error::other("Favoritos cambiaron durante lectura; recarga"))?;
    let document = decode(&data)?;
    Ok(Snapshot {
        document,
        file: Some(file),
        revision: Some(revision),
    })
}
struct CatalogueLock {
    file: File,
    #[cfg(unix)]
    owner_pid: libc::pid_t,
}
#[cfg(unix)]
impl Drop for CatalogueLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // A sibling thread's fork may retain the open description past close.
        // Only its creator releases advisory ownership; a child copy may not.
        if self.owner_pid == unsafe { libc::getpid() } {
            loop {
                if unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) } == 0 {
                    break;
                }
                if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
}
#[cfg(unix)]
fn lock(directory: &Path) -> io::Result<CatalogueLock> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    fs::create_dir_all(directory)?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(directory.join("bookmarks.lock"))?;
    if !f.metadata()?.is_file() {
        return Err(invalid());
    }
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Favoritos ocupados por otra ventana; reintenta",
        ));
    }
    Ok(CatalogueLock {
        file: f,
        owner_pid: unsafe { libc::getpid() },
    })
}
#[cfg(not(unix))]
fn lock(_: &Path) -> io::Result<CatalogueLock> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Escritura coordinada de favoritos pendiente en esta plataforma",
    ))
}
pub fn apply(directory: &Path, op: &Op) -> io::Result<Updated> {
    let file_path = directory.join("bookmarks.bin");
    if matches!(op, Op::Read) {
        return Ok(Updated {
            document: read(&file_path)?.document,
            unsynced: false,
            detail: None,
            notice: "Favoritos cargados",
        });
    }
    match op {
        Op::Add { path, label } if !path_valid(path) || !label_valid(label) => {
            return Err(invalid());
        }
        Op::Remove(e) | Op::Rename { expected: e, .. }
            if e.id == 0 || !path_valid(&e.path) || !label_valid(&e.label) =>
        {
            return Err(invalid());
        }
        Op::Rename { label, .. } if !label_valid(label) => return Err(invalid()),
        _ => {}
    }
    let guard = lock(directory)?;
    let mut old = read(&file_path)?;
    let notice = match op {
        Op::Add { path, label } => {
            if !path_valid(path) || !label_valid(label) {
                return Err(invalid());
            }
            if old.document.rows.iter().any(|e| e.path == *path) {
                return Ok(Updated {
                    document: old.document,
                    unsynced: false,
                    detail: None,
                    notice: "La carpeta ya está en favoritos",
                });
            }
            if old.document.rows.len() == MAX_ROWS {
                return Err(invalid());
            }
            let current = 21
                + old
                    .document
                    .rows
                    .iter()
                    .map(|e| 12 + e.label.len() + e.path.as_os_str().len())
                    .sum::<usize>();
            if current + 12 + label.len() + path.as_os_str().len() > MAX_BYTES {
                return Err(invalid());
            }
            let id = old.document.next;
            old.document.next = id.checked_add(1).ok_or_else(invalid)?;
            old.document.rows.push(Entry {
                id,
                path: path.clone(),
                label: label.clone(),
            });
            "Favorito añadido"
        }
        Op::Remove(expected) => {
            let index = old
                .document
                .rows
                .iter()
                .position(|e| e == expected)
                .ok_or_else(|| io::Error::other("El favorito cambió; recarga antes de quitarlo"))?;
            old.document.rows.remove(index);
            "Favorito quitado; carpeta intacta"
        }
        Op::Rename { expected, label } => {
            if !label_valid(label) {
                return Err(invalid());
            }
            let current = 21
                + old
                    .document
                    .rows
                    .iter()
                    .map(|e| 12 + e.label.len() + e.path.as_os_str().len())
                    .sum::<usize>();
            if current
                .saturating_sub(expected.label.len())
                .saturating_add(label.len())
                > MAX_BYTES
            {
                return Err(invalid());
            }
            let item = old
                .document
                .rows
                .iter_mut()
                .find(|e| *e == expected)
                .ok_or_else(|| {
                    io::Error::other("El favorito cambió; recarga antes de renombrarlo")
                })?;
            item.label = label.clone();
            "Etiqueta actualizada"
        }
        Op::Sync => {
            File::open(directory)?.sync_all()?;
            return Ok(Updated {
                document: old.document,
                unsynced: false,
                detail: None,
                notice: "Favoritos sincronizados",
            });
        }
        Op::Read => unreachable!(),
    };
    let bytes = encode(&old.document)?;
    let publication = crate::persistence::atomic_write_checked(&file_path, &bytes, || {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let a = guard.file.metadata()?;
            let b = fs::symlink_metadata(directory.join("bookmarks.lock"))?;
            if !b.is_file() || a.dev() != b.dev() || a.ino() != b.ino() {
                return Err(io::Error::other("Lock de favoritos cambió"));
            }
        }
        match (&old.file, old.revision) {
            (Some(file), Some(r)) => r.revalidate(file, &file_path).map_err(|_| {
                io::Error::other("Favoritos cambiaron antes de publicar; reintenta")
            })?,
            (None, None) => match fs::symlink_metadata(&file_path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                _ => {
                    return Err(io::Error::other(
                        "Favoritos creados por otro escritor; recarga",
                    ));
                }
            },
            _ => return Err(invalid()),
        }
        Ok(())
    })?;
    let detail = match &publication {
        crate::persistence::Publication::Unsynced(error) => Some(error.to_string()),
        _ => None,
    };
    Ok(Updated {
        document: old.document,
        detail,
        unsynced: matches!(publication, crate::persistence::Publication::Unsynced(_)),
        notice: if matches!(publication, crate::persistence::Publication::Unsynced(_)) {
            "Publicado, pero falta sincronizar favoritos"
        } else {
            notice
        },
    })
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    fn add(path: &str) -> Op {
        Op::Add {
            path: PathBuf::from(path),
            label: path.into(),
        }
    }
    #[test]
    fn raw_roundtrip_bounded_wire_ids_duplicates_and_future_preservation() {
        let raw = PathBuf::from(std::ffi::OsString::from_vec(b"/raw-\xff".to_vec()));
        let doc = Document {
            next: 2,
            rows: vec![Entry {
                id: 1,
                label: "á".into(),
                path: raw.clone(),
            }],
        };
        let bytes = encode(&doc).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back.rows[0].path, raw);
        assert_eq!(back.next, 2);
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n]).is_err());
        }
        let mut future = bytes.clone();
        future[8] = 2;
        assert!(decode(&future).is_err());
        let dup = Document {
            next: 3,
            rows: vec![
                doc.rows[0].clone(),
                Entry {
                    id: 2,
                    ..doc.rows[0].clone()
                },
            ],
        };
        assert!(encode(&dup).is_err());
        let huge = Document {
            next: 66,
            rows: (1..=65)
                .map(|id| Entry {
                    id,
                    label: "x".into(),
                    path: format!("/{id}").into(),
                })
                .collect(),
        };
        assert!(encode(&huge).is_err());
    }
    #[test]
    fn operations_reload_merge_stable_id_and_stale_label_rejection() {
        let root = crate::persistence::test_dir("bookmarks-ops");
        let a = apply(&root, &add("/one")).unwrap();
        let expected = a.document.rows[0].clone();
        let b = apply(&root, &add("/two")).unwrap();
        assert_eq!(b.document.rows.len(), 2);
        assert_eq!(apply(&root, &add("/one")).unwrap().document.rows.len(), 2);
        let renamed = apply(
            &root,
            &Op::Rename {
                expected: expected.clone(),
                label: "Nuevo".into(),
            },
        )
        .unwrap();
        assert!(apply(&root, &Op::Remove(expected)).is_err());
        apply(&root, &Op::Remove(renamed.document.rows[0].clone())).unwrap();
        let c = apply(&root, &add("/one")).unwrap();
        assert_eq!(c.document.rows[1].id, 3);
        assert!(apply(&root, &Op::Read).unwrap().document.rows[1].id == 3);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lock_busy_nonregular_invalid_future_and_symlink_do_not_overwrite() {
        use std::os::unix::fs::symlink;
        let root = crate::persistence::test_dir("bookmarks-preserve");
        let guard = lock(&root).unwrap();
        assert_eq!(
            apply(&root, &add("/one")).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(guard);
        fs::write(root.join("bookmarks.bin"), b"future").unwrap();
        assert!(apply(&root, &add("/one")).is_err());
        assert_eq!(fs::read(root.join("bookmarks.bin")).unwrap(), b"future");
        fs::remove_file(root.join("bookmarks.bin")).unwrap();
        fs::write(root.join("outside"), b"safe").unwrap();
        symlink("outside", root.join("bookmarks.bin")).unwrap();
        assert!(apply(&root, &add("/one")).is_err());
        assert_eq!(fs::read(root.join("outside")).unwrap(), b"safe");
        fs::remove_file(root.join("bookmarks.bin")).unwrap();
        fs::create_dir(root.join("bookmarks.bin")).unwrap();
        assert!(apply(&root, &Op::Read).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod fork_tests {
    use super::*;
    // The child executes only inherited File drop and libc read/write/_exit; no
    // allocator, logging, test assertions or other post-fork runtime locks.
    fn fork_case(child_drops: bool) {
        let root = crate::persistence::test_dir(if child_drops {
            "bookmark-lock-child-drop"
        } else {
            "bookmark-lock-parent-drop"
        });
        let guard = lock(&root).unwrap();
        let mut ready = [0; 2];
        let mut go = [0; 2];
        assert_eq!(
            unsafe { libc::pipe2(ready.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        assert_eq!(unsafe { libc::pipe2(go.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            unsafe {
                libc::close(ready[0]);
                libc::close(go[1]);
            }
            if child_drops {
                drop(guard)
            }
            let byte = 1u8;
            unsafe {
                libc::write(ready[1], (&byte as *const u8).cast(), 1);
                let mut ack = 0u8;
                libc::read(go[0], (&mut ack as *mut u8).cast(), 1);
                libc::_exit(0)
            }
        }
        unsafe {
            libc::close(ready[1]);
            libc::close(go[0]);
        }
        let mut byte = 0u8;
        let ready_n = unsafe { libc::read(ready[0], (&mut byte as *mut u8).cast(), 1) };
        let attempt = if child_drops {
            let r = lock(&root);
            drop(guard);
            r
        } else {
            drop(guard);
            lock(&root)
        };
        let correct = if child_drops {
            matches!(&attempt,Err(e)if e.kind()==io::ErrorKind::WouldBlock)
        } else {
            attempt.is_ok()
        };
        let child_still_live = unsafe { libc::kill(pid, 0) } == 0;
        unsafe {
            libc::write(go[1], (&byte as *const u8).cast(), 1);
            libc::close(go[1]);
            libc::close(ready[0]);
        }
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(pid, &mut status, 0) };
        drop(attempt);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(ready_n, 1);
        assert!(child_still_live);
        assert_eq!(reaped, pid);
        assert_eq!(status, 0);
        assert!(correct);
    }
    #[test]
    fn creator_unlocks_even_while_forked_child_retains_descriptor() {
        fork_case(false)
    }
    #[test]
    fn child_drop_never_unlocks_creators_live_owner() {
        fork_case(true)
    }
}
