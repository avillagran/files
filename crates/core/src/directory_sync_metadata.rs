//! Actual sync metadata values and bounded streaming preparation. Values/history
//! are not approval, durable storage or a live execution token.
use crate::move_manifest::Kind;
#[cfg(target_os = "linux")]
use crate::{
    directory_sync_record::EntryView,
    move_manifest::{AccessTime, Entry},
};
use sha2::{Digest, Sha256};
use std::io;
pub const MAX_ATTRIBUTES: usize = 64;
pub const MAX_ATTRIBUTE_BYTES: usize = 128 * 1024;
pub const MAX_VALUE_WIRE: usize = MAX_ATTRIBUTE_BYTES + 512;
pub const MAX_RECORDS: usize =
    crate::directory_sync_physical::MAX_NODES + 2 * crate::directory_sync_record::MAX_ENTRIES;
pub const DEFAULT_STREAM_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_STREAM_BYTES: u64 = 8 * 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub seconds: i64,
    pub nanos: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    name: Vec<u8>,
    value: Vec<u8>,
}
impl Attribute {
    pub fn name(&self) -> &[u8] {
        &self.name
    }
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Values {
    kind: Kind,
    mode: u32,
    uid: u32,
    gid: u32,
    access: Time,
    modified: Time,
    attributes: Vec<Attribute>,
}
/// Compact original values for checking a namespace-driven mtime change.
#[derive(Clone, Copy)]
pub(crate) struct EvidenceTemplate {
    mode: u32,
    uid: u32,
    gid: u32,
    access: Time,
    attributes: [u8; 32],
}
impl EvidenceTemplate {
    pub(crate) fn at(self, modified: Time) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.mode.to_le_bytes());
        h.update(self.uid.to_le_bytes());
        h.update(self.gid.to_le_bytes());
        h.update(self.access.seconds.to_le_bytes());
        h.update(self.access.nanos.to_le_bytes());
        h.update(modified.seconds.to_le_bytes());
        h.update(i64::from(modified.nanos).to_le_bytes());
        h.update(self.attributes);
        h.finalize().into()
    }
}
fn bad(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
impl Values {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn mode(&self) -> u32 {
        self.mode
    }
    pub fn uid(&self) -> u32 {
        self.uid
    }
    pub fn gid(&self) -> u32 {
        self.gid
    }
    pub fn access(&self) -> Time {
        self.access
    }
    pub fn modified(&self) -> Time {
        self.modified
    }
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn captured(
        e: &Entry,
        access: AccessTime,
        attributes: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> io::Result<Self> {
        let v = Self {
            kind: e.identity.kind,
            mode: e.revision.mode,
            uid: e.revision.uid,
            gid: e.revision.gid,
            access: Time {
                seconds: access.seconds,
                nanos: access.nanos,
            },
            modified: Time {
                seconds: e.revision.mtime_seconds,
                nanos: e.revision.mtime_nanos,
            },
            attributes: attributes
                .into_iter()
                .map(|(name, value)| Attribute { name, value })
                .collect(),
        };
        v.validate()?;
        Ok(v)
    }
    fn validate(&self) -> io::Result<()> {
        if self.mode & !0o7777 != 0
            || self.access.nanos >= 1_000_000_000
            || self.modified.nanos >= 1_000_000_000
            || self.attributes.len() > MAX_ATTRIBUTES
        {
            return Err(bad("Valores de metadatos inválidos"));
        }
        let mut size = 0usize;
        let mut previous: Option<&[u8]> = None;
        for a in &self.attributes {
            if a.name.is_empty()
                || a.name.len() > 255
                || a.name.contains(&0)
                || previous.is_some_and(|p| p >= a.name.as_slice())
            {
                return Err(bad("Catálogo xattr inválido"));
            }
            previous = Some(&a.name);
            size = size
                .checked_add(a.name.len() + 1)
                .and_then(|n| n.checked_add(a.value.len()))
                .filter(|&n| n <= MAX_ATTRIBUTE_BYTES)
                .ok_or_else(|| bad("Metadatos exceden límite"))?;
        }
        Ok(())
    }
    pub(crate) fn attributes_digest(&self) -> [u8; 32] {
        let mut attrs = Sha256::new();
        for a in &self.attributes {
            for b in [&a.name, &a.value] {
                attrs.update((b.len() as u64).to_le_bytes());
                attrs.update(b);
            }
        }
        attrs.finalize().into()
    }
    pub(crate) fn evidence_template(&self) -> EvidenceTemplate {
        EvidenceTemplate {
            mode: self.mode,
            uid: self.uid,
            gid: self.gid,
            access: self.access,
            attributes: self.attributes_digest(),
        }
    }
    /// Same full-value digest as the captured EXDEV/sync evidence; no kind field.
    pub fn evidence_digest(&self) -> [u8; 32] {
        self.evidence_template().at(self.modified)
    }
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(
            39 + self
                .attributes
                .iter()
                .map(|a| 6 + a.name.len() + a.value.len())
                .sum::<usize>(),
        );
        out.push(self.kind as u8);
        for n in [self.mode, self.uid, self.gid] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        for t in [self.access, self.modified] {
            out.extend_from_slice(&t.seconds.to_le_bytes());
            out.extend_from_slice(&t.nanos.to_le_bytes());
        }
        out.extend_from_slice(&(self.attributes.len() as u16).to_le_bytes());
        for a in &self.attributes {
            out.extend_from_slice(&(a.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(a.value.len() as u32).to_le_bytes());
            out.extend_from_slice(&a.name);
            out.extend_from_slice(&a.value);
        }
        if out.len() > MAX_VALUE_WIRE {
            return Err(bad("Valores codificados exceden límite"));
        }
        Ok(out)
    }
    /// Historical values only. This does not reopen paths or create live proof.
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        use crate::move_manifest::Reader;
        if bytes.len() < 39 || bytes.len() > MAX_VALUE_WIRE {
            return Err(bad("Valores truncados o excesivos"));
        }
        let mut r = Reader { data: bytes, at: 0 };
        let invalid = |_| bad("Valores truncados");
        let kind = match r.take(1).map_err(invalid)?[0] {
            1 => Kind::File,
            2 => Kind::Directory,
            3 => Kind::Symlink,
            _ => return Err(bad("Tipo inválido")),
        };
        let mode = r.u32().map_err(invalid)?;
        let uid = r.u32().map_err(invalid)?;
        let gid = r.u32().map_err(invalid)?;
        let mut time = || -> io::Result<Time> {
            Ok(Time {
                seconds: i64::from_le_bytes(r.array().map_err(invalid)?),
                nanos: r.u32().map_err(invalid)?,
            })
        };
        let access = time()?;
        let modified = time()?;
        let count = u16::from_le_bytes(r.array().map_err(invalid)?) as usize;
        if count > MAX_ATTRIBUTES {
            return Err(bad("Demasiados xattrs"));
        }
        let mut attributes = Vec::with_capacity(count);
        for _ in 0..count {
            let n = u16::from_le_bytes(r.array().map_err(invalid)?) as usize;
            let m = r.u32().map_err(invalid)? as usize;
            let name = r.take(n).map_err(invalid)?.to_vec();
            let value = r.take(m).map_err(invalid)?.to_vec();
            attributes.push(Attribute { name, value });
        }
        if r.at != bytes.len() {
            return Err(bad("Bytes sobrantes"));
        }
        let v = Self {
            kind,
            mode,
            uid,
            gid,
            access,
            modified,
            attributes,
        };
        v.validate()?;
        Ok(v)
    }
    #[cfg(target_os = "linux")]
    fn basic(&mut self, source: &EntryView<'_>, mtime: bool) -> io::Result<()> {
        if self.kind != source.identity.kind {
            return Err(bad("Overlay entre tipos distintos"));
        }
        let mode = source.revision.mode & 0o7777;
        if let Some(a) = self
            .attributes
            .iter_mut()
            .find(|a| a.name == b"system.posix_acl_access")
        {
            overlay_acl(&mut a.value, mode)?;
        }
        self.mode = mode;
        if mtime {
            self.modified = Time {
                seconds: source.revision.mtime_seconds,
                nanos: source.revision.mtime_nanos,
            };
        }
        self.validate()
    }
}
// Linux UAPI v2 ACL: LE header and 8-byte tag/perm/id entries. chmod-equivalent
// owner/mask(or group_obj)/other adjustment. Named principals/default ACL stay.
#[cfg(target_os = "linux")]
fn overlay_acl(bytes: &mut [u8], mode: u32) -> io::Result<()> {
    if bytes.len() < 28 || !(bytes.len() - 4).is_multiple_of(8) || bytes[..4] != 2u32.to_le_bytes()
    {
        return Err(bad("ACL access no soportada"));
    }
    let mut previous = (0u16, 0u32);
    let mut owner = None;
    let mut group = None;
    let mut mask = None;
    let mut other = None;
    let mut named = false;
    for (i, e) in bytes[4..].as_chunks::<8>().0.iter().enumerate() {
        let tag = u16::from_le_bytes(e[..2].try_into().map_err(|_| bad("ACL inválida"))?);
        let perm = u16::from_le_bytes(e[2..4].try_into().map_err(|_| bad("ACL inválida"))?);
        let id = u32::from_le_bytes(e[4..8].try_into().map_err(|_| bad("ACL inválida"))?);
        if perm > 7
            || !matches!(tag, 1 | 2 | 4 | 8 | 16 | 32)
            || (matches!(tag, 2 | 8) == (id == u32::MAX))
            || (tag, id) <= previous
        {
            return Err(bad("ACL access inválida"));
        }
        previous = (tag, id);
        match tag {
            1 => owner = Some(i),
            2 | 8 => named = true,
            4 => group = Some(i),
            16 => mask = Some(i),
            32 => other = Some(i),
            _ => unreachable!(),
        }
    }
    let owner = owner.ok_or_else(|| bad("ACL sin owner"))?;
    let group = group.ok_or_else(|| bad("ACL sin group"))?;
    let other = other.ok_or_else(|| bad("ACL sin other"))?;
    if named && mask.is_none() {
        return Err(bad("ACL nombrada sin máscara"));
    }
    for (i, perm) in [
        (owner, (mode >> 6) & 7),
        (mask.unwrap_or(group), (mode >> 3) & 7),
        (other, mode & 7),
    ] {
        let n = 4 + i * 8 + 2;
        bytes[n..n + 2].copy_from_slice(&(perm as u16).to_le_bytes());
    }
    Ok(())
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::{
        directory_sync_manifest::Prepared,
        directory_sync_physical::{Physical, Recipe},
    };
    use std::{
        io::Write,
        sync::atomic::{AtomicBool, Ordering},
    };
    /// Actual reads/recipe compatibility and full stream emission, not durable
    /// storage, operator approval or a promise that the caller retained bytes.
    pub struct Acquired<'a, 'p, 'r> {
        physical: &'a Physical<'p, 'r>,
        prepared: &'a Prepared,
        job: [u8; 16],
        seal: [u8; 32],
        bytes: u64,
        records: usize,
    }
    impl<'p, 'r> Acquired<'_, 'p, 'r> {
        pub fn physical(&self) -> &Physical<'p, 'r> {
            self.physical
        }
        pub fn prepared(&self) -> &Prepared {
            self.prepared
        }
        pub fn job(&self) -> [u8; 16] {
            self.job
        }
        pub fn seal(&self) -> [u8; 32] {
            self.seal
        }
        pub fn bytes(&self) -> u64 {
            self.bytes
        }
        pub fn records(&self) -> usize {
            self.records
        }
    }
    fn check(cancel: &AtomicBool) -> io::Result<()> {
        if cancel.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Adquisición cancelada",
            ))
        } else {
            Ok(())
        }
    }
    fn values(
        prepared: &Prepared,
        view: &crate::directory_sync_record::View<'_>,
        recipe: Recipe,
        cancel: &AtomicBool,
    ) -> io::Result<Values> {
        match recipe {
            Recipe::Full(l) => prepared.metadata_values(l, cancel),
            Recipe::Basic {
                base,
                source,
                mtime,
            } => {
                let mut v = prepared.metadata_values(base, cancel)?;
                v.basic(&view.roots[source.side].entries[source.index], mtime)?;
                Ok(v)
            }
        }
    }
    struct Stream<'a, W> {
        writer: &'a mut W,
        hash: Sha256,
        bytes: u64,
        limit: u64,
    }
    impl<W: Write> Stream<'_, W> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
            let next = self
                .bytes
                .checked_add(bytes.len() as u64)
                .filter(|&n| n + 32 <= self.limit)
                .ok_or_else(|| bad("Stream de metadatos excede presupuesto"))?;
            self.writer.write_all(bytes)?;
            self.hash.update(bytes);
            self.bytes = next;
            Ok(())
        }
        fn record(
            &mut self,
            tag: u8,
            index: usize,
            after_children: bool,
            v: &Values,
        ) -> io::Result<()> {
            let b = v.encode()?;
            let mut head = [0; 10];
            head[0] = tag;
            head[1..5].copy_from_slice(&(index as u32).to_le_bytes());
            head[5] = u8::from(after_children);
            head[6..10].copy_from_slice(&(b.len() as u32).to_le_bytes());
            self.write(&head)?;
            self.write(&b)
        }
    }
    pub fn acquire<'a, 'p, 'r>(
        physical: &'a Physical<'p, 'r>,
        prepared: &'a Prepared,
        job: [u8; 16],
        limit: u64,
        cancel: &AtomicBool,
        writer: &mut impl Write,
    ) -> io::Result<Acquired<'a, 'p, 'r>> {
        check(cancel)?;
        let records = physical.nodes().len() + 2 * physical.directory_metadata().len();
        if job == [0; 16]
            || limit > MAX_STREAM_BYTES
            || records > MAX_RECORDS
            || limit < 164 + records as u64 * 49
        {
            return Err(bad("Identidad/límite de adquisición inválido"));
        }
        if prepared.record()?.bytes() != physical.plan().record().bytes() {
            return Err(bad("Evidencia distinta de captura viva"));
        }
        let view = physical.plan().record().view()?;
        let mut h = Sha256::new();
        h.update(b"Files:sync-metadata-values:v1\0");
        let mut out = Stream {
            writer,
            hash: h,
            bytes: 0,
            limit,
        };
        let mut header = Vec::with_capacity(132);
        header.extend_from_slice(b"FLSMET\0\x01");
        header.extend_from_slice(&physical.plan().record().seal());
        header.extend_from_slice(&physical.plan().seal()?);
        header.extend_from_slice(&physical.seal()?);
        header.extend_from_slice(&job);
        header.extend_from_slice(&limit.to_le_bytes());
        header.extend_from_slice(&(records as u32).to_le_bytes());
        out.write(&header)?;
        let mut groups: Vec<Option<[u8; 32]>> = vec![None; physical.cohorts().len()];
        for (i, node) in physical.nodes().iter().enumerate() {
            check(cancel)?;
            let v = values(prepared, &view, node.recipe(), cancel)?;
            let digest = v.evidence_digest();
            if let Some(g) = node.cohort() {
                if groups[g].is_some_and(|previous| previous != digest) {
                    return Err(bad(
                        "Metadatos finales de hardlinks incompatibles; revisar elecciones/política",
                    ));
                }
                groups[g] = Some(digest);
            }
            out.record(0, i, false, &v)?;
        }
        for (i, d) in physical.directory_metadata().iter().enumerate() {
            check(cancel)?;
            let old = prepared.metadata_values(d.target(), cancel)?;
            out.record(2, i, false, &old)?;
            let v = match d.recipe() {
                Recipe::Full(l) => prepared.metadata_values(l, cancel)?,
                Recipe::Basic { source, mtime, .. } => {
                    let mut v = old;
                    v.basic(&view.roots[source.side].entries[source.index], mtime)?;
                    v
                }
            };
            out.record(
                1,
                i,
                matches!(d.recipe(), Recipe::Basic { mtime: false, .. }),
                &v,
            )?;
        }
        let seal: [u8; 32] = out.hash.finalize().into();
        out.writer.write_all(&seal)?;
        let bytes = out.bytes + 32;
        // Last consumer call may mutate/cancel: verify AFTER its final write.
        check(cancel)?;
        let _bound = physical.plan().bind(prepared, cancel)?;
        Ok(Acquired {
            physical,
            prepared,
            job,
            seal,
            bytes,
            records,
        })
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Target {
        Stage(usize),
        Directory(usize),
        UndoDirectory(usize),
    }
    pub struct StoredValues {
        job: [u8; 16],
        seal: [u8; 32],
        bytes: u64,
        records: usize,
    }
    impl StoredValues {
        pub fn job(&self) -> [u8; 16] {
            self.job
        }
        pub fn seal(&self) -> [u8; 32] {
            self.seal
        }
        pub fn bytes(&self) -> u64 {
            self.bytes
        }
        pub fn records(&self) -> usize {
            self.records
        }
        pub fn matches(&self, live: &Acquired<'_, '_, '_>) -> bool {
            self.job == live.job
                && self.seal == live.seal
                && self.bytes == live.bytes
                && self.records == live.records
        }
    }
    /// Streaming historical inspection. Visitor records are untrusted until the
    /// entire call succeeds, and even success grants no live/durable authority.
    struct RawRecord<'a> {
        offset: u64,
        head: &'a [u8; 10],
        body: &'a [u8],
    }
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct RecordInfo {
        pub offset: u64,
        pub length: u32,
        pub digest: [u8; 32],
    }
    pub(crate) fn record_digest(offset: u64, head: &[u8; 10], body: &[u8]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"Files:sync-index-record:v1\0");
        h.update(offset.to_le_bytes());
        h.update(head);
        h.update(body);
        h.finalize().into()
    }
    pub fn inspect(
        physical: &Physical<'_, '_>,
        reader: &mut impl std::io::Read,
        mut visit: impl FnMut(Target, bool, Values) -> io::Result<()>,
    ) -> io::Result<StoredValues> {
        inspect_raw(physical, reader, |target, flag, values, _| {
            visit(target, flag, values)
        })
    }
    pub(crate) fn inspect_indexed(
        physical: &Physical<'_, '_>,
        reader: &mut impl std::io::Read,
        mut visit: impl FnMut(Target, bool, Values, RecordInfo) -> io::Result<()>,
    ) -> io::Result<StoredValues> {
        inspect_raw(physical, reader, |target, flag, values, raw| {
            let info = RecordInfo {
                offset: raw.offset,
                length: (10 + raw.body.len()) as u32,
                digest: record_digest(raw.offset, raw.head, raw.body),
            };
            visit(target, flag, values, info)
        })
    }
    fn inspect_raw(
        physical: &Physical<'_, '_>,
        reader: &mut impl std::io::Read,
        mut visit: impl FnMut(Target, bool, Values, RawRecord<'_>) -> io::Result<()>,
    ) -> io::Result<StoredValues> {
        fn read(
            reader: &mut impl std::io::Read,
            h: &mut Sha256,
            bytes: &mut u64,
            limit: u64,
            buffer: &mut [u8],
        ) -> io::Result<()> {
            let next = bytes
                .checked_add(buffer.len() as u64)
                .filter(|&n| n + 32 <= limit)
                .ok_or_else(|| bad("Stream excede presupuesto"))?;
            reader.read_exact(buffer)?;
            h.update(&*buffer);
            *bytes = next;
            Ok(())
        }
        let mut h = Sha256::new();
        h.update(b"Files:sync-metadata-values:v1\0");
        let mut bytes = 0;
        let mut header = [0; 132];
        read(reader, &mut h, &mut bytes, MAX_STREAM_BYTES, &mut header)?;
        let count = u32::from_le_bytes(
            header[128..132]
                .try_into()
                .map_err(|_| bad("Cabecera inválida"))?,
        ) as usize;
        let job: [u8; 16] = header[104..120]
            .try_into()
            .map_err(|_| bad("Job inválido"))?;
        let limit = u64::from_le_bytes(
            header[120..128]
                .try_into()
                .map_err(|_| bad("Límite inválido"))?,
        );
        if &header[..8] != b"FLSMET\0\x01"
            || header[8..40] != physical.plan().record().seal()
            || header[40..72] != physical.plan().seal()?
            || header[72..104] != physical.seal()?
            || job == [0; 16]
            || count != physical.nodes().len() + 2 * physical.directory_metadata().len()
            || count > MAX_RECORDS
            || limit > MAX_STREAM_BYTES
            || limit < 164 + count as u64 * 49
        {
            return Err(bad("Cabecera de metadatos incoherente"));
        }
        let view = physical.plan().record().view()?;
        let mut groups = vec![None; physical.cohorts().len()];
        let mut old_directory = None;
        for n in 0..count {
            let mut head = [0; 10];
            read(reader, &mut h, &mut bytes, limit, &mut head)?;
            let index =
                u32::from_le_bytes(head[1..5].try_into().map_err(|_| bad("Índice inválido"))?)
                    as usize;
            let len = u32::from_le_bytes(
                head[6..10]
                    .try_into()
                    .map_err(|_| bad("Longitud inválida"))?,
            ) as usize;
            let (tag, i, after_children, recipe, target) = if n < physical.nodes().len() {
                let node = &physical.nodes()[n];
                (0, n, false, node.recipe(), Target::Stage(n))
            } else {
                let offset = n - physical.nodes().len();
                let i = offset / 2;
                let d = &physical.directory_metadata()[i];
                if offset.is_multiple_of(2) {
                    (
                        2,
                        i,
                        false,
                        Recipe::Full(d.target()),
                        Target::UndoDirectory(i),
                    )
                } else {
                    (
                        1,
                        i,
                        matches!(d.recipe(), Recipe::Basic { mtime: false, .. }),
                        d.recipe(),
                        Target::Directory(i),
                    )
                }
            };
            if head[0] != tag
                || index != i
                || head[5] != u8::from(after_children)
                || !(39..=MAX_VALUE_WIRE).contains(&len)
            {
                return Err(bad("Registro fuera de orden/alcance"));
            }
            let mut buffer = vec![0; len];
            read(reader, &mut h, &mut bytes, limit, &mut buffer)?;
            let v = Values::decode(&buffer)?;
            match recipe {
                Recipe::Full(l) => {
                    let e = &view.roots[l.side].entries[l.index];
                    if v.kind != e.identity.kind || v.evidence_digest() != e.metadata {
                        return Err(bad("Valores completos no corresponden a evidencia"));
                    }
                }
                Recipe::Basic {
                    base,
                    source,
                    mtime,
                } => {
                    let (b, s) = (
                        &view.roots[base.side].entries[base.index],
                        &view.roots[source.side].entries[source.index],
                    );
                    let modified = if mtime { s } else { b };
                    if v.kind != b.identity.kind
                        || v.mode != s.revision.mode
                        || v.uid != b.revision.uid
                        || v.gid != b.revision.gid
                        || v.access
                            != (Time {
                                seconds: b.access.seconds,
                                nanos: b.access.nanos,
                            })
                        || v.modified
                            != (Time {
                                seconds: modified.revision.mtime_seconds,
                                nanos: modified.revision.mtime_nanos,
                            })
                    {
                        return Err(bad("Overlay básico incoherente"));
                    }
                    if let Some(a) = v
                        .attributes
                        .iter()
                        .find(|a| a.name == b"system.posix_acl_access")
                    {
                        let mut adjusted = a.value.clone();
                        overlay_acl(&mut adjusted, v.mode)?;
                        if adjusted != a.value {
                            return Err(bad("ACL del overlay no corresponde a permisos"));
                        }
                    }
                    if tag == 1 {
                        let mut old: Values = old_directory
                            .take()
                            .ok_or_else(|| bad("Falta undo de directorio"))?;
                        old.basic(s, mtime)?;
                        if old != v {
                            return Err(bad("Overlay de directorio no corresponde al undo"));
                        }
                    }
                }
            }
            if tag == 0
                && let Some(g) = physical.nodes()[i].cohort()
            {
                let digest = v.evidence_digest();
                if groups[g].is_some_and(|p| p != digest) {
                    return Err(bad("Cohorte incoherente"));
                }
                groups[g] = Some(digest);
            }
            if tag == 2 {
                old_directory = Some(v.clone());
            } else if tag == 1 {
                old_directory = None;
            }
            visit(
                target,
                after_children,
                v,
                RawRecord {
                    offset: bytes - 10 - buffer.len() as u64,
                    head: &head,
                    body: &buffer,
                },
            )?;
        }
        let seal: [u8; 32] = h.finalize().into();
        let mut stored = [0; 32];
        reader.read_exact(&mut stored)?;
        let mut tail = [0];
        if seal != stored || reader.read(&mut tail)? != 0 {
            return Err(bad("Sello/trailing inválido"));
        }
        Ok(StoredValues {
            job,
            seal,
            bytes: bytes + 32,
            records: count,
        })
    }
}
#[cfg(target_os = "linux")]
pub use linux::{Acquired, StoredValues, Target, acquire, inspect};
#[cfg(target_os = "linux")]
pub(crate) use linux::{inspect_indexed, record_digest};
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        directory_sync::{Mode, Side},
        directory_sync_manifest::{Prepared, capture},
        directory_sync_physical::Physical,
        directory_sync_plan::{Decision, Options, Plan},
        directory_sync_wire::Intent,
    };
    use std::{
        fs,
        io::{Cursor, Write},
        os::unix::{
            ffi::OsStringExt,
            fs::{PermissionsExt, symlink},
        },
        path::{Path, PathBuf},
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
                "files-sync-values-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let paths = [base.join("a"), base.join("b")];
            for p in &paths {
                fs::create_dir_all(p).unwrap();
            }
            Self { base, paths }
        }
        fn file(&self, s: usize, name: &str, data: &[u8]) {
            fs::write(self.paths[s].join(name), data).unwrap();
        }
        fn prepare(&self) -> Prepared {
            capture(
                Intent {
                    version: 1,
                    id: 17,
                    left: self.paths[0].clone(),
                    right: self.paths[1].clone(),
                    expected: None,
                    mode: Mode::Merge,
                    read_limit: 1024 * 1024,
                },
                &AtomicBool::new(false),
                |_| Ok(()),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn choose(p: &mut Plan<'_>, name: &[u8]) {
        let i = (0..p.lines().len())
            .find(|&i| p.name(i) == Some(name))
            .unwrap();
        p.choose(p.receipt(i).unwrap(), Decision::Take(Side::Left))
            .unwrap();
    }
    fn attr(path: &Path, name: &[u8], value: &[u8]) {
        let p = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let n = std::ffi::CString::new(name).unwrap();
        assert_eq!(
            unsafe {
                libc::setxattr(
                    p.as_ptr(),
                    n.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
    }
    fn get_attr(path: &Path, name: &[u8]) -> Vec<u8> {
        let p = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let n = std::ffi::CString::new(name).unwrap();
        let size = unsafe { libc::getxattr(p.as_ptr(), n.as_ptr(), std::ptr::null_mut(), 0) };
        assert!(size >= 0);
        let mut b = vec![0; size as usize];
        assert_eq!(
            unsafe { libc::getxattr(p.as_ptr(), n.as_ptr(), b.as_mut_ptr().cast(), b.len()) },
            size
        );
        b
    }
    fn time(path: &Path) {
        fs::File::open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_accessed(std::time::UNIX_EPOCH + std::time::Duration::from_secs(100))
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(200)),
            )
            .unwrap();
    }
    fn sample() -> Values {
        Values {
            kind: Kind::File,
            mode: 0o640,
            uid: 1000,
            gid: 1000,
            access: Time {
                seconds: -1,
                nanos: 42,
            },
            modified: Time {
                seconds: 4,
                nanos: 7,
            },
            attributes: vec![Attribute {
                name: b"user.raw".to_vec(),
                value: vec![0, 255, b'\n'],
            }],
        }
    }
    #[test]
    fn value_codec_preserves_raw_fields_and_rejects_all_truncations_and_invalid_shapes() {
        let v = sample();
        let bytes = v.encode().unwrap();
        assert_eq!(Values::decode(&bytes).unwrap(), v);
        for n in 0..bytes.len() {
            assert!(Values::decode(&bytes[..n]).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(Values::decode(&extra).is_err());
        for (offset, value) in [(0, 4), (2, 255), (24, 255), (37, 255)] {
            let mut b = bytes.clone();
            b[offset] = value;
            assert!(Values::decode(&b).is_err(), "offset {offset}");
        }
        let mut v = sample();
        v.attributes.push(v.attributes[0].clone());
        assert!(v.encode().is_err());
        v.attributes[0].name.push(0);
        assert!(v.encode().is_err());
    }
    #[test]
    fn maximum_value_payload_is_bounded_and_not_truncated() {
        let mut v = sample();
        let name = v.attributes[0].name.len() + 1;
        v.attributes[0].value = vec![0x81; MAX_ATTRIBUTE_BYTES - name];
        let b = v.encode().unwrap();
        assert!(b.len() <= MAX_VALUE_WIRE);
        assert_eq!(Values::decode(&b).unwrap(), v);
        v.attributes[0].value.push(0);
        assert!(v.encode().is_err());
        v = sample();
        v.attributes = (0..65)
            .map(|i| Attribute {
                name: format!("user.{i:02}").into_bytes(),
                value: vec![],
            })
            .collect();
        assert!(v.encode().is_err());
    }
    #[test]
    fn actual_raw_xattrs_roundtrip_and_digest_matches_capture() {
        let f = Fixture::new();
        let name = std::ffi::OsString::from_vec(b".raw\xff\n".to_vec());
        let path = f.paths[0].join(name);
        fs::write(&path, b"x").unwrap();
        attr(&path, b"user.z", &[0, 255, b'\n']);
        attr(&path, b"user.a", b"first");
        attr(&path, b"user.raw\xff", b"raw-name");
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, Options::default()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        let live = acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        assert_eq!(live.records(), 1);
        let view = record.view().unwrap();
        let source = physical.nodes()[0].source();
        let mut seen = 0;
        let stored = inspect(&physical, &mut Cursor::new(&bytes), |target, flag, v| {
            assert_eq!(target, Target::Stage(0));
            assert!(!flag);
            assert_eq!(v.attributes[0].name, b"user.a");
            assert_eq!(v.attributes[1].name, b"user.raw\xff");
            assert_eq!(v.attributes[2].value, vec![0, 255, b'\n']);
            assert_eq!(
                v.evidence_digest(),
                view.roots[source.side].entries[source.index].metadata
            );
            seen += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, 1);
        assert!(stored.matches(&live));
        assert_eq!(live.bytes(), bytes.len() as u64);
        fs::remove_dir_all(&f.paths[0]).unwrap();
        fs::remove_dir_all(&f.paths[1]).unwrap();
        assert!(inspect(&physical, &mut Cursor::new(&bytes), |_, _, _| Ok(())).is_ok());
    }
    #[test]
    fn actual_basic_recipes_can_converge_despite_different_old_hashes() {
        let f = Fixture::new();
        f.file(0, "x", b"source");
        fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
        for name in ["x", "y"] {
            f.file(1, name, b"old");
            time(&f.paths[1].join(name));
        }
        fs::set_permissions(f.paths[1].join("x"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(f.paths[1].join("y"), fs::Permissions::from_mode(0o644)).unwrap();
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, Options::default()).unwrap();
        choose(&mut p, b"x");
        choose(&mut p, b"y");
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.metadata_constraints(), 1);
        let mut bytes = Vec::new();
        acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        let mut digests = Vec::new();
        inspect(&physical, &mut Cursor::new(bytes), |_, _, v| {
            digests.push(v.evidence_digest());
            Ok(())
        })
        .unwrap();
        assert_eq!(digests.len(), 2);
        assert_eq!(digests[0], digests[1]);
    }
    #[test]
    fn truly_incompatible_hardlink_attributes_reject_complete_preparation() {
        let f = Fixture::new();
        f.file(0, "x", b"source");
        fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
        for name in ["x", "y"] {
            f.file(1, name, b"old");
            time(&f.paths[1].join(name));
        }
        attr(&f.paths[1].join("x"), b"user.keep", b"different");
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, Options::default()).unwrap();
        choose(&mut p, b"x");
        choose(&mut p, b"y");
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        let error = acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("incompatibles"));
        assert!(inspect(&physical, &mut Cursor::new(bytes), |_, _, _| Ok(())).is_err());
        assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"old");
    }
    fn acl() -> Vec<u8> {
        let mut b = 2u32.to_le_bytes().to_vec();
        for (tag, perm, id) in [
            (1u16, 6u16, u32::MAX),
            (2, 4, 100001),
            (4, 4, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            b.extend_from_slice(&tag.to_le_bytes());
            b.extend_from_slice(&perm.to_le_bytes());
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }
    #[test]
    fn basic_acl_overlay_matches_real_linux_chmod_and_retains_named_entries() {
        let f = Fixture::new();
        f.file(0, "x", b"new");
        f.file(1, "x", b"old");
        fs::set_permissions(f.paths[0].join("x"), fs::Permissions::from_mode(0o715)).unwrap();
        attr(&f.paths[1].join("x"), b"system.posix_acl_access", &acl());
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, Options::default()).unwrap();
        choose(&mut p, b"x");
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        let mut desired = None;
        inspect(&physical, &mut Cursor::new(bytes), |_, _, v| {
            assert_eq!(v.mode(), 0o715);
            desired = Some(
                v.attributes
                    .iter()
                    .find(|a| a.name == b"system.posix_acl_access")
                    .unwrap()
                    .value
                    .clone(),
            );
            Ok(())
        })
        .unwrap();
        fs::set_permissions(f.paths[1].join("x"), fs::Permissions::from_mode(0o715)).unwrap();
        assert_eq!(
            desired.unwrap(),
            get_attr(&f.paths[1].join("x"), b"system.posix_acl_access")
        );
    }
    #[test]
    fn malformed_acl_overlay_fails_without_partial_value_mutation() {
        for bytes in [
            vec![],
            vec![0; 28],
            {
                let mut b = acl();
                b[4] = 3;
                b
            },
            {
                let mut b = acl();
                b[14] = 8;
                b
            },
            {
                let mut b = acl();
                b[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
                b
            },
        ] {
            let mut b = bytes;
            let before = b.clone();
            assert!(overlay_acl(&mut b, 0o755).is_err());
            assert_eq!(b, before);
        }
    }
    #[test]
    fn directory_undo_and_after_children_policy_are_separate_complete_records() {
        let f = Fixture::new();
        f.file(0, "new", b"x");
        fs::set_permissions(&f.paths[0], fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&f.paths[1], fs::Permissions::from_mode(0o755)).unwrap();
        attr(&f.paths[1], b"user.dir", b"keep");
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let mut p = Plan::new(
            &record,
            Options {
                root_metadata: true,
                ..Options::default()
            },
        )
        .unwrap();
        choose(&mut p, b"");
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        let live = acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        assert_eq!(live.records(), 3);
        let mut seen = Vec::new();
        inspect(&physical, &mut Cursor::new(bytes), |target, flag, v| {
            if target == Target::UndoDirectory(0) {
                assert_eq!(v.mode(), 0o755);
                assert!(!flag);
            }
            if target == Target::Directory(0) {
                assert_eq!(v.mode(), 0o700);
                assert!(flag);
                assert_eq!(v.attributes[0].value, b"keep");
            }
            seen.push(target);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            seen,
            vec![
                Target::Stage(0),
                Target::UndoDirectory(0),
                Target::Directory(0)
            ]
        );
    }
    type WriteHook<'a> = dyn FnMut(&[u8]) -> io::Result<()> + 'a;
    struct Hook<'a> {
        bytes: Vec<u8>,
        hook: Box<WriteHook<'a>>,
    }
    impl Write for Hook<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            (self.hook)(b)?;
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn final_consumer_mutation_or_cancellation_cannot_return_live_receipt() {
        for cancel_last in [false, true] {
            let f = Fixture::new();
            f.file(0, "x", b"x");
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let cancel = AtomicBool::new(false);
            let mut sink = Hook {
                bytes: Vec::new(),
                hook: Box::new(|b| {
                    if b.len() == 32 {
                        if cancel_last {
                            cancel.store(true, Ordering::Release)
                        } else {
                            f.file(0, "x", b"changed")
                        }
                    }
                    Ok(())
                }),
            };
            assert!(
                acquire(
                    &physical,
                    &prepared,
                    [7; 16],
                    DEFAULT_STREAM_BYTES,
                    &cancel,
                    &mut sink
                )
                .is_err()
            );
            assert!(inspect(&physical, &mut Cursor::new(&sink.bytes), |_, _, _| Ok(())).is_ok());
        }
    }
    #[test]
    fn ancestor_replacement_does_not_emit_outside_attributes() {
        let f = Fixture::new();
        for path in &f.paths {
            fs::create_dir_all(path.join("d/sub")).unwrap();
        }
        f.file(0, "d/sub/x", b"x");
        fs::create_dir(f.base.join("outside")).unwrap();
        fs::write(f.base.join("outside/x"), b"other").unwrap();
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, Options::default()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.nodes().len(), 1);
        let mut sink = Hook {
            bytes: Vec::new(),
            hook: Box::new(|b| {
                if b.len() == 132 {
                    fs::rename(f.paths[0].join("d/sub"), f.paths[0].join("d/old"))?;
                    symlink(f.base.join("outside"), f.paths[0].join("d/sub"))?;
                }
                Ok(())
            }),
        };
        assert!(
            acquire(
                &physical,
                &prepared,
                [7; 16],
                DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
                &mut sink
            )
            .is_err()
        );
        assert_eq!(sink.bytes.len(), 132);
    }
    #[test]
    fn byte_budget_sink_failure_job_and_foreign_capture_fail_closed() {
        let f = Fixture::new();
        f.file(0, "x", b"x");
        attr(&f.paths[0].join("x"), b"user.large", &[1; 1024]);
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, Options::default()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        assert!(
            acquire(
                &physical,
                &prepared,
                [0; 16],
                DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
                &mut bytes
            )
            .is_err()
        );
        assert!(bytes.is_empty());
        assert!(
            acquire(
                &physical,
                &prepared,
                [7; 16],
                164 + 49,
                &AtomicBool::new(false),
                &mut bytes
            )
            .is_err()
        );
        let mut fail = Hook {
            bytes: Vec::new(),
            hook: Box::new(|_| Err(io::Error::other("sink"))),
        };
        assert!(
            acquire(
                &physical,
                &prepared,
                [7; 16],
                DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
                &mut fail
            )
            .is_err()
        );
        let other = f.prepare();
        assert!(
            acquire(
                &physical,
                &other,
                [7; 16],
                MAX_STREAM_BYTES + 1,
                &AtomicBool::new(false),
                &mut Vec::new()
            )
            .is_err()
        );
        let f2 = Fixture::new();
        let foreign = f2.prepare();
        assert!(
            acquire(
                &physical,
                &foreign,
                [7; 16],
                DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
                &mut Vec::new()
            )
            .is_err()
        );
    }
    fn reseal(bytes: &mut [u8]) {
        let end = bytes.len() - 32;
        let mut h = Sha256::new();
        h.update(b"Files:sync-metadata-values:v1\0");
        h.update(&bytes[..end]);
        bytes[end..].copy_from_slice(&h.finalize());
    }
    #[test]
    fn stream_rejects_truncation_order_budget_and_resealed_full_value_claims() {
        let f = Fixture::new();
        f.file(0, "x", b"x");
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, Options::default()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        for n in 0..bytes.len() {
            assert!(inspect(&physical, &mut Cursor::new(&bytes[..n]), |_, _, _| Ok(())).is_err());
        }
        for offset in [0, 8, 40, 72, 128, 132, 133, 137, 142 + 1] {
            let mut b = bytes.clone();
            b[offset] ^= 1;
            reseal(&mut b);
            assert!(
                inspect(&physical, &mut Cursor::new(b), |_, _, _| Ok(())).is_err(),
                "offset {offset}"
            );
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(inspect(&physical, &mut Cursor::new(extra), |_, _, _| Ok(())).is_err());
    }
    #[test]
    fn well_formed_basic_history_can_be_forged_but_does_not_match_live_receipt() {
        let f = Fixture::new();
        f.file(0, "x", b"a");
        f.file(1, "x", b"b");
        attr(
            &f.paths[1].join("x"),
            b"user.keep",
            b"unique-original-attribute",
        );
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, Options::default()).unwrap();
        choose(&mut p, b"x");
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        let live = acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        let needle = b"unique-original-attribute";
        let pos = bytes
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        bytes[pos] ^= 1;
        reseal(&mut bytes);
        let history = inspect(&physical, &mut Cursor::new(bytes), |_, _, _| Ok(())).unwrap();
        assert!(!history.matches(&live));
    }
    #[test]
    fn symlink_values_keep_captured_canonical_access_time_without_following_target() {
        let f = Fixture::new();
        symlink("missing", f.paths[0].join("link")).unwrap();
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, Options::default()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        let mut bytes = Vec::new();
        acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut bytes,
        )
        .unwrap();
        inspect(&physical, &mut Cursor::new(bytes), |_, _, v| {
            assert_eq!(v.kind(), Kind::Symlink);
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn full_both_inventories_stream_all_values_without_retaining_payloads() {
        let f = Fixture::new();
        for s in 0..2 {
            for n in 0..crate::directory_compare::MAX_ITEMS {
                f.file(s, &format!("{s}-{n:04}"), b"");
            }
        }
        let prepared = f.prepare();
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, Options::default()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        let acquired = acquire(
            &physical,
            &prepared,
            [7; 16],
            DEFAULT_STREAM_BYTES,
            &AtomicBool::new(false),
            &mut io::sink(),
        )
        .unwrap();
        assert_eq!(acquired.records(), 16384);
        assert!(acquired.bytes() >= 164 + 16384 * 49);
        assert!(acquired.bytes() <= DEFAULT_STREAM_BYTES);
    }
}
