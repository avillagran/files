use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Directory,
    File,
    Symlink,
    Other,
}
#[derive(Clone, Debug)]
pub struct Metadata {
    pub size: u64,
    pub modified: SystemTime,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: OsString,
    pub kind: Kind,
    pub metadata: Option<Metadata>,
    pub stat_attempted: bool,
}
impl Entry {
    pub fn load_metadata(&mut self, parent: &Path) -> io::Result<()> {
        if self.stat_attempted {
            return Ok(());
        }
        self.stat_attempted = true;
        self.metadata = Some(LocalVfs.stat(&parent.join(&self.name))?);
        Ok(())
    }
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Directory
    }
}
pub trait Vfs: Send + Sync {
    fn list(&self, path: &Path) -> io::Result<Box<dyn Iterator<Item = io::Result<Entry>> + Send>>;
    fn stat(&self, path: &Path) -> io::Result<Metadata>;
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn io::Read + Send>>;
    // Contratos para futuros backends; ninguna operación mutante se habilita en 0.0.1.
    fn open_write(&self, _path: &Path) -> io::Result<Box<dyn io::Write + Send>> {
        Err(unsupported())
    }
    fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(unsupported())
    }
    fn remove(&self, _path: &Path) -> io::Result<()> {
        Err(unsupported())
    }
    fn mkdir(&self, _path: &Path) -> io::Result<()> {
        Err(unsupported())
    }
    fn watch(
        &self,
        _path: &Path,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<PathBuf>> + Send>> {
        Err(unsupported())
    }
}
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Disponible en una entrega posterior",
    )
}
pub struct LocalVfs;
impl Vfs for LocalVfs {
    fn list(&self, path: &Path) -> io::Result<Box<dyn Iterator<Item = io::Result<Entry>> + Send>> {
        Ok(Box::new(fs::read_dir(path)?.map(|item| {
            let item = item?;
            let t = item.file_type()?;
            Ok(Entry {
                name: item.file_name(),
                kind: if t.is_dir() {
                    Kind::Directory
                } else if t.is_symlink() {
                    Kind::Symlink
                } else if t.is_file() {
                    Kind::File
                } else {
                    Kind::Other
                },
                metadata: None,
                stat_attempted: false,
            })
        })))
    }
    fn stat(&self, path: &Path) -> io::Result<Metadata> {
        let m = fs::metadata(path)?;
        Ok(Metadata {
            size: m.len(),
            modified: m.modified()?,
        })
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn io::Read + Send>> {
        Ok(Box::new(fs::File::open(path)?))
    }
}
pub enum ListingEvent {
    Batch(Vec<Entry>),
    Error(String),
    Done,
}
pub struct Listing {
    pub receiver: Receiver<ListingEvent>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Listing {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn send(tx: &SyncSender<ListingEvent>, mut event: ListingEvent, cancel: &AtomicBool) -> bool {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        match tx.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(e)) => {
                event = e;
                thread::park_timeout(std::time::Duration::from_millis(2));
            }
        }
    }
}
pub fn stream(path: PathBuf) -> Listing {
    let (tx, receiver) = mpsc::sync_channel(4);
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    thread::spawn(move || {
        match LocalVfs.list(&path) {
            Ok(items) => {
                let mut batch = Vec::with_capacity(256);
                for item in items {
                    if c.load(Ordering::Relaxed) {
                        return;
                    }
                    match item {
                        Ok(e) => batch.push(e),
                        Err(e) => {
                            if !send(&tx, ListingEvent::Error(e.to_string()), &c) {
                                return;
                            }
                        }
                    }
                    if batch.len() == 256
                        && !send(
                            &tx,
                            ListingEvent::Batch(std::mem::replace(
                                &mut batch,
                                Vec::with_capacity(256),
                            )),
                            &c,
                        )
                    {
                        return;
                    }
                }
                if !batch.is_empty() {
                    send(&tx, ListingEvent::Batch(batch), &c);
                }
            }
            Err(e) => {
                send(&tx, ListingEvent::Error(e.to_string()), &c);
            }
        }
        send(&tx, ListingEvent::Done, &c);
    });
    Listing { receiver, cancel }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Name,
    Size,
    Modified,
    /// By kind (extension, lowercase), then name: Nautilus "Type".
    Type,
}
/// Sort key of [`Sort::Type`]: the lowercase extension ("" first).
pub fn type_key(name: &std::ffi::OsStr) -> String {
    let n = name.to_string_lossy();
    match n.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => ext.to_lowercase(),
        _ => String::new(),
    }
}
/// Known totals precede unknown observations in either direction. Zero is known.
pub fn compare_sizes(a: Option<u64>, b: Option<u64>, descending: bool) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            if descending {
                b.cmp(&a)
            } else {
                a.cmp(&b)
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}
pub fn sort(entries: &mut [Entry], by: Sort, descending: bool) {
    entries.sort_by_cached_key(|e| crate::natural_sort::NaturalKey::new(&e.name));
    if by != Sort::Name {
        entries.sort_by(|a, b| {
            let order = match by {
                Sort::Size => {
                    let known = |e: &Entry| {
                        if e.is_dir() {
                            None
                        } else {
                            e.metadata.as_ref().map(|m| m.size)
                        }
                    };
                    return compare_sizes(known(a), known(b), descending);
                }
                Sort::Modified => a
                    .metadata
                    .as_ref()
                    .map(|m| m.modified)
                    .cmp(&b.metadata.as_ref().map(|m| m.modified)),
                Sort::Type => type_key(&a.name).cmp(&type_key(&b.name)),
                _ => std::cmp::Ordering::Equal,
            };
            if descending { order.reverse() } else { order }
        });
    } else if descending {
        entries.reverse();
    }
    entries.sort_by_key(|e| !e.is_dir());
}
/// Filtro de subsecuencia Unicode; bonifica coincidencias contiguas y comienzos de palabra.
pub fn fuzzy_score(name: &str, query: &str) -> Option<i32> {
    let mut chars = name.chars().flat_map(char::to_lowercase).enumerate();
    let mut score = 0;
    let mut previous = None;
    for q in query.chars().flat_map(char::to_lowercase) {
        let (pos, _) = chars.find(|(_, c)| *c == q)?;
        score += 10;
        if previous.is_some_and(|p| p + 1 == pos) {
            score += 8;
        }
        if pos == 0 {
            score += 15;
        }
        score -= pos.min(100) as i32;
        previous = Some(pos);
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_order_ignores_directory_inode_lengths_and_keeps_unknown_after_zero() {
        let row = |name: &str, kind, size: Option<u64>| Entry {
            name: name.into(),
            kind,
            metadata: size.map(|size| Metadata {
                size,
                modified: SystemTime::UNIX_EPOCH,
            }),
            stat_attempted: true,
        };
        let mut rows = vec![
            row("z-dir", Kind::Directory, Some(1)),
            row("a-dir", Kind::Directory, Some(9999)),
            row("zero", Kind::File, Some(0)),
            row("seven", Kind::File, Some(7)),
            row("unknown", Kind::File, None),
        ];
        for (descending, expected) in [
            (false, vec!["a-dir", "z-dir", "zero", "seven", "unknown"]),
            (true, vec!["a-dir", "z-dir", "seven", "zero", "unknown"]),
        ] {
            sort(&mut rows, Sort::Size, descending);
            assert_eq!(
                rows.iter()
                    .map(|e| e.name.to_str().unwrap())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn ten_thousand_streaming_lazy_sort_filter() {
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap()
            .join(".cache/files-test");
        fs::create_dir_all(&base).unwrap();
        let dir = base.join(format!(
            "core-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(dir.clone());
        for i in 0..10_000 {
            fs::write(dir.join(format!("file-{i:05}.txt")), b"test").unwrap();
        }
        fs::create_dir(dir.join("zzz-folder")).unwrap();
        let listing = stream(dir.clone());
        let mut entries = Vec::new();
        loop {
            match listing
                .receiver
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
            {
                ListingEvent::Batch(b) => {
                    assert!(b.len() <= 256);
                    entries.extend(b);
                }
                ListingEvent::Done => break,
                ListingEvent::Error(e) => panic!("{e}"),
            }
        }
        assert_eq!(entries.len(), 10_001);
        assert!(entries.iter().all(|e| e.metadata.is_none()));
        sort(&mut entries, Sort::Name, false);
        assert!(entries[0].is_dir());
        entries[1].load_metadata(&dir).unwrap();
        assert_eq!(entries[1].metadata.as_ref().unwrap().size, 4);
        assert_eq!(entries.iter().filter(|e| e.metadata.is_some()).count(), 1);
        assert!(fuzzy_score("file-00123.txt", "f123").is_some());
        assert!(fuzzy_score("abc", "z").is_none());
    }
}
