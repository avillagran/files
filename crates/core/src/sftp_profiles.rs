//! Bounded SFTP connection drafts. Loading never connects, probes, mounts or authorizes I/O.
//! Only host/user/port/directory and a display label are stored: no credentials or options.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::Path,
};
pub const MAX_BYTES: usize = 128 * 1024;
pub const MAX_ROWS: usize = 32;
pub const MAX_LABEL: usize = 96;
pub const FIELD_LIMITS: [usize; 4] = [253, 64, 5, 4096];
const MAGIC: &[u8; 8] = b"FILESSFP";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Criteria {
    pub fields: [String; 4],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub label: String,
    pub criteria: Criteria,
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
        "Perfiles SFTP inválidos, incompatibles o fuera de presupuesto; se conservan sin modificar",
    )
}
fn label_valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= MAX_LABEL && !s.chars().any(char::is_control)
}
fn fields_valid(fields: [&str; 4]) -> bool {
    fields
        .iter()
        .zip(FIELD_LIMITS)
        .all(|(s, cap)| s.len() <= cap)
        && crate::sftp_target::Target::validate(fields[0], fields[1], fields[2], fields[3]).is_ok()
}
pub fn validate_criteria(c: &Criteria) -> io::Result<()> {
    if fields_valid(c.fields.each_ref().map(String::as_str)) {
        Ok(())
    } else {
        Err(invalid())
    }
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
            .filter(|&n| n <= self.bytes.len())
            .ok_or_else(invalid)?;
        let b = &self.bytes[self.at..end];
        self.at = end;
        Ok(b)
    }
    fn u16(&mut self) -> io::Result<usize> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize)
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn text(&mut self, cap: usize) -> io::Result<&'a str> {
        let len = self.u16()?;
        if len > cap {
            return Err(invalid());
        }
        std::str::from_utf8(self.take(len)?).map_err(|_| invalid())
    }
}
// First pass borrows all wire data and checks the whole catalogue before materialization.
fn walk(
    bytes: &[u8],
    mut row: impl FnMut(u64, &str, [&str; 4]) -> io::Result<()>,
) -> io::Result<(u64, usize)> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut r = Reader { bytes, at: 0 };
    if r.take(8)? != MAGIC || r.u16()? != 1 {
        return Err(invalid());
    }
    let next = r.u64()?;
    let count = r.u16()?;
    if next == 0 || count > MAX_ROWS {
        return Err(invalid());
    }
    let mut ids = [0; MAX_ROWS];
    let mut previous = [[""; 4]; MAX_ROWS];
    for i in 0..count {
        let id = r.u64()?;
        if id == 0 || id >= next || ids[..i].contains(&id) {
            return Err(invalid());
        }
        ids[i] = id;
        let label = r.text(MAX_LABEL)?;
        let mut fields = [""; 4];
        for (s, cap) in fields.iter_mut().zip(FIELD_LIMITS) {
            *s = r.text(cap)?;
        }
        if !label_valid(label) || !fields_valid(fields) || previous[..i].contains(&fields) {
            return Err(invalid());
        }
        previous[i] = fields;
        row(id, label, fields)?;
    }
    if r.at != bytes.len() {
        return Err(invalid());
    }
    Ok((next, count))
}
pub fn decode(bytes: &[u8]) -> io::Result<Document> {
    let (next, count) = walk(bytes, |_, _, _| Ok(()))?;
    let mut rows = Vec::with_capacity(count);
    walk(bytes, |id, label, fields| {
        rows.push(Entry {
            id,
            label: label.into(),
            criteria: Criteria {
                fields: fields.map(str::to_owned),
            },
        });
        Ok(())
    })?;
    Ok(Document { next, rows })
}
fn encoded_size(doc: &Document) -> io::Result<usize> {
    if doc.next == 0 || doc.rows.len() > MAX_ROWS {
        return Err(invalid());
    }
    let mut size = 20usize;
    for (i, e) in doc.rows.iter().enumerate() {
        if e.id == 0
            || e.id >= doc.next
            || !label_valid(&e.label)
            || doc.rows[..i]
                .iter()
                .any(|v| v.id == e.id || v.criteria == e.criteria)
        {
            return Err(invalid());
        }
        validate_criteria(&e.criteria)?;
        size = size
            .checked_add(
                18 + e.label.len() + e.criteria.fields.iter().map(String::len).sum::<usize>(),
            )
            .ok_or_else(invalid)?;
        if size > MAX_BYTES {
            return Err(invalid());
        }
    }
    Ok(size)
}
pub fn encode(doc: &Document) -> io::Result<Vec<u8>> {
    let size = encoded_size(doc)?;
    let mut b = Vec::with_capacity(size);
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&doc.next.to_le_bytes());
    b.extend_from_slice(&(doc.rows.len() as u16).to_le_bytes());
    for e in &doc.rows {
        b.extend_from_slice(&e.id.to_le_bytes());
        for text in std::iter::once(&e.label).chain(e.criteria.fields.iter()) {
            b.extend_from_slice(&(text.len() as u16).to_le_bytes());
            b.extend_from_slice(text.as_bytes());
        }
    }
    debug_assert_eq!(b.len(), size);
    Ok(b)
}
#[derive(Clone, Debug)]
pub enum Op {
    Read,
    Add { criteria: Criteria, label: String },
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
        .map_err(|_| io::Error::other("Perfiles SFTP cambiaron durante lectura; recarga"))?;
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
        .open(directory.join("sftp-profiles.lock"))?;
    if !f.metadata()?.is_file() {
        return Err(invalid());
    }
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Perfiles SFTP ocupados por otra ventana; reintenta",
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
        "Escritura coordinada de perfiles SFTP pendiente en esta plataforma",
    ))
}
pub fn apply(directory: &Path, op: &Op) -> io::Result<Updated> {
    let file_path = directory.join("sftp-profiles.bin");
    if matches!(op, Op::Read) {
        return Ok(Updated {
            document: read(&file_path)?.document,
            unsynced: false,
            detail: None,
            notice: "Perfiles SFTP cargados",
        });
    }
    match op {
        Op::Add { criteria, label }
            if validate_criteria(criteria).is_err() || !label_valid(label) =>
        {
            return Err(invalid());
        }
        Op::Remove(e) | Op::Rename { expected: e, .. }
            if e.id == 0 || validate_criteria(&e.criteria).is_err() || !label_valid(&e.label) =>
        {
            return Err(invalid());
        }
        Op::Rename { label, .. } if !label_valid(label) => return Err(invalid()),
        _ => {}
    }
    let guard = lock(directory)?;
    let mut old = read(&file_path)?;
    let notice = match op {
        Op::Add { criteria, label } => {
            if old.document.rows.iter().any(|e| e.criteria == *criteria) {
                return Ok(Updated {
                    document: old.document,
                    unsynced: false,
                    detail: None,
                    notice: "Este perfil SFTP ya estaba guardado",
                });
            }
            // Size validation happens on borrowed entries before cloning the new payload.
            let current = encoded_size(&old.document)?;
            let extra = 18 + label.len() + criteria.fields.iter().map(String::len).sum::<usize>();
            if old.document.rows.len() == MAX_ROWS
                || current.checked_add(extra).is_none_or(|n| n > MAX_BYTES)
            {
                return Err(io::Error::other(
                    "Límite de perfiles SFTP:32 entradas/128KiB; quita alguna antes de guardar",
                ));
            }
            let id = old.document.next;
            old.document.next = id.checked_add(1).ok_or_else(invalid)?;
            old.document.rows.push(Entry {
                id,
                label: label.clone(),
                criteria: criteria.clone(),
            });
            "Perfil SFTP guardado"
        }
        Op::Remove(expected) => {
            let i = old
                .document
                .rows
                .iter()
                .position(|e| e == expected)
                .ok_or_else(|| {
                    io::Error::other("El perfil SFTP cambió; recarga antes de quitar")
                })?;
            old.document.rows.remove(i);
            "Perfil SFTP quitado; archivos intactos"
        }
        Op::Rename { expected, label } => {
            let i = old
                .document
                .rows
                .iter()
                .position(|e| e == expected)
                .ok_or_else(|| {
                    io::Error::other("El perfil SFTP cambió; recarga antes de renombrar")
                })?;
            let current = encoded_size(&old.document)?;
            if current
                .saturating_sub(expected.label.len())
                .saturating_add(label.len())
                > MAX_BYTES
            {
                return Err(invalid());
            }
            old.document.rows[i].label = label.clone();
            "Etiqueta actualizada"
        }
        Op::Sync => {
            File::open(directory)?.sync_all()?;
            return Ok(Updated {
                document: old.document,
                unsynced: false,
                detail: None,
                notice: "Perfiles SFTP sincronizados",
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
            let b = fs::symlink_metadata(directory.join("sftp-profiles.lock"))?;
            if !b.is_file() || a.dev() != b.dev() || a.ino() != b.ino() {
                return Err(io::Error::other("Lock de perfiles SFTP cambió"));
            }
        }
        match (&old.file, old.revision) {
            (Some(file), Some(rev)) => rev.revalidate(file, &file_path).map_err(|_| {
                io::Error::other("Perfiles SFTP cambiaron antes de publicar; recarga")
            })?,
            (None, None) => match fs::symlink_metadata(&file_path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                _ => {
                    return Err(io::Error::other(
                        "Otro escritor creó las perfiles SFTP; recarga",
                    ));
                }
            },
            _ => return Err(invalid()),
        }
        Ok(())
    })?;
    let unsynced = matches!(publication, crate::persistence::Publication::Unsynced(_));
    let detail = match publication {
        crate::persistence::Publication::Unsynced(e) => Some(e.to_string()),
        _ => None,
    };
    Ok(Updated {
        document: old.document,
        unsynced,
        detail,
        notice: if unsynced {
            "Publicado; falta sincronizar perfiles SFTP"
        } else {
            notice
        },
    })
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn criteria(host: &str) -> Criteria {
        Criteria {
            fields: [
                host.into(),
                "u_ser".into(),
                "0022".into(),
                "/datos/ñ/%41/$()".into(),
            ],
        }
    }
    fn add(host: &str) -> Op {
        Op::Add {
            criteria: criteria(host),
            label: "Servidor de pruebas".into(),
        }
    }
    #[test]
    fn roundtrip_preserves_exact_draft_and_rejects_every_partial_frame() {
        let doc = Document {
            next: 2,
            rows: vec![Entry {
                id: 1,
                label: "Trabajo ñ".into(),
                criteria: criteria("[::1]"),
            }],
        };
        let bytes = encode(&doc).unwrap();
        assert_eq!(bytes.len(), encoded_size(&doc).unwrap());
        assert_eq!(decode(&bytes).unwrap().rows, doc.rows);
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end]).is_err(), "prefix {end}");
        }
        let mut more = bytes.clone();
        more.push(0);
        assert!(decode(&more).is_err());
        let mut future = bytes.clone();
        future[8] = 2;
        assert!(decode(&future).is_err());
        let mut unknown = bytes;
        unknown[0] = b'X';
        assert!(decode(&unknown).is_err());
    }
    #[test]
    fn invalid_fields_credentials_options_controls_and_labels_are_rejected() {
        for fields in [
            ["user:password@host", "", "", ""],
            ["-option", "", "", ""],
            ["host", "pass@user", "", ""],
            ["host", "", "65536", ""],
            ["host", "", "0", ""],
            ["host", "", "22", "line\nbreak"],
        ] {
            assert!(
                validate_criteria(&Criteria {
                    fields: fields.map(str::to_owned)
                })
                .is_err()
            );
        }
        for label in ["", " \t", "line\nbreak"] {
            assert!(
                encode(&Document {
                    next: 2,
                    rows: vec![Entry {
                        id: 1,
                        label: label.into(),
                        criteria: criteria("host")
                    }]
                })
                .is_err()
            );
        }
    }
    #[test]
    fn row_byte_and_identity_budgets_are_symmetric() {
        let mut doc = Document {
            next: 34,
            rows: (1..=32)
                .map(|id| Entry {
                    id,
                    label: "s".repeat(MAX_LABEL),
                    criteria: criteria(&format!("host{id}")),
                })
                .collect(),
        };
        let bytes = encode(&doc).unwrap();
        assert_eq!(decode(&bytes).unwrap().rows, doc.rows);
        doc.rows.push(Entry {
            id: 33,
            label: "extra".into(),
            criteria: criteria("extra"),
        });
        assert!(encode(&doc).is_err());
        doc.rows.pop();
        doc.rows[1].criteria = doc.rows[0].criteria.clone();
        assert!(encode(&doc).is_err());
        doc.rows[1].criteria = criteria("other");
        doc.rows[1].id = 1;
        assert!(encode(&doc).is_err());
        doc.rows[1].id = 2;
        for row in &mut doc.rows {
            row.criteria.fields[3] = "/".repeat(4080);
        }
        assert!(encode(&doc).is_err()); // aggregate, not individual fields
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn store_merges_updates_deduplicates_and_requires_exact_revision() {
        let parent = crate::persistence::test_dir("sftp-profiles-merge");
        let dir = parent.join("not-created");
        assert!(apply(&dir, &Op::Read).unwrap().document.rows.is_empty());
        assert!(!dir.exists()); // read of missing store does not create data
        let a = apply(&dir, &add("a")).unwrap();
        assert!(!a.unsynced);
        let a_entry = a.document.rows[0].clone();
        assert_eq!(apply(&dir, &add("a")).unwrap().document.rows.len(), 1);
        assert_eq!(apply(&dir, &add("b")).unwrap().document.rows.len(), 2);
        let renamed = apply(
            &dir,
            &Op::Rename {
                expected: a_entry.clone(),
                label: "Nuevo".into(),
            },
        )
        .unwrap();
        assert!(apply(&dir, &Op::Remove(a_entry)).is_err());
        let current = renamed.document.rows[0].clone();
        assert_eq!(
            apply(&dir, &Op::Remove(current))
                .unwrap()
                .document
                .rows
                .len(),
            1
        );
        assert!(!apply(&dir, &Op::Sync).unwrap().unsynced);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.join("sftp-profiles.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(parent).unwrap();
    }
    #[test]
    fn busy_invalid_future_symlink_and_lock_replacement_preserve_data() {
        use std::os::unix::fs::symlink;
        let dir = crate::persistence::test_dir("sftp-profiles-invalid");
        let guard = lock(&dir).unwrap();
        assert_eq!(
            apply(&dir, &add("a")).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(guard);
        let d = apply(&dir, &add("a")).unwrap().document;
        let mut bytes = encode(&d).unwrap();
        bytes[8] = 2;
        let file = dir.join("sftp-profiles.bin");
        fs::write(&file, &bytes).unwrap();
        assert!(apply(&dir, &add("b")).is_err());
        assert_eq!(fs::read(&file).unwrap(), bytes);
        fs::rename(&file, dir.join("other")).unwrap();
        symlink(dir.join("other"), &file).unwrap();
        assert!(apply(&dir, &Op::Read).is_err());
        assert!(apply(&dir, &add("b")).is_err());
        assert_eq!(fs::read(dir.join("other")).unwrap(), bytes);
        fs::remove_file(&file).unwrap();
        fs::remove_file(dir.join("sftp-profiles.lock")).unwrap();
        symlink(dir.join("other"), dir.join("sftp-profiles.lock")).unwrap();
        assert!(apply(&dir, &add("b")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn fifo_and_oversize_are_rejected_without_mutation() {
        use std::os::unix::ffi::OsStrExt;
        let dir = crate::persistence::test_dir("sftp-profiles-special");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sftp-profiles.bin");
        let name = std::ffi::CString::new(file.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(apply(&dir, &Op::Read).is_err());
        assert!(apply(&dir, &add("a")).is_err());
        fs::remove_file(&file).unwrap();
        fs::write(&file, vec![0; MAX_BYTES + 1]).unwrap();
        assert!(apply(&dir, &Op::Read).is_err());
        assert!(apply(&dir, &add("a")).is_err());
        assert_eq!(fs::metadata(&file).unwrap().len(), (MAX_BYTES + 1) as u64);
        fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(all(test, target_os = "linux"))]
mod fork_tests {
    use super::*;
    // The child executes only inherited File drop and libc read/write/_exit; no
    // allocator, logging, test assertions or other post-fork runtime locks.
    fn fork_case(child_drops: bool) {
        let root = crate::persistence::test_dir(if child_drops {
            "sftp-profiles-lock-child-drop"
        } else {
            "sftp-profiles-lock-parent-drop"
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
