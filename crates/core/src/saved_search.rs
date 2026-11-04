//! Bounded saved criteria. Loading data never executes a search or authorizes I/O.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};
pub const MAX_BYTES: usize = 128 * 1024;
pub const MAX_ROWS: usize = 32;
pub const MAX_LABEL: usize = 96;
const MAGIC: &[u8; 8] = b"FILESSCH";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Criteria {
    pub root: PathBuf,
    pub query: String,
    pub content: String,
    pub fields: [String; 4],
    pub hidden: bool,
    pub content_mode: bool,
    pub content_regex: bool,
    pub order: u8,
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
            rows: vec![],
        }
    }
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Búsquedas guardadas inválidas, incompatibles o fuera de presupuesto; archivo conservado",
    )
}
fn label_valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= MAX_LABEL && !s.chars().any(char::is_control)
}
fn platform() -> u8 {
    if cfg!(unix) { 1 } else { 2 }
}
#[cfg(unix)]
fn raw_path(bytes: &[u8]) -> &Path {
    use std::os::unix::ffi::OsStrExt;
    Path::new(std::ffi::OsStr::from_bytes(bytes))
}
#[cfg(not(unix))]
fn raw_path(bytes: &[u8]) -> &Path {
    Path::new(std::str::from_utf8(bytes).unwrap_or(""))
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Borrowed<'a> {
    root: &'a [u8],
    query: &'a str,
    content: &'a str,
    fields: [&'a str; 4],
    flags: u8,
    order: u8,
}
impl<'a> From<&'a Criteria> for Borrowed<'a> {
    fn from(c: &'a Criteria) -> Self {
        Self {
            root: c.root.as_os_str().as_encoded_bytes(),
            query: &c.query,
            content: &c.content,
            fields: std::array::from_fn(|i| c.fields[i].as_str()),
            flags: u8::from(c.hidden)
                | u8::from(c.content_mode) << 1
                | u8::from(c.content_regex) << 2,
            order: c.order,
        }
    }
}
impl Borrowed<'_> {
    fn valid(&self) -> bool {
        !self.root.is_empty()
            && self.root.len() <= 4096
            && !self.root.contains(&0)
            && raw_path(self.root).is_absolute()
            && self.query.len() <= 4096
            && !self.query.contains('\0')
            && self.content.len() <= 1024
            && !self.content.contains('\0')
            && self
                .fields
                .iter()
                .all(|s| s.len() <= 32 && !s.chars().any(char::is_control))
            && self.flags <= 7
            && self.order <= 2
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
            .filter(|&e| e <= self.bytes.len())
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
    fn text(&mut self, max: usize) -> io::Result<&'a str> {
        let n = self.u16()?;
        if n > max {
            return Err(invalid());
        }
        std::str::from_utf8(self.take(n)?).map_err(|_| invalid())
    }
}
fn walk(
    bytes: &[u8],
    mut row: impl FnMut(u64, &str, Borrowed<'_>) -> io::Result<()>,
) -> io::Result<(u64, usize)> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut r = Reader { bytes, at: 0 };
    let magic = r.take(8)?;
    let version = r.u16()?;
    if magic != MAGIC || !matches!(version, 1 | 2) || r.take(1)?[0] != platform() {
        return Err(invalid());
    }
    let next = r.u64()?;
    let count = r.u16()?;
    if next == 0 || count > MAX_ROWS {
        return Err(invalid());
    }
    let mut ids = [0; MAX_ROWS];
    let mut seen = [None; MAX_ROWS];
    for i in 0..count {
        let id = r.u64()?;
        let label = r.text(MAX_LABEL)?;
        let n = r.u16()?;
        if n > 4096 {
            return Err(invalid());
        }
        let root = r.take(n)?;
        let query = r.text(4096)?;
        let content = r.text(1024)?;
        let fields = [r.text(32)?, r.text(32)?, r.text(32)?, r.text(32)?];
        let flags = r.take(1)?[0];
        let order = r.take(1)?[0];
        let c = Borrowed {
            root,
            query,
            content,
            fields,
            flags,
            order,
        };
        if id == 0
            || id >= next
            || ids[..i].contains(&id)
            || !label_valid(label)
            || !c.valid()
            || (version == 1 && flags > 3)
            || seen[..i].contains(&Some(c))
        {
            return Err(invalid());
        }
        ids[i] = id;
        seen[i] = Some(c);
        row(id, label, c)?;
    }
    if r.at != bytes.len() {
        return Err(invalid());
    }
    Ok((next, count))
}
pub fn validate_criteria(c: &Criteria) -> io::Result<()> {
    if Borrowed::from(c).valid() {
        Ok(())
    } else {
        Err(invalid())
    }
}
pub fn decode(bytes: &[u8]) -> io::Result<Document> {
    let (next, count) = walk(bytes, |_, _, _| Ok(()))?;
    let mut rows = Vec::with_capacity(count);
    walk(bytes, |id, label, c| {
        rows.push(Entry {
            id,
            label: label.into(),
            criteria: Criteria {
                root: raw_path(c.root).to_path_buf(),
                query: c.query.into(),
                content: c.content.into(),
                fields: c.fields.map(str::to_owned),
                hidden: c.flags & 1 != 0,
                content_mode: c.flags & 2 != 0,
                content_regex: c.flags & 4 != 0,
                order: c.order,
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
    let mut n = 21usize;
    for (i, e) in doc.rows.iter().enumerate() {
        let c = Borrowed::from(&e.criteria);
        if e.id == 0
            || e.id >= doc.next
            || !label_valid(&e.label)
            || !c.valid()
            || doc.rows[..i]
                .iter()
                .any(|old| old.id == e.id || old.criteria == e.criteria)
        {
            return Err(invalid());
        }
        n = n
            .checked_add(
                26 + e.label.len()
                    + c.root.len()
                    + c.query.len()
                    + c.content.len()
                    + c.fields.iter().map(|s| s.len()).sum::<usize>(),
            )
            .filter(|&v| v <= MAX_BYTES)
            .ok_or_else(invalid)?;
    }
    Ok(n)
}
pub fn encode(doc: &Document) -> io::Result<Vec<u8>> {
    let n = encoded_size(doc)?;
    let mut out = Vec::with_capacity(n);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(
        &(if doc.rows.iter().any(|r| r.criteria.content_regex) {
            2u16
        } else {
            1u16
        })
        .to_le_bytes(),
    );
    out.push(platform());
    out.extend_from_slice(&doc.next.to_le_bytes());
    out.extend_from_slice(&(doc.rows.len() as u16).to_le_bytes());
    fn part(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u16).to_le_bytes());
        out.extend_from_slice(s)
    }
    for e in &doc.rows {
        let c = Borrowed::from(&e.criteria);
        out.extend_from_slice(&e.id.to_le_bytes());
        part(&mut out, e.label.as_bytes());
        part(&mut out, c.root);
        part(&mut out, c.query.as_bytes());
        part(&mut out, c.content.as_bytes());
        for f in c.fields {
            part(&mut out, f.as_bytes())
        }
        out.push(c.flags);
        out.push(c.order)
    }
    debug_assert_eq!(out.len(), n);
    Ok(out)
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
        .map_err(|_| io::Error::other("Búsquedas guardadas cambiaron durante lectura; recarga"))?;
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
        .open(directory.join("searches.lock"))?;
    if !f.metadata()?.is_file() {
        return Err(invalid());
    }
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Búsquedas guardadas ocupados por otra ventana; reintenta",
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
        "Escritura coordinada de búsquedas guardadas pendiente en esta plataforma",
    ))
}
pub fn apply(directory: &Path, op: &Op) -> io::Result<Updated> {
    let file_path = directory.join("searches.bin");
    if matches!(op, Op::Read) {
        return Ok(Updated {
            document: read(&file_path)?.document,
            unsynced: false,
            detail: None,
            notice: "Búsquedas cargadas",
        });
    }
    match op {
        Op::Add { criteria, label } if !Borrowed::from(criteria).valid() || !label_valid(label) => {
            return Err(invalid());
        }
        Op::Remove(e) | Op::Rename { expected: e, .. }
            if e.id == 0 || !Borrowed::from(&e.criteria).valid() || !label_valid(&e.label) =>
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
                    notice: "Esta búsqueda ya estaba guardada",
                });
            }
            // Size validation happens on borrowed entries before cloning the new payload.
            let current = encoded_size(&old.document)?;
            let c = Borrowed::from(criteria);
            let extra = 26
                + label.len()
                + c.root.len()
                + c.query.len()
                + c.content.len()
                + c.fields.iter().map(|s| s.len()).sum::<usize>();
            if old.document.rows.len() == MAX_ROWS
                || current.checked_add(extra).is_none_or(|n| n > MAX_BYTES)
            {
                return Err(io::Error::other(
                    "Límite de búsquedas:32 entradas/128KiB; quita alguna antes de guardar",
                ));
            }
            let id = old.document.next;
            old.document.next = id.checked_add(1).ok_or_else(invalid)?;
            old.document.rows.push(Entry {
                id,
                label: label.clone(),
                criteria: criteria.clone(),
            });
            "Búsqueda guardada"
        }
        Op::Remove(expected) => {
            let i = old
                .document
                .rows
                .iter()
                .position(|e| e == expected)
                .ok_or_else(|| io::Error::other("La búsqueda cambió; recarga antes de quitar"))?;
            old.document.rows.remove(i);
            "Búsqueda quitada; archivos intactos"
        }
        Op::Rename { expected, label } => {
            let i = old
                .document
                .rows
                .iter()
                .position(|e| e == expected)
                .ok_or_else(|| {
                    io::Error::other("La búsqueda cambió; recarga antes de renombrar")
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
                notice: "Búsquedas sincronizadas",
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
            let b = fs::symlink_metadata(directory.join("searches.lock"))?;
            if !b.is_file() || a.dev() != b.dev() || a.ino() != b.ino() {
                return Err(io::Error::other("Lock de búsquedas cambió"));
            }
        }
        match (&old.file, old.revision) {
            (Some(file), Some(rev)) => rev
                .revalidate(file, &file_path)
                .map_err(|_| io::Error::other("Búsquedas cambiaron antes de publicar; recarga"))?,
            (None, None) => match fs::symlink_metadata(&file_path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                _ => {
                    return Err(io::Error::other(
                        "Otro escritor creó las búsquedas; recarga",
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
            "Publicado; falta sincronizar búsquedas"
        } else {
            notice
        },
    })
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn criteria(root: &str) -> Criteria {
        Criteria {
            root: root.into(),
            query: "ext:rs !draft".into(),
            content: "ñ\ntext".into(),
            fields: [
                "1KiB".into(),
                "2MiB".into(),
                "2026-01-01".into(),
                "2027-01-01".into(),
            ],
            hidden: true,
            content_mode: true,
            content_regex: false,
            order: 2,
        }
    }
    fn add(root: &str) -> Op {
        Op::Add {
            criteria: criteria(root),
            label: "Código".into(),
        }
    }
    #[test]
    fn regex_mode_upgrades_only_when_needed_and_never_leaks_into_v1() {
        let literal = criteria("/fixture");
        let mut regex = literal.clone();
        regex.content_regex = true;
        let mut doc = Document {
            next: 2,
            rows: vec![Entry {
                id: 1,
                label: "literal".into(),
                criteria: literal.clone(),
            }],
        };
        let legacy = encode(&doc).unwrap();
        assert_eq!(u16::from_le_bytes([legacy[8], legacy[9]]), 1);
        assert_eq!(decode(&legacy).unwrap().rows[0].criteria, literal);
        doc.next = 3;
        doc.rows.push(Entry {
            id: 2,
            label: "regex".into(),
            criteria: regex.clone(),
        });
        let wire = encode(&doc).unwrap();
        assert_eq!(u16::from_le_bytes([wire[8], wire[9]]), 2);
        assert_eq!(decode(&wire).unwrap().rows, doc.rows);
        let mut forged = wire.clone();
        forged[8] = 1;
        assert!(decode(&forged).is_err());
        doc.rows.pop();
        assert_eq!(encode(&doc).unwrap()[8], 1);
        assert!(Borrowed::from(&literal) != Borrowed::from(&regex));
    }
    #[test]
    fn codec_preserves_raw_root_fields_unicode_and_rejects_all_partial_inputs() {
        use std::os::unix::ffi::OsStringExt;
        let mut c = criteria("/none");
        c.root = std::ffi::OsString::from_vec(b"/raw-\xff/%41/$()".to_vec()).into();
        let doc = Document {
            next: 2,
            rows: vec![Entry {
                id: 1,
                label: "Código ñ".into(),
                criteria: c,
            }],
        };
        let wire = encode(&doc).unwrap();
        assert_eq!(decode(&wire).unwrap().rows, doc.rows);
        for n in 0..wire.len() {
            assert!(decode(&wire[..n]).is_err())
        }
        let mut invalid = wire.clone();
        invalid[8] = 3;
        assert!(decode(&invalid).is_err());
        invalid = wire.clone();
        invalid.push(0);
        assert!(decode(&invalid).is_err());
        invalid = wire;
        *invalid.last_mut().unwrap() = 3;
        assert!(decode(&invalid).is_err());
    }
    #[test]
    fn borrowed_limits_duplicate_criteria_and_aggregate_budget() {
        let mut doc = Document {
            next: 34,
            rows: (1..=32)
                .map(|i| Entry {
                    id: i,
                    label: "x".into(),
                    criteria: criteria(&format!("/{i}")),
                })
                .collect(),
        };
        assert!(encode(&doc).is_ok());
        doc.rows.push(Entry {
            id: 33,
            label: "x".into(),
            criteria: criteria("/33"),
        });
        assert!(encode(&doc).is_err());
        doc.rows.pop();
        doc.rows[1].criteria = doc.rows[0].criteria.clone();
        assert!(encode(&doc).is_err());
        doc.rows[1].criteria = criteria("/2");
        doc.rows[0].criteria.query = "x".repeat(4097);
        assert!(encode(&doc).is_err());
        for e in &mut doc.rows {
            e.criteria.query = "x".repeat(4096)
        }
        assert!(encode(&doc).is_err());
        doc.rows.truncate(1);
        doc.rows[0].criteria = criteria("relative");
        assert!(encode(&doc).is_err());
    }
    #[test]
    fn locked_updates_merge_deduplicate_and_require_exact_entry_for_remove_rename() {
        let root = crate::persistence::test_dir("searches-ops");
        let absent = root.join("never-created");
        let absent = absent.to_str().unwrap();
        let first = apply(&root, &add(absent)).unwrap();
        let old = first.document.rows[0].clone();
        assert!(!Path::new(absent).exists());
        let second = apply(&root, &add("/other-root")).unwrap();
        assert_eq!(second.document.rows.len(), 2);
        assert_eq!(apply(&root, &add(absent)).unwrap().document.rows.len(), 2);
        let changed = apply(
            &root,
            &Op::Rename {
                expected: old.clone(),
                label: "Nuevo".into(),
            },
        )
        .unwrap();
        let before = fs::read(root.join("searches.bin")).unwrap();
        assert!(apply(&root, &Op::Remove(old.clone())).is_err());
        assert_eq!(fs::read(root.join("searches.bin")).unwrap(), before);
        assert!(
            apply(
                &root,
                &Op::Rename {
                    expected: old,
                    label: "Actor".into()
                }
            )
            .is_err()
        );
        apply(&root, &Op::Remove(changed.document.rows[0].clone())).unwrap();
        let third = apply(&root, &add("/third")).unwrap();
        assert_eq!(third.document.rows[1].id, 3);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join("searches.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!apply(&root, &Op::Sync).unwrap().unsynced);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn busy_lock_invalid_future_and_symlink_preserve_existing_bytes() {
        use std::os::unix::fs::symlink;
        let root = crate::persistence::test_dir("searches-preserve");
        let guard = lock(&root).unwrap();
        assert_eq!(
            apply(&root, &add("/one")).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(guard);
        let a = apply(&root, &add("/one")).unwrap();
        let mut bytes = encode(&a.document).unwrap();
        bytes[8] = 3;
        fs::write(root.join("searches.bin"), &bytes).unwrap();
        assert!(apply(&root, &add("/two")).is_err());
        assert_eq!(fs::read(root.join("searches.bin")).unwrap(), bytes);
        fs::rename(root.join("searches.bin"), root.join("target")).unwrap();
        symlink(root.join("target"), root.join("searches.bin")).unwrap();
        assert!(apply(&root, &Op::Read).is_err());
        assert!(apply(&root, &add("/two")).is_err());
        assert_eq!(fs::read(root.join("target")).unwrap(), bytes);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fifo_and_oversize_rejected_without_publication() {
        use std::os::unix::ffi::OsStrExt;
        let root = crate::persistence::test_dir("searches-special");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("searches.bin");
        let c = std::ffi::CString::new(file.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(apply(&root, &Op::Read).is_err());
        fs::remove_file(&file).unwrap();
        let too_big = vec![0; MAX_BYTES + 1];
        fs::write(&file, &too_big).unwrap();
        assert!(apply(&root, &Op::Read).is_err());
        assert!(apply(&root, &add("/two")).is_err());
        assert_eq!(fs::metadata(&file).unwrap().len(), too_big.len() as u64);
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
            "search-lock-child-drop"
        } else {
            "search-lock-parent-drop"
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
