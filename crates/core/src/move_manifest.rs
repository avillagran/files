//! Bounded EXDEV source inventory. Digests are observations supplied by the
//! executor, never deletion authority. No filesystem I/O or implicit lossy paths.
use std::collections::HashMap;
pub const MAX_ENTRIES: usize = 8192;
pub const MAX_PATH: usize = 4096;
pub const MAX_WIRE: usize = 4 * 1024 * 1024;
pub const MAX_RETAINED: usize = 8 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"FLMOVE\0\x01";
const FIXED_ENTRY: usize = 147;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    File = 1,
    Directory = 2,
    Symlink = 3,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    pub kind: Kind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    pub size: u64,
    pub mtime_seconds: i64,
    pub mtime_nanos: u32,
    pub ctime_seconds: i64,
    pub ctime_nanos: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub links: u64,
}
impl Revision {
    /// Only after a positively identified rename by this executor. This is NOT
    /// a general source-change comparator; caller must rehash contents/metadata.
    pub fn matches_after_rename(self, previous: Self) -> bool {
        if self.ctime_nanos >= 1_000_000_000 {
            return false;
        }
        let mut comparable = self;
        comparable.ctime_seconds = previous.ctime_seconds;
        comparable.ctime_nanos = previous.ctime_nanos;
        comparable == previous
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessTime {
    pub seconds: i64,
    pub nanos: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// Raw slash-separated path below the captured source root(s). Parents first.
    pub relative: Vec<u8>,
    pub identity: Identity,
    pub revision: Revision,
    pub access: AccessTime,
    pub content_digest: [u8; 32],
    pub metadata_digest: [u8; 32],
}
#[derive(Debug, PartialEq, Eq)]
pub struct Manifest {
    job: [u8; 16],
    source_parent: Vec<u8>,
    destination_parent: Vec<u8>,
    source_identity: Identity,
    destination_identity: Identity,
    entries: Vec<Entry>,
}
#[derive(Clone, Copy)]
struct Borrowed<'a> {
    relative: &'a [u8],
    identity: Identity,
    revision: Revision,
    access: AccessTime,
    content_digest: [u8; 32],
    metadata_digest: [u8; 32],
}
impl Entry {
    fn borrowed(&self) -> Borrowed<'_> {
        Borrowed {
            relative: &self.relative,
            identity: self.identity,
            revision: self.revision,
            access: self.access,
            content_digest: self.content_digest,
            metadata_digest: self.metadata_digest,
        }
    }
}
fn absolute(p: &[u8]) -> bool {
    p.first() == Some(&b'/')
        && p.len() <= MAX_PATH
        && !p.contains(&0)
        && (p == b"/" || valid_relative(&p[1..]))
}
fn valid_relative(p: &[u8]) -> bool {
    !p.is_empty()
        && p.len() <= MAX_PATH
        && !p.contains(&0)
        && p.split(|b| *b == b'/')
            .all(|c| !c.is_empty() && c != b"." && c != b".." && c.len() <= 255)
        && p.iter().filter(|b| **b == b'/').count() < 64
}
struct Scanned<'a> {
    job: [u8; 16],
    source: &'a [u8],
    destination: &'a [u8],
    source_identity: Identity,
    destination_identity: Identity,
    rows: Vec<Borrowed<'a>>,
}
fn structure(entries: &[Borrowed<'_>]) -> Result<(), &'static str> {
    let mut parents = HashMap::with_capacity(entries.len());
    let mut previous: Option<&[u8]> = None;
    let mut inodes: HashMap<(u64, u64), (&Borrowed<'_>, u64)> =
        HashMap::with_capacity(entries.len());
    let mut roots = 0;
    for e in entries {
        if !valid_relative(e.relative) || previous.is_some_and(|p| p >= e.relative) {
            return Err("Manifiesto: rutas inválidas, duplicadas o fuera de orden raw");
        }
        if e.identity.inode == 0
            || e.revision.links == 0
            || e.access.nanos >= 1_000_000_000
            || e.revision.mtime_nanos >= 1_000_000_000
            || e.revision.ctime_nanos >= 1_000_000_000
            || e.revision.mode & !0o7777 != 0
        {
            return Err("Manifiesto: identidad o revisión inválida");
        }
        if let Some(n) = e.relative.iter().rposition(|b| *b == b'/') {
            if parents.get(&e.relative[..n]) != Some(&Kind::Directory) {
                return Err("Manifiesto: falta directorio padre");
            }
        } else {
            roots += 1;
            if roots > 128 {
                return Err("Manifiesto: más de128 raíces");
            }
        }
        let key = (e.identity.device, e.identity.inode);
        if let Some((first, count)) = inodes.get_mut(&key) {
            if e.identity.kind == Kind::Directory
                || first.identity.kind != e.identity.kind
                || first.revision != e.revision
                || first.access != e.access
                || first.content_digest != e.content_digest
                || first.metadata_digest != e.metadata_digest
            {
                return Err("Manifiesto: grupo hardlink incoherente o alias de directorio");
            }
            *count += 1;
            if *count > e.revision.links {
                return Err("Manifiesto: enlaces internos exceden nlink");
            }
        } else {
            inodes.insert(key, (e, 1));
        }
        parents.insert(e.relative, e.identity.kind);
        previous = Some(e.relative);
    }
    Ok(())
}
impl Manifest {
    pub fn new(
        job: [u8; 16],
        source_parent: Vec<u8>,
        destination_parent: Vec<u8>,
        source_identity: Identity,
        destination_identity: Identity,
        entries: Vec<Entry>,
    ) -> Result<Self, &'static str> {
        let m = Self {
            job,
            source_parent,
            destination_parent,
            source_identity,
            destination_identity,
            entries,
        };
        m.preflight()?;
        let borrowed: Vec<_> = m.entries.iter().map(Entry::borrowed).collect();
        structure(&borrowed)?;
        Ok(m)
    }
    pub fn root_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| !e.relative.contains(&b'/'))
            .count()
    }
    pub fn job(&self) -> [u8; 16] {
        self.job
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn source_parent(&self) -> &[u8] {
        &self.source_parent
    }
    pub fn destination_parent(&self) -> &[u8] {
        &self.destination_parent
    }
    pub fn source_identity(&self) -> Identity {
        self.source_identity
    }
    pub fn destination_identity(&self) -> Identity {
        self.destination_identity
    }
    fn preflight(&self) -> Result<usize, &'static str> {
        if self.job == [0; 16]
            || !absolute(&self.source_parent)
            || !absolute(&self.destination_parent)
            || self.source_identity.kind != Kind::Directory
            || self.destination_identity.kind != Kind::Directory
            || self.source_identity.inode == 0
            || self.destination_identity.inode == 0
            || self.source_parent.capacity() > MAX_PATH * 2
            || self.destination_parent.capacity() > MAX_PATH * 2
            || self.entries.is_empty()
            || self.entries.len() > MAX_ENTRIES
            || self.entries.capacity() > MAX_ENTRIES
        {
            return Err("Manifiesto: cabecera o cantidad inválida");
        }
        let mut retained = std::mem::size_of_val(self)
            + self.source_parent.capacity()
            + self.destination_parent.capacity()
            + self.entries.capacity() * std::mem::size_of::<Entry>();
        let mut wire =
            8 + 16 + 4 + self.source_parent.len() + 4 + self.destination_parent.len() + 34 + 4;
        for e in &self.entries {
            if e.relative.len() > MAX_PATH || e.relative.capacity() > MAX_PATH {
                return Err("Manifiesto: ruta excede límite");
            }
            wire = wire
                .checked_add(FIXED_ENTRY + e.relative.len())
                .ok_or("Manifiesto: overflow")?;
            retained = retained
                .checked_add(e.relative.capacity())
                .ok_or("Manifiesto: overflow")?;
            if wire > MAX_WIRE || retained > MAX_RETAINED {
                return Err("Manifiesto excede presupuesto");
            }
        }
        if retained > MAX_RETAINED || wire > MAX_WIRE {
            return Err("Manifiesto excede presupuesto");
        }
        Ok(wire)
    }
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        let bytes = self.preflight()?;
        let borrowed: Vec<_> = self.entries.iter().map(Entry::borrowed).collect();
        structure(&borrowed)?;
        let mut out = Vec::with_capacity(bytes);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.job);
        for p in [&self.source_parent, &self.destination_parent] {
            out.extend_from_slice(&(p.len() as u32).to_le_bytes());
            out.extend_from_slice(p);
        }
        put_identity(&mut out, self.source_identity);
        put_identity(&mut out, self.destination_identity);
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&(e.relative.len() as u16).to_le_bytes());
            out.extend_from_slice(&e.relative);
            put_identity(&mut out, e.identity);
            let r = e.revision;
            out.extend_from_slice(&r.size.to_le_bytes());
            out.extend_from_slice(&r.mtime_seconds.to_le_bytes());
            out.extend_from_slice(&r.mtime_nanos.to_le_bytes());
            out.extend_from_slice(&r.ctime_seconds.to_le_bytes());
            for v in [r.ctime_nanos, r.mode, r.uid, r.gid] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&r.links.to_le_bytes());
            out.extend_from_slice(&e.access.seconds.to_le_bytes());
            out.extend_from_slice(&e.access.nanos.to_le_bytes());
            out.extend_from_slice(&e.content_digest);
            out.extend_from_slice(&e.metadata_digest);
        }
        debug_assert_eq!(out.len(), bytes);
        Ok(out)
    }
    /// Full borrowed validation before owned names. Transient index <=8192 rows;
    /// wire and destination inventory coexist; not a claim about total process PSS.
    fn scan(data: &[u8]) -> Result<Scanned<'_>, &'static str> {
        if data.len() > MAX_WIRE {
            return Err("Manifiesto wire excede límite");
        }
        let mut r = Reader { data, at: 0 };
        if r.take(8)? != MAGIC {
            return Err("Manifiesto: versión incompatible");
        }
        let job = r.array()?;
        let source = r.path()?;
        let destination = r.path()?;
        let si = r.identity()?;
        let di = r.identity()?;
        let count = r.u32()? as usize;
        if count == 0
            || count > MAX_ENTRIES
            || job == [0; 16]
            || !absolute(source)
            || !absolute(destination)
            || si.kind != Kind::Directory
            || di.kind != Kind::Directory
            || si.inode == 0
            || di.inode == 0
        {
            return Err("Manifiesto: cabecera inválida");
        }
        if count > r.data.len().saturating_sub(r.at) / FIXED_ENTRY {
            return Err("Manifiesto truncado");
        }
        let mut rows = Vec::with_capacity(count);
        let mut retained = std::mem::size_of::<Self>()
            + source.len()
            + destination.len()
            + count * std::mem::size_of::<Entry>();
        for _ in 0..count {
            let len = u16::from_le_bytes(r.array()?) as usize;
            if len > MAX_PATH {
                return Err("Manifiesto: ruta excede límite");
            }
            let relative = r.take(len)?;
            retained += len;
            if retained > MAX_RETAINED {
                return Err("Manifiesto excede presupuesto");
            }
            let identity = r.identity()?;
            let revision = Revision {
                size: r.u64()?,
                mtime_seconds: r.i64()?,
                mtime_nanos: r.u32()?,
                ctime_seconds: r.i64()?,
                ctime_nanos: r.u32()?,
                mode: r.u32()?,
                uid: r.u32()?,
                gid: r.u32()?,
                links: r.u64()?,
            };
            let access = AccessTime {
                seconds: r.i64()?,
                nanos: r.u32()?,
            };
            rows.push(Borrowed {
                relative,
                identity,
                revision,
                access,
                content_digest: r.array()?,
                metadata_digest: r.array()?,
            });
        }
        if r.at != data.len() {
            return Err("Manifiesto: bytes sobrantes");
        }
        structure(&rows)?;
        Ok(Scanned {
            job,
            source,
            destination,
            source_identity: si,
            destination_identity: di,
            rows,
        })
    }
    pub fn decode(data: &[u8]) -> Result<Self, &'static str> {
        let s = Self::scan(data)?;
        Ok(Self {
            job: s.job,
            source_parent: s.source.to_vec(),
            destination_parent: s.destination.to_vec(),
            source_identity: s.source_identity,
            destination_identity: s.destination_identity,
            entries: s
                .rows
                .into_iter()
                .map(|e| Entry {
                    relative: e.relative.to_vec(),
                    identity: e.identity,
                    revision: e.revision,
                    access: e.access,
                    content_digest: e.content_digest,
                    metadata_digest: e.metadata_digest,
                })
                .collect(),
        })
    }
    pub fn validate(data: &[u8]) -> Result<(), &'static str> {
        Self::scan(data).map(|_| ())
    }
}
pub(crate) fn put_revision(out: &mut Vec<u8>, r: Revision) {
    out.extend_from_slice(&r.size.to_le_bytes());
    out.extend_from_slice(&r.mtime_seconds.to_le_bytes());
    out.extend_from_slice(&r.mtime_nanos.to_le_bytes());
    out.extend_from_slice(&r.ctime_seconds.to_le_bytes());
    for value in [r.ctime_nanos, r.mode, r.uid, r.gid] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&r.links.to_le_bytes());
}
pub(crate) fn put_identity(out: &mut Vec<u8>, i: Identity) {
    out.extend_from_slice(&i.device.to_le_bytes());
    out.extend_from_slice(&i.inode.to_le_bytes());
    out.push(i.kind as u8);
}
pub(crate) struct Reader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) at: usize,
}
impl<'a> Reader<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        let end = self.at.checked_add(n).ok_or("Overflow wire")?;
        let s = self.data.get(self.at..end).ok_or("Wire truncado")?;
        self.at = end;
        Ok(s)
    }
    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], &'static str> {
        self.take(N)?.try_into().map_err(|_| "Wire truncado")
    }
    pub(crate) fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, &'static str> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn i64(&mut self) -> Result<i64, &'static str> {
        Ok(i64::from_le_bytes(self.array()?))
    }
    fn path(&mut self) -> Result<&'a [u8], &'static str> {
        let n = self.u32()? as usize;
        if n > MAX_PATH {
            return Err("Ruta wire excede límite");
        }
        self.take(n)
    }
    pub(crate) fn revision(&mut self) -> Result<Revision, &'static str> {
        Ok(Revision {
            size: self.u64()?,
            mtime_seconds: self.i64()?,
            mtime_nanos: self.u32()?,
            ctime_seconds: self.i64()?,
            ctime_nanos: self.u32()?,
            mode: self.u32()?,
            uid: self.u32()?,
            gid: self.u32()?,
            links: self.u64()?,
        })
    }
    pub(crate) fn identity(&mut self) -> Result<Identity, &'static str> {
        let device = self.u64()?;
        let inode = self.u64()?;
        let kind = match self.take(1)?[0] {
            1 => Kind::File,
            2 => Kind::Directory,
            3 => Kind::Symlink,
            _ => return Err("Tipo wire inválido"),
        };
        Ok(Identity {
            device,
            inode,
            kind,
        })
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn entry(name: &[u8], kind: Kind) -> Entry {
        Entry {
            relative: name.to_vec(),
            identity: Identity {
                device: 7,
                inode: if kind == Kind::Directory { 20 } else { 30 },
                kind,
            },
            revision: Revision {
                size: 3,
                mtime_seconds: -1,
                mtime_nanos: 2,
                ctime_seconds: 3,
                ctime_nanos: 4,
                mode: 0o640,
                uid: 10,
                gid: 11,
                links: 1,
            },
            access: AccessTime {
                seconds: -5,
                nanos: 6,
            },
            content_digest: [12; 32],
            metadata_digest: [13; 32],
        }
    }
    pub(crate) fn manifest() -> Manifest {
        Manifest::new(
            [1; 16],
            b"/source".to_vec(),
            b"/destination".to_vec(),
            Identity {
                device: 7,
                inode: 10,
                kind: Kind::Directory,
            },
            Identity {
                device: 8,
                inode: 11,
                kind: Kind::Directory,
            },
            vec![
                entry(b"folder", Kind::Directory),
                entry(b"folder/\xff", Kind::File),
            ],
        )
        .unwrap()
    }
    #[test]
    fn raw_roundtrip_and_all_truncations() {
        let m = manifest();
        let wire = m.encode().unwrap();
        assert_eq!(Manifest::decode(&wire).unwrap(), m);
        for n in 0..wire.len() {
            assert!(Manifest::decode(&wire[..n]).is_err(), "prefix {n}");
        }
        let mut extra = wire.clone();
        extra.push(0);
        assert!(Manifest::decode(&extra).is_err());
        let mut future = wire;
        future[7] = 99;
        assert!(Manifest::decode(&future).is_err());
    }
    #[test]
    fn structure_rejects_traversal_duplicate_missing_parent_and_bad_revision() {
        for names in [
            vec![b"../x".as_slice()],
            vec![b"a".as_slice(), b"a"],
            vec![b"a/b".as_slice()],
            vec![b"b".as_slice(), b"a"],
            vec![b"a//b".as_slice()],
        ] {
            let mut m = manifest();
            m.entries = names.iter().map(|n| entry(n, Kind::File)).collect();
            assert!(m.encode().is_err());
        }
        let mut m = manifest();
        m.entries[0].revision.ctime_nanos = 1_000_000_000;
        assert!(m.encode().is_err());
    }
    #[test]
    fn caps_checked_before_output_and_preserve_hardlink_topology() {
        let mut m = manifest();
        m.entries = (0..MAX_ENTRIES + 1)
            .map(|_| entry(b"x", Kind::File))
            .collect();
        assert!(m.encode().is_err());
        let mut m = manifest();
        m.entries[0].relative = vec![b'x'; MAX_PATH + 1];
        assert!(m.encode().is_err());
        let mut m = manifest();
        m.entries = vec![entry(b"a", Kind::File), entry(b"b", Kind::File)];
        for e in &mut m.entries {
            e.revision.links = 2;
        }
        assert_eq!(Manifest::decode(&m.encode().unwrap()).unwrap(), m);
        let mut m = manifest();
        m.source_parent = Vec::with_capacity(MAX_RETAINED + 1);
        m.source_parent.extend_from_slice(b"/a");
        assert!(m.encode().is_err());
    }
    #[test]
    fn hardlinks_require_matching_revision_digests_kind_and_link_count() {
        let mut m = manifest();
        m.entries = vec![entry(b"a", Kind::File), entry(b"b", Kind::File)];
        for e in &mut m.entries {
            e.revision.links = 2;
        }
        let wire = m.encode().unwrap();
        Manifest::validate(&wire).unwrap();
        m.entries[1].revision.size += 1;
        assert!(m.encode().is_err());
        m.entries[1].revision = m.entries[0].revision;
        m.entries[1].content_digest[0] ^= 1;
        assert!(m.encode().is_err());
        m.entries[1].content_digest = m.entries[0].content_digest;
        m.entries[1].metadata_digest[0] ^= 1;
        assert!(m.encode().is_err());
        m.entries[1].metadata_digest = m.entries[0].metadata_digest;
        m.entries[1].identity.kind = Kind::Symlink;
        assert!(m.encode().is_err());
        m.entries[1].identity.kind = Kind::File;
        for e in &mut m.entries {
            e.revision.links = 1;
        }
        assert!(m.encode().is_err());
        for e in &mut m.entries {
            e.identity.kind = Kind::Directory;
            e.revision.links = 2;
        }
        assert!(m.encode().is_err());
    }
}
