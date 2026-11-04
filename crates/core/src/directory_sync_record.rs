//! Canonical read-only sync evidence. A valid seal is integrity/coherence, never
//! operator approval, a live filesystem proof, durability or mutation authority.
#[cfg(target_os = "linux")]
use crate::move_manifest::{put_identity, put_revision};
use crate::{
    directory_compare,
    directory_sync::{Mode, Side},
    move_manifest::{AccessTime, Identity, Kind, Reader, Revision},
    relative_source::DirectoryIdentity,
    selection_source::ObjectIdentity,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io,
};
const MAGIC: &[u8; 8] = b"FLSYNC\0\x01";
const DOMAIN: &[u8] = b"Files:sync-evidence:v1\0";
const BIND_MAGIC: &[u8; 8] = b"FLSBND\0\x01";
pub const MAX_ENTRIES: usize = directory_compare::MAX_ITEMS + 1;
pub const MAX_ANCESTORS: usize = 128;
pub const FIXED_ENTRY: usize = 155;
// Both normalized names budgets, full per-object proof, four absolute paths,
// two bounded ancestor chains and ample fixed-header/trailer allowance.
pub const MAX_WIRE: usize = 2 * directory_compare::MAX_NAMES
    + 2 * MAX_ENTRIES * FIXED_ENTRY
    + 4 * 4096
    + 2 * MAX_ANCESTORS * 16
    + 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryView<'a> {
    pub relative: &'a [u8],
    pub identity: Identity,
    pub revision: Revision,
    pub access: AccessTime,
    pub content: [u8; 32],
    pub metadata: [u8; 32],
    pub mount: u64,
}
#[derive(Debug)]
pub struct RootView<'a> {
    pub original: &'a [u8],
    pub parent: &'a [u8],
    pub parent_mount: u64,
    pub ancestors: Vec<ObjectIdentity>,
    pub entries: Vec<EntryView<'a>>,
}
#[derive(Debug)]
pub struct View<'a> {
    pub id: u64,
    pub mode: Mode,
    pub read_limit: u64,
    pub payload_read: u64,
    pub expected: Option<[DirectoryIdentity; 2]>,
    pub roots: [RootView<'a>; 2],
}
#[derive(Debug)]
pub struct Record {
    bytes: Vec<u8>,
    seal: [u8; 32],
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Registro de evidencia de sincronización inválido",
    )
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(bytes);
    h.finalize().into()
}
fn absolute(p: &[u8]) -> bool {
    p.first() == Some(&b'/')
        && p.len() <= 4096
        && !p.contains(&0)
        && (p == b"/"
            || p[1..]
                .split(|&b| b == b'/')
                .all(|c| !c.is_empty() && c != b"." && c != b".."))
}
fn relative(p: &[u8]) -> bool {
    !p.is_empty()
        && p.len() <= 4096
        && !p.contains(&0)
        && p.split(|&b| b == b'/')
            .all(|c| !c.is_empty() && c.len() <= 255 && c != b"." && c != b"..")
        && p.iter().filter(|&&b| b == b'/').count() < directory_compare::MAX_DEPTH + 1
}
#[cfg(target_os = "linux")]
fn mode_byte(mode: Mode) -> u8 {
    match mode {
        Mode::Update(Side::Left) => 1,
        Mode::Update(Side::Right) => 2,
        Mode::Mirror(Side::Left) => 3,
        Mode::Mirror(Side::Right) => 4,
        Mode::Merge => 5,
    }
}
fn mode(byte: u8) -> io::Result<Mode> {
    Ok(match byte {
        1 => Mode::Update(Side::Left),
        2 => Mode::Update(Side::Right),
        3 => Mode::Mirror(Side::Left),
        4 => Mode::Mirror(Side::Right),
        5 => Mode::Merge,
        _ => return Err(bad()),
    })
}
fn path<'a>(r: &mut Reader<'a>) -> io::Result<&'a [u8]> {
    let n = u16::from_le_bytes(r.array().map_err(|_| bad())?) as usize;
    if n > 4096 {
        return Err(bad());
    }
    r.take(n).map_err(|_| bad())
}
fn root<'a>(r: &mut Reader<'a>) -> io::Result<RootView<'a>> {
    let original = path(r)?;
    let parent = path(r)?;
    if !absolute(original) || original == b"/" || !absolute(parent) {
        return Err(bad());
    }
    let parent_mount = r.u64().map_err(|_| bad())?;
    let count = u16::from_le_bytes(r.array().map_err(|_| bad())?) as usize;
    if parent_mount == 0
        || count == 0
        || count > MAX_ANCESTORS
        || count > r.data.len().saturating_sub(r.at) / 16
    {
        return Err(bad());
    }
    let mut ancestors = Vec::with_capacity(count);
    let mut seen = HashSet::new();
    for _ in 0..count {
        let device = r.u64().map_err(|_| bad())?;
        let inode = r.u64().map_err(|_| bad())?;
        if inode == 0 || !seen.insert((device, inode)) {
            return Err(bad());
        }
        ancestors.push(ObjectIdentity {
            device,
            inode,
            directory: true,
        });
    }
    let count = r.u32().map_err(|_| bad())? as usize;
    if count == 0 || count > MAX_ENTRIES || count > r.data.len().saturating_sub(r.at) / FIXED_ENTRY
    {
        return Err(bad());
    }
    let mut entries = Vec::with_capacity(count);
    let mut names = 0usize;
    for i in 0..count {
        let relative = path(r)?;
        if i == 0 {
            if !relative.is_empty() {
                return Err(bad());
            }
        } else {
            if !self::relative(relative)
                || entries
                    .last()
                    .is_some_and(|e: &EntryView<'_>| e.relative >= relative)
            {
                return Err(bad());
            }
            names = names
                .checked_add(relative.len())
                .filter(|&n| n <= directory_compare::MAX_NAMES)
                .ok_or_else(bad)?;
        }
        let identity = r.identity().map_err(|_| bad())?;
        let revision = r.revision().map_err(|_| bad())?;
        let access = AccessTime {
            seconds: i64::from_le_bytes(r.array().map_err(|_| bad())?),
            nanos: r.u32().map_err(|_| bad())?,
        };
        let content = r.array().map_err(|_| bad())?;
        let metadata = r.array().map_err(|_| bad())?;
        let mount = r.u64().map_err(|_| bad())?;
        if identity.inode == 0
            || revision.links == 0
            || revision.mode & !0o7777 != 0
            || revision.mtime_nanos >= 1_000_000_000
            || revision.ctime_nanos >= 1_000_000_000
            || access.nanos >= 1_000_000_000
            || mount == 0
            || i == 0 && identity.kind != Kind::Directory
            || identity.kind == Kind::Symlink && (revision.size == 0 || revision.size > 4096)
        {
            return Err(bad());
        }
        entries.push(EntryView {
            relative,
            identity,
            revision,
            access,
            content,
            metadata,
            mount,
        });
    }
    Ok(RootView {
        original,
        parent,
        parent_mount,
        ancestors,
        entries,
    })
}
fn validate(view: &View<'_>) -> io::Result<()> {
    let roots = view.roots.each_ref().map(|r| r.entries[0].identity);
    let [a, b] = view.roots.each_ref();
    if a.parent == b.parent
        && (a.parent_mount != b.parent_mount
            || a.ancestors != b.ancestors
            || a.original.rsplit(|&v| v == b'/').next() == b.original.rsplit(|&v| v == b'/').next())
    {
        return Err(bad());
    }
    if view.roots[0].original == view.roots[1].original {
        return Err(bad());
    }
    if roots[0] == roots[1]
        || view.expected.is_some_and(|ids| {
            (0..2).any(|s| {
                ids[s]
                    != DirectoryIdentity {
                        device: roots[s].device,
                        inode: roots[s].inode,
                    }
            })
        })
    {
        return Err(bad());
    }
    if view.roots[0].ancestors.last() != view.roots[1].ancestors.last() {
        return Err(bad());
    }
    let mut objects: HashMap<(u64, u64), (&EntryView<'_>, u64)> = HashMap::new();
    let mut payload = 0u64;
    for root in &view.roots {
        if root.ancestors.iter().any(|a| {
            roots
                .iter()
                .any(|r| a.device == r.device && a.inode == r.inode)
        }) {
            return Err(bad());
        }
        let mut names: BTreeMap<&[u8], usize> = BTreeMap::new();
        let mut directory_hashes: Vec<Option<Sha256>> = root
            .entries
            .iter()
            .map(|e| (e.identity.kind == Kind::Directory).then(Sha256::new))
            .collect();
        let mut local: HashMap<(u64, u64), &EntryView<'_>> = HashMap::new();
        for (index, e) in root.entries.iter().enumerate() {
            if index > 0 {
                let split = e.relative.iter().rposition(|&b| b == b'/');
                let (parent, name) = split.map_or((&b""[..], e.relative), |n| {
                    (&e.relative[..n], &e.relative[n + 1..])
                });
                let parent_index = *names.get(parent).ok_or_else(bad)?;
                let hash = directory_hashes
                    .get_mut(parent_index)
                    .and_then(Option::as_mut)
                    .ok_or_else(bad)?;
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name);
            }
            names.insert(e.relative, index);
            let key = (e.identity.device, e.identity.inode);
            if e.identity.kind != Kind::Directory {
                if let Some(first) = local.insert(key, e) {
                    if e.revision != first.revision
                        || e.access != first.access
                        || e.content != first.content
                        || e.metadata != first.metadata
                    {
                        return Err(bad());
                    }
                } else if e.identity.kind == Kind::File {
                    payload = payload.checked_add(e.revision.size).ok_or_else(bad)?;
                }
            }
            if let Some((first, count)) = objects.get_mut(&key) {
                if e.identity.kind == Kind::Directory
                    || e.identity.kind != first.identity.kind
                    || e.revision != first.revision
                    || e.content != first.content
                    || e.identity.kind == Kind::File
                        && (e.metadata != first.metadata || e.access != first.access)
                {
                    return Err(bad());
                }
                *count += 1;
                if *count > e.revision.links {
                    return Err(bad());
                }
            } else {
                objects.insert(key, (e, 1));
            }
        }
        for (e, hash) in root.entries.iter().zip(directory_hashes) {
            if let Some(hash) = hash
                && <[u8; 32]>::from(hash.finalize()) != e.content
            {
                return Err(bad());
            }
        }
    }
    if payload.checked_mul(2) != Some(view.payload_read)
        || view.payload_read > view.read_limit
        || view.read_limit > directory_compare::MAX_READ
    {
        return Err(bad());
    }
    Ok(())
}
fn scan(bytes: &[u8]) -> io::Result<(View<'_>, [u8; 32])> {
    if bytes.len() > MAX_WIRE || bytes.len() < 32 + 8 {
        return Err(bad());
    }
    let (data, tail) = bytes.split_at(bytes.len() - 32);
    let seal = digest(data);
    if tail != seal {
        return Err(bad());
    }
    let mut r = Reader { data, at: 0 };
    if r.take(8).map_err(|_| bad())? != MAGIC {
        return Err(bad());
    }
    let id = r.u64().map_err(|_| bad())?;
    let mode = mode(r.take(1).map_err(|_| bad())?[0])?;
    let read_limit = r.u64().map_err(|_| bad())?;
    let payload_read = r.u64().map_err(|_| bad())?;
    if read_limit > directory_compare::MAX_READ || payload_read > read_limit {
        return Err(bad());
    }
    let expected = match r.take(1).map_err(|_| bad())?[0] {
        0 => None,
        1 => Some([
            DirectoryIdentity {
                device: r.u64().map_err(|_| bad())?,
                inode: r.u64().map_err(|_| bad())?,
            },
            DirectoryIdentity {
                device: r.u64().map_err(|_| bad())?,
                inode: r.u64().map_err(|_| bad())?,
            },
        ]),
        _ => return Err(bad()),
    };
    let roots = [root(&mut r)?, root(&mut r)?];
    if r.at != data.len() {
        return Err(bad());
    }
    let view = View {
        id,
        mode,
        read_limit,
        payload_read,
        expected,
        roots,
    };
    validate(&view)?;
    Ok((view, seal))
}
impl View<'_> {
    /// Derived display/planning data only. External-link count locates no name
    /// and grants no right to change a shared inode.
    pub fn links(&self) -> io::Result<Vec<crate::directory_sync_manifest::LinkGroup>> {
        use crate::directory_sync_manifest::{LinkGroup, Location};
        let mut groups: HashMap<(u64, u64), LinkGroup> = HashMap::new();
        for (side, root) in self.roots.iter().enumerate() {
            if root.entries.len() > MAX_ENTRIES {
                return Err(bad());
            }
            for (index, e) in root.entries.iter().enumerate() {
                if e.identity.kind != Kind::Directory && e.revision.links > 1 {
                    let id = (e.identity.device, e.identity.inode);
                    let g = groups.entry(id).or_insert_with(|| LinkGroup {
                        device: id.0,
                        inode: id.1,
                        kind: e.identity.kind,
                        observed_links: e.revision.links,
                        occurrences: Vec::new(),
                        external_links: 0,
                    });
                    if g.kind != e.identity.kind || g.observed_links != e.revision.links {
                        return Err(bad());
                    }
                    g.occurrences.push(Location { side, index });
                }
            }
        }
        let mut out: Vec<_> = groups.into_values().collect();
        out.sort_unstable_by_key(|g| (g.device, g.inode));
        for g in &mut out {
            g.external_links = g
                .observed_links
                .checked_sub(g.occurrences.len() as u64)
                .ok_or_else(bad)?;
        }
        Ok(out)
    }
}
impl Record {
    /// Borrowed structural validation and seal check before copying owned bytes.
    /// Decoding does no filesystem I/O and cannot recreate pinned Prepared data.
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        let (_, seal) = scan(bytes)?;
        Ok(Self {
            bytes: bytes.to_vec(),
            seal,
        })
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn from_owned(bytes: Vec<u8>) -> io::Result<Self> {
        let (_, seal) = scan(&bytes)?;
        Ok(Self { bytes, seal })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn view(&self) -> io::Result<View<'_>> {
        scan(&self.bytes).map(|v| v.0)
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn from_prepared(p: &crate::directory_sync_manifest::Prepared) -> io::Result<Self> {
        let i = p.intent();
        let mut size = 8 + 8 + 1 + 8 + 8 + 1 + 32 + if i.expected.is_some() { 32 } else { 0 };
        for side in 0..2 {
            let h = p.record_root(side).ok_or_else(bad)?;
            if !absolute(h.original)
                || !absolute(h.parent)
                || h.ancestors.is_empty()
                || h.ancestors.len() > MAX_ANCESTORS
            {
                return Err(bad());
            }
            let entries = p.entries(side).ok_or_else(bad)?;
            if entries.is_empty() || entries.len() > MAX_ENTRIES {
                return Err(bad());
            }
            let mut names = 0usize;
            for index in 0..entries.len() {
                let name = p.relative(side, index).ok_or_else(bad)?;
                if name.len() > 4096 {
                    return Err(bad());
                }
                names = names
                    .checked_add(name.len())
                    .filter(|&n| n <= directory_compare::MAX_NAMES)
                    .ok_or_else(bad)?;
            }
            size += 4
                + h.original.len()
                + h.parent.len()
                + 8
                + 2
                + 16 * h.ancestors.len()
                + 4
                + entries.len() * FIXED_ENTRY
                + names;
        }
        if size > MAX_WIRE {
            return Err(bad());
        }
        let mut out = Vec::with_capacity(size);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&i.id.to_le_bytes());
        out.push(mode_byte(i.mode));
        out.extend_from_slice(&i.read_limit.to_le_bytes());
        out.extend_from_slice(&p.payload_read().to_le_bytes());
        out.push(u8::from(i.expected.is_some()));
        if let Some(ids) = i.expected {
            for id in ids {
                out.extend_from_slice(&id.device.to_le_bytes());
                out.extend_from_slice(&id.inode.to_le_bytes());
            }
        }
        for side in 0..2 {
            let header = p.record_root(side).ok_or_else(bad)?;
            for path in [header.original, header.parent] {
                if !absolute(path) {
                    return Err(bad());
                }
                out.extend_from_slice(&(path.len() as u16).to_le_bytes());
                out.extend_from_slice(path);
            }
            out.extend_from_slice(&header.parent_mount.to_le_bytes());
            out.extend_from_slice(&(header.ancestors.len() as u16).to_le_bytes());
            for a in header.ancestors {
                out.extend_from_slice(&a.device.to_le_bytes());
                out.extend_from_slice(&a.inode.to_le_bytes());
            }
            let entries = p.entries(side).ok_or_else(bad)?;
            let mounts = p.mount_ids(side).ok_or_else(bad)?;
            if entries.len() > MAX_ENTRIES || entries.len() != mounts.len() {
                return Err(bad());
            }
            out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
            for (index, (e, mount)) in entries.iter().zip(mounts).enumerate() {
                let name = p.relative(side, index).ok_or_else(bad)?;
                if name.len() > 4096 {
                    return Err(bad());
                }
                out.extend_from_slice(&(name.len() as u16).to_le_bytes());
                out.extend_from_slice(name);
                put_identity(&mut out, e.identity);
                put_revision(&mut out, e.revision);
                out.extend_from_slice(&e.access.seconds.to_le_bytes());
                out.extend_from_slice(&e.access.nanos.to_le_bytes());
                out.extend_from_slice(&e.content_digest);
                out.extend_from_slice(&e.metadata_digest);
                out.extend_from_slice(&mount.to_le_bytes());
                if out.len() > MAX_WIRE - 32 {
                    return Err(bad());
                }
            }
        }
        let seal = digest(&out);
        out.extend_from_slice(&seal);
        if out.len() != size {
            return Err(bad());
        }
        scan(&out)?;
        Ok(Self { bytes: out, seal })
    }
}
#[cfg(target_os = "linux")]
pub(crate) struct RootHeader<'a> {
    pub original: &'a [u8],
    pub parent: &'a [u8],
    pub parent_mount: u64,
    pub ancestors: &'a [ObjectIdentity],
}
/// A journal comparison key, not an approved plan or fresh fsync barrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceKey {
    job: [u8; 16],
    seal: [u8; 32],
}
/// Reading a stored key cannot reconstruct a live binding or operator approval.
#[derive(Debug)]
pub struct StoredKey {
    job: [u8; 16],
    seal: [u8; 32],
}
impl EvidenceKey {
    #[cfg(target_os = "linux")]
    pub(crate) fn new(job: [u8; 16], seal: [u8; 32]) -> io::Result<Self> {
        if job == [0; 16] {
            return Err(bad());
        }
        Ok(Self { job, seal })
    }
    pub fn encode(&self) -> [u8; 56] {
        let mut out = [0; 56];
        out[..8].copy_from_slice(BIND_MAGIC);
        out[8..24].copy_from_slice(&self.job);
        out[24..].copy_from_slice(&self.seal);
        out
    }
    pub fn job(&self) -> [u8; 16] {
        self.job
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
}
impl StoredKey {
    pub fn job(&self) -> [u8; 16] {
        self.job
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 56 || &bytes[..8] != BIND_MAGIC {
            return Err(bad());
        }
        let job = bytes[8..24].try_into().map_err(|_| bad())?;
        if job == [0; 16] {
            return Err(bad());
        }
        Ok(Self {
            job,
            seal: bytes[24..].try_into().map_err(|_| bad())?,
        })
    }
    pub fn matches(&self, live: &EvidenceKey) -> bool {
        self.job == live.job && self.seal == live.seal
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        directory_sync_manifest::{self as manifest, Prepared},
        directory_sync_wire::Intent,
    };
    use std::{
        fs,
        os::unix::{
            ffi::OsStringExt,
            fs::{MetadataExt, symlink},
        },
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    };
    struct Fixture {
        base: PathBuf,
        paths: [PathBuf; 2],
    }
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "files-sync-record-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let paths = [base.join("a"), base.join("b")];
            for p in &paths {
                fs::create_dir_all(p).unwrap();
            }
            Self { base, paths }
        }
        fn intent(&self, mode: Mode) -> Intent {
            let ids = self.paths.each_ref().map(|p| {
                let m = fs::metadata(p).unwrap();
                DirectoryIdentity {
                    device: m.dev(),
                    inode: m.ino(),
                }
            });
            Intent {
                version: 1,
                id: 7,
                left: self.paths[0].clone(),
                right: self.paths[1].clone(),
                expected: Some(ids),
                mode,
                read_limit: 1024 * 1024,
            }
        }
        fn prepare(&self, mode: Mode) -> Prepared {
            manifest::capture(self.intent(mode), &AtomicBool::new(false), |_| Ok(()))
                .unwrap_or_else(|e| panic!("capture: {e}"))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn seal(bytes: &mut Vec<u8>) {
        bytes.truncate(bytes.len() - 32);
        let hash = digest(bytes);
        bytes.extend_from_slice(&hash);
    }
    fn offset(record: &Record, slice: &[u8]) -> usize {
        slice.as_ptr() as usize - record.bytes.as_ptr() as usize
    }
    fn entry_data(record: &Record, side: usize, name: &[u8]) -> usize {
        let view = record.view().unwrap();
        let e = view.roots[side]
            .entries
            .iter()
            .find(|e| e.relative == name)
            .unwrap();
        offset(record, e.relative) + e.relative.len()
    }
    #[test]
    fn canonical_roundtrip_preserves_raw_fields_roots_mounts_and_cached_identity() {
        let f = Fixture::new();
        let raw = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec());
        fs::create_dir(f.paths[0].join("dir")).unwrap();
        fs::create_dir(f.paths[0].join("empty")).unwrap();
        fs::write(f.paths[0].join("dir").join(&raw), b"data").unwrap();
        fs::write(f.paths[1].join("other"), b"other").unwrap();
        symlink(
            std::ffi::OsString::from_vec(b"literal\xff".to_vec()),
            f.paths[0].join("link"),
        )
        .unwrap();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        assert_eq!(record.bytes, p.record().unwrap().bytes);
        let restored = Record::decode(record.bytes()).unwrap();
        assert_eq!(record.seal, restored.seal);
        assert_eq!(record.bytes, restored.bytes);
        let view = restored.view().unwrap();
        assert_eq!(view.id, 7);
        assert_eq!(view.mode, Mode::Merge);
        assert_eq!(view.payload_read, 18);
        assert_eq!(view.expected, p.intent().expected);
        for side in 0..2 {
            assert_eq!(
                view.roots[side].original,
                f.paths[side].as_os_str().as_encoded_bytes()
            );
            for (index, (a, b)) in view.roots[side]
                .entries
                .iter()
                .zip(p.entries(side).unwrap())
                .enumerate()
            {
                assert_eq!(Some(a.relative), p.relative(side, index));
                assert_eq!(a.identity, b.identity);
                assert_eq!(a.revision, b.revision);
                assert_eq!(a.access, b.access);
                assert_eq!(a.content, b.content_digest);
                assert_eq!(a.metadata, b.metadata_digest);
                assert_eq!(a.mount, p.mount_ids(side).unwrap()[index]);
            }
        }
        assert!(
            view.roots[0]
                .entries
                .iter()
                .any(|e| e.relative == b"dir/raw\xff\n")
        );
        assert!(record.bytes.len() <= MAX_WIRE);
        assert!(crate::move_manifest::Manifest::decode(record.bytes()).is_err());
    }
    #[test]
    fn mode_and_request_policy_changes_produce_distinct_records_and_cannot_bind() {
        let f = Fixture::new();
        let mut seals = HashSet::new();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        for mode in [
            Mode::Update(Side::Left),
            Mode::Update(Side::Right),
            Mode::Mirror(Side::Left),
            Mode::Mirror(Side::Right),
            Mode::Merge,
        ] {
            let q = f.prepare(mode);
            let r = q.record().unwrap();
            assert_eq!(r.view().unwrap().mode, mode);
            assert!(seals.insert(r.seal()));
            if mode != Mode::Merge {
                assert!(p.bind_record(&r, &AtomicBool::new(false)).is_err());
            }
        }
        for (at, value) in [(8, 8u64), (17, 2 * 1024 * 1024)] {
            let mut bytes = record.bytes.clone();
            bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
            seal(&mut bytes);
            let foreign = Record::decode(&bytes).unwrap();
            assert!(p.bind_record(&foreign, &AtomicBool::new(false)).is_err());
        }
    }
    #[test]
    fn unresealed_changes_truncation_extra_bytes_and_oversize_are_rejected() {
        let f = Fixture::new();
        let record = f.prepare(Mode::Merge).record().unwrap();
        for n in [
            0,
            7,
            8,
            33,
            record.bytes.len() - 33,
            record.bytes.len() - 32,
            record.bytes.len() - 1,
        ] {
            assert!(Record::decode(&record.bytes[..n]).is_err());
        }
        for at in [0, 7, 8, 16, 17, 25, record.bytes.len() - 1] {
            let mut bytes = record.bytes.clone();
            bytes[at] ^= 1;
            assert!(Record::decode(&bytes).is_err());
        }
        let mut bytes = record.bytes.clone();
        bytes.push(0);
        assert!(Record::decode(&bytes).is_err());
        assert!(Record::decode(&vec![0; MAX_WIRE + 1]).is_err());
    }
    #[test]
    fn resealed_impossible_version_mode_budget_expected_identity_and_counts_are_rejected() {
        let f = Fixture::new();
        let record = f.prepare(Mode::Merge).record().unwrap();
        let view = record.view().unwrap();
        let root_count = offset(&record, view.roots[0].entries[0].relative) - 6;
        for case in 0..7 {
            let mut bytes = record.bytes.clone();
            match case {
                0 => bytes[7] = 2,
                1 => bytes[16] = 6,
                2 => {
                    bytes[17..25].copy_from_slice(&(directory_compare::MAX_READ + 1).to_le_bytes())
                }
                3 => bytes[25..33].copy_from_slice(&u64::MAX.to_le_bytes()),
                4 => bytes[33] = 2,
                5 => bytes[42..50].copy_from_slice(&0u64.to_le_bytes()),
                _ => bytes[root_count..root_count + 4].copy_from_slice(&u32::MAX.to_le_bytes()),
            };
            seal(&mut bytes);
            assert!(Record::decode(&bytes).is_err(), "case{case}");
        }
    }
    #[test]
    fn resealed_bad_paths_ancestors_timestamps_kind_links_mount_and_directory_hash_fail() {
        let f = Fixture::new();
        fs::write(f.paths[0].join("abc"), b"data").unwrap();
        let record = f.prepare(Mode::Merge).record().unwrap();
        let view = record.view().unwrap();
        let root0 = &view.roots[0];
        let original = offset(&record, root0.original);
        let parent_end = offset(&record, root0.parent) + root0.parent.len();
        let data = entry_data(&record, 0, b"abc");
        let directory = entry_data(&record, 0, b"");
        for case in 0..10 {
            let mut bytes = record.bytes.clone();
            match case {
                0 => bytes[original] = b'x',
                1 => bytes[original - 2..original].copy_from_slice(&4097u16.to_le_bytes()),
                2 => bytes[parent_end + 8..parent_end + 10].copy_from_slice(&0u16.to_le_bytes()),
                3 => bytes[data + 16] = 4,
                4 => bytes[data + 33..data + 37].copy_from_slice(&1_000_000_000u32.to_le_bytes()),
                5 => bytes[data + 49..data + 53].copy_from_slice(&u32::MAX.to_le_bytes()),
                6 => bytes[data + 61..data + 69].copy_from_slice(&0u64.to_le_bytes()),
                7 => bytes[data + 145..data + 153].copy_from_slice(&0u64.to_le_bytes()),
                8 => bytes[directory + 81] ^= 1,
                _ => {
                    let name = data - 3;
                    bytes[name..data].copy_from_slice(b"../");
                }
            };
            seal(&mut bytes);
            assert!(Record::decode(&bytes).is_err(), "case{case}");
        }
    }
    #[test]
    fn duplicated_out_of_order_or_missing_parent_namespace_is_rejected_after_reseal() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("dir")).unwrap();
        fs::write(f.paths[0].join("dir/one"), b"x").unwrap();
        fs::write(f.paths[0].join("foo"), b"x").unwrap();
        let record = f.prepare(Mode::Merge).record().unwrap();
        let child = entry_data(&record, 0, b"dir/one") - 7;
        let parent = entry_data(&record, 0, b"dir") - 3;
        let foo = entry_data(&record, 0, b"foo") - 3;
        for case in 0..3 {
            let mut bytes = record.bytes.clone();
            match case {
                0 => bytes[foo..foo + 3].copy_from_slice(b"dir"),
                1 => bytes[parent..parent + 3].copy_from_slice(b"zzz"),
                _ => bytes[child..child + 3].copy_from_slice(b"bad"),
            };
            seal(&mut bytes);
            assert!(Record::decode(&bytes).is_err());
        }
    }
    #[test]
    fn valid_resealed_content_or_metadata_claims_remain_history_not_live_authority() {
        let f = Fixture::new();
        fs::write(f.paths[0].join("file"), b"data").unwrap();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        let d = entry_data(&record, 0, b"file");
        for field in [81, 113] {
            let mut bytes = record.bytes.clone();
            bytes[d + field] ^= 1;
            seal(&mut bytes);
            let history = Record::decode(&bytes).unwrap();
            assert_ne!(history.seal(), record.seal());
            assert!(p.bind_record(&history, &AtomicBool::new(false)).is_err());
        }
        let bound = p.bind_record(&record, &AtomicBool::new(false)).unwrap();
        assert_eq!(bound.record().seal(), record.seal());
        assert_eq!(bound.prepared().intent().id, 7);
    }
    #[test]
    fn decoded_history_and_matching_stored_key_do_not_grant_freshness_or_approval() {
        let f = Fixture::new();
        fs::write(f.paths[0].join("file"), b"data").unwrap();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        let key = p
            .bind_record(&record, &AtomicBool::new(false))
            .unwrap()
            .evidence_key([7; 16])
            .unwrap();
        let stored = StoredKey::decode(&key.encode()).unwrap();
        assert!(stored.matches(&key));
        fs::write(f.paths[0].join("file"), b"diff").unwrap();
        assert!(Record::decode(record.bytes()).is_ok());
        assert!(stored.matches(&key));
        assert!(p.bind_record(&record, &AtomicBool::new(false)).is_err());
        assert!(p.bind_record(&record, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn journal_comparison_key_requires_job_identity_and_cannot_recreate_live_binding() {
        let f = Fixture::new();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        let bound = p.bind_record(&record, &AtomicBool::new(false)).unwrap();
        assert!(bound.evidence_key([0; 16]).is_err());
        let key = bound.evidence_key([1; 16]).unwrap();
        let other = bound.evidence_key([2; 16]).unwrap();
        let stored = StoredKey::decode(&key.encode()).unwrap();
        assert!(stored.matches(&key));
        assert!(!stored.matches(&other));
        assert_eq!(stored.job(), key.job());
        assert_eq!(stored.seal(), key.seal());
        for n in [0, 8, 24, 55] {
            assert!(StoredKey::decode(&key.encode()[..n]).is_err());
        }
        let mut bytes = key.encode();
        bytes[7] = 2;
        assert!(StoredKey::decode(&bytes).is_err());
        let mut bytes = key.encode();
        bytes[8..24].fill(0);
        assert!(StoredKey::decode(&bytes).is_err());
    }
    #[test]
    fn hardlinks_are_derived_with_external_counts_and_inconsistent_revisions_fail() {
        let f = Fixture::new();
        fs::write(f.paths[0].join("first"), b"data").unwrap();
        fs::hard_link(f.paths[0].join("first"), f.paths[1].join("cross")).unwrap();
        fs::hard_link(f.paths[0].join("first"), f.base.join("outside")).unwrap();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        let view = record.view().unwrap();
        let groups = view.links().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            (groups[0].occurrences.len(), groups[0].external_links),
            (2, 1)
        );
        assert_eq!(groups[0].inode, p.links()[0].inode);
        let at = entry_data(&record, 1, b"cross");
        let mut bytes = record.bytes.clone();
        bytes[at + 17..at + 25].copy_from_slice(&5u64.to_le_bytes());
        seal(&mut bytes);
        assert!(Record::decode(&bytes).is_err());
    }
    #[test]
    fn shared_canonical_parent_cannot_claim_two_mount_or_parent_identities() {
        let f = Fixture::new();
        let record = f.prepare(Mode::Merge).record().unwrap();
        let view = record.view().unwrap();
        let end = offset(&record, view.roots[0].parent) + view.roots[0].parent.len();
        let mut bytes = record.bytes.clone();
        bytes[end..end + 8].copy_from_slice(&(view.roots[0].parent_mount + 1).to_le_bytes());
        seal(&mut bytes);
        assert!(Record::decode(&bytes).is_err());
        let mut bytes = record.bytes.clone();
        let ancestor_inode = end + 10 + 8;
        let ino = view.roots[0].ancestors[0].inode + 1;
        bytes[ancestor_inode..ancestor_inode + 8].copy_from_slice(&ino.to_le_bytes());
        seal(&mut bytes);
        assert!(Record::decode(&bytes).is_err());
        if view.roots[0].ancestors.len() > 2 {
            let mut bytes = record.bytes.clone();
            let middle = end + 10 + 16 + 8;
            bytes[middle..middle + 8].copy_from_slice(&(u64::MAX - 1).to_le_bytes());
            seal(&mut bytes);
            assert!(Record::decode(&bytes).is_err());
        }
    }
    #[test]
    fn symlink_hardlink_access_canonicalization_survives_record_roundtrip() {
        let f = Fixture::new();
        symlink("literal-target", f.paths[0].join("link")).unwrap();
        fs::hard_link(f.paths[0].join("link"), f.paths[1].join("cross")).unwrap();
        fs::hard_link(f.paths[0].join("link"), f.paths[1].join("other")).unwrap();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        let decoded = Record::decode(record.bytes()).unwrap();
        let links = decoded.view().unwrap().links().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(
            (
                links[0].kind,
                links[0].occurrences.len(),
                links[0].external_links
            ),
            (Kind::Symlink, 3, 0)
        );
        p.bind_record(&decoded, &AtomicBool::new(false)).unwrap();
    }
    #[test]
    fn distinct_filesystem_parent_chains_share_the_namespace_root_and_encode() {
        if !std::path::Path::new("/dev/shm").is_dir() {
            return;
        }
        let f = Fixture::new();
        let other = PathBuf::from("/dev/shm").join(f.base.file_name().unwrap());
        fs::create_dir(&other).unwrap();
        let mut intent = f.intent(Mode::Merge);
        intent.right = other.clone();
        intent.expected = None;
        let p = manifest::capture(intent, &AtomicBool::new(false), |_| Ok(()))
            .unwrap_or_else(|e| panic!("cross filesystem: {e}"));
        let record = p.record().unwrap();
        let decoded = Record::decode(record.bytes()).unwrap();
        let view = decoded.view().unwrap();
        assert_ne!(
            view.roots[0].entries[0].mount,
            view.roots[1].entries[0].mount
        );
        assert_eq!(
            view.roots[0].ancestors.last(),
            view.roots[1].ancestors.last()
        );
        p.bind_record(&decoded, &AtomicBool::new(false)).unwrap();
        fs::remove_dir_all(other).unwrap();
    }
    #[test]
    fn deep_absolute_root_paths_do_not_consume_descendant_depth_budget() {
        let f = Fixture::new();
        let mut parent = f.base.join("deep");
        for _ in 0..40 {
            parent.push("d");
        }
        let root = parent.join("root");
        fs::create_dir_all(&root).unwrap();
        let mut intent = f.intent(Mode::Merge);
        intent.left = root.clone();
        intent.expected = None;
        let p = manifest::capture(intent, &AtomicBool::new(false), |_| Ok(()))
            .unwrap_or_else(|e| panic!("valid deep capture: {e}"));
        let record = p
            .record()
            .unwrap_or_else(|e| panic!("deep absolute root lost descendant budget: {e}"));
        let decoded = Record::decode(record.bytes()).unwrap();
        let view = decoded.view().unwrap();
        assert_eq!(view.roots[0].original, root.as_os_str().as_encoded_bytes());
        assert_eq!(view.roots[0].entries.len(), 1);
        assert!(view.roots[0].ancestors.len() > 33);
    }
    #[test]
    fn both_complete_inventory_records_exceed_four_mib_without_truncation() {
        let f = Fixture::new();
        for (side, path) in f.paths.iter().enumerate() {
            for n in 0..directory_compare::MAX_ITEMS {
                fs::File::create(path.join(format!("{side}{n:04x}{}", "\u{1}".repeat(123))))
                    .unwrap();
            }
        }
        let p = f.prepare(Mode::Mirror(Side::Right));
        let record = p.record().unwrap();
        assert!(record.bytes.len() > 4 * 1024 * 1024 && record.bytes.len() <= MAX_WIRE);
        assert_eq!(record.bytes.capacity(), record.bytes.len());
        let restored = Record::decode(record.bytes()).unwrap();
        assert_eq!(restored.bytes.capacity(), restored.bytes.len());
        let view = restored.view().unwrap();
        for root in &view.roots {
            assert_eq!(root.entries.len(), 8193);
            assert_eq!(
                root.entries.iter().map(|e| e.relative.len()).sum::<usize>(),
                directory_compare::MAX_NAMES
            );
        }
        assert_eq!(view.payload_read, 0);
        assert_eq!(restored.seal, record.seal);
    }
    #[test]
    fn decoding_needs_no_live_paths_or_descriptors() {
        let f = Fixture::new();
        let p = f.prepare(Mode::Merge);
        let record = p.record().unwrap();
        fs::remove_dir_all(&f.paths[0]).unwrap();
        fs::remove_dir_all(&f.paths[1]).unwrap();
        let history = Record::decode(record.bytes()).unwrap();
        assert_eq!(history.view().unwrap().roots[0].entries.len(), 1);
        assert!(p.bind_record(&history, &AtomicBool::new(false)).is_err());
    }
}
