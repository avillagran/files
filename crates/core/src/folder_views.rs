//! Explicit per-folder view snapshots, stored only in the application config directory.
//! Keys are exact lexical paths, not filesystem identities or mutation authority.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};
pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_ROWS: usize = 256;
const MAGIC: &[u8; 8] = b"FILESVW1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Stable tags: Groups=0, List=1, Columns=2, Grid=3, SpaceMap=4.
    pub view: u8,
    /// Name=0, Size=1, Modified=2.
    pub sort: u8,
    pub descending: bool,
    pub zoom: u8,
    pub scroll: u32,
}
impl Settings {
    fn valid(self) -> bool {
        self.view <= 4
            && self.sort <= 2
            && (65..=180).contains(&self.zoom)
            && self.scroll <= 100_000_000
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub settings: Settings,
}
#[derive(Debug, Default)]
pub struct Document {
    pub rows: Vec<Entry>,
}
impl Document {
    pub fn get(&self, path: &Path) -> Option<&Entry> {
        self.rows
            .binary_search_by(|e| key(&e.path).cmp(key(path)))
            .ok()
            .map(|i| &self.rows[i])
    }
}
fn key(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Vistas de carpetas inválidas, incompatibles o fuera de presupuesto; se conservan sin modificar",
    )
}
fn path_valid(path: &Path) -> bool {
    path.is_absolute()
        && key(path).len() <= 4096
        && !key(path).contains(&0)
        && (cfg!(unix) || path.to_str().is_some())
}
fn platform() -> u8 {
    if cfg!(unix) { 1 } else { 2 }
}
#[cfg(unix)]
fn borrowed_path(bytes: &[u8]) -> io::Result<&Path> {
    use std::os::unix::ffi::OsStrExt;
    Ok(Path::new(std::ffi::OsStr::from_bytes(bytes)))
}
#[cfg(not(unix))]
fn borrowed_path(bytes: &[u8]) -> io::Result<&Path> {
    Ok(Path::new(
        std::str::from_utf8(bytes).map_err(|_| invalid())?,
    ))
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
}
fn walk(bytes: &[u8], mut row: impl FnMut(&Path, Settings)) -> io::Result<usize> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut r = Reader { bytes, at: 0 };
    if r.take(8)? != MAGIC || r.u16()? != 1 || r.take(1)?[0] != platform() {
        return Err(invalid());
    }
    let count = r.u16()?;
    if count > MAX_ROWS {
        return Err(invalid());
    }
    let mut previous: Option<&[u8]> = None;
    for _ in 0..count {
        let n = r.u16()?;
        if n > 4096 {
            return Err(invalid());
        }
        let raw = r.take(n)?;
        let path = borrowed_path(raw)?;
        if !path_valid(path) || previous.is_some_and(|p| p >= raw) {
            return Err(invalid());
        }
        previous = Some(raw);
        let b = r.take(4)?;
        if b[2] > 1 {
            return Err(invalid());
        }
        let settings = Settings {
            view: b[0],
            sort: b[1],
            descending: b[2] == 1,
            zoom: b[3],
            scroll: u32::from_le_bytes(r.take(4)?.try_into().unwrap()),
        };
        if !settings.valid() {
            return Err(invalid());
        }
        row(path, settings);
    }
    if r.at != bytes.len() {
        return Err(invalid());
    }
    Ok(count)
}
pub fn decode(bytes: &[u8]) -> io::Result<Document> {
    let count = walk(bytes, |_, _| {})?; // Validate all bytes before any path allocations.
    let mut rows = Vec::with_capacity(count);
    walk(bytes, |path, settings| {
        rows.push(Entry {
            path: path.to_owned(),
            settings,
        })
    })?;
    Ok(Document { rows })
}
pub fn encode(document: &Document) -> io::Result<Vec<u8>> {
    if document.rows.len() > MAX_ROWS {
        return Err(invalid());
    }
    let mut size = 13usize;
    let mut previous: Option<&[u8]> = None;
    for e in &document.rows {
        let raw = key(&e.path);
        if !path_valid(&e.path) || !e.settings.valid() || previous.is_some_and(|p| p >= raw) {
            return Err(invalid());
        }
        previous = Some(raw);
        size = size
            .checked_add(10 + raw.len())
            .filter(|&s| s <= MAX_BYTES)
            .ok_or_else(invalid)?;
    }
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.push(platform());
    bytes.extend_from_slice(&(document.rows.len() as u16).to_le_bytes());
    for e in &document.rows {
        bytes.extend_from_slice(&(key(&e.path).len() as u16).to_le_bytes());
        bytes.extend_from_slice(key(&e.path));
        bytes.extend_from_slice(&[
            e.settings.view,
            e.settings.sort,
            u8::from(e.settings.descending),
            e.settings.zoom,
        ]);
        bytes.extend_from_slice(&e.settings.scroll.to_le_bytes());
    }
    Ok(bytes)
}
#[derive(Debug)]
pub enum Op {
    Read,
    Save(Entry),
    Forget(Entry),
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
    let mut data = Vec::with_capacity(revision.bytes as usize + 1);
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
        .map_err(|_| io::Error::other("Vistas de carpetas cambiaron durante lectura; recarga"))?;
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
        .open(directory.join("folder-views.lock"))?;
    if !f.metadata()?.is_file() {
        return Err(invalid());
    }
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Vistas de carpetas ocupadas por otra ventana; reintenta",
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
        "Escritura coordinada de vistas de carpetas pendiente en esta plataforma",
    ))
}
fn check_snapshot(
    directory: &Path,
    guard: &File,
    old: &Snapshot,
    file_path: &Path,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let a = guard.metadata()?;
        let b = fs::symlink_metadata(directory.join("folder-views.lock"))?;
        if !b.is_file() || a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(io::Error::other("Lock de vistas de carpetas cambió"));
        }
    }
    match (&old.file, old.revision) {
        (Some(file), Some(r)) => r.revalidate(file, file_path).map_err(|_| {
            io::Error::other("Vistas de carpetas cambiaron antes de publicar; reintenta")
        })?,
        (None, None) => match fs::symlink_metadata(file_path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            _ => {
                return Err(io::Error::other(
                    "Vistas de carpetas creadas por otro escritor; recarga",
                ));
            }
        },
        _ => return Err(invalid()),
    }
    Ok(())
}
pub fn apply(directory: &Path, op: &Op) -> io::Result<Updated> {
    let file_path = directory.join("folder-views.bin");
    if matches!(op, Op::Read) {
        return Ok(Updated {
            document: read(&file_path)?.document,
            unsynced: false,
            detail: None,
            notice: "Vistas de carpetas cargadas",
        });
    }
    if let Op::Save(e) | Op::Forget(e) = op
        && (!path_valid(&e.path) || !e.settings.valid())
    {
        return Err(invalid());
    }
    let guard = lock(directory)?;
    let mut old = read(&file_path)?;
    let notice = match op {
        Op::Save(e) => {
            match old
                .document
                .rows
                .binary_search_by(|r| key(&r.path).cmp(key(&e.path)))
            {
                Ok(i) => old.document.rows[i].settings = e.settings,
                Err(i) => {
                    if old.document.rows.len() == MAX_ROWS {
                        return Err(invalid());
                    }
                    let current = 13
                        + old
                            .document
                            .rows
                            .iter()
                            .map(|r| 10 + key(&r.path).len())
                            .sum::<usize>();
                    if current + 10 + key(&e.path).len() > MAX_BYTES {
                        return Err(invalid());
                    }
                    old.document.rows.insert(i, e.clone());
                }
            }
            "Vista de esta carpeta guardada"
        }
        Op::Forget(e) => {
            let i = old
                .document
                .rows
                .binary_search_by(|r| key(&r.path).cmp(key(&e.path)))
                .map_err(|_| io::Error::other("Vista cambió; recarga antes de quitarla"))?;
            if old.document.rows[i].settings != e.settings {
                return Err(io::Error::other("Vista cambió; recarga antes de quitarla"));
            }
            old.document.rows.remove(i);
            "Preferencia quitada; carpeta intacta"
        }
        Op::Sync => {
            if let Some(file) = &old.file {
                file.sync_all()?;
            }
            File::open(directory)?.sync_all()?;
            return Ok(Updated {
                document: old.document,
                unsynced: false,
                detail: None,
                notice: "Vistas de carpetas sincronizadas",
            });
        }
        Op::Read => unreachable!(),
    };
    let bytes = encode(&old.document)?;
    let publication = crate::persistence::atomic_write_checked(&file_path, &bytes, || {
        check_snapshot(directory, &guard.file, &old, &file_path)
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
            "Publicado, pero falta sincronizar vistas de carpetas"
        } else {
            notice
        },
    })
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    fn entry(path: &str) -> Entry {
        Entry {
            path: path.into(),
            settings: Settings {
                view: 3,
                sort: 2,
                descending: true,
                zoom: 135,
                scroll: 12345,
            },
        }
    }
    #[test]
    fn raw_keys_and_settings_roundtrip_without_lexical_normalization() {
        let mut a = entry("/á//%2F/../x");
        a.path = PathBuf::from(std::ffi::OsString::from_vec(
            b"/raw-\xff//%2F/../x".to_vec(),
        ));
        let b = entry("/z");
        let doc = Document { rows: vec![a, b] };
        let wire = encode(&doc).unwrap();
        let decoded = decode(&wire).unwrap();
        for (a, b) in doc.rows.iter().zip(&decoded.rows) {
            assert_eq!(key(&a.path), key(&b.path));
            assert_eq!(a.settings, b.settings);
        }
        assert_eq!(
            decoded.get(&doc.rows[0].path).unwrap().settings.scroll,
            12345
        );
        assert!(decoded.get(Path::new("/raw-other")).is_none());
        for n in 0..wire.len() {
            assert!(decode(&wire[..n]).is_err(), "{n}");
        }
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        let mut future = wire.clone();
        future[8] = 2;
        assert!(decode(&future).is_err());
        let mut foreign = wire.clone();
        foreign[10] = 3;
        assert!(decode(&foreign).is_err());
        let mut bad = wire;
        let zoom = 13 + 2 + key(&doc.rows[0].path).len() + 3;
        bad[zoom] = 64;
        assert!(decode(&bad).is_err());
    }
    #[test]
    fn rejects_unsorted_duplicate_and_aggregate_overflow() {
        for rows in [
            vec![entry("/z"), entry("/a")],
            vec![entry("/a"), entry("/a")],
        ] {
            assert!(encode(&Document { rows }).is_err());
        }
        let rows = (0..MAX_ROWS)
            .map(|n| entry(&format!("/{n:03}{}", "a".repeat(2000))))
            .collect();
        assert!(encode(&Document { rows }).is_err());
        let mut e = entry("relative");
        assert!(
            encode(&Document {
                rows: vec![e.clone()]
            })
            .is_err()
        );
        e.path = "/ok".into();
        e.settings.scroll = 100_000_001;
        assert!(encode(&Document { rows: vec![e] }).is_err());
    }
    #[test]
    fn save_merges_latest_and_stale_forget_cannot_remove_updated_value() {
        let root = crate::persistence::test_dir("folder-views-merge");
        let a = entry("/not-created/a");
        let b = entry("/not-created/b");
        apply(&root, &Op::Save(a.clone())).unwrap();
        let merged = apply(&root, &Op::Save(b.clone())).unwrap();
        assert_eq!(merged.document.rows.len(), 2);
        let mut newer = a.clone();
        newer.settings.zoom = 180;
        apply(&root, &Op::Save(newer.clone())).unwrap();
        let before = fs::read(root.join("folder-views.bin")).unwrap();
        assert!(apply(&root, &Op::Forget(a)).is_err());
        assert_eq!(before, fs::read(root.join("folder-views.bin")).unwrap());
        let reduced = apply(&root, &Op::Forget(newer)).unwrap();
        assert_eq!(reduced.document.rows.len(), 1);
        assert_eq!(reduced.document.get(&b.path).unwrap().settings, b.settings);
        apply(&root, &Op::Sync).unwrap();
        assert!(!root.join("not-created").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn full_document_allows_update_but_never_silently_evicts() {
        let root = crate::persistence::test_dir("folder-views-full");
        let doc = Document {
            rows: (0..MAX_ROWS).map(|n| entry(&format!("/{n:03}"))).collect(),
        };
        fs::write(root.join("folder-views.bin"), encode(&doc).unwrap()).unwrap();
        let mut changed = entry("/000");
        changed.settings.view = 1;
        assert_eq!(
            apply(&root, &Op::Save(changed))
                .unwrap()
                .document
                .rows
                .len(),
            MAX_ROWS
        );
        let before = fs::read(root.join("folder-views.bin")).unwrap();
        assert!(apply(&root, &Op::Save(entry("/extra"))).is_err());
        assert_eq!(before, fs::read(root.join("folder-views.bin")).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn readonly_missing_does_not_create_config_or_folder() {
        let root = crate::persistence::test_dir("folder-views-read");
        let missing = root.join("missing");
        assert!(apply(&missing, &Op::Read).unwrap().document.rows.is_empty());
        assert!(!missing.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn final_check_rejects_target_or_lock_replacement() {
        let root = crate::persistence::test_dir("folder-views-final-check");
        apply(&root, &Op::Save(entry("/a"))).unwrap();
        let guard = lock(&root).unwrap();
        let file = root.join("folder-views.bin");
        let old = read(&file).unwrap();
        check_snapshot(&root, &guard.file, &old, &file).unwrap();
        let replacement = root.join("replacement");
        fs::write(&replacement, encode(&Document::default()).unwrap()).unwrap();
        fs::rename(&replacement, &file).unwrap();
        assert!(check_snapshot(&root, &guard.file, &old, &file).is_err());
        let current = read(&file).unwrap();
        let lock_path = root.join("folder-views.lock");
        fs::rename(&lock_path, root.join("old-lock")).unwrap();
        fs::write(&lock_path, b"").unwrap();
        assert!(check_snapshot(&root, &guard.file, &current, &file).is_err());
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn final_check_rejects_creation_after_absent_snapshot() {
        let root = crate::persistence::test_dir("folder-views-created");
        let guard = lock(&root).unwrap();
        let file = root.join("folder-views.bin");
        let absent = read(&file).unwrap();
        check_snapshot(&root, &guard.file, &absent, &file).unwrap();
        fs::write(&file, encode(&Document::default()).unwrap()).unwrap();
        assert!(check_snapshot(&root, &guard.file, &absent, &file).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn wire_rejects_noncanonical_order_and_invalid_flags_before_materialization() {
        let wire = encode(&Document {
            rows: vec![entry("/a"), entry("/b")],
        })
        .unwrap();
        let mut duplicate = wire.clone();
        duplicate[28] = b'a';
        assert!(decode(&duplicate).is_err());
        let mut swapped = wire.clone();
        swapped[16] = b'z';
        assert!(decode(&swapped).is_err());
        for (offset, value) in [(17, 5), (18, 3), (19, 2), (20, 181)] {
            let mut bad = wire.clone();
            bad[offset] = value;
            assert!(decode(&bad).is_err());
        }
        let mut excessive = wire;
        excessive[11..13].copy_from_slice(&257u16.to_le_bytes());
        assert!(decode(&excessive).is_err());
    }
    #[test]
    fn busy_invalid_future_and_symlink_are_preserved() {
        let root = crate::persistence::test_dir("folder-views-preserve");
        let guard = lock(&root).unwrap();
        assert_eq!(
            apply(&root, &Op::Save(entry("/a"))).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(guard);
        let file = root.join("folder-views.bin");
        for bytes in [
            b"bad".to_vec(),
            {
                let mut x = encode(&Document::default()).unwrap();
                x[8] = 99;
                x
            },
            vec![0; MAX_BYTES + 1],
        ] {
            fs::write(&file, &bytes).unwrap();
            assert!(apply(&root, &Op::Save(entry("/a"))).is_err());
            assert_eq!(fs::read(&file).unwrap(), bytes);
        }
        fs::remove_file(&file).unwrap();
        let target = root.join("precious");
        fs::write(&target, b"original").unwrap();
        symlink(&target, &file).unwrap();
        assert!(apply(&root, &Op::Read).is_err());
        assert!(apply(&root, &Op::Save(entry("/a"))).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        fs::remove_file(&file).unwrap();
        fs::remove_file(root.join("folder-views.lock")).unwrap();
        symlink(&target, root.join("folder-views.lock")).unwrap();
        assert!(apply(&root, &Op::Save(entry("/a"))).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
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
            "folder-view-lock-child-drop"
        } else {
            "folder-view-lock-parent-drop"
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
